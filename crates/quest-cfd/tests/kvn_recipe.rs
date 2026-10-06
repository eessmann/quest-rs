#![allow(
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	clippy::suboptimal_flops,
	clippy::panic_in_result_fn,
	reason = "Independent stored-generator and whole weighted-adjoint comparisons"
)]
use mathcore::multivariate::PolynomialLimits;
use quest_cfd::{
	PeriodicBdm1,
	configuration::ConfigurationGrid,
	kvn_recipe::{KvnHistoryRecipe, KvnRecipeLimits},
	polynomial::PolynomialOde,
	stream_history::HistoryRowDynamics,
};
use quest_numerics::{Complex64, SparseFormat, SparseLimits, SparseMatrix};
#[test]
fn generated_rows_match_all_five_coordinate_csr_and_weighted_adjoint()
-> Result<(), Box<dyn std::error::Error>> {
	// Inviscid input isolates the nonlinear transport contribution.
	let flow = PeriodicBdm1::assemble(0.)?;
	let ode = PolynomialOde::from_periodic_bdm1(&flow, PolynomialLimits::default())?.dynamics;
	for (cells, order) in [(2, 1), (1, 2)] {
		let grid = ConfigurationGrid::uniform(5, -0.3, 0.3, cells, order, 2048)?;
		let recipe = KvnHistoryRecipe::new(&grid, &ode, KvnRecipeLimits::default())?;
		let reference = grid.generator(&flow, SparseLimits::default())?;
		let mut entries = Vec::new();
		for row in 0..grid.dimension() {
			recipe.visit_row(0., row, &mut |column, value| {
				entries.push((row, column, value));
				Ok(())
			})?;
		}
		let actual = SparseMatrix::from_triplets(
			grid.dimension(),
			grid.dimension(),
			SparseFormat::Csr,
			entries,
			SparseLimits::default(),
		)?;
		assert_eq!(recipe.dimension(), grid.dimension());
		let mut nonzero = 0.;
		for row in 0..grid.dimension() {
			let mut expected = vec![Complex64::new(0., 0.); grid.dimension()];
			for (column, value) in reference.row(row) {
				expected[column] = value;
			}
			for (column, value) in actual.row(row) {
				nonzero += value.norm_sqr();
				assert!((value - expected[column]).norm() < 1e-10);
				expected[column] = Complex64::new(0., 0.);
				let reverse = actual
					.row(column)
					.find(|(i, _)| *i == row)
					.map_or(Complex64::new(0., 0.), |(_, v)| v);
				assert!((value + reverse.conj()).norm() < 1e-11);
			}
			assert!(expected.iter().all(|v| v.norm() < 1e-10));
		}
		assert!(
			nonzero > 1e-5,
			"must exercise nonzero nonlinear KvN generator"
		);
	}
	Ok(())
}

#[test]
fn time_dependent_forcing_is_in_drift_and_generated_history_matches_stored()
-> Result<(), Box<dyn std::error::Error>> {
	use mathcore::{
		RBig,
		exact::{Owner, Symbol},
		multivariate::SparsePolynomial,
	};
	use quest_cfd::{
		history::HistorySystem,
		stream_history::{HistoryStreamLimits, TemporalHistoryRecipe},
	};
	let symbols = (0..3)
		.map(|i| Symbol::new(Owner::new(852), i))
		.collect::<Vec<_>>();
	let first = SparsePolynomial::from_terms(
		symbols.clone(),
		[
			(vec![0, 0, 2], RBig::ONE),
			(vec![1, 0, 1], RBig::ONE),
			(vec![1, 1, 0], RBig::from(-2)),
		],
		PolynomialLimits::default(),
	)?;
	let second = SparsePolynomial::from_terms(
		symbols,
		[(vec![0, 1, 0], RBig::from(-1)), (vec![2, 0, 0], RBig::ONE)],
		PolynomialLimits::default(),
	)?;
	let ode = PolynomialOde::from_polynomials(vec![first, second], 2, PolynomialLimits::default())?;
	let grid = ConfigurationGrid::uniform(2, -1., 1., 2, 2, 64)?;
	let recipe = KvnHistoryRecipe::new(&grid, &ode, KvnRecipeLimits::default())?;
	for time in [0., 0.31] {
		let reference = grid.generator_from(2, |x| ode.drift(time, x), SparseLimits::default())?;
		for row in 0..grid.dimension() {
			let mut values = vec![Complex64::new(0., 0.); grid.dimension()];
			recipe.visit_row(time, row, &mut |column, value| {
				values[column] += value;
				Ok(())
			})?;
			for (column, value) in reference.row(row) {
				assert!((values[column] - value).norm() < 1e-12);
				values[column] = Complex64::new(0., 0.);
			}
			assert!(values.iter().all(|v| v.norm() < 1e-12));
			assert_eq!(recipe.source_entry(time, row)?, Complex64::new(0., 0.));
		}
	}
	// Autonomous nonlinear full physical fixture through both temporal DG orders.
	let flow = PeriodicBdm1::assemble(0.01)?;
	let ode = PolynomialOde::from_periodic_bdm1(&flow, PolynomialLimits::default())?.dynamics;
	let grid = ConfigurationGrid::uniform(5, -0.3, 0.3, 1, 2, 243)?;
	let generator = grid.generator(&flow, SparseLimits::default())?;
	let recipe = KvnHistoryRecipe::new(&grid, &ode, KvnRecipeLimits::default())?;
	let initial = vec![Complex64::new(0.1, 0.2); grid.dimension()];
	for order in [1, 2] {
		let stored = HistorySystem::assemble(
			&generator,
			&initial,
			0.01,
			2,
			order,
			SparseLimits::default(),
		)?;
		let history = TemporalHistoryRecipe::new(
			&recipe,
			0.01,
			2,
			order,
			HistoryStreamLimits {
				max_work_per_visit: 2_000_000_000,
				..HistoryStreamLimits::default()
			},
		)?;
		let mut triplets = Vec::new();
		for entry in history.rows(0..history.dimension())? {
			let e = entry?;
			triplets.push((e.row, e.column, e.value));
		}
		let generated = SparseMatrix::from_triplets(
			history.dimension(),
			history.dimension(),
			SparseFormat::Csr,
			triplets,
			SparseLimits::default(),
		)?;
		for row in 0..history.dimension() {
			let mut residual = vec![Complex64::new(0., 0.); history.dimension()];
			for (column, v) in stored.operator().row(row) {
				residual[column] += v;
			}
			for (column, v) in generated.row(row) {
				residual[column] -= v;
			}
			assert!(residual.iter().all(|v| v.norm() < 1e-11));
			assert!(
				(history.rhs_value(row, |i| Ok(initial[i]))? - stored.rhs()[row]).norm() < 1e-14
			);
		}
	}
	Ok(())
}

