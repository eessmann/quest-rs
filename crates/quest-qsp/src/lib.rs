#![forbid(unsafe_code)]
#![doc = include_str!("../README.md")]

mod admission;
#[cfg(feature = "artifact")]
pub mod artifact;
#[cfg(feature = "benchmark-support")]
pub mod benchmark_support;
#[cfg(feature = "certification")]
pub mod certification;
#[cfg(feature = "offline-synthesis")]
pub mod offline;
#[cfg(feature = "certification")]
pub mod precision;

mod kernel;
pub use kernel::{ForwardNlftWorkspace, InverseNlftWorkspace, ScatteringPair};
mod sequence;
mod stages;
pub use sequence::{
	ControlSequence, ControlSequenceBuilder, ConvertedProjectorPhases, MissingControls,
	PhaseConvention, PhaseSequence, PhaseSequenceBuilder, SuppliedControls, WxImaginaryU00,
	WxLaurent, WxSymmetric,
};

pub use num_complex::Complex64;
pub use quest_numerics::FftBackend;
pub use stages::{
	AdmittedTarget, CompletedPolynomial, FrozenCandidate, MissingTarget, OuterGauge, Policy,
	ReadyTarget, RealParityWx, SynthesisAlgorithm, SynthesisBuilder, SynthesisPrecision,
	UnitCircleResponse, WeissRatio,
};

/// A frozen binary64 two-dimensional matrix, indexed by row then column.
pub type Control = [[Complex64; 2]; 2];
/// Result of production synthesis, numerical evaluation or sequence admission.
pub type Result<T> = std::result::Result<T, Error>;

/// Production failures, distinct from optional certification and offline errors.
///
/// A failed sufficient bound does not establish mathematical impossibility.
/// No variant automatically dispatches to arbitrary-precision computation.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
	/// A binary64 numerical kernel rejected the operation.
	#[error(transparent)]
	Numerics(#[from] quest_numerics::Error),
	/// Polynomial construction or evaluation rejected the operation.
	#[error(transparent)]
	Polynomial(#[from] quest_polynomial::Error),
	/// Invalid tolerances or completion-grid configuration.
	#[error("invalid QSP policy: {0}")]
	Policy(&'static str),
	/// Unsupported support, parity, convention or evaluation domain.
	#[error("unsupported target: {0}")]
	Target(&'static str),
	/// A size, allocation or modeled resource limit was exceeded.
	#[error("QSP resource limit: {0}")]
	Budget(&'static str),
	/// A finite-only operation encountered a nonfinite value.
	#[error("nonfinite value in {0}")]
	NonFinite(&'static str),
	/// An outward sample lower bound proves the configured margin is violated.
	#[error(
		"contractivity margin violated: magnitude lower bound {lower} >= threshold {threshold}"
	)]
	ContractivityViolation {
		/// Outward lower witness at a particular signal.
		lower: f64,
		/// Maximum magnitude permitted by the requested margin.
		threshold: f64,
	},
	/// Admission could not establish the configured strict contractivity margin.
	#[error("contractivity with positive margin could not be established (upper bound {upper})")]
	Contractivity {
		/// Last computed outward magnitude upper bound.
		upper: f64,
	},
	/// A numerical diagnostic exceeded the requested stage tolerance.
	#[error("{stage} bound {bound} does not establish requested tolerance {tolerance}")]
	NotEstablished {
		/// Operation whose check failed.
		stage: &'static str,
		/// Computed diagnostic; this is not an independent certificate.
		bound: f64,
		/// Maximum accepted value for that check.
		tolerance: f64,
	},
	/// The inverse transform could not safely divide by its pivot.
	#[error("inverse NLFT encountered a singular pivot")]
	SingularPivot,
}

fn zeros(count: usize, limits: quest_numerics::Limits) -> Result<Vec<Complex64>> {
	let bytes = count
		.checked_mul(size_of::<Complex64>())
		.ok_or(Error::Budget("size overflow"))?;
	if bytes > limits.resources.max_peak_bytes {
		return Err(Error::Budget("coefficient allocation"));
	}
	let mut output = Vec::new();
	output
		.try_reserve_exact(count)
		.map_err(|_| Error::Budget("coefficient allocation"))?;
	output.resize(count, Complex64::new(0.0, 0.0));
	Ok(output)
}

const fn finite(value: Complex64, stage: &'static str) -> Result<Complex64> {
	if value.re.is_finite() && value.im.is_finite() {
		Ok(value)
	} else {
		Err(Error::NonFinite(stage))
	}
}

// Reserve concurrently retained scalar storage before admitting workspaces.
#[cfg(test)]
fn workspace_policy(mut policy: Policy, scalars: usize) -> Result<Policy> {
	let bytes = scalars
		.checked_mul(size_of::<Complex64>())
		.ok_or(Error::Budget("stage storage"))?;
	policy.limits.resources.max_peak_bytes = policy
		.limits
		.resources
		.max_peak_bytes
		.checked_sub(bytes)
		.ok_or(Error::Budget("stage storage"))?;
	Ok(policy)
}

/// Numerical acceptance gates, independent of shapes and resource capacity.
#[derive(Clone, Copy, Debug)]
pub struct AccuracyPolicy {
	pub response_tolerance: f64,
	pub contractivity_margin: f64,
}
impl Default for AccuracyPolicy {
	fn default() -> Self {
		Self {
			response_tolerance: 1e-11,
			contractivity_margin: 1e-12,
		}
	}
}
