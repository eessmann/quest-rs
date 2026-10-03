use crate::{
	Complex64, Error, LogicalSpace, NumericalPolicy, OperandLayout, OracleFragment,
	ProjectedEncoding, Projection, QueryCounts, Result, Route, StandardConvention,
	TransformContinuation, TransformEvidence, ValidatedTransform, materialize_oracle, matrix,
};
use faer::MatRef;
use quest_compile::{
	Angle, BoundRegion, Control, ControlState, Gate, NumericalOperator, ProgramLimits,
	QuantumRegionBuilder, QubitId,
};
use quest_qsp::{ControlSequence, PhaseSequence};
use std::ops::{Add, Mul, Sub};

struct Circuit {
	builder: QuantumRegionBuilder,
	policy: NumericalPolicy,
	retained_bytes: usize,
	fragments: Vec<OracleFragment>,
}
impl Circuit {
	fn new(qubits: usize, policy: NumericalPolicy) -> Result<Self> {
		let limits = ProgramLimits {
			max_matrix_bytes: policy.max_bytes,
			max_operations: policy.max_bytes / 128,
			..ProgramLimits::default()
		};
		Ok(Self {
			builder: QuantumRegionBuilder::with_limits(qubits, 0, limits)?,
			policy,
			retained_bytes: 0,
			fragments: Vec::new(),
		})
	}
	fn reserve(&mut self, targets: usize, controls: usize, payload: usize) -> Result<()> {
		let bytes = targets
			.checked_mul(size_of::<QubitId>())
			.and_then(|n| n.checked_add(controls.checked_mul(size_of::<Control>())?))
			.and_then(|n| n.checked_add(size_of::<quest_compile::Operation>()))
			.and_then(|n| n.checked_add(payload))
			.ok_or(Error::Budget("transform operation storage"))?;
		self.retained_bytes = self
			.retained_bytes
			.checked_add(bytes)
			.ok_or(Error::Budget("transform retained storage"))?;
		let shared = OracleFragment::shared_storage_bytes(&self.fragments)?;
		if self
			.retained_bytes
			.checked_add(shared)
			.is_none_or(|n| n > self.policy.max_bytes)
		{
			return Err(Error::Budget("transform retained storage"));
		}
		Ok(())
	}
	fn phase(&mut self, radians: f64, controls: &[(usize, bool)]) -> Result<()> {
		self.reserve(0, controls.len(), 0)?;
		self.builder
			.global_phase(Angle::radians(radians)?, &self.controls(controls)?)?;
		Ok(())
	}
	fn targets(&self, targets: &[usize]) -> Result<Vec<QubitId>> {
		targets
			.iter()
			.map(|&index| self.builder.qubit(index).map_err(Error::from))
			.collect()
	}
	fn controls(&self, controls: &[(usize, bool)]) -> Result<Vec<Control>> {
		controls
			.iter()
			.map(|&(index, value)| {
				Ok(Control::new(
					self.builder.qubit(index)?,
					if value {
						ControlState::One
					} else {
						ControlState::Zero
					},
				))
			})
			.collect()
	}
	fn gate(&mut self, gate: Gate, target: usize, controls: &[(usize, bool)]) -> Result<()> {
		self.reserve(1, controls.len(), 0)?;
		self.builder.gate(
			gate,
			&[self.builder.qubit(target)?],
			&self.controls(controls)?,
		)?;
		Ok(())
	}
	fn numerical(
		&mut self,
		matrix: MatRef<'_, Complex64>,
		targets: &[usize],
		controls: &[(usize, bool)],
	) -> Result<()> {
		let bytes = self.policy.check(matrix.nrows(), matrix.ncols(), 1)?;
		self.reserve(targets.len(), controls.len(), bytes)?;
		self.builder.numerical(
			NumericalOperator::from_view(matrix, self.policy.matrix_policy())?,
			&self.targets(targets)?,
			&self.controls(controls)?,
		)?;
		Ok(())
	}
	fn oracle(
		&mut self,
		oracle: &OracleFragment,
		targets: &[usize],
		controls: &[(usize, bool)],
	) -> Result<()> {
		if !self
			.fragments
			.iter()
			.any(|existing| existing.shares_storage_with(oracle))
		{
			self.fragments.push(oracle.clone());
		}
		self.reserve(targets.len(), controls.len(), 0)?;
		self.builder
			.oracle(oracle, &self.targets(targets)?, &self.controls(controls)?)?;
		Ok(())
	}
	fn projector<Side>(
		&mut self,
		space: &LogicalSpace<Side>,
		targets: &[usize],
		controls: &[(usize, bool)],
		phase: Option<f64>,
	) -> Result<()> {
		if let crate::ProjectorKind::Coordinates(indices) = space.kind() {
			let (base, selected) = phase
				.map_or((std::f64::consts::PI, std::f64::consts::PI), |angle| {
					(-angle, 2.0 * angle)
				});
			self.phase(base, controls)?;
			for &coordinate in indices.iter() {
				let mut selected_controls = controls.to_vec();
				for (local, &target) in targets.iter().enumerate() {
					let mask = 1usize
						.checked_shl(
							u32::try_from(local).map_err(|_| Error::Budget("projector bit"))?,
						)
						.ok_or(Error::Budget("projector bit"))?;
					selected_controls.push((target, coordinate & mask != 0));
				}
				self.phase(selected, &selected_controls)?;
			}
			return Ok(());
		}
		let projector = space.projector_matrix(self.policy)?;
		let (positive, negative) = phase.map_or(
			(Complex64::new(1.0, 0.0), Complex64::new(-1.0, 0.0)),
			|angle| {
				let positive = Complex64::from_polar(1.0, angle);
				(positive, positive.conj())
			},
		);
		let operator = matrix::allocate(
			projector.nrows(),
			projector.ncols(),
			self.policy,
			|row, col| {
				positive
					.sub(negative)
					.mul(projector[(row, col)])
					.add(if row == col {
						negative
					} else {
						Complex64::new(0.0, 0.0)
					})
			},
		)?;
		self.numerical(operator.as_ref(), targets, controls)
	}
	fn control_matrix(&mut self, control: &quest_qsp::Control, response: usize) -> Result<()> {
		let matrix = matrix::allocate(2, 2, self.policy, |row, col| {
			control
				.get(row)
				.and_then(|r| r.get(col))
				.copied()
				.unwrap_or_default()
		})?;
		self.numerical(matrix.as_ref(), &[response], &[])
	}
	fn finish(self) -> Result<BoundRegion> {
		Ok(self.builder.finish()?.bind(&[])?)
	}
}

