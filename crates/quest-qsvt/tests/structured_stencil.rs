use googletest::{expect_that, expect_true, gtest, matchers::eq};
use quest_qsvt::{
	Complex64, NumericalPolicy, ShiftRegister, StructuredStencilEncoding, TensorShiftEncoding,
	materialize_oracle,
};
use std::ops::{Add, Div, Mul, Neg, Sub};
fn shift(width: usize, offset: usize) -> quest_qsvt::Result<TensorShiftEncoding> {
	TensorShiftEncoding::new(
		width,
		vec![ShiftRegister::new(0, width, offset)?],
		NumericalPolicy::default(),
	)
}
#[gtest]
fn weighted_complex_stencil_and_whole_unitary_match_independent_formula() -> googletest::Result<()>
{
	let weights = [
		Complex64::new(1.0, 0.0),
		Complex64::new(-0.3, 0.4),
		Complex64::new(0.0, -0.8),
	];
	let plan = StructuredStencilEncoding::new(
		2,
		vec![
			(weights[0], shift(2, 0)?),
			(weights[1], shift(2, 1)?),
			(weights[2], shift(2, 3)?),
		],
		NumericalPolicy::default(),
	)?;
	expect_that!(plan.num_colors(), eq(4));
	let projected = plan.projected_encoding(NumericalPolicy::default())?;
	let matrix = projected.logical_matrix()?;
	for row in 0_usize..4 {
		for col in 0_usize..4 {
			let expected = weights
				.iter()
				.zip([0_usize, 1, 3])
				.filter(|(_, offset)| col.wrapping_add(*offset) % 4 == row)
				.fold(Complex64::new(0.0, 0.0), |a, (w, _)| a.add(*w));
			expect_true!(matrix[(row, col)].sub(expected).norm() < 1e-12);
		}
	}
	let unitary = materialize_oracle(
		&plan.to_oracle(NumericalPolicy::default())?,
		NumericalPolicy::default(),
	)?;
	// Independent Walsh-conjugated controlled rotate/phase/permutation formula, all flag/color sectors.
	for row in 0_usize..32 {
		for col in 0_usize..32 {
			let rf = row & 1;
			let cf = col & 1;
			let rs = (row >> 1) & 3;
			let cs = (col >> 1) & 3;
			let rc = row >> 3;
			let cc = col >> 3;
			let mut expected = Complex64::new(0.0, 0.0);
			for color in 0_usize..4 {
				let weight = weights
					.get(color)
					.copied()
					.unwrap_or(Complex64::new(0.0, 0.0));
				let offset = [0_usize, 1, 3, 0]
					.get(color)
					.copied()
					.ok_or(quest_qsvt::Error::Encoding("color"))?;
				if cs.wrapping_add(offset) % 4 != rs {
					continue;
				}
				let mag = weight.norm();
				let sine = 1.0_f64.sub(mag.mul(mag)).max(0.0).sqrt();
				let rotation = match (rf, cf) {
					(0, 0) | (1, 1) => mag,
					(0, 1) => sine.neg(),
					_ => sine,
				};
				let phase = if rf == 0 && mag > 0.0 {
					weight.div(mag)
				} else {
					Complex64::new(1.0, 0.0)
				};
				let sign = if ((rc ^ cc) & color).count_ones().is_multiple_of(2) {
					0.25
				} else {
					-0.25
				};
				expected = expected.add(phase.mul(rotation.mul(sign)));
			}
			expect_true!(unitary[(row, col)].sub(expected).norm() < 1e-12);
		}
	}
	let mut forward = Vec::new();
	plan.visit_gates(false, |g| {
		forward.push(g);
		Ok(())
	})?;
	let mut inverse = Vec::new();
	plan.visit_gates(true, |g| {
		inverse.push(g);
		Ok(())
	})?;
	expect_that!(forward.len(), eq(inverse.len()));
	for (f, b) in forward.iter().rev().zip(inverse) {
		let mut expected = *f;
		expected.kind = match f.kind {
			quest_qsvt::ReplayKind::Ry(a) => quest_qsvt::ReplayKind::Ry(a.neg()),
			quest_qsvt::ReplayKind::Phase(a) => quest_qsvt::ReplayKind::Phase(a.neg()),
			k => k,
		};
		expect_that!(b, eq(expected));
	}
	Ok(())
}
#[gtest]
fn zero_stencil_large_width_and_resource_failures() -> googletest::Result<()> {
	let large = StructuredStencilEncoding::new(
		40,
		vec![(Complex64::new(0.3, 0.4), shift(40, 19)?)],
		NumericalPolicy { max_bytes: 4096 },
	)?;
	expect_that!(large.num_qubits(), eq(41));
	expect_true!(large.retained_bytes()? < 4096);
	let zero = StructuredStencilEncoding::new(2, vec![], NumericalPolicy::default())?;
	let matrix = zero
		.projected_encoding(NumericalPolicy::default())?
		.logical_matrix()?;
	expect_true!(matrix.col(0).iter().all(|v| v.norm() < 1e-12));
	expect_true!(
		StructuredStencilEncoding::new(
			2,
			vec![(Complex64::new(f64::NAN, 0.0), shift(2, 1)?)],
			NumericalPolicy::default()
		)
		.is_err()
	);
	expect_true!(
		StructuredStencilEncoding::new(
			2,
			vec![(Complex64::new(1.0, 0.0), shift(3, 1)?)],
			NumericalPolicy::default()
		)
		.is_err()
	);
	expect_true!(
		large
			.to_oracle(NumericalPolicy { max_bytes: 1024 })
			.is_err()
	);
	Ok(())
}
