//! Constructed portable costs only: no Hilbert-space allocation or gate stream retention.
#![allow(
	dead_code,
	reason = "Shared example/test receipt fields and reporting entry points"
)]
use quest_qsvt::{
	Complex64, EncodingDescriptor, Error, MatchingEncoding, NumericalPolicy, ReplayEncoding,
	ReplayGate, ReplayKind, Result, ShiftRegister, TensorShiftEncoding,
	portfolio::{
		ArithmeticStencil, ArithmeticStencilTerm, Boundary, PerMatchingBounds, PortfolioLimits,
		PortfolioResources, SparseAccessEncoding, StructuredScheme, WeightedLcu,
	},
};
use std::time::Instant;

#[derive(Clone, Copy, Debug)]
pub struct Limits {
	/// One allowance shared by forward and adjoint for each constructed source.
	pub count_work: usize,
	pub gates_per_orientation: usize,
	pub construction: PortfolioLimits,
}
impl Default for Limits {
	fn default() -> Self {
		Self {
			count_work: 100_000,
			gates_per_orientation: 50_000,
			construction: PortfolioLimits::default(),
		}
	}
}
#[derive(Clone, Copy, Debug, Default)]
pub struct Inventory {
	pub gates: usize,
	/// X, H, Ry, scalar Phase (portable primitives, not T decompositions).
	pub primitives: [usize; 4],
	pub controlled_gates: usize,
	pub control_occurrences: usize,
}
#[derive(Debug)]
pub struct Row {
	pub scheme: &'static str,
	pub dimension: usize,
	pub descriptor: EncodingDescriptor,
	pub retained_bytes: usize,
	pub peak_bytes: usize,
	pub portfolio: Option<PortfolioResources>,
	pub preparation_gates: usize,
	pub forward: Inventory,
	pub adjoint: Inventory,
	pub count_work: usize,
	/// Includes the one rejecting callback, if a visitor budget is exhausted.
	pub observed_emissions: usize,
	pub count_error: Option<String>,
	pub construction_seconds: f64,
	pub count_seconds: f64,
}
#[derive(Debug)]
pub struct SourceCost {
	pub dimension: usize,
	pub nonzeros: usize,
	pub retained_bytes: usize,
	pub construction_seconds: f64,
}
#[derive(Debug)]
pub struct Campaign {
	pub sources: Vec<SourceCost>,
	pub rows: Vec<Row>,
}
fn add(a: usize, b: usize) -> Result<usize> {
	a.checked_add(b)
		.ok_or(Error::Budget("resource-curve count overflow"))
}
fn record(inventory: &mut Inventory, gate: ReplayGate) -> Result<()> {
	inventory.gates = add(inventory.gates, 1)?;
	let index = match gate.kind {
		ReplayKind::X => 0,
		ReplayKind::H => 1,
		ReplayKind::Ry(_) => 2,
		ReplayKind::Phase(_) => 3,
	};
	let primitive = inventory
		.primitives
		.get_mut(index)
		.ok_or(Error::Encoding("primitive family"))?;
	*primitive = add(*primitive, 1)?;
	inventory.controlled_gates = add(
		inventory.controlled_gates,
		usize::from(gate.control_mask != 0),
	)?;
	inventory.control_occurrences = add(
		inventory.control_occurrences,
		usize::try_from(gate.control_mask.count_ones())
			.map_err(|_| Error::Budget("control count"))?,
	)?;
	Ok(())
}
#[allow(
	clippy::too_many_arguments,
	reason = "Each caller explicitly supplies source-local accounting and shared count limits"
)]
fn measure<E: ReplayEncoding>(
	scheme: &'static str,
	source: &E,
	dimension: usize,
	construction_seconds: f64,
	portfolio: Option<PortfolioResources>,
	preparation_gates: usize,
	peak_bytes: usize,
	limits: Limits,
) -> Result<Row> {
	let descriptor = source.descriptor()?;
	descriptor.validate()?;
	let mut row = Row {
		scheme,
		dimension,
		descriptor,
		retained_bytes: source.retained_bytes()?,
		peak_bytes,
		portfolio,
		preparation_gates,
		forward: Inventory::default(),
		adjoint: Inventory::default(),
		count_work: 0,
		observed_emissions: 0,
		count_error: None,
		construction_seconds,
		count_seconds: 0.,
	};
	let started = Instant::now();
	for adjoint in [false, true] {
		if row.count_work >= limits.count_work || limits.gates_per_orientation == 0 {
			row.count_error = Some("resource-curve count allowance exhausted before replay".into());
			break;
		}
		let orientation_limit = limits.gates_per_orientation;
		let inventory = if adjoint {
			&mut row.adjoint
		} else {
			&mut row.forward
		};
		let outcome = source.visit_replay(adjoint, &mut |gate| {
			row.observed_emissions = add(row.observed_emissions, 1)?;
			if row.count_work >= limits.count_work || inventory.gates >= orientation_limit {
				return Err(Error::Budget("resource-curve streamed count"));
			}
			row.count_work = add(row.count_work, 1)?;
			record(inventory, gate)
		});
		if let Err(error) = outcome {
			row.count_error = Some(error.to_string());
			break;
		}
	}
	row.count_seconds = started.elapsed().as_secs_f64();
	Ok(row)
}
fn shift(width: usize, offset: usize, policy: NumericalPolicy) -> Result<TensorShiftEncoding> {
	TensorShiftEncoding::new(width, vec![ShiftRegister::new(0, width, offset)?], policy)
}
/// Construct the six sources independently at each size; retain only scalar receipts.
#[allow(
	clippy::too_many_lines,
	reason = "Six independent source constructions remain beside their scope-specific inventories"
)]
pub fn run(limits: Limits) -> Result<Campaign> {
	let mut campaign = Campaign {
		sources: Vec::with_capacity(4),
		rows: Vec::with_capacity(24),
	};
	let policy = NumericalPolicy {
		max_bytes: limits.construction.max_bytes,
	};
	for dimension in [4_usize, 8, 16, 32] {
		let started = Instant::now();
		let mut entries = Vec::with_capacity(
			dimension
				.checked_mul(2)
				.ok_or(Error::Budget("source entries"))?,
		);
		for j in 0..dimension {
			entries.push((j, j, Complex64::new(0.5, 0.)));
			entries.push((j ^ (dimension / 2), j, Complex64::new(0., 0.125)));
		}
		let matrix = quest_numerics::SparseMatrix::from_triplets(
			dimension,
			dimension,
			quest_numerics::SparseFormat::Csr,
			entries,
			quest_numerics::SparseLimits {
				max_dimension: 32,
				max_entries: 64,
				max_bytes: limits.construction.max_bytes,
				max_work: limits.construction.max_compile_work,
			},
		)?;
		campaign.sources.push(SourceCost {
			dimension,
			nonzeros: matrix.nnz(),
			retained_bytes: matrix.retained_bytes()?,
			construction_seconds: started.elapsed().as_secs_f64(),
		});
		let width =
			usize::try_from(dimension.ilog2()).map_err(|_| Error::Budget("curve index width"))?;
		let started = Instant::now();
		let base = MatchingEncoding::from_sparse(&matrix, policy)?;
		let base_seconds = started.elapsed().as_secs_f64();
		let base_cost = base.resources();
		let prep = usize::try_from(base.num_colors().ilog2())
			.map_err(|_| Error::Budget("color width"))?
			.checked_mul(2)
			.ok_or(Error::Budget("matching preparation"))?;
		campaign.rows.push(measure(
			"uniform-matching",
			&base,
			dimension,
			base_seconds,
			None,
			prep,
			base_cost.construction_peak_bytes,
			limits,
		)?);
		let started = Instant::now();
		let per_matching = PerMatchingBounds::new(&base, limits.construction)?;
		let elapsed = base_seconds + started.elapsed().as_secs_f64();
		let costs = per_matching.resources();
		// This constructor's envelope already includes its retained prerequisite base.
		let peak = base_cost
			.construction_peak_bytes
			.max(costs.construction_peak_bytes);
		campaign.rows.push(measure(
			"per-matching",
			&per_matching,
			dimension,
			elapsed,
			Some(costs),
			costs.preparation_gates,
			peak,
			limits,
		)?);
		drop(per_matching);
		drop(base);
		for (scheme, name) in [
			(StructuredScheme::Base, "scc-base"),
			(StructuredScheme::Prep, "scc-prep"),
		] {
			let started = Instant::now();
			let source = ArithmeticStencil::new(
				vec![width],
				vec![
					ArithmeticStencilTerm {
						weight: 0.5.into(),
						offsets: vec![0],
					},
					ArithmeticStencilTerm {
						weight: Complex64::new(0., 0.125),
						offsets: vec![
							i64::try_from(dimension / 2)
								.map_err(|_| Error::Budget("arithmetic offset"))?,
						],
					},
				],
				Boundary::Periodic,
				scheme,
				limits.construction,
			)?;
			let elapsed = started.elapsed().as_secs_f64();
			let costs = source.resources();
			campaign.rows.push(measure(
				name,
				&source,
				dimension,
				elapsed,
				Some(costs),
				costs.preparation_gates,
				costs.construction_peak_bytes,
				limits,
			)?);
		}
		let started = Instant::now();
		let qrom = SparseAccessEncoding::new(&matrix, 2, limits.construction)?;
		let elapsed = started.elapsed().as_secs_f64();
		let costs = qrom.resources();
		campaign.rows.push(measure(
			"sparse-qrom",
			&qrom,
			dimension,
			elapsed,
			Some(costs),
			costs.preparation_gates,
			costs.construction_peak_bytes,
			limits,
		)?);
		drop(qrom);
		let started = Instant::now();
		let lcu = WeightedLcu::new(
			vec![
				(0.5.into(), shift(width, 0, policy)?),
				(
					Complex64::new(0., 0.125),
					shift(width, dimension / 2, policy)?,
				),
			],
			limits.construction,
		)?;
		let elapsed = started.elapsed().as_secs_f64();
		let costs = lcu.resources();
		campaign.rows.push(measure(
			"weighted-lcu",
			&lcu,
			dimension,
			elapsed,
			Some(costs),
			costs.preparation_gates,
			costs.construction_peak_bytes,
			limits,
		)?);
	}
	Ok(campaign)
}