pub fn standard<C: StandardConvention>(
	encoding: ProjectedEncoding,
	sequence: &PhaseSequence<C>,
	layout: OperandLayout,
) -> Result<ValidatedTransform> {
	let converted = C::projector_phases(sequence);
	let degree = sequence.degree();
	let reduced = if degree == 0 {
		0
	} else {
		degree.saturating_sub(1) % 4
	};
	let readout = -f64::from(u32::try_from(reduced).map_err(|_| Error::Budget("readout angle"))?)
		* std::f64::consts::PI;
	standard_projector(
		encoding,
		converted.values(),
		readout,
		C::TAG,
		converted.roundoff_estimate(),
		layout,
	)
}

pub fn standard_projector(
	encoding: ProjectedEncoding,
	phases: &[f64],
	readout: f64,
	convention: &'static str,
	conversion_roundoff: f64,
	layout: OperandLayout,
) -> Result<ValidatedTransform> {
	if phases.iter().any(|value| !value.is_finite()) || !readout.is_finite() {
		return Err(Error::NonFinite);
	}
	let degree = phases
		.len()
		.checked_sub(1)
		.ok_or(Error::Encoding("empty projector sequence"))?;
	let mut circuit = Circuit::new(layout.num_qubits(), encoding.policy())?;
	circuit.gate(Gate::H, layout.response(), &[])?;
	// Open positive branch equals the C++ X / closed-control / X construction.
	for (conjugate, value) in [(false, false), (true, true)] {
		let controls = [(layout.response(), value)];
		for round in 0..degree / 2 {
			let k = degree
				.checked_sub(round.checked_mul(2).ok_or(Error::Budget("phase index"))?)
				.ok_or(Error::Budget("phase index"))?;
			circuit.projector(
				encoding.right(),
				layout.source(),
				&controls,
				Some(phase(phases, k, conjugate)?),
			)?;
			circuit.oracle(encoding.oracle(), layout.source(), &controls)?;
			circuit.projector(
				encoding.left(),
				layout.source(),
				&controls,
				Some(phase(phases, k.saturating_sub(1), conjugate)?),
			)?;
			circuit.oracle(&encoding.oracle().adjoint(), layout.source(), &controls)?;
		}
		if degree.is_multiple_of(2) {
			circuit.projector(
				encoding.right(),
				layout.source(),
				&controls,
				Some(phase(phases, 0, conjugate)?),
			)?;
		} else {
			circuit.projector(
				encoding.right(),
				layout.source(),
				&controls,
				Some(phase(phases, 1, conjugate)?),
			)?;
			circuit.oracle(encoding.oracle(), layout.source(), &controls)?;
			circuit.projector(
				encoding.left(),
				layout.source(),
				&controls,
				Some(phase(phases, 0, conjugate)?),
			)?;
		}
	}
	circuit.gate(Gate::Rz(Angle::radians(readout)?), layout.response(), &[])?;
	circuit.gate(Gate::H, layout.response(), &[])?;
	let main = circuit.finish()?;
	let forward = degree
		.checked_add(degree % 2)
		.ok_or(Error::Budget("source queries"))?;
	let adjoint = degree
		.checked_sub(degree % 2)
		.ok_or(Error::Budget("source queries"))?;
	let queries = QueryCounts {
		semantic: degree,
		source_forward: forward,
		source_adjoint: adjoint,
		retained_oracle_calls: retained_counts(&main)?,
	};
	let evidence = TransformEvidence {
		phase_conversion_roundoff_estimate: conversion_roundoff,
		..TransformEvidence::default()
	};
	Ok(ValidatedTransform {
		input: Projection::source(&encoding, &layout, false, None),
		output: Projection::source(&encoding, &layout, !degree.is_multiple_of(2), None),
		encoding,
		route: Route::Standard,
		meaning: None,
		convention,
		degree,
		layout,
		main,
		continuation_stage: TransformContinuation::Direct,
		queries,
		evidence,
		#[cfg(feature = "certification")]
		projector_certificate: None,
	})
}
fn phase(phases: &[f64], index: usize, conjugate: bool) -> Result<f64> {
	let value = *phases.get(index).ok_or(Error::Encoding("phase index"))?;
	Ok(if conjugate { -value } else { value })
}

