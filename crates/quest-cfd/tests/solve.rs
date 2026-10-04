#![allow(
	clippy::panic_in_result_fn,
	clippy::default_trait_access,
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	reason = "Small independent exact constant-history solution"
)]
#[test]
fn qsvt_solves_causal_history_with_physical_scale_and_success_mass()
-> Result<(), Box<dyn std::error::Error>> {
	use quest_cfd::{
		history::HistorySystem,
		solve::{SolveBudget, solve_history},
	};
	use quest_numerics::{Complex64, SparseFormat, SparseMatrix};
	let zero =
		SparseMatrix::from_triplets(2, 2, SparseFormat::Csr, Vec::new(), Default::default())?;
	let initial = vec![Complex64::new(1.0, 0.0), Complex64::new(0.0, 2.0)];
	let history = HistorySystem::assemble(&zero, &initial, 0.1, 1, 1, Default::default())?;
	let result = solve_history(
		&history,
		SolveBudget {
			relative_residual: 0.01,
			certify: true,
			..Default::default()
		},
	)?;
	assert!(result.relative_residual < 0.01);
	assert!(result.success_probability > 0.0 && result.success_probability <= 1.0);
	assert!(result.projector_response_bound.is_some());
	for i in 0..4 {
		assert!((result.solution[i] - initial[i % 2]).norm() < 0.01);
	}
	Ok(())
}

#[cfg(feature = "quantum")]
#[test]
fn native_qsvt_history_matches_physical_constant_reference()
-> Result<(), Box<dyn std::error::Error>> {
	use quest_cfd::{
		history::HistorySystem,
		solve::{SolveBackend, SolveBudget, solve_history_with_backend},
	};
	use quest_numerics::{Complex64, SparseFormat, SparseLimits, SparseMatrix};
	const NAME: &str = "native_qsvt_history_matches_physical_constant_reference";
	if std::env::var("QUEST_CFD_NATIVE_TEST").as_deref() != Ok(NAME) {
		let status = std::process::Command::new(std::env::current_exe()?)
			.args(["--exact", NAME, "--test-threads=1"])
			.env("QUEST_CFD_NATIVE_TEST", NAME)
			.status()?;
		assert!(status.success());
		return Ok(());
	}
	let zero =
		SparseMatrix::from_triplets(2, 2, SparseFormat::Csr, Vec::new(), SparseLimits::default())?;
	let initial = vec![Complex64::new(1.0, 0.0), Complex64::new(0.0, 2.0)];
	let history = HistorySystem::assemble(&zero, &initial, 0.1, 1, 1, SparseLimits::default())?;
	let result =
		solve_history_with_backend(&history, SolveBudget::default(), SolveBackend::QuestCpu)?;
	assert!(result.relative_residual < 0.01);
	assert!(result.backend.contains("QuEST CPU"));
	for i in 0..4 {
		assert!((result.solution[i] - initial[i % 2]).norm() < 0.01);
	}
	Ok(())
}

#[test]
fn caller_memory_budget_reaches_certification_before_transform_construction()
-> Result<(), Box<dyn std::error::Error>> {
	use quest_cfd::{
		history::HistorySystem,
		solve::{SolveBudget, solve_history},
	};
	use quest_numerics::{Complex64, SparseFormat, SparseMatrix};
	let zero = SparseMatrix::from_triplets(2, 2, SparseFormat::Csr, vec![], Default::default())?;
	let history = HistorySystem::assemble(
		&zero,
		&[Complex64::new(1.0, 0.0), Complex64::new(0.0, 2.0)],
		0.1,
		1,
		1,
		Default::default(),
	)?;
	let error = solve_history(
		&history,
		SolveBudget {
			max_bytes: 1_048_576,
			certify: true,
			..Default::default()
		},
	)
	.expect_err("small caller budget must reject preprocessing");
	assert!(
		error.to_string().contains("certification") && error.to_string().contains("budget"),
		"must reject within caller-bounded certification before constructing a transform: {error}"
	);
	Ok(())
}
