//! Optional candidate workers with bounded processes and independent admission.
#![forbid(unsafe_code)]
use quest_math::{ApproxCertificate, ExactCertificate, Limits, Sequence, Target};
use quest_optimizer_protocol::{
    MitmLimits, Outcome, Request, RequestEnvelope, ResponseEnvelope, VERSION,
};
use std::{path::PathBuf, time::Duration};

#[cfg(target_os = "linux")]
mod process;

/// Hard ceilings are thirty seconds, 512 MiB address space and 64 KiB per stream.
#[derive(Debug, Clone, Copy)]
pub struct WorkerLimits {
    pub wall_time: Duration,
    pub memory_bytes: usize,
    pub output_bytes: usize,
}
impl Default for WorkerLimits {
    fn default() -> Self {
        Self {
            wall_time: Duration::from_secs(30),
            memory_bytes: 536_870_912,
            output_bytes: 65_536,
        }
    }
}
/// A failed optional candidate must leave the original region intact.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("bounded optimizer processes are unavailable: {0}")]
    Capability(&'static str),
    #[error("invalid worker limits")]
    Limits,
    #[error("optimizer process I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("optimizer protocol: {0}")]
    Wire(#[from] quest_optimizer_protocol::WireError),
    #[error("optimizer wall-time limit exceeded")]
    Timeout,
    #[error("optimizer output limit exceeded")]
    OutputLimit,
    #[error("optimizer failed ({status}): {stderr}")]
    Failed { status: String, stderr: String },
    #[error("optimizer response version or seed does not match request")]
    Envelope,
    #[error("optimizer declined candidate: {code}: {message}")]
    Candidate { code: String, message: String },
    #[error("optimizer returned an unexpected response")]
    Unexpected,
    #[error("independent candidate verification failed: {0}")]
    Verification(#[from] quest_math::Error),
}
/// An optional MITM search result. Only `Candidate` contains a parent-owned proof.
#[derive(Debug)]
pub enum MitmResult<T> {
    Candidate(T),
    NoCandidate {
        explored: u64,
    },
    Incomplete {
        reason: String,
        explored: u64,
    },
    Exhausted {
        explored: u64,
    },
    Unresolved {
        precision_bits: usize,
        explored: u64,
    },
}
/// Explicit executable path; no search of untrusted working directories.
#[derive(Debug, Clone)]
pub struct Client {
    #[cfg(target_os = "linux")]
    executable: PathBuf,
    limits: WorkerLimits,
}
impl Client {
    /// # Errors
    /// Rejects limits above the supported ceilings or an unsupported platform.
    pub fn new(executable: impl Into<PathBuf>, limits: WorkerLimits) -> Result<Self, Error> {
        if limits.wall_time.is_zero()
            || limits.wall_time > Duration::from_secs(30)
            || limits.memory_bytes == 0
            || limits.memory_bytes > 536_870_912
            || limits.output_bytes == 0
            || limits.output_bytes > 65_536
        {
            return Err(Error::Limits);
        }
        #[cfg(not(target_os = "linux"))]
        {
            drop(executable);
            Err(Error::Capability("Linux prlimit enforcement required"))
        }
        #[cfg(target_os = "linux")]
        {
            Ok(Self {
                executable: executable.into(),
                limits,
            })
        }
    }
    /// Obtain an untrusted response. Use `synthesize`/`optimize_zx` for admission.
    /// # Errors
    /// Rejects unavailable enforcement, failed processes and malformed output.
    pub fn request(&self, request: Request, seed: u64) -> Result<Outcome, Error> {
        let request = RequestEnvelope {
            version: VERSION,
            seed,
            request,
        };
        let bytes = quest_optimizer_protocol::encode(
            &request,
            quest_optimizer_protocol::MAX_REQUEST_BYTES,
        )?;
        let output = self.run(bytes)?;
        let response: ResponseEnvelope =
            quest_optimizer_protocol::decode(&output, self.limits.output_bytes)?;
        if response.version != VERSION || response.seed != seed {
            return Err(Error::Envelope);
        }
        Ok(response.outcome)
    }
    #[cfg(not(target_os = "linux"))]
    #[allow(clippy::unused_self)] // Keep the same receiver as the Linux implementation.
    fn run(&self, _bytes: Vec<u8>) -> Result<Vec<u8>, Error> {
        Err(Error::Capability("Linux prlimit enforcement required"))
    }
    /// Explicit synthesis returns a certificate or an error, never a partial circuit.
    /// # Errors
    /// Rejects worker failures and candidates lacking a full-phase accuracy proof.
    pub fn synthesize(
        &self,
        target: &Target,
        epsilon: f64,
        seed: u64,
        limits: Limits,
    ) -> Result<ApproxCertificate, Error> {
        // Parent validates inputs independently before starting an expensive process.
        quest_math::dyadic_from_bits(epsilon.to_bits(), limits)?;
        if !epsilon.is_finite() || epsilon <= 0.0 || epsilon >= 1.0 {
            return Err(Error::Limits);
        }
        match &target.angle {
            quest_math::AngleTarget::DyadicRadians { bits } => {
                quest_math::dyadic_from_bits(*bits, limits)?;
            }
            quest_math::AngleTarget::RationalPi {
                numerator,
                denominator,
            } => {
                let bits = limits.coefficient_bits.min(16_384);
                if denominator.bits() == 0 || numerator.bits() > bits || denominator.bits() > bits {
                    return Err(Error::Limits);
                }
            }
            quest_math::AngleTarget::AffinePi { .. } => {
                quest_math::admit_rotation_target(target, limits).map_err(|_| Error::Limits)?;
            }
        }
        let sequence = candidate(self.request(
            Request::Synthesize {
                target: target.clone(),
                epsilon_bits: epsilon.to_bits(),
            },
            seed,
        )?)?;
        Ok(quest_math::certify_rotation(
            &sequence,
            target,
            epsilon.to_bits(),
            limits,
        )?)
    }
    /// Exactly admit a ZX candidate, restoring a missing eighth-root phase if proved.
    /// # Errors
    /// Rejects worker failure, interface changes and failure of exact equality.
    pub fn optimize_zx(
        &self,
        original: &Sequence,
        seed: u64,
        limits: Limits,
    ) -> Result<ExactCertificate, Error> {
        quest_math::reconstruct(original, limits)?;
        let sequence = candidate(self.request(
            Request::ZxBest {
                sequence: original.clone(),
            },
            seed,
        )?)?;
        let recovery = quest_math::recover_eighth_root_phase(&sequence, original, limits)?;
        Ok(recovery.certificate().clone())
    }
    /// Admit the baseline flow-simplification candidate independently of the
    /// expanded ZX rules, so cost-based search can compare both variants.
    /// # Errors
    /// Rejects worker failure, interface changes and failure of exact equality.
    pub fn optimize_zx_baseline(
        &self,
        original: &Sequence,
        seed: u64,
        limits: Limits,
    ) -> Result<ExactCertificate, Error> {
        quest_math::reconstruct(original, limits)?;
        let sequence = candidate(self.request(
            Request::Zx {
                sequence: original.clone(),
            },
            seed,
        )?)?;
        let recovery = quest_math::recover_eighth_root_phase(&sequence, original, limits)?;
        Ok(recovery.certificate().clone())
    }
    /// Admit the independent expanded ZX worker candidate, preserving full
    /// scalar phase even if its operation count grows.
    /// # Errors
    /// Rejects worker failure, interface changes and failure of exact equality.
    pub fn optimize_zx_expanded(
        &self,
        original: &Sequence,
        seed: u64,
        limits: Limits,
    ) -> Result<ExactCertificate, Error> {
        quest_math::reconstruct(original, limits)?;
        let sequence = candidate(self.request(
            Request::ZxExpanded {
                sequence: original.clone(),
            },
            seed,
        )?)?;
        let recovery = quest_math::recover_eighth_root_phase(&sequence, original, limits)?;
        Ok(recovery.certificate().clone())
    }
    /// Search one- or two-qubit full-phase exact identities. A returned candidate
    /// is reconstructed and compared independently in the parent process.
    /// # Errors
    /// Rejects invalid input, process/protocol failure or a forged candidate.
    pub fn exact_mitm(
        &self,
        original: &Sequence,
        seed: u64,
        search_limits: MitmLimits,
        proof_limits: Limits,
    ) -> Result<MitmResult<ExactCertificate>, Error> {
        search_limits
            .validate(original.qubits)
            .map_err(|_| Error::Limits)?;
        quest_math::reconstruct(original, proof_limits)?;
        match self.request(
            Request::ExactMitm {
                target: original.clone(),
                limits: search_limits,
            },
            seed,
        )? {
            Outcome::Candidate { sequence, .. } => Ok(MitmResult::Candidate(
                quest_math::verify_exact(&sequence, original, proof_limits)?,
            )),
            other => map_mitm_terminal(other),
        }
    }
    /// Search a one-qubit rotation using exact matrix halves and outward interval
    /// pruning. A candidate is certified against the original target here.
    /// # Errors
    /// Rejects invalid input, process/protocol failure or a forged candidate.
    pub fn approx_mitm(
        &self,
        target: &Target,
        epsilon_bits: u64,
        seed: u64,
        search_limits: MitmLimits,
        proof_limits: Limits,
    ) -> Result<MitmResult<ApproxCertificate>, Error> {
        search_limits.validate(1).map_err(|_| Error::Limits)?;
        let epsilon = quest_math::dyadic_from_bits(epsilon_bits, proof_limits)?;
        if epsilon <= quest_math::Rational::from_integer(0.into())
            || epsilon >= quest_math::Rational::from_integer(1.into())
        {
            return Err(Error::Limits);
        }
        quest_math::admit_rotation_target(target, proof_limits)?;
        match self.request(
            Request::ApproxMitm {
                target: target.clone(),
                epsilon_bits,
                limits: search_limits,
            },
            seed,
        )? {
            Outcome::Candidate { sequence, .. } => Ok(MitmResult::Candidate(
                quest_math::certify_rotation(&sequence, target, epsilon_bits, proof_limits)?,
            )),
            other => map_mitm_terminal(other),
        }
    }
}
fn map_mitm_terminal<T>(outcome: Outcome) -> Result<MitmResult<T>, Error> {
    match outcome {
        Outcome::NoCandidate { explored } => Ok(MitmResult::NoCandidate { explored }),
        Outcome::Incomplete { reason, explored } => Ok(MitmResult::Incomplete { reason, explored }),
        Outcome::Exhausted { explored } => Ok(MitmResult::Exhausted { explored }),
        Outcome::Unresolved {
            precision_bits,
            explored,
        } => Ok(MitmResult::Unresolved {
            precision_bits,
            explored,
        }),
        Outcome::Failure { code, message } => Err(Error::Candidate { code, message }),
        Outcome::Candidate { .. } | Outcome::Capabilities { .. } => Err(Error::Unexpected),
    }
}
fn candidate(outcome: Outcome) -> Result<Sequence, Error> {
    match outcome {
        Outcome::Candidate { sequence, .. } => Ok(sequence),
        Outcome::Failure { code, message } => Err(Error::Candidate { code, message }),
        Outcome::Capabilities { .. }
        | Outcome::NoCandidate { .. }
        | Outcome::Incomplete { .. }
        | Outcome::Exhausted { .. }
        | Outcome::Unresolved { .. } => Err(Error::Unexpected),
    }
}