pub fn generalized(
	encoding: ProjectedEncoding,
	sequence: &ControlSequence,
	route: Route,
	layout: OperandLayout,
) -> Result<ValidatedTransform> {
	let degree = sequence.degree();
	let evidence = if route == Route::DirectHermitian {
		direct_evidence(&encoding)?
	} else {
		TransformEvidence::default()
	};
	let mut circuit = Circuit::new(layout.num_qubits(), encoding.policy())?;
	let (final_control, controls) = sequence
		.matrices()
		.split_last()
		.ok_or(Error::Encoding("empty control sequence"))?;
	circuit.control_matrix(final_control, layout.response())?;
	let barred = if matches!(
		route,
		Route::HermitianizedFull | Route::HermitianizedEven | Route::HermitianizedOdd
	) {
		Some(barred_oracle(&encoding)?)
	} else {
		None
	};
	for control in controls.iter().rev() {
		generalized_walk(&mut circuit, &encoding, route, &layout, barred.as_ref())?;
		circuit.control_matrix(control, layout.response())?;
	}
	let main = circuit.finish()?;
	let (input, output, continuation_stage) = stages(&encoding, &layout, route)?;
	let mut queries = QueryCounts {
		semantic: degree,
		source_forward: degree,
		source_adjoint: if route == Route::DirectHermitian {
			0
		} else {
			degree
		},
		retained_oracle_calls: retained_counts(&main)?,
	};
	if let TransformContinuation::Projected {
		program: continuation,
		..
	} = &continuation_stage
	{
		queries.source_forward = queries
			.source_forward
			.checked_add(1)
			.ok_or(Error::Budget("source queries"))?;
		queries.retained_oracle_calls = queries
			.retained_oracle_calls
			.checked_add(retained_counts(continuation)?)
			.ok_or(Error::Budget("oracle queries"))?;
	}
	Ok(ValidatedTransform {
		encoding,
		route,
		meaning: None,
		convention: "ni-generalized-upper-left-final-k",
		degree,
		layout,
		main,
		input,
		output,
		continuation_stage,
		queries,
		evidence,
		#[cfg(feature = "certification")]
		projector_certificate: None,
	})
}
fn barred_oracle(encoding: &ProjectedEncoding) -> Result<OracleFragment> {
	let width = encoding
		.num_qubits()
		.checked_add(1)
		.ok_or(Error::Budget("barred oracle width"))?;
	let mut body = Circuit::new(width, encoding.policy())?;
	let targets = (1..width).collect::<Vec<_>>();
	body.gate(Gate::X, 0, &[])?;
	body.oracle(encoding.oracle(), &targets, &[(0, false)])?;
	body.oracle(&encoding.oracle().adjoint(), &targets, &[(0, true)])?;
	Ok(OracleFragment::builder(body.finish()?)
		.matrix_tolerance(1e-12)?
		.matrix_policy(encoding.policy().matrix_policy())
		.build()?)
}
type Stages = (Projection, Projection, TransformContinuation);
fn stages(encoding: &ProjectedEncoding, layout: &OperandLayout, route: Route) -> Result<Stages> {
	match route {
		Route::DirectHermitian => {
			let input = Projection::source(encoding, layout, false, None);
			Ok((input.clone(), input, TransformContinuation::Direct))
		}
		Route::HermitianizedFull => {
			let input = Projection::joint(encoding, layout)?;
			Ok((input.clone(), input, TransformContinuation::Direct))
		}
		Route::HermitianizedEven | Route::HermitianizedOdd => Ok((
			Projection::source(encoding, layout, false, Some(true)),
			Projection::source(
				encoding,
				layout,
				route == Route::HermitianizedOdd,
				Some(route == Route::HermitianizedEven),
			),
			TransformContinuation::Direct,
		)),
		Route::MultiplicationEven | Route::MultiplicationOdd => {
			let input = Projection::source(encoding, layout, false, Some(false));
			if route == Route::MultiplicationEven {
				return Ok((input.clone(), input, TransformContinuation::Direct));
			}
			let mut continuation = Circuit::new(layout.num_qubits(), encoding.policy())?;
			continuation.oracle(encoding.oracle(), layout.source(), &[])?;
			Ok((
				input.clone(),
				Projection::source(encoding, layout, true, Some(false)),
				TransformContinuation::Projected {
					bridge: Box::new(input),
					program: continuation.finish()?,
				},
			))
		}
		Route::Standard => Err(Error::Encoding("standard stages require degree")),
	}
}
fn direct_evidence(encoding: &ProjectedEncoding) -> Result<TransformEvidence> {
	let oracle = materialize_oracle(encoding.oracle(), encoding.policy())?;
	let adjoint = matrix::snapshot(oracle.adjoint(), encoding.policy())?;
	let hermiticity = matrix::difference(oracle.as_ref(), adjoint.as_ref());
	admit("full oracle Hermiticity", hermiticity)?;
	let left = encoding.left().projector_matrix(encoding.policy())?;
	let right = encoding.right().projector_matrix(encoding.policy())?;
	let agreement = matrix::difference(left.as_ref(), right.as_ref());
	admit("direct projector agreement", agreement)?;
	// A single logical interface is required, including its ordered basis.
	if encoding.left().logical_dimension() != encoding.right().logical_dimension() {
		return Err(Error::Encoding("direct logical dimensions differ"));
	}
	let left_basis = encoding.left().isometry_snapshot(encoding.policy())?;
	let right_basis = encoding.right().isometry_snapshot(encoding.policy())?;
	let basis = matrix::difference(left_basis.as_ref(), right_basis.as_ref());
	admit("direct logical basis agreement", basis)?;
	Ok(TransformEvidence {
		whole_oracle_hermiticity_residual: Some(hermiticity),
		projector_agreement_residual: Some(agreement),
		..TransformEvidence::default()
	})
}
fn admit(operation: &'static str, residual: f64) -> Result<()> {
	if !residual.is_finite() || residual > 1e-12 {
		Err(Error::Residual {
			operation,
			residual,
			tolerance: 1e-12,
		})
	} else {
		Ok(())
	}
}
fn retained_counts(program: &BoundRegion) -> Result<usize> {
	program
		.instructions()
		.iter()
		.try_fold(0usize, |count, instruction| {
			let calls = match instruction.operation() {
				quest_compile::Operation::Oracle { fragment, .. } => fragment.query_count(),
				_ => 0,
			};
			count
				.checked_add(calls)
				.ok_or(Error::Budget("oracle query count"))
		})
}

