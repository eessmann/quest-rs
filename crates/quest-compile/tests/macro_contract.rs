#![cfg(feature = "macros")]
use googletest::{Result, prelude::*};
#[allow(unused_imports)]
use quest_compile::prelude::*;
use quest_compile::*;

#[gtest]
fn bell_macro_uses_the_same_builder_semantics() -> Result<()> {
	let p = circuit! {qubit[2] q; bit[2] c; h q[0]; cx q[0],q[1]; c[0] = measure q[0]; c[1] = measure q[1];}?;
	let plan = p.verify()?.lower()?.plan()?;
	expect_eq!(plan.num_qubits(), 2);
	let trace = trace(&plan)?;
	expect_eq!(trace.len(), 4);
	expect_eq!(trace[1].targets, vec![1]);
	expect_eq!(trace[1].controls, vec![(0, true)]);
	Ok(())
}

#[gtest]
fn interpolation_evaluates_once_in_source_order_and_modifiers_keep_phase() -> Result<()> {
	let mut visits = vec![];
	let __quest_builder = 0.125;
	let p = circuit! {
		qubit[3] q;
		rx(${ { visits.push(1); __quest_builder } }) q[0];
		ry(${ { visits.push(2); 0.25 } }) q[1];
		negctrl @ inv @ rz(2*pi) q[2],q[1];
		ctrl @ gphase(pi / 2) q[2];
		U(pi/3, pi/5, -pi/7) q[0];
		barrier q[0],q[1];
		reset q[0];
	}?;
	expect_eq!(visits, &[1, 2]);
	expect_eq!(trace(&p.verify()?.lower()?.plan()?)?.len(), 7);
	Ok(())
}

#[gtest]
fn nonfinite_interpolation_is_rejected_at_shared_admission() {
	let p = circuit! {qubit q; rx(${f64::NAN}) q;};
	expect_true!(p.is_err());
}

#[gtest]
#[expect(
	clippy::arithmetic_side_effects,
	reason = "Finite unitary matrix residual in a regression assertion"
)]
fn full_macro_gate_inventory_matches_builder() -> Result<()> {
	let macro_program = circuit! {
		qubit[3] q;
		id q[0]; x q[0]; y q[0]; z q[0]; h q[0];
		s q[0]; sdg q[0]; t q[0]; tdg q[0]; sx q[0]; inv @ sx q[0];
		rx(pi/3) q[0]; ry(-pi/5) q[0]; rz(2*pi) q[0]; p(pi/7) q[0];
		cx q[1],q[0]; cy q[1],q[0]; cz q[1],q[0];
		swap q[2],q[0]; ccx q[2],q[1],q[0]; U(pi/3,pi/5,pi/7) q[0];
	}?;
	let mut builder = QuantumRegionBuilder::new(3, 0)?;
	let q = builder.qubit(0)?;
	let c = builder.qubit(1)?;
	let t = builder.qubit(2)?;
	for gate in [
		Gate::Id,
		Gate::X,
		Gate::Y,
		Gate::Z,
		Gate::H,
		Gate::S,
		Gate::Sdg,
		Gate::T,
		Gate::Tdg,
		Gate::Sx,
		Gate::Sxdg,
		Gate::Rx(Angle::pi(1, 3)?),
		Gate::Ry(Angle::pi(-1, 5)?),
		Gate::Rz(Angle::pi(2, 1)?),
		Gate::Phase(Angle::pi(1, 7)?),
	] {
		builder.gate(gate, &[q], &[])?;
	}
	for gate in [Gate::X, Gate::Y, Gate::Z] {
		builder.gate(gate, &[q], &[Control::new(c, ControlState::One)])?;
	}
	builder.gate(Gate::Swap, &[t, q], &[])?;
	builder.gate(
		Gate::X,
		&[q],
		&[
			Control::new(t, ControlState::One),
			Control::new(c, ControlState::One),
		],
	)?;
	builder.gate(
		Gate::U {
			theta: Angle::pi(1, 3)?,
			phi: Angle::pi(1, 5)?,
			lambda: Angle::pi(1, 7)?,
		},
		&[q],
		&[],
	)?;
	let a = trace(&macro_program.verify()?.lower()?.plan()?)?;
	let b = trace(
		&Program::<Constructed>::from_region(builder.finish()?, &[])?
			.verify()?
			.lower()?
			.plan()?,
	)?;
	expect_eq!(a.len(), b.len());
	for (a, b) in a.iter().zip(&b) {
		expect_eq!(&a.targets, &b.targets);
		let mut ac = a.controls.clone();
		let mut bc = b.controls.clone();
		ac.sort_unstable();
		bc.sort_unstable();
		expect_eq!(ac, bc);
		let a = a.matrix.as_ref().ok_or(Error::NotUnitary)?;
		let b = b.matrix.as_ref().ok_or(Error::NotUnitary)?;
		expect_lt!(a.unitarity_residual(MatrixPolicy::default())?, 1e-14);
		for i in 0..a.dimension() {
			for j in 0..a.dimension() {
				expect_lt!((a.view()[(i, j)] - b.view()[(i, j)]).norm(), 1e-14);
			}
		}
	}
	Ok(())
}

