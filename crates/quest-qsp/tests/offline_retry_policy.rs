#![cfg(feature = "offline-synthesis")]
use googletest::prelude::*;
use quest_polynomial::{Laurent, Limits, Polynomial};
use quest_qsp::offline::{OfflineBuilder, OfflineError, OfflinePolicy};
use quest_qsp::{Complex64, certification::CertificationError};

#[gtest]
fn fixed_verifier_work_failure_stops_after_the_first_export_and_retains_source() -> Result<()> {
	let target = Polynomial::new(
		Laurent::new(0),
		vec![Complex64::new(0.3, 0.4)],
		Limits::default(),
	)?;
	let mut policy = OfflinePolicy::default();
	policy.certification.max_work = 1;
	let result = OfflineBuilder::new()
		.unit_circle_response(&target)?
		.policy(policy)?
		.solve();
	let Err(OfflineError::Certification { report, source }) = result else {
		return fail!("expected the immutable verifier budget failure with an offline report");
	};
	expect_true!(matches!(*source, CertificationError::Budget(_)));
	expect_eq!(report.attempts().len(), 1);
	expect_eq!(report.source_coefficients(), target.coefficients());
	expect_true!(report.last_export().is_some());
	Ok(())
}