fn generalized_walk(
	circuit: &mut Circuit,
	encoding: &ProjectedEncoding,
	route: Route,
	layout: &OperandLayout,
	barred: Option<&OracleFragment>,
) -> Result<()> {
	let response = [(layout.response(), false)];
	match route {
		Route::DirectHermitian => {
			circuit.oracle(encoding.oracle(), layout.source(), &response)?;
			circuit.projector(encoding.right(), layout.source(), &response, None)?;
		}
		Route::HermitianizedFull | Route::HermitianizedEven | Route::HermitianizedOdd => {
			let auxiliary = layout
				.auxiliary()
				.ok_or(Error::Encoding("missing hermitianization qubit"))?;
			let mut targets = vec![auxiliary];
			targets.extend_from_slice(layout.source());
			circuit.oracle(
				barred
					.as_ref()
					.ok_or(Error::Encoding("missing barred oracle"))?,
				&targets,
				&response,
			)?;
			circuit.projector(
				encoding.left(),
				layout.source(),
				&[(layout.response(), false), (auxiliary, false)],
				None,
			)?;
			circuit.projector(
				encoding.right(),
				layout.source(),
				&[(layout.response(), false), (auxiliary, true)],
				None,
			)?;
		}
		Route::MultiplicationEven | Route::MultiplicationOdd => {
			let auxiliary = layout
				.auxiliary()
				.ok_or(Error::Encoding("missing multiplication qubit"))?;
			circuit.oracle(encoding.oracle(), layout.source(), &response)?;
			circuit.gate(Gate::H, auxiliary, &response)?;
			circuit.projector(
				encoding.left(),
				layout.source(),
				&[(layout.response(), false), (auxiliary, true)],
				None,
			)?;
			circuit.gate(Gate::H, auxiliary, &response)?;
			circuit.oracle(&encoding.oracle().adjoint(), layout.source(), &response)?;
			circuit.gate(Gate::Z, auxiliary, &response)?;
			circuit.projector(
				encoding.right(),
				layout.source(),
				&[(layout.response(), false), (auxiliary, false)],
				None,
			)?;
		}
		Route::Standard => {
			return Err(Error::Encoding(
				"generalized controls cannot select standard route",
			));
		}
	}
	Ok(())
}
