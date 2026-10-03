//! Explicit generalized catalog acceptance. Requires the certification feature.
//! Every original Chebyshev family is converted with checked exact binary64 halves,
//! then synthesized through generalized controls and independently certified.
use quest_polynomial::{Laurent, Limits, Polynomial};
use quest_qsp::certification::{CertificationBuilder, CertificationPolicy};
use quest_qsp::{Complex64, SynthesisBuilder};
use quest_qsvt_io::{CatalogFamily, InverseCatalog, IoPolicy};
use std::{
	ops::{Add, Mul},
	time::Instant,
};

#[derive(serde::Serialize)]
struct SuccessReport<'a> {
	family: String,
	mode: &'a str,
	degree: Option<i32>,
	status: &'static str,
	completion_grid: usize,
	response_upper: f64,
	completion_upper: f64,
	conversion_upper: f64,
	reconstruction_upper: f64,
	unitarity_upper: f64,
	certification_attempts: usize,
	conversion_seconds: f64,
	admission_seconds: f64,
	completion_seconds: f64,
	synthesis_seconds: f64,
	certification_seconds: f64,
	total_seconds: f64,
}

#[derive(serde::Serialize)]
struct FailureReport<'a> {
	family: String,
	mode: &'a str,
	status: &'static str,
	stage: &'a str,
}

fn family_name(family: &CatalogFamily) -> String {
	format!(
		"kappa_{}_epsilon_{}",
		family.kappa(),
		family.epsilon_label()
	)
}
#[derive(Debug, thiserror::Error)]
#[error("{stage}: {source}")]
struct Failure {
	stage: &'static str,
	source: Box<dyn std::error::Error>,
}
impl Failure {
	fn new(stage: &'static str, error: impl Into<Box<dyn std::error::Error>>) -> Self {
		Self {
			stage,
			source: error.into(),
		}
	}
}
fn target(family: &CatalogFamily, complex: bool) -> Result<Polynomial<Laurent>, Failure> {
	let source = family.coefficients();
	let degree = source.iter().rposition(|value| *value != 0.0).unwrap_or(0);
	let count = degree
		.checked_add(1)
		.ok_or_else(|| Failure::new("source", "degree overflow"))?;
	let mut target = vec![Complex64::new(0.0, 0.0); count];
	for (index, value) in source.iter().enumerate() {
		if !value.is_finite() || (index % 2 != degree % 2 && *value != 0.0) {
			return Err(Failure::new(
				"source",
				"invalid original parity or coefficient",
			));
		}
		if *value == 0.0 {
			continue;
		}
		let half = value * 0.5;
		if (half * 2.0).to_bits() != value.to_bits() {
			return Err(Failure::new("conversion", "inexact binary64 halving"));
		}
		let high = degree
			.checked_add(index)
			.ok_or_else(|| Failure::new("conversion", "support overflow"))?
			/ 2;
		let low = degree
			.checked_sub(index)
			.ok_or_else(|| Failure::new("conversion", "support overflow"))?
			/ 2;
		for position in [high, low] {
			let entry = target
				.get_mut(position)
				.ok_or_else(|| Failure::new("conversion", "support"))?;
			*entry = entry.add(Complex64::new(half, 0.0));
		}
	}
	if complex {
		// This is a separately defined binary64 complex target. It is not an
		// assertion that rounded multiplication preserves an exact global phase.
		for value in &mut target {
			*value = value.mul(Complex64::new(0.6, 0.8));
		}
	}
	Polynomial::new(Laurent::new(0), target, Limits::default())
		.map_err(|error| Failure::new("source", error))
}
fn run(family: &CatalogFamily, complex: bool) -> Result<(), Failure> {
	let total = Instant::now();
	let target = target(family, complex)?;
	let converted = total.elapsed().as_secs_f64();
	let started = Instant::now();
	let admitted = SynthesisBuilder::new()
		.unit_circle_response(&target)
		.and_then(SynthesisBuilder::admit)
		.map_err(|error| Failure::new("admission", error))?;
	let admission = started.elapsed().as_secs_f64();
	let started = Instant::now();
	let completed = admitted
		.complete()
		.map_err(|error| Failure::new("completion", error))?;
	let grid = completed.completion_grid();
	let completion = started.elapsed().as_secs_f64();
	let started = Instant::now();
	let candidate = completed
		.synthesize()
		.map_err(|error| Failure::new("synthesis", error))?;
	let synthesis = started.elapsed().as_secs_f64();
	let bytes: Vec<_> = candidate
		.controls()
		.iter()
		.flatten()
		.flatten()
		.flat_map(|value| [value.re.to_bits(), value.im.to_bits()])
		.collect();
	let started = Instant::now();
	let certified = CertificationBuilder::new()
		.candidate(candidate)
		.policy(CertificationPolicy::default())
		.and_then(CertificationBuilder::certify)
		.map_err(|error| Failure::new("certification", error))?;
	let certification = started.elapsed().as_secs_f64();
	if !bytes.iter().copied().eq(certified
		.candidate()
		.controls()
		.iter()
		.flatten()
		.flatten()
		.flat_map(|value| [value.re.to_bits(), value.im.to_bits()]))
	{
		return Err(Failure::new(
			"immutability",
			"verifier changed exported bits",
		));
	}
	let report = certified.report();
	let mode = if complex {
		"complex_rotation_binary64"
	} else {
		"real_canonical_family"
	};
	let response_upper = report.response().upper_f64();
	let completion_upper = report.completion().upper_f64();
	let conversion_upper = report.conversion().upper_f64();
	let reconstruction_upper = report.reconstruction().upper_f64();
	let unitarity_upper = report.unitarity().upper_f64();
	if [
		response_upper,
		completion_upper,
		conversion_upper,
		reconstruction_upper,
		unitarity_upper,
	]
	.iter()
	.any(|value| !value.is_finite())
	{
		return Err(Failure::new(
			"report",
			"non-finite certification report bound",
		));
	}
	let output = SuccessReport {
		family: family_name(family),
		mode,
		degree: target.degree(),
		status: "certified",
		completion_grid: grid,
		response_upper,
		completion_upper,
		conversion_upper,
		reconstruction_upper,
		unitarity_upper,
		certification_attempts: report.attempts().len(),
		conversion_seconds: converted,
		admission_seconds: admission,
		completion_seconds: completion,
		synthesis_seconds: synthesis,
		certification_seconds: certification,
		total_seconds: total.elapsed().as_secs_f64(),
	};
	println!(
		"{}",
		serde_json::to_string(&output).map_err(|error| Failure::new("report", error))?
	);
	Ok(())
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
	let mut failures = 0_usize;
	let catalog = InverseCatalog::bundled(IoPolicy::default())?;
	for family in catalog.families() {
		if let Err(error) = run(family, false) {
			eprintln!("{}: {error}", family_name(family));
			println!(
				"{}",
				serde_json::to_string(&FailureReport {
					family: family_name(family),
					mode: "real_canonical_family",
					status: "failed",
					stage: error.stage,
				})?
			);
			failures = failures.saturating_add(1);
		}
	}
	let largest = catalog
		.families()
		.iter()
		.max_by_key(|family| family.coefficients().len())
		.ok_or("empty catalog")?;
	if let Err(error) = run(largest, true) {
		eprintln!("complex {}: {error}", family_name(largest));
		println!(
			"{}",
			serde_json::to_string(&FailureReport {
				family: family_name(largest),
				mode: "complex_rotation_binary64",
				status: "failed",
				stage: error.stage,
			})?
		);
		failures = failures.saturating_add(1);
	}
	if failures > 0 {
		return Err(format!("{failures} generalized catalog cases failed without fallback").into());
	}
	Ok(())
}

