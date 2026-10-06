#![allow(
	clippy::unwrap_used,
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	clippy::cast_precision_loss,
	clippy::suboptimal_flops,
	reason = "Bounded independent dense constraint references use indexed arithmetic and exactly representable fixture counts"
)]
use quest_cfd::{
	physical_space::{BoxConstraintRecipe, ConstraintRecipeLimits, PhysicalSpace},
	simplex::BoxBoundary,
};
#[test]
fn generated_mass_and_full_constraints_match_dense_physical_kernel() {
	for d in [2, 3] {
		for p in [1, 2] {
			for boundary in [BoxBoundary::Periodic, BoxBoundary::Cavity { lid_speed: 1. }] {
				let recipe = BoxConstraintRecipe::new(
					d,
					1,
					1.,
					boundary,
					p,
					ConstraintRecipeLimits::default(),
				)
				.unwrap();
				let dense = PhysicalSpace::box_mesh(d, 1, 1., 0., boundary, p).unwrap();
				assert_eq!(recipe.expected_nullity(), dense.dimension());
				let local = recipe.local_velocity_dimension();
				for cell in 0..recipe.cell_count() {
					let mass = dense.cell_mass_matrix(cell).unwrap();
					for i in 0..local {
						for j in 0..local {
							assert!(
								(mass[i * local + j] - recipe.mass_value(cell, i, j).unwrap())
									.abs()
									< 2e-14
							);
						}
					}
				}
				// Independent dense basis must satisfy every generated face/divergence constraint.
				for q in dense.chart() {
					for column in 0..recipe.constraint_count() {
						let mut residual = 0.;
						for cell in 0..recipe.cell_count() {
							for i in 0..local {
								residual += recipe.constraint_value(cell, i, column).unwrap()
									* q[cell * local + i];
							}
						}
						assert!(
							residual.abs() < 1e-9,
							"d={d} p={p} column={column} residual={residual}"
						);
					}
				}
			}
		}
	}
}
#[test]
fn huge_logical_grid_uses_fixed_storage_and_rejects_invalid_queries() {
	let small = BoxConstraintRecipe::new(
		3,
		1,
		1.,
		BoxBoundary::Periodic,
		2,
		ConstraintRecipeLimits::default(),
	)
	.unwrap();
	let huge = BoxConstraintRecipe::new(
		3,
		1000,
		1.,
		BoxBoundary::Periodic,
		2,
		ConstraintRecipeLimits::default(),
	)
	.unwrap();
	assert_eq!(small.source_bytes(), huge.source_bytes());
	assert_eq!(huge.cell_count(), 6_000_000_000);
	// Last tetrahedron has permutation (z,y,x). Its face opposite vertex zero
	// wraps to z=0, permutation (y,x,z), sharing owner vertex 1 / neighbor vertex 0.
	let last_face = (huge.cell_count() - 1) * 24;
	assert_eq!(
		huge.constraint_value(huge.cell_count() - 1, 21, last_face)
			.unwrap(),
		1.
	);
	assert_eq!(
		huge.constraint_value(5_999_996, 20, last_face).unwrap(),
		-1.
	);
	assert!(huge.mass_value(huge.cell_count() - 1, 0, 0).unwrap() > 0.);
	assert_eq!(
		huge.constraint_value(0, 0, huge.constraint_count() - 1)
			.unwrap(),
		0.
	);
	assert!(huge.mass_value(huge.cell_count(), 0, 0).is_err());
	assert!(
		huge.constraint_value(0, 0, huge.constraint_count())
			.is_err()
	);
	assert!(
		BoxConstraintRecipe::new(
			3,
			1,
			1.,
			BoxBoundary::Periodic,
			2,
			ConstraintRecipeLimits {
				max_source_bytes: 0,
				..ConstraintRecipeLimits::default()
			}
		)
		.is_err()
	);
	assert!(
		BoxConstraintRecipe::new(
			3,
			1,
			1.,
			BoxBoundary::Periodic,
			2,
			ConstraintRecipeLimits {
				max_scalar_query_work: 0,
				..ConstraintRecipeLimits::default()
			}
		)
		.is_err()
	);
}

