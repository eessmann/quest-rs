//! Bounded measured comparison fixture; independent basis-state block extraction.
#![allow(
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	clippy::too_many_lines,
	dead_code,
	reason = "Fixed four-dimensional fixture, bounded scalar states and printed receipt fields"
)]
use quest_qsvt::{
	Complex64, Error, MatchingEncoding, NumericalPolicy, ReplayEncoding, ReplayGate, ReplayKind,
	Result, ShiftRegister, TensorShiftEncoding,
	portfolio::{
		ArithmeticStencil, ArithmeticStencilTerm, Boundary, KroneckerSum, PerMatchingBounds,
		PortfolioLimits, PortfolioResources, SparseAccessEncoding, StructuredScheme, TensorProduct,
		WeightedLcu,
	},
};
use std::time::Instant;
#[derive(Debug)]
pub struct Receipt {
	pub scheme: &'static str,
	pub alpha: f64,
	pub qubits: usize,
	pub workspace_qubits: usize,
	pub elementary_gates: usize,
	pub preparation_gates: usize,
	pub select_and_other_gates: usize,
	pub x: usize,
	pub h: usize,
	pub ry: usize,
	pub phase: usize,
	pub controlled_gates: usize,
	pub control_occurrences: usize,
	pub retained_bytes: usize,
	pub construction_peak_bytes: usize,
	pub portfolio_resources: Option<PortfolioResources>,
	pub compile_seconds: f64,
	pub block_extraction_seconds: f64,
	pub extracted: Vec<Complex64>,
}
#[derive(Debug)]
pub struct Campaign {
	pub source_seconds: f64,
	pub input_bytes: usize,
	pub receipts: Vec<Receipt>,
}
fn apply(state: &mut [Complex64], gate: ReplayGate) -> Result<()> {
	for i in 0..state.len() {
		if i & gate.control_mask != gate.control_value {
			continue;
		}
		if let ReplayKind::Phase(angle) = gate.kind {
			state[i] *= Complex64::from_polar(1., angle);
			continue;
		}
		let target = 1
			<< gate
				.target
				.ok_or(Error::Encoding("fixture primitive target"))?;
		if i & target != 0 {
			continue;
		}
		let (a, b) = (state[i], state[i | target]);
		match gate.kind {
			ReplayKind::X => {
				state[i] = b;
				state[i | target] = a;
			}
			ReplayKind::H => {
				state[i] = (a + b) * std::f64::consts::FRAC_1_SQRT_2;
				state[i | target] = (a - b) * std::f64::consts::FRAC_1_SQRT_2;
			}
			ReplayKind::Ry(angle) => {
				let (s, c) = (angle * 0.5).sin_cos();
				state[i] = c * a - s * b;
				state[i | target] = s * a + c * b;
			}
			ReplayKind::Phase(_) => return Err(Error::Encoding("fixture phase target")),
		}
	}
	Ok(())
}
fn receipt<E: ReplayEncoding>(
	scheme: &'static str,
	source: &E,
	compile_seconds: f64,
	resources: Option<PortfolioResources>,
	preparation_gates: usize,
	peak: usize,
) -> Result<Receipt> {
	let descriptor = source.descriptor()?;
	if descriptor.layout.num_qubits > 12 {
		return Err(Error::Budget("comparison reference width"));
	}
	let mut result = Receipt {
		scheme,
		alpha: descriptor.normalization,
		qubits: descriptor.layout.num_qubits,
		workspace_qubits: usize::try_from(descriptor.layout.workspace_mask.count_ones())
			.map_err(|_| Error::Budget("fixture width"))?,
		elementary_gates: 0,
		preparation_gates,
		select_and_other_gates: 0,
		x: 0,
		h: 0,
		ry: 0,
		phase: 0,
		controlled_gates: 0,
		control_occurrences: 0,
		retained_bytes: source.retained_bytes()?,
		construction_peak_bytes: peak,
		portfolio_resources: resources,
		compile_seconds,
		block_extraction_seconds: 0.,
		extracted: vec![Complex64::default(); 16],
	};
	source.visit_replay(false, &mut |gate| {
		result.elementary_gates += 1;
		match gate.kind {
			ReplayKind::X => result.x += 1,
			ReplayKind::H => result.h += 1,
			ReplayKind::Ry(_) => result.ry += 1,
			ReplayKind::Phase(_) => result.phase += 1,
		}
		result.controlled_gates += usize::from(gate.control_mask != 0);
		result.control_occurrences += usize::try_from(gate.control_mask.count_ones())
			.map_err(|_| Error::Budget("fixture controls"))?;
		Ok(())
	})?;
	result.select_and_other_gates = result
		.elementary_gates
		.checked_sub(preparation_gates)
		.ok_or(Error::Encoding("fixture prep count"))?;
	let left = descriptor.left.logical_space::<quest_qsvt::Left>(
		descriptor.layout.num_qubits,
		NumericalPolicy::default(),
	)?;
	let right = descriptor.right.logical_space::<quest_qsvt::Right>(
		descriptor.layout.num_qubits,
		NumericalPolicy::default(),
	)?;
	if left.logical_dimension() != 4 || right.logical_dimension() != 4 {
		return Err(Error::Encoding("comparison block shape"));
	}
	let started = Instant::now();
	for j in 0..4 {
		let mut state = vec![Complex64::default(); 1 << descriptor.layout.num_qubits];
		state[right
			.coordinate_at(j)
			.ok_or(Error::Encoding("fixture right coordinate"))?] = 1.0.into();
		source.visit_replay(false, &mut |gate| apply(&mut state, gate))?;
		for i in 0..4 {
			result.extracted[4 * i + j] = state[left
				.coordinate_at(i)
				.ok_or(Error::Encoding("fixture left coordinate"))?]
				* descriptor.normalization;
		}
	}
	result.block_extraction_seconds = started.elapsed().as_secs_f64();
	Ok(result)
}
fn shift(width: usize, offset: usize) -> Result<TensorShiftEncoding> {
	TensorShiftEncoding::new(
		width,
		vec![ShiftRegister::new(0, width, offset)?],
		NumericalPolicy::default(),
	)
}
fn factor() -> Result<WeightedLcu<TensorShiftEncoding>> {
	WeightedLcu::new(
		vec![
			(0.5.into(), shift(1, 0)?),
			(Complex64::new(0., 0.125), shift(1, 1)?),
		],
		PortfolioLimits::default(),
	)
}
pub fn run() -> Result<Campaign> {
	let limits = PortfolioLimits::default();
	let started = Instant::now();
	let entries = (0..4)
		.flat_map(|j| {
			[
				(j, j, Complex64::new(0.5, 0.)),
				((j + 2) % 4, j, Complex64::new(0., 0.125)),
			]
		})
		.collect();
	let matrix = quest_numerics::SparseMatrix::from_triplets(
		4,
		4,
		quest_numerics::SparseFormat::Csr,
		entries,
		quest_numerics::SparseLimits::default(),
	)?;
	let mut campaign = Campaign {
		source_seconds: started.elapsed().as_secs_f64(),
		input_bytes: matrix.retained_bytes()?,
		receipts: Vec::with_capacity(8),
	};
	let started = Instant::now();
	let base = MatchingEncoding::from_sparse(&matrix, NumericalPolicy::default())?;
	let elapsed = started.elapsed().as_secs_f64();
	let base_seconds = elapsed;
	let costs = base.resources();
	let prep = 2 * usize::try_from(base.num_colors().ilog2())
		.map_err(|_| Error::Budget("fixture matching colors"))?;
	campaign.receipts.push(receipt(
		"uniform-matching",
		&base,
		elapsed,
		None,
		prep,
		costs.construction_peak_bytes,
	)?);
	let started = Instant::now();
	let weighted = PerMatchingBounds::new(&base, limits)?;
	// The prerequisite base was already measured; include its actual construction
	// time without including the independent validation performed between stages.
	let elapsed = base_seconds + started.elapsed().as_secs_f64();
	let costs = weighted.resources();
	campaign.receipts.push(receipt(
		"per-matching",
		&weighted,
		elapsed,
		Some(costs),
		costs.preparation_gates,
		costs.construction_peak_bytes,
	)?);
	drop(weighted);
	drop(base);
	for (scheme, name) in [
		(StructuredScheme::Base, "scc-base"),
		(StructuredScheme::Prep, "scc-prep"),
	] {
		let started = Instant::now();
		let source = ArithmeticStencil::new(
			vec![2],
			vec![
				ArithmeticStencilTerm {
					weight: 0.5.into(),
					offsets: vec![0],
				},
				ArithmeticStencilTerm {
					weight: Complex64::new(0., 0.125),
					offsets: vec![2],
				},
			],
			Boundary::Periodic,
			scheme,
			limits,
		)?;
		let elapsed = started.elapsed().as_secs_f64();
		let costs = source.resources();
		campaign.receipts.push(receipt(
			name,
			&source,
			elapsed,
			Some(costs),
			costs.preparation_gates,
			costs.construction_peak_bytes,
		)?);
	}
	let started = Instant::now();
	let sparse = SparseAccessEncoding::new(&matrix, 2, limits)?;
	let elapsed = started.elapsed().as_secs_f64();
	let costs = sparse.resources();
	campaign.receipts.push(receipt(
		"sparse-qrom",
		&sparse,
		elapsed,
		Some(costs),
		costs.preparation_gates,
		costs.construction_peak_bytes,
	)?);
	drop(sparse);
	let started = Instant::now();
	let lcu = WeightedLcu::new(
		vec![
			(0.5.into(), shift(2, 0)?),
			(Complex64::new(0., 0.125), shift(2, 2)?),
		],
		limits,
	)?;
	let elapsed = started.elapsed().as_secs_f64();
	let costs = lcu.resources();
	campaign.receipts.push(receipt(
		"weighted-lcu",
		&lcu,
		elapsed,
		Some(costs),
		costs.preparation_gates,
		costs.construction_peak_bytes,
	)?);
	drop(lcu);
	let started = Instant::now();
	let b = factor()?;
	let preparation = b.resources().preparation_gates;
	let product = TensorProduct::new(shift(1, 0)?, b, limits)?;
	let elapsed = started.elapsed().as_secs_f64();
	let costs = product.resources();
	campaign.receipts.push(receipt(
		"tensor-product",
		&product,
		elapsed,
		Some(costs),
		preparation,
		costs.construction_peak_bytes,
	)?);
	drop(product);
	let started = Instant::now();
	let a = WeightedLcu::new(vec![(0.5.into(), shift(1, 0)?)], limits)?;
	let b = WeightedLcu::new(vec![(Complex64::new(0., 0.125), shift(1, 1)?)], limits)?;
	let children_preparation = a.resources().preparation_gates + b.resources().preparation_gates;
	let sum = KroneckerSum::new(a, b, limits)?;
	let elapsed = started.elapsed().as_secs_f64();
	let costs = sum.resources();
	campaign.receipts.push(receipt(
		"kronecker-sum",
		&sum,
		elapsed,
		Some(costs),
		costs.preparation_gates + children_preparation,
		costs.construction_peak_bytes,
	)?);
	Ok(campaign)
}