#[cfg(test)]
mod tests {
	use super::{FailureReport, SuccessReport};
	use googletest::prelude::*;

	#[gtest]
	fn failure_report_preserves_escaped_labels() -> googletest::Result<()> {
		let report = FailureReport {
			family: "family \"quoted\"\n😀".to_owned(),
			mode: "real_canonical_family",
			status: "failed",
			stage: "stage\nwith\\escape",
		};
		let value: serde_json::Value =
			serde_json::from_str(&serde_json::to_string(&report).or_fail()?).or_fail()?;
		expect_that!(
			value.get("family").and_then(serde_json::Value::as_str),
			some(eq(report.family.as_str()))
		);
		verify_that!(
			value.get("stage").and_then(serde_json::Value::as_str),
			some(eq(report.stage))
		)
	}

	#[gtest]
	fn success_report_retains_numeric_bounds_and_nullable_degree() -> googletest::Result<()> {
		let bound = f64::from_bits(0x3cb0_0000_0000_0001);
		let report = SuccessReport {
			family: "family".to_owned(),
			mode: "real_canonical_family",
			degree: None,
			status: "certified",
			completion_grid: 8,
			response_upper: bound,
			completion_upper: bound,
			conversion_upper: bound,
			reconstruction_upper: bound,
			unitarity_upper: bound,
			certification_attempts: 1,
			conversion_seconds: 0.0,
			admission_seconds: 0.0,
			completion_seconds: 0.0,
			synthesis_seconds: 0.0,
			certification_seconds: 0.0,
			total_seconds: 0.0,
		};
		let value: serde_json::Value =
			serde_json::from_str(&serde_json::to_string(&report).or_fail()?).or_fail()?;
		expect_that!(value.get("degree"), some(eq(&serde_json::Value::Null)));
		verify_that!(
			value
				.get("response_upper")
				.and_then(serde_json::Value::as_f64)
				.or_fail()?
				.to_bits(),
			eq(bound.to_bits())
		)
	}
}
