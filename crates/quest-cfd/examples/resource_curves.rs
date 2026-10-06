//! Constructed five-coordinate CFD source costs; no state or dense unitary execution.
use quest_cfd::{
	CfdError, PeriodicBdm1,
	carleman::{CarlemanLimits, SymmetricCarleman},
	configuration::ConfigurationGrid,
	constructed_resources::{
		ConstructedResourceLimits, InverseResourceRequest, ObservationResourceRequest,
		constructed_history_resources,
	},
	history::HistorySystem,
	kvn_recipe::KvnRecipeLimits,
	polynomial::PolynomialOde,
	probability_observation::{DiagonalRange, KvnKineticEnergy, SamplingRequest},
};
use quest_numerics::{Complex64, SparseLimits};
use serde_json::{Value, json};
use std::{sync::Arc, time::Instant};
const fn sampling(range: DiagonalRange) -> SamplingRequest<'static> {
	SamplingRequest {
		range,
		absolute_error: 0.05,
		failure_probability: 0.05,
		joint_success_lower_bound: 0.001,
		success_bound_provenance: "illustrative external joint inverse/temporal/degree success hypothesis; not derived from simulator or certified for this CFD source",
		max_selected_shots: 1_000_000,
		max_attempted_shots: 1_000_000_000,
		max_provenance_bytes: 4096,
		systematic_bias_bound: None,
	}
}
fn kvn(
	order: usize,
	cells: usize,
	time_order: usize,
	limits: ConstructedResourceLimits,
	inverse: bool,
) -> Result<Value, CfdError> {
	let started = Instant::now();
	let model = PeriodicBdm1::assemble(0.01)?;
	let physical = model.dimension();
	let grid = ConfigurationGrid::uniform(physical, -0.3, 0.3, 1, order, 4096)?;
	let generator = grid.generator(&model, SparseLimits::default())?;
	let generator_nonzeros = generator.nnz();
	let initial = grid.initial_bump(&[0.04, -0.03, 0.02, 0.05, -0.01], 0.55)?;
	let history = HistorySystem::assemble(
		&generator,
		&initial,
		0.001,
		cells,
		time_order,
		SparseLimits::default(),
	)?;
	let energy = KvnKineticEnergy::periodic_bdm1(&grid, &model, KvnRecipeLimits::default())?;
	drop(initial);
	drop(generator);
	drop(model);
	let build_seconds = started.elapsed().as_secs_f64();
	let first_final = history
		.operator()
		.rows()
		.checked_sub(grid.dimension())
		.ok_or(CfdError::InvalidInput("final node block"))?;
	let query = |index: usize| {
		Ok((
			index >= first_final,
			energy.value(
				index
					.checked_rem(grid.dimension())
					.ok_or(CfdError::InvalidInput("positive configuration dimension"))?,
			)?,
		))
	};
	let observation=inverse.then(||ObservationResourceRequest {sampling:sampling(energy.range()),description:"full mass-orthonormal KvN kinetic energy conditional on inverse/last stored temporal node; ensemble-to-trajectory bias unestablished",retained_bytes:energy.retained_bytes(),scratch_bytes:0,selection_query_work:1,observable_query_work:u64::try_from(energy.query_work()).unwrap_or(u64::MAX),max_validation_work:1_000_000_000,query:&query});
	let result = constructed_history_resources(
		&history,
		ConstructedResourceLimits {
			// The borrowed grid and energy owner are also live in count-only runs.
			// Sampling requests declare this same payload themselves when enabled.
			external_retained_bytes: if inverse { 0 } else { energy.retained_bytes() },
			..limits
		},
		inverse.then_some(InverseResourceRequest::default()),
		observation,
	)?;
	Ok(
		json!({"lift":"kvn","all_physical_coordinates":physical,"configuration_order":order,"generator_nonzeros":generator_nonzeros,"exact_zero_generator":generator_nonzeros==0,"configuration_nodes":grid.dimension(),"time_cells":cells,"time_order":time_order,"horizon":0.001,"physical_order":1,"configuration_boundary":"periodic central; bump touches boundary; not boundary-resolved", "physical_response_accuracy_established":false,"history_build_seconds":build_seconds,"history_build_peak_bytes":null,"resources":result}),
	)
}
fn carleman(
	cells: usize,
	time_order: usize,
	limits: ConstructedResourceLimits,
	inverse: bool,
) -> Result<Value, CfdError> {
	let started = Instant::now();
	let model = PeriodicBdm1::assemble(0.01)?;
	let physical = model.dimension();
	let ode = Arc::new(
		PolynomialOde::from_periodic_bdm1(
			&model,
			mathcore::multivariate::PolynomialLimits::default(),
		)?
		.dynamics,
	);
	let hierarchy = SymmetricCarleman::new(ode, 2, 0.2, CarlemanLimits::default())?;
	let initial = hierarchy
		.lift(&[0.04, -0.03, 0.02, 0.05, -0.01])?
		.into_iter()
		.map(|x| Complex64::new(x, 0.))
		.collect::<Vec<_>>();
	let history = HistorySystem::assemble_dynamics(
		&hierarchy,
		&initial,
		0.001,
		cells,
		time_order,
		SparseLimits::default(),
	)?;
	let lifted = hierarchy.dimension();
	drop(initial);
	drop(hierarchy);
	drop(model);
	let build_seconds = started.elapsed().as_secs_f64();
	let first_final = history
		.operator()
		.rows()
		.checked_sub(lifted)
		.ok_or(CfdError::InvalidInput("final node block"))?;
	// The public symmetric ordering groups all degree-one monomials first.
	let query = |index: usize| {
		let local = index
			.checked_rem(lifted)
			.ok_or(CfdError::InvalidInput("positive lifted dimension"))?;
		Ok((
			index >= first_final && local < physical,
			f64::from(u8::from(local == 0)),
		))
	};
	let range = DiagonalRange::new(0., 1.)?;
	let observation=inverse.then(||ObservationResourceRequest {sampling:sampling(range),description:"normalized first degree-one monomial population conditional on inverse/degree-one/last temporal node; not amplitude reconstruction or KvN-energy equality",retained_bytes:0,scratch_bytes:0,selection_query_work:3,observable_query_work:1,max_validation_work:1_000_000_000,query:&query});
	let result = constructed_history_resources(
		&history,
		limits,
		inverse.then_some(InverseResourceRequest::default()),
		observation,
	)?;
	Ok(
		json!({"lift":"symmetric-carleman","all_physical_coordinates":physical,"fixed_lift_order":2,"physical_scale":0.2,"lift_dimension":lifted,"time_cells":cells,"time_order":time_order,"horizon":0.001,"physical_order":1,"truncation_accuracy_established":false,"history_build_seconds":build_seconds,"history_build_peak_bytes":null,"resources":result}),
	)
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
	let mut limits = ConstructedResourceLimits {
		max_matching_work: 10_000_000_000_000,
		max_count_work: 200_000_000,
		max_gates_per_orientation: 100_000_000,
		..ConstructedResourceLimits::default()
	};
	let mut inverse = false;
	let mut arguments = std::env::args().skip(1);
	while let Some(argument) = arguments.next() {
		match argument.as_str() {
			"--inverse-plan" => inverse = true,
			"--count-work" => {
				limits.max_count_work = arguments
					.next()
					.ok_or("count allowance required")?
					.parse()?;
			}
			_ => return Err("unknown resource curve argument".into()),
		}
	}
	let mut rows = Vec::new();
	for time_order in [1, 2] {
		for cells in [1, 2] {
			for order in [1, 2] {
				rows.push(kvn(order, cells, time_order, limits, inverse)?);
			}
			rows.push(carleman(cells, time_order, limits, inverse)?);
		}
	}
	println!(
		"{}",
		serde_json::to_string_pretty(
			&json!({"schema":"quest-cfd-constructed-resource-curves-v1","source":"complete PeriodicBdm1 two-triangle mass-orthonormal five-coordinate chart, viscosity0.01","same_complete_physical_model":true,"physical_order":1,"time_orders":[1,2],"carleman_order":2,"equal_accuracy_comparison":false,"quantum_execution":false,"native_dispatch_or_fault_tolerant_costs":null,"history_build_peak_scope":"unavailable; preceding bounded stored history baseline is timed separately", "sampling_premises":"illustrative caller hypotheses, not simulator-derived or certified","rows":rows})
		)?
	);
	Ok(())
}