#[test]
fn factory_accounting_charges_every_generated_scalar_and_rejects_overflow() {
	let recipe = BoxConstraintRecipe::new(
		3,
		1,
		1.,
		BoxBoundary::Periodic,
		2,
		ConstraintRecipeLimits::default(),
	)
	.unwrap();
	let costs = recipe.factory_source_costs(1).unwrap();
	// Six tetrahedra,30 velocity nodes,168 redundant constraints: 30x30 M +30x168 CT per cell.
	assert_eq!(costs.callback_count_per_rank, 35_640);
	assert_eq!(costs.work_per_rank, 1_000_000 + 35_640 * 4096);
	assert!(costs.live_bytes >= recipe.source_bytes());
	assert!(recipe.factory_source_costs(0).is_err());
	let huge = BoxConstraintRecipe::new(
		3,
		1000,
		1.,
		BoxBoundary::Periodic,
		2,
		ConstraintRecipeLimits::default(),
	)
	.unwrap();
	assert!(huge.factory_source_costs(1).is_err());
	assert!(
		BoxConstraintRecipe::new(
			3,
			1,
			1.,
			BoxBoundary::Periodic,
			2,
			ConstraintRecipeLimits {
				max_construction_work: 0,
				..ConstraintRecipeLimits::default()
			}
		)
		.is_err()
	);
}
// Independent elimination on the generated rectangular matrix catches missing or extra
// constraints; checking only C*Q=0 would admit a source that always returned zero.
fn numerical_rank(mut rows: Vec<Vec<f64>>) -> usize {
	let mut rank = 0;
	for column in 0..rows[0].len() {
		let Some(pivot) = (rank..rows.len())
			.max_by(|&a, &b| rows[a][column].abs().total_cmp(&rows[b][column].abs()))
		else {
			break;
		};
		if rows[pivot][column].abs() < 1e-9 {
			continue;
		}
		rows.swap(rank, pivot);
		let value = rows[rank][column];
		for j in column..rows[0].len() {
			rows[rank][j] /= value;
		}
		for i in rank + 1..rows.len() {
			let factor = rows[i][column];
			for j in column..rows[0].len() {
				rows[i][j] -= factor * rows[rank][j];
			}
		}
		rank += 1;
	}
	rank
}
#[test]
fn redundant_face_rows_have_exact_complete_rank_and_divergence_signs() {
	for (d, n, p) in [(2, 1, 1), (2, 2, 1), (2, 2, 2), (3, 1, 1), (3, 1, 2)] {
		for boundary in [BoxBoundary::Periodic, BoxBoundary::Cavity { lid_speed: 0. }] {
			let recipe =
				BoxConstraintRecipe::new(d, n, 2., boundary, p, ConstraintRecipeLimits::default())
					.unwrap();
			let dense = PhysicalSpace::box_mesh(d, n, 2., 0., boundary, p).unwrap();
			let local = recipe.local_velocity_dimension();
			let columns = recipe.constraint_count();
			let mut rows = vec![vec![0.; local * recipe.cell_count()]; columns];
			for cell in 0..recipe.cell_count() {
				let reference = dense.cell_constraint_transpose(cell).unwrap();
				let pressure = if p == 1 { 1 } else { d + 1 };
				for i in 0..local {
					for (j, row) in rows.iter_mut().enumerate() {
						row[cell * local + i] = recipe.constraint_value(cell, i, j).unwrap();
					}
					// Both schemas put the identical signed cell divergence blocks last.
					for k in 0..pressure * recipe.cell_count() {
						let value = recipe
							.constraint_value(cell, i, columns - pressure * recipe.cell_count() + k)
							.unwrap();
						let expected = reference[i * dense.constraint_count()
							+ dense.constraint_count()
							- pressure * recipe.cell_count()
							+ k];
						assert!((value - expected).abs() < 1e-13);
					}
				}
			}
			if d == 2 && n == 2 {
				for q in dense.chart() {
					for row in &rows {
						assert!(row.iter().zip(q).map(|(a, b)| a * b).sum::<f64>().abs() < 1e-9);
					}
				}
			}
			assert_eq!(
				local * recipe.cell_count() - numerical_rank(rows),
				dense.dimension(),
				"d={d} n={n} p={p}"
			);
		}
	}
}
