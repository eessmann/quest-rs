use googletest::prelude::*;
use num_complex::Complex64 as C;
use quest_qsp::SynthesisBuilder;
use quest_qsvt_io::{IoPolicy, catalog_families};
use std::ops::{Add, Mul, Sub};

fn matrix(phases: &[f64], x: f64) -> [[C; 2]; 2] {
	let mut product = [
		[C::new(1.0, 0.0), C::new(0.0, 0.0)],
		[C::new(0.0, 0.0), C::new(1.0, 0.0)],
	];
	let off = C::new(0.0, x.mul_add(-x, 1.0).sqrt());
	for (index, phase) in phases.iter().enumerate() {
		if index != 0 {
			for row in &mut product {
				let [a, b] = *row;
				*row = [a.mul(x).add(b.mul(off)), a.mul(off).add(b.mul(x))];
			}
		}
		let factor = C::new(phase.cos(), phase.sin());
		for [a, b] in &mut product {
			*a = a.mul(factor);
			*b = b.mul(factor.conj());
		}
	}
	product
}

#[gtest]
fn canonical_full_phase_matches_pinned_cpp_through_degree_8105() -> Result<()> {
	let fixtures: [(usize, &[u8]); 4] = [
		(9, include_bytes!("data/cpp-phases-k5-e0p1.bin")),
		(197, include_bytes!("data/cpp-phases-k50-e0p01.bin")),
		(1581, include_bytes!("data/cpp-phases-k250-e0p001.bin")),
		(8105, include_bytes!("data/cpp-phases-k1500-e0p001.bin")),
	];
	for (degree, bytes) in fixtures {
		let family = catalog_families()
			.iter()
			.find(|f| f.degree() == degree)
			.ok_or_else(|| std::io::Error::other("missing reference catalog family"))?;
		let polynomial = family.polynomial(IoPolicy::default())?;
		let candidate = SynthesisBuilder::new()
			.real_parity_wx(&polynomial)?
			.admit()?
			.complete()?
			.synthesize()?;
		let mut reference = Vec::new();
		let (chunks, remainder) = bytes.as_chunks::<8>();
		expect_true!(remainder.is_empty());
		for chunk in chunks {
			reference.push(f64::from_le_bytes(*chunk));
		}
		expect_eq!(reference.len(), candidate.phases().len());
		expect_true!(reference.iter().all(|p| p.is_finite()));
		for step in 0..=32 {
			let x = f64::from(step).mul(0.0625).sub(1.0);
			let expected = matrix(&reference, x);
			let actual = matrix(candidate.phases(), x);
			for (left, right) in actual.iter().flatten().zip(expected.iter().flatten()) {
				expect_that!(left.sub(right).norm(), le(1e-11));
			}
			expect_that!(actual[0][0].im, near(polynomial.evaluate_real(x)?, 1e-11));
		}
	}
	Ok(())
}

#[gtest]
fn generalized_complex_controls_match_cpp_including_every_matrix_entry_and_k_factor() -> Result<()>
{
	use quest_polynomial::{Laurent, Limits, Polynomial};
	let target = Polynomial::new(
		Laurent::new(0),
		vec![C::new(0.25, 0.0), C::new(0.0, 0.2), C::new(-0.1, 0.1)],
		Limits::default(),
	)?;
	let candidate = SynthesisBuilder::new()
		.unit_circle_response(&target)?
		.admit()?
		.complete()?
		.synthesize()?;
	let values: Vec<[f64; 2]> =
		serde_json::from_str(include_str!("data/cpp-complex-controls.json"))?;
	let mut controls = Vec::new();
	let (chunks, remainder) = values.as_chunks::<4>();
	expect_true!(remainder.is_empty());
	for chunk in chunks {
		let [a, b, c, d] = *chunk;
		let complex = |[re, im]: [f64; 2]| C::new(re, im);
		controls.push([[complex(a), complex(b)], [complex(c), complex(d)]]);
	}
	expect_eq!(controls.len(), candidate.controls().len());
	for step in 0..64 {
		let z = C::from_polar(1.0, f64::from(step).mul(std::f64::consts::TAU) / 64.0);
		let mut reference = [
			[C::new(1.0, 0.0), C::new(0.0, 0.0)],
			[C::new(0.0, 0.0), C::new(1.0, 0.0)],
		];
		for (index, control) in controls.iter().enumerate() {
			if index > 0 {
				for [a, _] in &mut reference {
					*a = a.mul(z);
				}
			}
			for row in &mut reference {
				let [a, b] = *row;
				*row = [
					a.mul(control[0][0]).add(b.mul(control[1][0])),
					a.mul(control[0][1]).add(b.mul(control[1][1])),
				];
			}
		}
		let actual = candidate.evaluate(z)?;
		for (a, b) in actual.iter().flatten().zip(reference.iter().flatten()) {
			expect_that!(a.sub(b).norm(), le(1e-11));
		}
		expect_that!(reference[0][0].sub(target.evaluate(z)?).norm(), le(1e-11));
	}
	Ok(())
}
