//! Compiled artifacts preserve exact payload bytes and freshly recomputed evidence.
use crate::{IoPolicy, Result};
use quest_qsp::artifact::{
	ArtifactLimits, LoadPolicy, LoadedCertified, export_certified, load_certified,
};
use quest_qsp::certification::CertificationPolicy;
use std::sync::Arc;
/// Immutable compiled payload and independent certification of those exact values.
#[derive(Debug, Clone)]
pub struct CompiledInput {
	json: Arc<String>,
	certified: Arc<LoadedCertified>,
}
impl CompiledInput {
	/// Preserve newly constructed evidence and export its historical receipt.
	/// # Errors
	/// Rejects artifact storage or serialization limits.
	pub fn from_certified(certified: LoadedCertified, limits: ArtifactLimits) -> Result<Self> {
		let bytes = match &certified {
			LoadedCertified::RealParityWx(c) => export_certified(c, limits)?,
			LoadedCertified::UnitCircleResponse(c) => export_certified(c, limits)?,
		};
		let json =
			String::from_utf8(bytes).map_err(|_| crate::Error::Format("artifact JSON encoding"))?;
		Ok(Self {
			json: Arc::new(json),
			certified: Arc::new(certified),
		})
	}
	#[must_use]
	pub fn json(&self) -> &str {
		&self.json
	}
	#[must_use]
	pub fn certified(&self) -> &LoadedCertified {
		&self.certified
	}
	#[must_use]
	pub fn into_certified(self) -> LoadedCertified {
		Arc::unwrap_or_clone(self.certified)
	}
}
/// Load a compiled JSON artifact and independently recertify its actual payload.
/// The caller chooses verification tolerances; historical receipts are not trusted.
/// # Errors
/// Rejects structural, resource, contractivity, or independent certification failures.
pub fn read_compiled_qsp_json(
	source: &str,
	io: IoPolicy,
	mut verification: CertificationPolicy,
) -> Result<CompiledInput> {
	verification.max_bytes = verification.max_bytes.min(io.max_bytes);
	verification.max_coefficients = verification.max_coefficients.min(io.max_coefficients);
	let mut load = LoadPolicy {
		storage: ArtifactLimits {
			max_bytes: io.max_bytes,
			max_decoded_bytes: io.max_bytes,
			max_coefficients: io.max_coefficients,
		},
		..LoadPolicy::default()
	};
	load.admission.limits.max_bytes = load.admission.limits.max_bytes.min(io.max_bytes);
	load.admission.limits.max_len = load.admission.limits.max_len.min(io.max_coefficients);
	let certified = load_certified(source.as_bytes(), load, verification)?;
	Ok(CompiledInput {
		json: Arc::new(source.to_owned()),
		certified: Arc::new(certified),
	})
}
