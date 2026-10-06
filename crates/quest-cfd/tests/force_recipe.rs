#![allow(
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	clippy::suboptimal_flops,
	clippy::unwrap_used,
	reason = "Bounded independent full-force comparisons and implicit large-grid probes"
)]
use quest_cfd::{
	physical_space::{
		BoxConstraintRecipe, BoxForceRecipe, ConstraintRecipeLimits, ForceRecipeLimits,
		PhysicalSpace,
	},
	simplex::BoxBoundary,
};

#[test]
fn generated_complete_cell_force_matches_independent_dense_chart() {
	for d in [2, 3] {
		for p in [1, 2] {
			for boundary in [
				BoxBoundary::Periodic,
				BoxBoundary::Cavity { lid_speed: 0.7 },
			] {
				let geometry = BoxConstraintRecipe::new(
					d,
					1,
					1.7,
					boundary,
					p,
					ConstraintRecipeLimits::default(),
				)
				.unwrap();
				let dense = PhysicalSpace::box_mesh(d, 1, 1.7, 0.023, boundary, p).unwrap();
				let recipe = BoxForceRecipe::new(
					&geometry,
					0.023,
					if boundary == BoxBoundary::Periodic {
						0.
					} else {
						0.7
					},
					ForceRecipeLimits::default(),
				)
				.unwrap();
				let state = (0..dense.dimension())
					.map(|i| 0.02 * f64::from(u32::try_from(i + 1).unwrap()).sin())
					.collect::<Vec<_>>();
				let coefficients = dense.coefficients(&state).unwrap();
				let expected = dense.momentum_force(&coefficients).unwrap();
				let local = geometry.local_velocity_dimension();
				let mut actual = Vec::new();
				for cell in 0..geometry.cell_count() {
					let mut calls = 0;
					let force = recipe
						.cell_force(cell, &mut |neighbour| {
							calls += 1;
							let mut words = [0.; 30];
							words[..local].copy_from_slice(
								&coefficients[neighbour * local..(neighbour + 1) * local],
							);
							Ok(words)
						})
						.unwrap();
					assert!(calls <= d + 2);
					actual.extend_from_slice(&force[..local]);
				}
				for (&a, &b) in actual.iter().zip(&expected) {
					assert!((a - b).abs() < 2e-9, "d={d},p={p},{boundary:?}: {a} != {b}");
				}
				let drift = dense.drift(&state).unwrap();
				for (q, &expected) in dense.chart().iter().zip(&drift) {
					let projected = q.iter().zip(&actual).map(|(a, b)| a * b).sum::<f64>();
					assert!((projected - expected).abs() < 2e-9);
				}
				assert!(
					(dense.energy(&state).unwrap()
						- 0.5 * state.iter().map(|v| v * v).sum::<f64>())
					.abs()
						< 1e-12
				);
			}
		}
	}
}

#[test]
fn broken_state_viscosity_and_boundary_load_are_not_chart_assumptions() {
	let geometry = BoxConstraintRecipe::new(
		2,
		2,
		1.3,
		BoxBoundary::Cavity { lid_speed: 0.4 },
		2,
		ConstraintRecipeLimits::default(),
	)
	.unwrap();
	let dense = PhysicalSpace::box_mesh(2, 2, 1.3, 0.01, BoxBoundary::Cavity { lid_speed: 0.4 }, 2)
		.unwrap();
	let recipe = BoxForceRecipe::new(&geometry, 0.01, 0.4, ForceRecipeLimits::default()).unwrap();
	let size = geometry.cell_count() * geometry.local_velocity_dimension();
	let positive = (0..size)
		.map(|i| 0.03 * f64::from(u32::try_from(i + 3).unwrap()).cos())
		.collect::<Vec<_>>();
	let negative = positive.iter().map(|v| -v).collect::<Vec<_>>();
	let zero = vec![0.; size];
	let full = |words: &[f64]| {
		let local = geometry.local_velocity_dimension();
		(0..geometry.cell_count())
			.flat_map(|cell| {
				recipe
					.cell_force(cell, &mut |neighbour| {
						let mut block = [0.; 30];
						block[..local]
							.copy_from_slice(&words[neighbour * local..(neighbour + 1) * local]);
						Ok(block)
					})
					.unwrap()[..local]
					.to_vec()
			})
			.collect::<Vec<_>>()
	};
	let (ap, an, az) = (full(&positive), full(&negative), full(&zero));
	let (bp, bn, bz) = (
		dense.momentum_force(&positive).unwrap(),
		dense.momentum_force(&negative).unwrap(),
		dense.momentum_force(&zero).unwrap(),
	);
	for i in 0..size {
		assert!(((ap[i] - an[i]) - (bp[i] - bn[i])).abs() < 2e-11);
		assert!((az[i] - bz[i]).abs() < 2e-11);
	}
	assert!(
		ap.iter().zip(bp).any(|(a, b)| (a - b).abs() > 1e-7),
		"arbitrary broken convection explicitly uses averaged normal traces"
	);
}

#[test]
fn fixed_source_storage_and_admission_before_any_callback() {
	let geometry = BoxConstraintRecipe::new(
		3,
		1000,
		1.,
		BoxBoundary::Periodic,
		2,
		ConstraintRecipeLimits::default(),
	)
	.unwrap();
	let small = BoxConstraintRecipe::new(
		3,
		1,
		1.,
		BoxBoundary::Periodic,
		2,
		ConstraintRecipeLimits::default(),
	)
	.unwrap();
	let limits = ForceRecipeLimits::default();
	let recipe = BoxForceRecipe::new(&geometry, 0.01, 0., limits).unwrap();
	assert_eq!(
		recipe.source_bytes(),
		BoxForceRecipe::new(&small, 0.01, 0., limits)
			.unwrap()
			.source_bytes()
	);
	let mut calls = 0;
	assert!(
		recipe
			.cell_force(geometry.cell_count(), &mut |_| {
				calls += 1;
				Ok([0.; 30])
			})
			.is_err()
	);
	assert_eq!(calls, 0);
	let words = recipe
		.cell_force(geometry.cell_count() - 1, &mut |_| {
			calls += 1;
			Ok([0.; 30])
		})
		.unwrap();
	assert!(words.iter().all(|v| *v == 0.));
	assert_eq!(calls, 5);
	assert!(BoxForceRecipe::new(&small, -1., 0., limits).is_err());
	for tight in [
		ForceRecipeLimits {
			max_source_bytes: 1,
			..limits
		},
		ForceRecipeLimits {
			max_construction_work: 1,
			..limits
		},
		ForceRecipeLimits {
			max_cell_query_work: 1,
			..limits
		},
		ForceRecipeLimits {
			max_scratch_bytes: 1,
			..limits
		},
	] {
		assert!(BoxForceRecipe::new(&small, 0.01, 0., tight).is_err());
	}
	assert!(recipe.cell_force(0, &mut |_| Ok([f64::NAN; 30])).is_err());
}
