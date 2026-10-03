use googletest::prelude::*;
use quest_qsp::{Complex64, Control, PhaseSequence, WxImaginaryU00, WxLaurent};
use std::ops::{Add, Mul, Sub};

fn phase(value: f64) -> Control {
	let z = Complex64::from_polar(1.0, value);
	let zero = Complex64::new(0.0, 0.0);
	[[z, zero], [zero, z.conj()]]
}
#[expect(
	clippy::many_single_char_names,
	reason = "Independent explicit 2x2 matrix multiplication"
)]
fn multiply(left: Control, right: Control) -> Control {
	let [[a, b], [c, d]] = left;
	let [[e, f], [g, h]] = right;
	[
		[a.mul(e).add(b.mul(g)), a.mul(f).add(b.mul(h))],
		[c.mul(e).add(d.mul(g)), c.mul(f).add(d.mul(h))],
	]
}
fn product(phases: impl IntoIterator<Item = Control>) -> Control {
	let mut result = phase(0.0);
	let off = Complex64::new(0.0, 0.5_f64.sqrt());
	let on = Complex64::new(0.5_f64.sqrt(), 0.0);
	let signal = [[on, off], [off, on]];
	for (index, rotation) in phases.into_iter().enumerate() {
		if index > 0 {
			result = multiply(result, signal);
		}
		result = multiply(result, rotation);
	}
	result
}
fn close(actual: Control, expected: Control) {
	for (a, b) in actual
		.into_iter()
		.flatten()
		.zip(expected.into_iter().flatten())
	{
		expect_that!(a.sub(b).norm(), lt(2e-15));
	}
}

#[gtest]
fn huge_phase_conversion_preserves_full_matrices_without_period_reduction() -> Result<()> {
	for huge in [1e20, -1e20, f64::MAX, -f64::MAX] {
		let original = [huge, 0.37, -huge];
		let sequence = PhaseSequence::<WxLaurent>::builder(original.to_vec()).build()?;
		let real_parity_wx = sequence.real_parity_wx();
		let expected = original.into_iter().enumerate().map(|(index, value)| {
			let rotation = phase(value);
			if index == 0 {
				multiply(rotation, phase(std::f64::consts::FRAC_PI_2))
			} else {
				rotation
			}
		});
		close(
			product(real_parity_wx.values().iter().copied().map(phase)),
			product(expected),
		);
		expect_that!(real_parity_wx.values().get(1), some(eq(&original[1])));
		expect_that!(real_parity_wx.values().get(2), some(eq(&original[2])));
		let real_parity_wx = PhaseSequence::<WxImaginaryU00>::builder(original.to_vec()).build()?;
		let converted = real_parity_wx.projector_phases_with_diagnostics();
		let expected = original.into_iter().enumerate().map(|(index, value)| {
			multiply(
				phase(value),
				phase(if index == 1 {
					-std::f64::consts::FRAC_PI_2
				} else {
					-std::f64::consts::FRAC_PI_4
				}),
			)
		});
		close(
			product(converted.values().iter().copied().map(phase)),
			product(expected),
		);
		expect_true!(converted.roundoff_estimate().is_finite());
		expect_that!(converted.roundoff_estimate(), lt(1e-13));
	}
	Ok(())
}

#[gtest]
fn ordinary_phase_shifts_keep_existing_bits_and_accumulate_diagnostics() -> Result<()> {
	let values = [0.2, -0.4, 0.7];
	let sequence = PhaseSequence::<WxLaurent>::builder(values.to_vec()).build()?;
	let real_parity_wx = sequence.real_parity_wx();
	let expected = [
		values[0] + std::f64::consts::FRAC_PI_2,
		values[1],
		values[2],
	];
	for (actual, expected) in real_parity_wx.values().iter().zip(expected) {
		expect_that!(actual.to_bits(), eq(expected.to_bits()));
	}
	let projector = real_parity_wx.projector_phases_with_diagnostics();
	for ((actual, value), offset) in projector.values().iter().zip(expected).zip([
		std::f64::consts::FRAC_PI_4,
		std::f64::consts::FRAC_PI_2,
		std::f64::consts::FRAC_PI_4,
	]) {
		expect_that!(actual.to_bits(), eq((value - offset).to_bits()));
	}
	expect_that!(
		projector.roundoff_estimate(),
		gt(real_parity_wx.conversion_roundoff_estimate())
	);
	expect_that!(projector.roundoff_estimate(), lt(1e-13));
	Ok(())
}
