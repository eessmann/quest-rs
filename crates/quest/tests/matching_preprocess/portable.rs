//! Tiny cold differential only: actual controlled primitive circuit vs produced fused shard.
#![allow(
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	clippy::panic_in_result_fn,
	reason = "Independent cold numerical fixture is restricted to 128 amplitudes and 32 sparse records"
)]
use quest::qsvt::matching::preprocess::{ProducedMatching, ReplayEdge};
use quest::{Complex64, MemoryBudget, QubitCount, collective::CollectiveEnvironment};
use quest_compile::{Angle, Control, ControlState, Gate, QuantumRegionBuilder};
use quest_qsvt::{MatchingColumn, MatchingHeader, NumericalPolicy, materialize_program};
use quest_sys::mpi::MpiCommunicator;
type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

// Explicitly bounded post-production gathering is solely an independent cold test.
fn gather(
	comm: &MpiCommunicator<'_>,
	produced: &ProducedMatching,
) -> TestResult<(Vec<MatchingColumn>, Vec<ReplayEdge>)> {
	let mut lane = comm.collective_lane()?;
	let rank = usize::try_from(comm.rank()?)?;
	let mut columns = Vec::new();
	let mut edges = Vec::new();
	for root in 0..comm.size()? {
		let mut count = u64::try_from(produced.shard().records().len())?.to_le_bytes();
		lane.broadcast_bytes(root, &mut count)?;
		let count = usize::try_from(u64::from_le_bytes(count))?;
		assert!(count <= 32);
		for i in 0..count {
			let mut packet = [0; 56];
			if usize::try_from(root)? == rank {
				let c = produced.shard().records()[i];
				for (word, bytes) in [
					u64::try_from(c.color)?,
					u64::try_from(c.source)?,
					u64::try_from(c.destination)?,
					c.cosine.to_bits(),
					c.sine.to_bits(),
					c.phase.re.to_bits(),
					c.phase.im.to_bits(),
				]
				.into_iter()
				.zip(packet.as_chunks_mut::<8>().0)
				{
					bytes.copy_from_slice(&word.to_le_bytes());
				}
			}
			lane.broadcast_bytes(root, &mut packet)?;
			let words = packet
				.as_chunks::<8>()
				.0
				.iter()
				.map(|bytes| u64::from_le_bytes(*bytes))
				.collect::<Vec<_>>();
			columns.push(MatchingColumn {
				color: usize::try_from(words[0])?,
				source: usize::try_from(words[1])?,
				destination: usize::try_from(words[2])?,
				cosine: f64::from_bits(words[3]),
				sine: f64::from_bits(words[4]),
				phase: Complex64::new(f64::from_bits(words[5]), f64::from_bits(words[6])),
			});
		}
		let mut count = u64::try_from(produced.edges().len())?.to_le_bytes();
		lane.broadcast_bytes(root, &mut count)?;
		let count = usize::try_from(u64::from_le_bytes(count))?;
		assert!(count <= 16);
		for i in 0..count {
			let mut packet = [0; 56];
			if usize::try_from(root)? == rank {
				let e = produced.edges()[i];
				for (word, bytes) in [
					u64::try_from(e.color)?,
					u64::try_from(e.row)?,
					u64::try_from(e.column)?,
					e.value.re.to_bits(),
					e.value.im.to_bits(),
					e.theta.to_bits(),
					e.phase.to_bits(),
				]
				.into_iter()
				.zip(packet.as_chunks_mut::<8>().0)
				{
					bytes.copy_from_slice(&word.to_le_bytes());
				}
			}
			lane.broadcast_bytes(root, &mut packet)?;
			let words = packet
				.as_chunks::<8>()
				.0
				.iter()
				.map(|bytes| u64::from_le_bytes(*bytes))
				.collect::<Vec<_>>();
			edges.push(ReplayEdge {
				color: usize::try_from(words[0])?,
				row: usize::try_from(words[1])?,
				column: usize::try_from(words[2])?,
				value: Complex64::new(f64::from_bits(words[3]), f64::from_bits(words[4])),
				theta: f64::from_bits(words[5]),
				phase: f64::from_bits(words[6]),
			});
		}
	}
	columns.sort_unstable_by_key(|c| (c.color, c.source));
	edges.sort_unstable_by_key(|e| (e.row, e.column));
	Ok((columns, edges))
}
fn emit(
	builder: &mut QuantumRegionBuilder,
	gate: Gate,
	target: usize,
	mask: usize,
	value: usize,
) -> TestResult {
	let mut controls = Vec::new();
	for bit in 0..7 {
		if mask & (1 << bit) != 0 {
			controls.push(Control::new(
				builder.qubit(bit)?,
				if value & (1 << bit) == 0 {
					ControlState::Zero
				} else {
					ControlState::One
				},
			));
		}
	}
	builder.gate(gate, &[builder.qubit(target)?], &controls)?;
	Ok(())
}
fn phase(builder: &mut QuantumRegionBuilder, angle: f64, mask: usize, value: usize) -> TestResult {
	let mut controls = Vec::new();
	for bit in 0..7 {
		if mask & (1 << bit) != 0 {
			controls.push(Control::new(
				builder.qubit(bit)?,
				if value & (1 << bit) == 0 {
					ControlState::Zero
				} else {
					ControlState::One
				},
			));
		}
	}
	builder.global_phase(Angle::radians(angle)?, &controls)?;
	Ok(())
}
fn mapped(value: usize, targets: &[usize]) -> usize {
	targets
		.iter()
		.enumerate()
		.fold(0, |out, (local, &physical)| {
			out | (((value >> local) & 1) << physical)
		})
}
fn swap(
	builder: &mut QuantumRegionBuilder,
	left: usize,
	right: usize,
	header: MatchingHeader,
	targets: &[usize],
	mask: usize,
	value: usize,
) -> TestResult {
	let mut path = vec![left];
	let mut current = left;
	for bit in 0..header.system_qubits {
		if (left ^ right) & (1 << bit) != 0 {
			current ^= 1 << bit;
			path.push(current);
		}
	}
	let mut moves: Vec<_> = path.windows(2).map(|p| (p[0], p[1])).collect();
	moves.extend(
		path.windows(2)
			.take(path.len().saturating_sub(2))
			.rev()
			.map(|p| (p[0], p[1])),
	);
	for (a, b) in moves {
		let bit = usize::try_from((a ^ b).trailing_zeros())?;
		let target = targets[bit + 1];
		let target_mask = 1 << target;
		let system_mask = mapped(((1 << header.system_qubits) - 1) << 1, targets);
		emit(
			builder,
			Gate::X,
			target,
			mask | (system_mask & !target_mask),
			value | (mapped(a << 1, targets) & !target_mask),
		)?;
	}
	Ok(())
}
fn circuit(
	header: MatchingHeader,
	columns: &[MatchingColumn],
	edges: &[ReplayEdge],
	targets: &[usize],
	outer_value: usize,
) -> TestResult<quest_compile::BoundRegion> {
	let mut builder = QuantumRegionBuilder::new(7, 0)?;
	let outer_mask = 1 << 6;
	for bit in 0..header.color_qubits {
		emit(
			&mut builder,
			Gate::H,
			targets[header.system_qubits + 1 + bit],
			outer_mask,
			outer_value,
		)?;
	}
	let color_mask = mapped(
		((1 << header.color_qubits) - 1) << (header.system_qubits + 1),
		targets,
	);
	let system_mask = mapped(((1 << header.system_qubits) - 1) << 1, targets);
	for color in 0..header.num_colors {
		let mask = color_mask | outer_mask;
		let value = mapped(color << (header.system_qubits + 1), targets) | outer_value;
		emit(
			&mut builder,
			Gate::Ry(Angle::radians(std::f64::consts::PI)?),
			targets[0],
			mask,
			value,
		)?;
		for edge in edges.iter().filter(|e| e.color == color) {
			let edge_value = value | mapped(edge.column << 1, targets);
			emit(
				&mut builder,
				Gate::Ry(Angle::radians(edge.theta - std::f64::consts::PI)?),
				targets[0],
				mask | system_mask,
				edge_value,
			)?;
			phase(
				&mut builder,
				edge.phase,
				mask | system_mask | (1 << targets[0]),
				edge_value,
			)?;
		}
		let n = header.system_dimension()?;
		let mut visited = vec![false; n];
		for initial in 0..n {
			if visited[initial] {
				continue;
			}
			let mut current = initial;
			let mut cycle = Vec::new();
			loop {
				assert!(!visited[current]);
				visited[current] = true;
				cycle.push(current);
				current = columns
					.iter()
					.find(|c| c.color == color && c.source == current)
					.map_or(current, |c| c.destination);
				if current == initial {
					break;
				}
			}
			for &destination in cycle.iter().skip(1) {
				swap(
					&mut builder,
					initial,
					destination,
					header,
					targets,
					mask,
					value,
				)?;
			}
		}
	}
	for bit in 0..header.color_qubits {
		emit(
			&mut builder,
			Gate::H,
			targets[header.system_qubits + 1 + bit],
			outer_mask,
			outer_value,
		)?;
	}
	Ok(builder.finish()?.bind(&[])?)
}
fn h(state: &mut [Complex64], target: usize) {
	let bit = 1 << target;
	for i in 0..state.len() {
		if i & bit == 0 {
			let a = state[i];
			let b = state[i | bit];
			state[i] = (a + b) * std::f64::consts::FRAC_1_SQRT_2;
			state[i | bit] = (a - b) * std::f64::consts::FRAC_1_SQRT_2;
		}
	}
}
fn independent(
	header: MatchingHeader,
	columns: &[MatchingColumn],
	edges: &[ReplayEdge],
	basis: usize,
) -> TestResult<Vec<Complex64>> {
	let n = header.system_dimension()?;
	let mut state = vec![Complex64::new(0.0, 0.0); n * header.num_colors * 2];
	state[basis] = Complex64::new(1.0, 0.0);
	for bit in 0..header.color_qubits {
		h(&mut state, header.system_qubits + 1 + bit);
	}
	let mut out = vec![Complex64::new(0.0, 0.0); state.len()];
	for color in 0..header.num_colors {
		for source in 0..n {
			let a = (color * n + source) * 2;
			let edge = edges
				.iter()
				.find(|e| e.color == color && e.column == source);
			let ratio = edge.map_or(0.0, |e| e.value.norm() / header.beta);
			let sine = ratio.mul_add(-ratio, 1.0).max(0.0).sqrt();
			let phase = edge.map_or(Complex64::new(1.0, 0.0), |e| {
				Complex64::from_polar(1.0, e.value.arg())
			});
			let dest = columns
				.iter()
				.find(|c| c.color == color && c.source == source)
				.map_or(source, |c| c.destination);
			let b = (color * n + dest) * 2;
			out[b] = (state[a] * ratio - state[a + 1] * sine) * phase;
			out[b + 1] = state[a] * sine + state[a + 1] * ratio;
		}
	}
	for bit in 0..header.color_qubits {
		h(&mut out, header.system_qubits + 1 + bit);
	}
	Ok(out)
}
#[allow(
	clippy::too_many_lines,
	reason = "Cold differential checks all logical columns and native control sectors under one environment"
)]
pub fn check(
	comm: &MpiCommunicator<'_>,
	produced: &ProducedMatching,
	expected: &[(usize, usize, usize)],
) -> TestResult {
	let header = produced.shard().header();
	let (columns, edges) = gather(comm, produced)?;
	assert_eq!(
		edges
			.iter()
			.map(|e| (e.row, e.column, e.color))
			.collect::<Vec<_>>(),
		expected
	);
	for color in 0..header.num_colors {
		let mut destinations: Vec<_> = (0..header.system_dimension()?)
			.map(|source| {
				columns
					.iter()
					.find(|c| c.color == color && c.source == source)
					.map_or(source, |c| c.destination)
			})
			.collect();
		destinations.sort_unstable();
		assert_eq!(
			destinations,
			(0..header.system_dimension()?).collect::<Vec<_>>()
		);
	}
	let environment = CollectiveEnvironment::builder(comm)?
		.memory_budget(MemoryBudget::new(4 * 1024 * 1024))
		.build()?;
	let targets = [2, 0, 4, 1, 5];
	for outer_value in [0, 1 << 6] {
		let program = circuit(header, &columns, &edges, &targets, outer_value)?;
		let unitary = materialize_program(&program, NumericalPolicy::default())?;
		// Every column, including arbitrary flag one, padding and unused color labels.
		for basis in 0..32 {
			let expected = independent(header, &columns, &edges, basis)?;
			let physical = mapped(basis, &targets) | outer_value;
			for (logical, value) in expected.iter().enumerate() {
				assert!(
					(unitary[(mapped(logical, &targets) | outer_value, physical)] - *value).norm()
						< 2e-12
				);
			}
		}
		// Independent physical A/alpha block, rectangular row/column coordinates.
		for row in 0..header.rows {
			for col in 0..header.cols {
				let expected = edges
					.iter()
					.find(|e| e.row == row && e.column == col)
					.map_or(Complex64::new(0.0, 0.0), |e| e.value / header.alpha);
				assert!(
					(unitary[(
						mapped(row << 1, &targets) | outer_value,
						mapped(col << 1, &targets) | outer_value
					)] - expected)
						.norm()
						< 2e-12
				);
			}
		}
		let mut prepared = environment.prepare_matching(
			produced.shard().clone(),
			QubitCount::new(7)?,
			targets.to_vec(),
		)?;
		let mut register = environment.state_vector_local(QubitCount::new(7)?)?;
		let local = register.deployment().local_amplitudes();
		let start = register.deployment().rank() * local;
		let state: Vec<_> = (0..128_u32)
			.map(|i| Complex64::new(f64::from(i % 13) - 6.0, f64::from(i % 7) - 3.0))
			.collect();
		let norm = state.iter().map(Complex64::norm_sqr).sum::<f64>().sqrt();
		let state: Vec<_> = state.into_iter().map(|v| v / norm).collect();
		register.init_zero()?;
		register.write_local_amplitudes(0, &state[start..start + local])?;
		let expected: Vec<_> = (start..start + local)
			.map(|row| {
				state
					.iter()
					.enumerate()
					.map(|(col, value)| unitary[(row, col)] * value)
					.sum::<Complex64>()
			})
			.collect();
		for scalar in [false, true] {
			if scalar {
				prepared.apply_scalar(&mut register, false, 1 << 6, outer_value)?;
			} else {
				prepared.apply(&mut register, false, 1 << 6, outer_value)?;
			}
			for (a, b) in register
				.read_local_amplitudes(0, local)?
				.iter()
				.zip(&expected)
			{
				assert!((*a - *b).norm() < 2e-12);
			}
			if scalar {
				prepared.apply_scalar(&mut register, true, 1 << 6, outer_value)?;
			} else {
				prepared.apply(&mut register, true, 1 << 6, outer_value)?;
			}
			for (a, b) in register
				.read_local_amplitudes(0, local)?
				.iter()
				.zip(&state[start..start + local])
			{
				assert!((*a - *b).norm() < 2e-12);
			}
		}
	}
	Ok(())
}