#[test]
fn arbitrary_tensor_rows_require_only_local_scratch_and_errors_propagate()
-> Result<(), Box<dyn std::error::Error>> {
	use quest_cfd::CfdError;
	let flow = PeriodicBdm1::assemble(0.01)?;
	let ode = PolynomialOde::from_periodic_bdm1(&flow, PolynomialLimits::default())?.dynamics;
	let grid = ConfigurationGrid::uniform(5, -0.3, 0.3, 512, 1, usize::MAX)?;
	let recipe = KvnHistoryRecipe::new(&grid, &ode, KvnRecipeLimits::default())?;
	assert_eq!(recipe.dimension(), 1_125_899_906_842_624);
	let mut seen = 0;
	recipe.visit_row(0., recipe.dimension() - 1, &mut |column, v| {
		assert!(column < recipe.dimension() && v.re.is_finite());
		seen += 1;
		Ok(())
	})?;
	assert_eq!(seen, 20);
	assert_eq!(recipe.resources().maximum_drift_evaluations_per_row, 21);
	assert!(recipe.row_query_bytes() < 4096);
	assert!(
		recipe
			.visit_row(0., 0, &mut |_, _| Err(CfdError::InvalidInput(
				"visitor rejection"
			)))
			.is_err()
	);
	assert!(recipe.visit_row(f64::NAN, 0, &mut |_, _| Ok(())).is_err());
	assert!(recipe.source_entry(0., recipe.dimension()).is_err());
	let resources = recipe.resources();
	for limits in [
		KvnRecipeLimits {
			max_bytes: resources.retained_bytes + resources.row_query_bytes - 1,
			..Default::default()
		},
		KvnRecipeLimits {
			max_query_work: resources.row_query_work - 1,
			..Default::default()
		},
		KvnRecipeLimits {
			max_dimension: grid.dimension() - 1,
			..Default::default()
		},
	] {
		assert!(KvnHistoryRecipe::new(&grid, &ode, limits).is_err());
	}
	let reduced = ConfigurationGrid::uniform(4, -1., 1., 1, 2, 81)?;
	assert!(KvnHistoryRecipe::new(&reduced, &ode, KvnRecipeLimits::default()).is_err());
	Ok(())
}

#[test]
fn scalar_bump_matches_normalized_reference_and_rejects_empty_support_and_capacity()
-> Result<(), Box<dyn std::error::Error>> {
	use quest_cfd::kvn_recipe::CompactBumpRecipe;
	let grid = ConfigurationGrid::uniform(5, -1., 1., 2, 2, 10_000)?;
	let bump = CompactBumpRecipe::new(&grid, vec![0.1; 5], 0.8, KvnRecipeLimits::default())?;
	let reference = grid.initial_bump(&[0.1; 5], 0.8)?;
	let values = (0..grid.dimension())
		.map(|i| bump.amplitude(i))
		.collect::<Result<Vec<_>, _>>()?;
	let norm = values.iter().fold(0_f64, |n, v| n.hypot(v.norm()));
	for (a, b) in values.iter().zip(reference) {
		assert!((*a / norm - b).norm() < 1e-12);
	}
	assert!(bump.amplitude(grid.dimension()).is_err());
	let sparse = ConfigurationGrid::uniform(5, -1., 1., 1, 1, 32)?;
	assert!(CompactBumpRecipe::new(&sparse, vec![0.; 5], 0.5, KvnRecipeLimits::default()).is_err());
	// Support intersects every axis, but all sampled products underflow.
	assert!(
		CompactBumpRecipe::new(
			&sparse,
			vec![0.; 5],
			1.000_000_1,
			KvnRecipeLimits::default()
		)
		.is_err()
	);
	let mut center = Vec::with_capacity(32_768);
	center.extend_from_slice(&[0.; 5]);
	assert!(
		CompactBumpRecipe::new(
			&grid,
			center,
			0.8,
			KvnRecipeLimits {
				max_bytes: 65_536,
				..Default::default()
			}
		)
		.is_err()
	);
	Ok(())
}
