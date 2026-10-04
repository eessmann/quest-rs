#![allow(
	clippy::panic_in_result_fn,
	clippy::default_trait_access,
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	clippy::suboptimal_flops,
	reason = "Independent bounded numerical references and manufactured convergence comparisons"
)]
use quest_cfd::{PeriodicBdm1, configuration::ConfigurationGrid, history::HistorySystem};

#[test]
fn weighted_kvn_retains_all_five_coordinates_and_history_initial_data()
-> Result<(), Box<dyn std::error::Error>> {
	let flow = PeriodicBdm1::assemble(0.01)?;
	let grid = ConfigurationGrid::uniform(flow.dimension(), -3.0, 3.0, 2, 1, 20_000)?;
	assert_eq!(grid.dimension(), 1024);
	let generator = grid.generator(&flow, Default::default())?;
	let skew = generator
		.entries()
		.map(|(row, col, value)| {
			let reverse = generator
				.row(col)
				.find(|(j, _)| *j == row)
				.map_or(0.0, |(_, z)| z.re);
			(value.re + reverse).abs()
		})
		.fold(0.0_f64, f64::max);
	assert!(skew < 1e-11, "mass-scaled KvN skew residual {skew}");
	let initial = grid.initial_bump(&[0.0; 5], 2.0)?;
	let norm = initial
		.iter()
		.map(quest_numerics::Complex64::norm_sqr)
		.sum::<f64>();
	assert!((norm - 1.0).abs() < 1e-12);
	let history = HistorySystem::assemble(&generator, &initial, 0.2, 2, 1, Default::default())?;
	assert_eq!(history.operator().rows(), 4096);
	assert_eq!(history.rhs().len(), 4096);
	assert_eq!(history.configuration_dimension(), 1024);
	Ok(())
}

#[test]
fn causal_dg_history_reproduces_constant_solution_and_rejects_exponential_budget()
-> Result<(), Box<dyn std::error::Error>> {
	use quest_numerics::{Complex64, SparseFormat, SparseMatrix};
	let zero =
		SparseMatrix::from_triplets(2, 2, SparseFormat::Csr, Vec::new(), Default::default())?;
	let initial = vec![Complex64::new(1.0, 0.0), Complex64::new(0.0, 1.0)];
	let history = HistorySystem::assemble(&zero, &initial, 1.0, 3, 2, Default::default())?;
	let exact: Vec<_> = (0..9).flat_map(|_| initial.iter().copied()).collect();
	let applied = history.operator().matvec(&exact, Default::default())?;
	let error = applied
		.iter()
		.zip(history.rhs())
		.map(|(a, b)| (*a - *b).norm())
		.fold(0.0_f64, f64::max);
	assert!(error < 1e-12, "constant history residual {error}");
	assert!(ConfigurationGrid::uniform(80, -1.0, 1.0, 2, 1, 100_000).is_err());
	Ok(())
}

#[test]
fn configuration_manufactured_wave_converges_separately_from_skew_identity()
-> Result<(), Box<dyn std::error::Error>> {
	use quest_numerics::Complex64;
	let mut errors = Vec::new();
	for cells in [4, 8, 16] {
		let grid = ConfigurationGrid::uniform(1, -1.0, 1.0, cells, 2, 1024)?;
		let generator = grid.generator_from(1, |_| Ok(vec![1.0]), Default::default())?;
		let values: Vec<_> = (0..grid.dimension())
			.map(|i| {
				let point = grid.point(i).unwrap_or_default();
				Complex64::from_polar(
					grid.weight(i).unwrap_or(0.0).sqrt(),
					std::f64::consts::PI * point[0],
				)
			})
			.collect();
		let actual = generator.matvec(&values, Default::default())?;
		errors.push(
			actual
				.iter()
				.zip(&values)
				.map(|(a, z)| (*a + Complex64::new(0.0, std::f64::consts::PI) * z).norm_sqr())
				.sum::<f64>()
				.sqrt(),
		);
	}
	assert!(
		errors[1] < 0.3 * errors[0] && errors[2] < 0.3 * errors[1],
		"manufactured DG2 errors {errors:?}"
	);
	Ok(())
}

