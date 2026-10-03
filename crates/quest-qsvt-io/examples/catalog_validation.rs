//! Emit one measured JSON result per catalog family, retaining unsuccessful cases.
use quest_qsp::{FrozenCandidate, RealParityWx, SynthesisBuilder};
use quest_qsvt_io::{IoPolicy, catalog_families};
use std::time::Instant;

fn main() -> Result<(), Box<dyn std::error::Error>> {
	let certify = std::env::args().any(|arg| arg == "--certify");
	if certify && !cfg!(feature = "certification") {
		return Err("--certify requires the certification feature".into());
	}
	for family in catalog_families() {
		let started = Instant::now();
		let polynomial = family.polynomial(IoPolicy::default())?;
		let mut admission_seconds = 0.0;
		let mut completion_seconds = 0.0;
		let mut inverse_seconds = 0.0;
		let result = (|| -> quest_qsp::Result<FrozenCandidate<RealParityWx>> {
			let begin = Instant::now();
			let admitted = SynthesisBuilder::new()
				.real_parity_wx(&polynomial)?
				.admit()?;
			admission_seconds = begin.elapsed().as_secs_f64();
			let begin = Instant::now();
			let completed = admitted.complete()?;
			completion_seconds = begin.elapsed().as_secs_f64();
			let begin = Instant::now();
			let frozen = completed.synthesize()?;
			inverse_seconds = begin.elapsed().as_secs_f64();
			Ok(frozen)
		})();
		let status = match result {
			Ok(candidate) => {
				let mut result = serde_json::json!({"production":"success","completion_residual":candidate.completion_residual(),"response_residual":candidate.reconstruction_residual(),"completion_grid":candidate.completion_grid()});
				if certify {
					result
						.as_object_mut()
						.ok_or("catalog report must be an object")?
						.insert(String::from("certification"), certification(candidate));
				}
				result
			}
			Err(error) => {
				serde_json::json!({"production":"unestablished","error":error.to_string()})
			}
		};
		println!(
			"{}",
			serde_json::json!({"kappa":family.kappa(),"epsilon_label":family.epsilon_label(),"degree":family.degree(),"response_tolerance":1e-11,"admission_seconds":admission_seconds,"completion_seconds":completion_seconds,"inverse_seconds":inverse_seconds,"total_seconds":started.elapsed().as_secs_f64(),"result":status})
		);
	}
	Ok(())
}
#[cfg(feature = "certification")]
fn certification(candidate: FrozenCandidate<RealParityWx>) -> serde_json::Value {
	use quest_qsp::certification::{CertificationBuilder, CertificationError, CertificationPolicy};
	let begin = Instant::now();
	let result = CertificationBuilder::new()
		.candidate(candidate)
		.policy(CertificationPolicy::default())
		.and_then(CertificationBuilder::certify);
	match result {
		Ok(certified) => {
			serde_json::json!({"status":"established","seconds":begin.elapsed().as_secs_f64(),"bounds":bounds(certified.report())})
		}
		Err(error) => {
			let report = match &error {
				CertificationError::NotEstablished { report }
				| CertificationError::Violation { report, .. } => bounds(report),
				_ => serde_json::Value::Null,
			};
			let status = if matches!(error, CertificationError::Violation { .. }) {
				"violation"
			} else {
				"unestablished"
			};
			serde_json::json!({"status":status,"seconds":begin.elapsed().as_secs_f64(),"error":error.to_string(),"bounds":report})
		}
	}
}
#[cfg(not(feature = "certification"))]
fn certification(_candidate: FrozenCandidate<RealParityWx>) -> serde_json::Value {
	serde_json::Value::Null
}

#[cfg(feature = "certification")]
fn bounds(report: &quest_qsp::certification::CertificationReport) -> serde_json::Value {
	serde_json::json!({
		"response":report.response().upper_f64(),
		"completion":report.completion().upper_f64(),
		"conversion":report.conversion().upper_f64(),
		"reconstruction":report.reconstruction().upper_f64(),
		"unitarity":report.unitarity().upper_f64(),
		"attempts":report.attempts().iter().map(|a| serde_json::json!({
			"precision":a.precision(), "seconds":a.elapsed().as_secs_f64(),
			"work_units":a.work_units(), "modeled_peak_bytes":a.modeled_peak_bytes()
		})).collect::<Vec<_>>()
	})
}
