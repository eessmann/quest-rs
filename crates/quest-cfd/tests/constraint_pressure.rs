#![allow(
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	clippy::suboptimal_flops,
	clippy::panic_in_result_fn,
	reason = "Independent complete multiplier gauge null-vector check"
)]
use quest_cfd::{
	physical_space::{BoxConstraintRecipe, ConstraintRecipeLimits},
	simplex::BoxBoundary,
};
#[test]
fn pressure_mean_gauge_vector_preserves_every_original_momentum_coefficient()
-> Result<(), Box<dyn std::error::Error>> {
	for d in [2, 3] {
		for p in [1, 2] {
			for boundary in [BoxBoundary::Periodic, BoxBoundary::Cavity { lid_speed: 1. }] {
				let source = BoxConstraintRecipe::new(
					d,
					1,
					2.,
					boundary,
					p,
					ConstraintRecipeLimits::default(),
				)?;
				assert!(source.pressure_integral_weight() > 0.);
				for column in source.pressure_constraint_start()..source.constraint_count() {
					assert_eq!(source.pressure_gauge_coefficient(column)?, -1.);
				}
				let mut normal_sum = 0.;
				for column in 0..source.pressure_constraint_start() {
					normal_sum += source.pressure_gauge_coefficient(column)?;
				}
				assert!(normal_sum > 0.);
				for cell in 0..source.cell_count() {
					for row in 0..source.local_velocity_dimension() {
						let mut residual = 0.;
						for column in 0..source.constraint_count() {
							residual += source.constraint_value(cell, row, column)?
								* source.pressure_gauge_coefficient(column)?;
						}
						assert!(
							residual.abs() < 1e-13,
							"d={d} p={p} cell={cell} row={row} residual={residual}"
						);
					}
				}
				assert!(
					source
						.pressure_gauge_coefficient(source.constraint_count())
						.is_err()
				);
			}
		}
	}
	Ok(())
}
