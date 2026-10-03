#![forbid(unsafe_code)]
//! Bounded Clifford+T synthesis, derived independently from published mathematics.
mod approximation;
mod exact;
mod grid;
mod lowering;
mod normal;
pub use approximation::{Approximation, approximate_rotation};
pub use exact::synthesize_matrix;
pub use normal::{NormalForm, Syllable, normalize_one_qubit};
use quest_math::{Limits, Sequence, SynthesisCertificate, SynthesisProof};
use std::sync::{
	Arc,
	atomic::{AtomicBool, Ordering},
};

/// Stable identity of the bounded rotation candidate engine, shared with
/// compiler and worker provenance.
///
/// This identifies the algorithm, not a proof
/// of completeness or optimality of a resource-bounded search.
pub const ROTATION_ALGORITHM: &str = "ross-selinger-lll-prime-norm-v1";

/// A request-owned cancellation flag; it does not affect deterministic search
/// choices when left unset. A cancelled request never yields an impossibility.
#[derive(Debug, Clone, Default)]
pub struct CancellationToken(Arc<AtomicBool>);
impl CancellationToken {
	pub fn cancel(&self) {
		self.0.store(true, Ordering::Relaxed);
	}
	#[must_use]
	pub fn is_cancelled(&self) -> bool {
		self.0.load(Ordering::Relaxed)
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AncillaPolicy {
	#[default]
	AllowOneClean,
	NoAncilla,
}
#[derive(Debug, Clone)]
pub struct SynthesisOptions {
	pub limits: Limits,
	pub max_work: u64,
	pub max_proof_steps: usize,
	pub ancilla_policy: AncillaPolicy,
	pub seed: u64,
	/// Maximum sqrt(2) denominator exponent. Grid enumeration visits the even
	/// exponents; canonical ring denominators remain powers of two.
	pub max_grid_exponent: u32,
	pub cancellation: Option<CancellationToken>,
}
impl Default for SynthesisOptions {
	fn default() -> Self {
		Self {
			limits: Limits::default(),
			max_work: 10_000_000,
			max_proof_steps: 100_000,
			ancilla_policy: AncillaPolicy::AllowOneClean,
			seed: 0,
			max_grid_exponent: 128,
			cancellation: None,
		}
	}
}
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SynthesisError {
	#[error("synthesis resource {resource} exhausted")]
	Budget { resource: &'static str },
	#[error("input matrix is not unitary")]
	NotUnitary,
	#[error(
		"determinant omega^{determinant_power} requires a clean ancilla on {qubits} data qubits"
	)]
	AncillaRequired {
		qubits: usize,
		determinant_power: u8,
	},
	#[error("logical synthesis work exhausted: {used} exceeds {limit}")]
	WorkExhausted { used: u64, limit: u64 },
	#[error("synthesis cancelled")]
	Cancelled,
	#[error("request precision {precision_bits} does not resolve the required interval decision")]
	PrecisionUnresolved { precision_bits: usize },
	#[error("independent synthesis certificate rejected: {0}")]
	CertificateRejected(quest_math::Error),
	#[error("invalid synthesis request: {0}")]
	Invalid(&'static str),
	#[error("exact arithmetic or independent certificate failed: {0}")]
	Math(#[from] quest_math::Error),
}
pub type Result<T> = std::result::Result<T, SynthesisError>;
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExactSynthesis {
	certificate: SynthesisCertificate,
	work: u64,
}
impl ExactSynthesis {
	#[must_use]
	pub const fn sequence(&self) -> &Sequence {
		self.certificate.candidate()
	}
	#[must_use]
	pub const fn proof(&self) -> &SynthesisProof {
		self.certificate.proof()
	}
	#[must_use]
	pub const fn certificate(&self) -> &SynthesisCertificate {
		&self.certificate
	}
	#[must_use]
	pub const fn work(&self) -> u64 {
		self.work
	}
}
struct Budget {
	options: SynthesisOptions,
	used: u64,
}
impl Budget {
	fn with_reserved_bytes<T>(
		&mut self,
		bytes: usize,
		operation: impl FnOnce(&mut Self) -> Result<T>,
	) -> Result<T> {
		let original = self.options.limits.bytes;
		self.options.limits.bytes = original.checked_sub(bytes).ok_or(SynthesisError::Budget {
			resource: "live synthesis bytes",
		})?;
		let result = operation(self);
		self.options.limits.bytes = original;
		result
	}
	// Conservative logical units for independent scalar interval reconstruction:
	// fixed 2x2 arithmetic plus bounded Taylor and precision-grid iterations.
	fn rotation_certificate(&mut self, gates: usize, attempts: usize) -> Result<()> {
		let work = self
			.options
			.limits
			.taylor_terms
			.checked_add(self.options.limits.precision_bits)
			.and_then(|n| n.checked_mul(128))
			.and_then(|n| n.checked_mul(attempts))
			.and_then(|n| gates.checked_mul(64).and_then(|g| n.checked_add(g)))
			.ok_or(SynthesisError::Budget {
				resource: "certificate work",
			})?;
		self.charge(work)
	}
	fn charge(&mut self, count: usize) -> Result<()> {
		if self
			.options
			.cancellation
			.as_ref()
			.is_some_and(CancellationToken::is_cancelled)
		{
			return Err(SynthesisError::Cancelled);
		}
		self.used = self
			.used
			.checked_add(
				u64::try_from(count).map_err(|_| SynthesisError::Budget { resource: "work" })?,
			)
			.ok_or(SynthesisError::Budget { resource: "work" })?;
		if self.used > self.options.max_work {
			return Err(SynthesisError::WorkExhausted {
				used: self.used,
				limit: self.options.max_work,
			});
		}
		Ok(())
	}
	fn output(&mut self, current: usize, count: usize) -> Result<()> {
		self.charge(count)?;
		if current
			.checked_add(count)
			.is_none_or(|n| n > self.options.limits.gates)
		{
			return Err(SynthesisError::Budget {
				resource: "output gates",
			});
		}
		let bytes = current
			.checked_add(count)
			.and_then(|n| n.checked_mul(1024))
			.ok_or(SynthesisError::Budget {
				resource: "output bytes",
			})?;
		if bytes > self.options.limits.bytes {
			return Err(SynthesisError::Budget {
				resource: "output bytes",
			});
		}
		Ok(())
	}
}

mod norm_equation;

#[cfg(test)]
mod budget_tests {
	use super::*;
	#[test]
	fn byte_reservation_restores_limits_on_success_and_failure() {
		let mut budget = Budget {
			options: SynthesisOptions::default(),
			used: 0,
		};
		let original = budget.options.limits;
		let result = budget.with_reserved_bytes(original.bytes, |remaining| {
			assert_eq!(remaining.options.limits.bytes, 0);
			remaining.charge(1)?;
			Ok(17)
		});
		assert_eq!(result, Ok(17));
		assert_eq!(budget.options.limits, original);
		let result = budget.with_reserved_bytes(original.bytes, |remaining| {
			assert_eq!(remaining.options.limits.bytes, 0);
			remaining.charge(1)?;
			Err::<(), _>(SynthesisError::Cancelled)
		});
		assert_eq!(result, Err(SynthesisError::Cancelled));
		assert_eq!(budget.options.limits, original);
		assert_eq!(budget.used, 2);
	}
}
