#![allow(
	clippy::panic_in_result_fn,
	reason = "Regression tests assert independently observable resource and numerical behavior"
)]
use quest_numerics::{Complex64, ConvolutionWorkspace, FftBackend, Limits};

#[test]
fn repeated_convolutions_exhaust_the_same_workspace_work_budget()
-> Result<(), Box<dyn std::error::Error>> {
	let limits = Limits {
		shapes: (Limits::default()).shapes,
		resources: quest_numerics::ResourceLimits {
			max_work_units: 400,
			..(Limits::default()).resources
		},
	};
	let mut workspace = ConvolutionWorkspace::new(2, 2, FftBackend::Scalar, limits)?;
	let input = [Complex64::new(1.0, 0.0), Complex64::new(2.0, 0.0)];
	assert!(workspace.convolve(&input, &input).is_ok());
	assert!(workspace.convolve(&input, &input).is_ok());
	assert!(workspace.convolve(&input, &input).is_err());
	Ok(())
}

#[test]
fn reservations_follow_shared_ownership_and_errors_keep_prior_work()
-> Result<(), Box<dyn std::error::Error>> {
	use quest_numerics::{OperationResources, ResourceLimits, ShapeLimits};
	let resources = OperationResources::new(
		ShapeLimits::default(),
		ResourceLimits {
			max_peak_bytes: 100,
			max_work_units: 10,
		},
	);
	let owned = resources.reserve(60, 0)?;
	let shared = owned.clone();
	assert!(resources.reserve(41, 0).is_err());
	drop(owned);
	assert_eq!(resources.report().live_bytes, 60);
	drop(shared);
	assert_eq!(resources.report().live_bytes, 0);
	resources.charge_work(6)?;
	assert!(resources.charge_work(5).is_err());
	assert_eq!(resources.report().work_units, 6);
	assert_eq!(resources.report().peak_bytes, 60);
	assert!(resources.reserve(usize::MAX, 1).is_err());
	Ok(())
}

#[test]
fn parallel_batch_overflow_is_reported_and_releases_nothing()
-> Result<(), Box<dyn std::error::Error>> {
	use quest_numerics::{OperationResources, ResourceError};
	let resources = OperationResources::default();
	let owner = resources.reserve(32, 0)?;
	assert!(resources.reserve_many(&[(usize::MAX, 0), (1, 0)]).is_err());
	assert!(resources.reserve_many(&[(usize::MAX, 1)]).is_err());
	assert!(resources.reserve_many(&[(usize::MAX, 0)]).is_err());
	assert_eq!(resources.report().live_bytes, 32);
	assert_eq!(
		resources.report().last_rejection,
		Some(ResourceError::Overflow)
	);
	drop(owner);
	assert_eq!(resources.report().live_bytes, 0);
	Ok(())
}

#[test]
fn coefficient_fft_and_completion_gates_are_independent() -> Result<(), Box<dyn std::error::Error>>
{
	use quest_numerics::{
		ExecutionPolicy, FftWorkspace, OperationLimits, OperationResources, ShapeLimits,
	};
	let resources = OperationResources::from_limits(OperationLimits {
		shapes: ShapeLimits {
			max_coefficients: 5,
			max_fft_len: 8,
			max_completion_grid: 16,
		},
		..OperationLimits::default()
	});
	let workspace = ConvolutionWorkspace::new_with_resources(
		5,
		4,
		FftBackend::Scalar,
		&resources,
		ExecutionPolicy::Sequential,
	)?;
	assert_eq!(workspace.fft_len(), 8);
	assert!(
		ConvolutionWorkspace::new_with_resources(
			5,
			5,
			FftBackend::Scalar,
			&resources,
			ExecutionPolicy::Sequential
		)
		.is_err()
	);
	assert!(FftWorkspace::new_with_resources(16, FftBackend::Scalar, resources.clone()).is_err());
	let completion = FftWorkspace::new_for_completion(16, FftBackend::Scalar, resources)?;
	assert_eq!(completion.len(), 16);
	Ok(())
}

#[test]
fn constructor_admission_reports_the_requested_peak_before_allocation() {
	use quest_numerics::{FftWorkspace, OperationLimits, OperationResources, ResourceError};
	let mut limits = OperationLimits::default();
	limits.resources.max_peak_bytes = 1;
	let resources = OperationResources::from_limits(limits);
	assert!(FftWorkspace::new_with_resources(8, FftBackend::Scalar, resources.clone()).is_err());
	let report = resources.report();
	assert_eq!(report.live_bytes, 0);
	assert_eq!(report.peak_bytes, 0);
	assert_eq!(report.requested_peak_bytes, 8192);
	assert_eq!(
		report.last_rejection,
		Some(ResourceError::Limit {
			resource: "peak modeled bytes",
			requested: 8192,
			limit: 1
		})
	);
}
