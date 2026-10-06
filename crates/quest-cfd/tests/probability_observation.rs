#![allow(
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	clippy::panic_in_result_fn,
	reason = "Independent finite-distribution and analytic sampling checks"
)]
use quest_cfd::{
	CfdError, PeriodicBdm1,
	configuration::ConfigurationGrid,
	kvn_recipe::KvnRecipeLimits,
	probability_observation::{
		DiagonalRange, KvnKineticEnergy, SamplingRequest, TemporalNodeSelection, TemporalNodeSide,
		plan_sampling,
	},
	stream_history::{HistoryRowDynamics, HistoryStreamLimits, TemporalHistoryRecipe},
};
use quest_numerics::Complex64;
struct Zero(usize);
impl HistoryRowDynamics for Zero {
	fn dimension(&self) -> usize {
		self.0
	}
	fn max_row_entries(&self) -> usize {
		0
	}
	fn retained_bytes(&self) -> Result<usize, CfdError> {
		Ok(0)
	}
	fn row_query_bytes(&self) -> usize {
		0
	}
	fn row_query_work(&self) -> usize {
		1
	}
	fn visit_row(
		&self,
		_: f64,
		_: usize,
		_: &mut dyn FnMut(usize, Complex64) -> Result<(), CfdError>,
	) -> Result<(), CfdError> {
		Ok(())
	}
	fn source_entry(&self, _: f64, _: usize) -> Result<Complex64, CfdError> {
		Ok(Complex64::new(0., 0.))
	}
}
#[test]
fn sampling_plan_reserves_both_conditional_mean_and_postselection_tails()
-> Result<(), Box<dyn std::error::Error>> {
	let range = DiagonalRange::new(-2., 3.)?;
	let request = SamplingRequest {
		range,
		absolute_error: 0.1,
		failure_probability: 0.01,
		joint_success_lower_bound: 0.125,
		success_bound_provenance: "external analytic joint projector bound",
		max_selected_shots: 1_000_000,
		max_attempted_shots: 10_000_000,
		max_provenance_bytes: 4096,
		systematic_bias_bound: None,
	};
	let plan = plan_sampling(request)?;
	assert!(f64::from(u32::try_from(plan.selected_shots)?) >= 25. / 0.02 * (400_f64).ln());
	assert!(
		0.125 * f64::from(u32::try_from(plan.attempted_shots)?)
			>= 2. * f64::from(u32::try_from(plan.selected_shots)?)
	);
	assert!(0.125 * f64::from(u32::try_from(plan.attempted_shots)?) >= 8. * (200_f64).ln());
	assert!(plan.total_error_bound.is_none());
	assert!(!plan.quantum_measurements_executed);
	assert!(
		plan_sampling(SamplingRequest {
			max_selected_shots: plan.selected_shots - 1,
			..request
		})
		.is_err()
	);
	assert!(
		plan_sampling(SamplingRequest {
			max_attempted_shots: plan.attempted_shots - 1,
			..request
		})
		.is_err()
	);
	for p in [0., -1., 1.01, f64::NAN] {
		assert!(
			plan_sampling(SamplingRequest {
				joint_success_lower_bound: p,
				..request
			})
			.is_err()
		);
	}
	assert!(
		plan_sampling(SamplingRequest {
			success_bound_provenance: " ",
			..request
		})
		.is_err()
	);
	assert!(
		plan_sampling(SamplingRequest {
			absolute_error: f64::MIN_POSITIVE,
			..request
		})
		.is_err()
	);
	assert!(DiagonalRange::new(2., 1.).is_err());
	assert!(DiagonalRange::new(0., f64::INFINITY).is_err());
	let constant = plan_sampling(SamplingRequest {
		range: DiagonalRange::new(3., 3.)?,
		systematic_bias_bound: Some(0.2),
		..request
	})?;
	assert_eq!(constant.selected_shots, 1);
	for absolute_error in [f64::MIN_POSITIVE, f64::from_bits(1)] {
		let tiny = plan_sampling(SamplingRequest {
			range: DiagonalRange::new(3., 3.)?,
			absolute_error,
			..request
		})?;
		assert_eq!(tiny.selected_shots, 1);
	}

	assert!(constant.total_error_bound.is_some_and(|x| x >= 0.3));
	Ok(())
}
#[test]
fn node_projection_is_one_sided_nodal_evaluation_not_a_modal_slice()
-> Result<(), Box<dyn std::error::Error>> {
	for order in [1, 2] {
		let history =
			TemporalHistoryRecipe::new(&Zero(3), 2., 2, order, HistoryStreamLimits::default())?;
		let left = TemporalNodeSelection::new(&history, 1, 0)?;
		let right = TemporalNodeSelection::new(&history, 0, order)?;
		assert_eq!(left.time(), 1.);
		assert_eq!(right.time(), 1.);
		assert_eq!(left.side(), TemporalNodeSide::SlabLeft);
		assert_eq!(right.side(), TemporalNodeSide::SlabRight);
		assert_ne!(left.slab(), right.slab());
		for i in 0..history.dimension() {
			assert_eq!(
				left.contains(i),
				(3 * (order + 1)..3 * (order + 2)).contains(&i)
			);
		}
		assert!((left.quadrature_weight() - if order == 1 { 0.5 } else { 1. / 6. }).abs() < 1e-15);
		assert!(TemporalNodeSelection::new(&history, 2, 0).is_err());
		assert!(TemporalNodeSelection::new(&history, 0, order + 1).is_err());
	}
	Ok(())
}
#[test]
fn kinetic_energy_uses_every_mass_orthonormal_coordinate() -> Result<(), Box<dyn std::error::Error>>
{
	let flow = PeriodicBdm1::assemble(0.01)?;
	let grid = ConfigurationGrid::uniform(5, -0.3, 0.4, 1, 2, 1024)?;
	let energy = KvnKineticEnergy::periodic_bdm1(&grid, &flow, KvnRecipeLimits::default())?;
	for i in 0..grid.dimension() {
		let point = grid.point(i).ok_or("point")?;
		let expected = flow.energy(&point)?;
		assert!((energy.value(i)? - expected).abs() < 1e-12);
		assert!(energy.range().contains(energy.value(i)?));
	}
	assert!(energy.value(grid.dimension()).is_err());
	let smaller = ConfigurationGrid::uniform(4, -1., 1., 1, 1, 1024)?;
	assert!(KvnKineticEnergy::periodic_bdm1(&smaller, &flow, KvnRecipeLimits::default()).is_err());
	assert!(
		KvnKineticEnergy::periodic_bdm1(
			&grid,
			&flow,
			KvnRecipeLimits {
				max_bytes: 0,
				..KvnRecipeLimits::default()
			}
		)
		.is_err()
	);
	assert!(
		KvnKineticEnergy::periodic_bdm1(
			&grid,
			&flow,
			KvnRecipeLimits {
				max_query_work: 0,
				..KvnRecipeLimits::default()
			}
		)
		.is_err()
	);
	Ok(())
}

#[test]
fn box_energy_is_integrated_energy_without_dividing_by_volume()
-> Result<(), Box<dyn std::error::Error>> {
	let model = quest_cfd::physical_space::PhysicalSpace::box_mesh(
		2,
		1,
		2.,
		0.1,
		quest_cfd::simplex::BoxBoundary::Periodic,
		2,
	)?;
	let grid = ConfigurationGrid::uniform(model.dimension(), -0.2, 0.3, 1, 1, 2048)?;
	let energy = KvnKineticEnergy::box_space(&grid, &model, KvnRecipeLimits::default())?;
	for i in 0..grid.dimension() {
		let point = grid.point(i).ok_or("point")?;
		assert!((model.energy(&point)? - energy.value(i)?).abs() < 1e-12);
	}
	Ok(())
}