#[gtest]
fn macro_operations_retain_original_file_and_keyword_byte_ranges() -> Result<()> {
	let mut visits = vec![];
	let program = circuit! {
		qubit q;
		bit c;
		rx(${ { visits.push(1); 0.125 } }) q;
		gphase(${ { visits.push(2); 0.25 } });
		c = measure q;
		reset q;
		barrier;
		measure q -> c;
	}?;
	expect_eq!(visits, &[1, 2]);
	let plan = program.verify()?.lower()?.plan()?;
	let operations = plan
		.ssa()
		.blocks()
		.iter()
		.flat_map(|block| &block.instructions)
		.filter(|item| {
			matches!(
				item.kind,
				language::ssa::InstructionKind::Gate { .. }
					| language::ssa::InstructionKind::Measure { .. }
					| language::ssa::InstructionKind::Reset { .. }
					| language::ssa::InstructionKind::Barrier { .. }
			)
		})
		.collect::<Vec<_>>();
	expect_eq!(operations.len(), 6);
	let mut previous = 0;
	for instruction in operations {
		let span = instruction.span.ok_or(Error::SourceRange)?;
		expect_ge!(span.range().start, previous);
		previous = span.range().start;
		expect_true!(
			plan.locations()
				.iter()
				.any(|location| location.span.source() == span.source()
					&& location.file.ends_with("macro_contract.rs"))
		);
	}
	Ok(())
}

struct Trace {
	matrix: Option<NumericalOperator>,
	targets: Vec<usize>,
	controls: Vec<(usize, bool)>,
}
#[derive(Default)]
struct Backend(Vec<Trace>);
impl language::vm::QuantumBackend for Backend {
	type Error = Error;
	fn apply_gate(&mut self, r: language::vm::GateRequest<'_>) -> std::result::Result<(), Error> {
		let matrix = if r.gate == language::GateKind::GlobalPhase {
			None
		} else {
			let matrix =
				BoundGate::from_kind(r.gate, r.parameters)?.matrix(MatrixPolicy::default())?;
			Some(if r.inverse {
				matrix.conjugate_transpose(MatrixPolicy::default())?
			} else {
				matrix
			})
		};
		self.0.push(Trace {
			matrix,
			targets: r.targets.to_vec(),
			controls: r.controls.iter().map(|c| (c.qubit, c.positive)).collect(),
		});
		Ok(())
	}
	fn measure(&mut self, q: usize) -> std::result::Result<bool, Error> {
		self.0.push(Trace {
			matrix: None,
			targets: vec![q],
			controls: vec![],
		});
		Ok(false)
	}
	fn reset(&mut self, q: usize) -> std::result::Result<(), Error> {
		self.0.push(Trace {
			matrix: None,
			targets: vec![q],
			controls: vec![],
		});
		Ok(())
	}
	fn barrier(&mut self, q: &[usize]) -> std::result::Result<(), Error> {
		self.0.push(Trace {
			matrix: None,
			targets: q.to_vec(),
			controls: vec![],
		});
		Ok(())
	}
}
fn trace(program: &Program<Executable>) -> Result<Vec<Trace>> {
	let mut backend = Backend::default();
	language::vm::Interpreter::default().run_prepared(
		program.ssa(),
		program.dispatch(),
		&mut backend,
		&language::vm::RunInputs::default(),
		program.captures(),
	)?;
	Ok(backend.0)
}
