//! Opt-in access to production kernels for reproducible stage measurements.
//!
//! These functions retain numerical policies and errors; they do not certify
//! inputs. Callers must perform their source-specific preflight outside timing.
use crate::{Complex64, Error, Policy, Result, kernel};
use quest_numerics::{Accounted, ExecutionPolicy, OperationResources};

pub use crate::ScatteringPair;

/// Complete a canonical target using the production Weiss kernel.
/// # Errors
/// Preserves invalid-policy, resource, nonfinite and accuracy failures.
pub fn complete(target: &[Complex64], policy: Policy) -> Result<Accounted<ScatteringPair>> {
	policy.validate()?;
	let resources = OperationResources::from_limits(policy.limits);
	let target_ownership = resources
		.reserve(
			target
				.len()
				.checked_mul(size_of::<Complex64>())
				.ok_or(Error::Budget("target result storage"))?,
			0,
		)
		.map_err(quest_numerics::Error::from)?;
	let (result, mut ownership) =
		kernel::complete_with_resources(target, policy, ExecutionPolicy::Sequential, &resources)?
			.into_parts();
	let (complement, _, _, _) = result;
	ownership.truncate(1); // The RHW ratio was dropped; only complement ownership survives.
	ownership.push(target_ownership);
	Ok(Accounted::new_many(
		ScatteringPair {
			conjugate_complement: complement,
			target: target.to_vec(),
		},
		ownership,
	))
}

/// Run only the production inverse NLFT on a prepared pair.
/// # Errors
/// Preserves original kernel failures; rejects nonfinite or malformed input.
pub fn inverse(pair: &ScatteringPair, policy: Policy) -> Result<Accounted<Vec<Complex64>>> {
	policy.validate()?;
	for value in pair.conjugate_complement.iter().chain(&pair.target) {
		crate::finite(*value, "benchmark inverse input")?;
	}
	let mut workspace = crate::InverseNlftWorkspace::new(
		policy.backend,
		OperationResources::from_limits(policy.limits),
		ExecutionPolicy::Sequential,
	);
	workspace.inverse(&pair.conjugate_complement, &pair.target)
}

/// Reconstruct a scattering pair with the same production product tree used
/// to validate synthesized controls. Reflection normalization is included.
/// # Errors
/// Rejects empty/nonfinite reflections and preserves production resource failures.
pub fn forward(reflections: &[Complex64], policy: Policy) -> Result<Accounted<ScatteringPair>> {
	policy.validate()?;
	if reflections.is_empty() {
		return Err(Error::Target("empty reflection sequence"));
	}
	let mut workspace = crate::ForwardNlftWorkspace::new(
		policy.backend,
		OperationResources::from_limits(policy.limits),
		ExecutionPolicy::Sequential,
	);
	workspace.forward(reflections)
}