fn dense_reference(history: &HistorySystem) -> Vec<quest_numerics::Complex64> {
	use quest_numerics::Complex64;
	let n = history.rhs().len();
	assert!(n <= 256);
	let mut a = vec![vec![Complex64::new(0.0, 0.0); n + 1]; n];
	for (r, c, v) in history.operator().entries() {
		a[r][c] = v;
	}
	for (r, b) in history.rhs().iter().enumerate() {
		a[r][n] = *b;
	}
	for k in 0..n {
		let pivot = (k..n)
			.max_by(|r, s| a[*r][k].norm().total_cmp(&a[*s][k].norm()))
			.unwrap_or(k);
		a.swap(k, pivot);
		let d = a[k][k];
		assert!(d.norm() > 1e-12);
		for value in a[k].iter_mut().take(n + 1).skip(k) {
			*value /= d;
		}
		for i in 0..n {
			if i != k {
				let factor = a[i][k];
				for j in k..=n {
					let value = a[k][j];
					a[i][j] -= factor * value;
				}
			}
		}
	}
	a.iter().map(|row| row[n]).collect()
}
#[test]
fn global_temporal_dg_refines_nonconstant_skew_evolution() -> Result<(), Box<dyn std::error::Error>>
{
	use quest_numerics::{Complex64, SparseFormat, SparseMatrix};
	let generator = SparseMatrix::from_triplets(
		1,
		1,
		SparseFormat::Csr,
		vec![(0, 0, Complex64::new(0.0, 2.0))],
		Default::default(),
	)?;
	for order in [1, 2] {
		let mut errors = Vec::new();
		for cells in [2, 4, 8] {
			let history = HistorySystem::assemble(
				&generator,
				&[Complex64::new(1.0, 0.0)],
				1.0,
				cells,
				order,
				Default::default(),
			)?;
			let values = dense_reference(&history);
			errors.push(
				(values.last().copied().unwrap_or_default() - Complex64::from_polar(1.0, 2.0))
					.norm(),
			);
			if order == 1 {
				assert!(history.spectral_bounds(Default::default())?.lower() > 0.0);
			}
		}
		assert!(
			errors[1] < 0.4 * errors[0] && errors[2] < 0.4 * errors[1],
			"DG{order} endpoint errors {errors:?}"
		);
	}
	Ok(())
}

#[test]
fn tensor_mass_and_observable_overflow_fail_before_success_reports()
-> Result<(), Box<dyn std::error::Error>> {
	use quest_numerics::Complex64;
	assert!(ConfigurationGrid::uniform(2, 0.0, 1e-300, 1, 1, 32).is_err());
	let grid = ConfigurationGrid::uniform(1, -1.0, 1.0, 2, 1, 32)?;
	let state = vec![Complex64::new(0.5, 0.0); 4];
	assert!(grid.observables(&state, |_| Ok(f64::INFINITY)).is_err());
	let generator = grid.generator_from(1, |_| Ok(vec![1.0]), Default::default())?;
	assert!(generator.nnz() > 0);
	Ok(())
}

#[test]
fn temporal_mass_underflow_and_concurrent_assembly_storage_are_rejected()
-> Result<(), Box<dyn std::error::Error>> {
	use quest_numerics::{Complex64, SparseFormat, SparseLimits, SparseMatrix};
	let zero = SparseMatrix::from_triplets(1, 1, SparseFormat::Csr, vec![], Default::default())?;
	let initial = [Complex64::new(1.0, 0.0)];
	assert!(
		HistorySystem::assemble(&zero, &initial, f64::from_bits(1), 1, 1, Default::default())
			.is_err()
	);
	let triplet = std::mem::size_of::<(usize, usize, Complex64)>();
	let canonical_peak = 6 * triplet
		+ 2 * (4 * (std::mem::size_of::<Complex64>() + std::mem::size_of::<usize>())
			+ 3 * std::mem::size_of::<usize>())
		+ 4 * triplet;
	assert!(
		HistorySystem::assemble(
			&zero,
			&initial,
			1.0,
			1,
			1,
			SparseLimits {
				max_bytes: canonical_peak,
				..Default::default()
			}
		)
		.is_err()
	);
	Ok(())
}
#[test]
fn unrepresentable_long_horizon_bound_preserves_history_with_explicit_unsupported_evidence()
-> Result<(), Box<dyn std::error::Error>> {
	use quest_numerics::{Complex64, SparseFormat, SparseMatrix};
	let zero = SparseMatrix::from_triplets(1, 1, SparseFormat::Csr, vec![], Default::default())?;
	let history = HistorySystem::assemble(
		&zero,
		&[Complex64::new(1.0, 0.0)],
		1.0,
		1024,
		1,
		Default::default(),
	)?;
	assert_eq!(history.operator().rows(), 2048);
	assert!(matches!(
		history.spectral_bounds(Default::default()),
		Err(quest_cfd::CfdError::Unsupported(_))
	));
	Ok(())
}
