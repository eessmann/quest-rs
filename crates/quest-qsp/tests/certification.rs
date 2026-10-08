#![cfg(feature = "certification")]
use googletest::prelude::*;
use quest_polynomial::{Chebyshev, Laurent, Limits, Polynomial};
use quest_qsp::certification::{CertificationBuilder, CertificationPolicy, ConvolutionMethod};
use quest_qsp::{Complex64, SynthesisBuilder};
#[gtest]
fn frozen_complex_export_is_checked_independently_with_both_convolutions() -> Result<()> {
	let target = Polynomial::new(
		Laurent::new(0),
		vec![Complex64::new(0.1, 0.2), Complex64::new(-0.3, 0.1)],
		Limits::default(),
	)?;
	let candidate = SynthesisBuilder::new()
		.unit_circle_response(&target)?
		.admit()?
		.complete()?
		.synthesize()?;
	let original = candidate.controls().to_vec();
	let direct = CertificationBuilder::new()
		.candidate(candidate.clone())
		.policy(CertificationPolicy {
			method: ConvolutionMethod::Direct,
			..CertificationPolicy::default()
		})?
		.certify()?;
	let tree = CertificationBuilder::new()
		.candidate(candidate)
		.policy(CertificationPolicy::default())?
		.certify()?;
	expect_that!(direct.report().response().upper_f64(), le(1e-11));
	expect_that!(tree.report().response().upper_f64(), le(1e-11));
	expect_that!(tree.report().reconstruction().upper_f64(), le(1e-11));
	expect_that!(tree.report().unitarity().upper_f64(), le(1e-11));
	expect_that!(tree.candidate().controls(), eq(original.as_slice()));
	expect_that!(tree.report().coefficients().len(), eq(2));
	Ok(())
}
#[gtest]
fn wx_certification_reconstructs_exported_phases_and_source_conversion() -> Result<()> {
	let target = Polynomial::new(
		Chebyshev,
		vec![Complex64::new(0.0, 0.0), Complex64::new(0.6, 0.0)],
		Limits::default(),
	)?;
	let candidate = SynthesisBuilder::new()
		.real_parity_wx(&target)?
		.admit()?
		.complete()?
		.synthesize()?;
	let phases = candidate.phases().to_vec();
	let certified = CertificationBuilder::new()
		.candidate(candidate)
		.policy(CertificationPolicy::default())?
		.certify()?;
	expect_that!(certified.report().conversion().upper_f64(), eq(0.0));
	expect_that!(certified.report().response().upper_f64(), le(1e-11));
	expect_that!(certified.candidate().phases(), eq(phases.as_slice()));
	Ok(())
}
#[gtest]
fn near_contractivity_boundary_is_verified_without_normalizing_export() -> Result<()> {
	let value = 1.0 - 2.0_f64.powi(-40);
	let target = Polynomial::new(
		Laurent::new(0),
		vec![Complex64::new(value, 0.0)],
		Limits::default(),
	)?;
	let candidate = SynthesisBuilder::new()
		.policy(quest_qsp::Policy {
			accuracy: quest_qsp::AccuracyPolicy {
				contractivity_margin: 1e-14,
				..(quest_qsp::Policy::default()).accuracy
			},
			..quest_qsp::Policy::default()
		})
		.unit_circle_response(&target)?
		.admit()?
		.complete()?
		.synthesize()?;
	let controls = candidate.controls().to_vec();
	let certified = CertificationBuilder::new()
		.candidate(candidate)
		.policy(CertificationPolicy::default())?
		.certify()?;
	expect_that!(certified.report().response().upper_f64(), le(1e-11));
	expect_that!(certified.report().completion().upper_f64(), le(1e-11));
	expect_that!(certified.candidate().controls(), eq(controls.as_slice()));
	Ok(())
}
#[gtest]
fn dense_complex_product_tree_overlaps_direct_all_four_coefficients() -> Result<()> {
	let coefficients = (0_i32..17)
		.map(|index| {
			Complex64::new(
				if index % 2 == 0 {
					1.0 / 64.0
				} else {
					-1.0 / 64.0
				},
				f64::from(index.rem_euclid(3).saturating_sub(1)) / 128.0,
			)
		})
		.collect();
	let target = Polynomial::new(Laurent::new(0), coefficients, Limits::default())?;
	let candidate = SynthesisBuilder::new()
		.unit_circle_response(&target)?
		.admit()?
		.complete()?
		.synthesize()?;
	let direct = CertificationBuilder::new()
		.candidate(candidate.clone())
		.policy(CertificationPolicy {
			method: ConvolutionMethod::Direct,
			..CertificationPolicy::default()
		})?
		.certify()?;
	let tree = CertificationBuilder::new()
		.candidate(candidate)
		.policy(CertificationPolicy::default())?
		.certify()?;
	for (direct, tree) in direct
		.report()
		.coefficients()
		.iter()
		.zip(tree.report().coefficients())
	{
		for (direct, tree) in direct.iter().flatten().zip(tree.iter().flatten()) {
			for (direct, tree) in [
				(direct.real(), tree.real()),
				(direct.imaginary(), tree.imaginary()),
			] {
				expect_that!(direct.lower(), le(tree.upper()));
				expect_that!(direct.upper(), ge(tree.lower()));
			}
		}
	}
	expect_that!(tree.report().reconstruction().upper_f64(), le(1e-11));
	Ok(())
}
#[gtest]
fn higher_degree_symmetric_phases_certify_the_real_parity_wx_response() -> Result<()> {
	let target = Polynomial::new(
		Chebyshev,
		vec![
			Complex64::new(0.0, 0.0),
			Complex64::new(0.125, 0.0),
			Complex64::new(0.0, 0.0),
			Complex64::new(-0.0625, 0.0),
			Complex64::new(0.0, 0.0),
			Complex64::new(0.03125, 0.0),
		],
		Limits::default(),
	)?;
	let candidate = SynthesisBuilder::new()
		.real_parity_wx(&target)?
		.admit()?
		.complete()?
		.synthesize()?;
	let certified = CertificationBuilder::new()
		.candidate(candidate)
		.policy(CertificationPolicy::default())?
		.certify()?;
	expect_that!(certified.report().response().upper_f64(), le(1e-11));
	for x in [-1.0, -0.8, -0.2, 0.0, 0.3, 0.8, 1.0] {
		expect_that!(
			certified.candidate().response(x)?,
			near(target.evaluate_real(x)?, 1e-11)
		);
	}
	Ok(())
}
#[gtest]
fn cancellation_heavy_complex_degree64_direct_and_fft_enclosures_overlap() -> Result<()> {
	use dashu_base::Abs;
	use dashu_float::{Context, round::mode::HalfEven};
	use quest_qsp::precision::Binary;
	use quest_qsp::precision::{BinaryRounding, checked, exact_from_f64, to_f64};
	let mut binomial = 1_u128;
	let denominator = 1_u128
		.checked_shl(64)
		.ok_or_else(|| std::io::Error::other("binomial scale"))?;
	let mut coefficients = Vec::with_capacity(65);
	for k in 0_u32..=64 {
		if k > 0 {
			binomial = binomial
				.checked_mul(u128::from(
					65_u32
						.checked_sub(k)
						.ok_or_else(|| std::io::Error::other("binomial index"))?,
				))
				.and_then(|v| v.checked_div(u128::from(k)))
				.ok_or_else(|| std::io::Error::other("binomial coefficient"))?;
		}
		let numerator = Binary::from(binomial).with_precision(256).value();
		let divisor = Binary::from(denominator).with_precision(256).value();
		let fraction = checked(
			Context::<HalfEven>::new(256)
				.div(numerator.repr(), divisor.repr())?
				.value(),
		)?;
		let value = to_f64(&fraction, BinaryRounding::Nearest)?;
		let signed = if k % 2 == 0 { value } else { -value };
		coefficients.push(Complex64::new(0.5 * signed, 0.25 * signed));
	}
	// Binary64 coefficient snapshot of (0.5+0.25i)*(1-z)^64/2^64: large coefficient
	// cancellation at z=1 and dense nontrivial complex controls.
	let target = Polynomial::new(Laurent::new(0), coefficients, Limits::default())?;
	let candidate = SynthesisBuilder::new()
		.unit_circle_response(&target)?
		.admit()?
		.complete()?
		.synthesize()?;
	let direct = CertificationBuilder::new()
		.candidate(candidate.clone())
		.policy(CertificationPolicy {
			method: ConvolutionMethod::Direct,
			..CertificationPolicy::default()
		})?
		.certify()?;
	let tree = CertificationBuilder::new()
		.candidate(candidate)
		.policy(CertificationPolicy::default())?
		.certify()?;
	let mut exact_sum = checked(Binary::ZERO.with_precision(256).value())?;
	for value in target.coefficients() {
		let next = exact_from_f64(value.re, 256)?;
		exact_sum = checked(
			Context::<HalfEven>::new(256)
				.add(exact_sum.repr(), next.repr())?
				.value(),
		)?;
	}
	expect_true!(exact_sum.abs() < exact_from_f64(1e-16, 256)?);
	expect_that!(tree.report().response().upper_f64(), le(1e-11));
	for (direct, tree) in direct
		.report()
		.coefficients()
		.iter()
		.zip(tree.report().coefficients())
	{
		for (direct, tree) in direct.iter().flatten().zip(tree.iter().flatten()) {
			for (direct, tree) in [
				(direct.real(), tree.real()),
				(direct.imaginary(), tree.imaginary()),
			] {
				expect_that!(direct.lower(), le(tree.upper()));
				expect_that!(direct.upper(), ge(tree.lower()));
			}
		}
	}
	Ok(())
}
