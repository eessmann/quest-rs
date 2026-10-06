#![allow(
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	reason = "Small independent projected matrix references"
)]
mod portfolio_support;
use quest_qsvt::{
	Complex64, NumericalPolicy, ReplayEncoding,
	portfolio::{
		ArithmeticStencil, ArithmeticStencilTerm, Boundary, PortfolioLimits, StructuredScheme,
	},
};
#[googletest::gtest]
fn arithmetic_base_and_justified_prep_encode_toeplitz_and_circulants() -> googletest::Result<()> {
	for boundary in [Boundary::Periodic, Boundary::Zero] {
		for scheme in [StructuredScheme::Base, StructuredScheme::Prep] {
			let terms = vec![
				ArithmeticStencilTerm {
					weight: Complex64::new(0.0, 1.0),
					offsets: vec![1],
				},
				ArithmeticStencilTerm {
					weight: Complex64::new(-0.2, 0.1),
					offsets: vec![-1],
				},
				ArithmeticStencilTerm {
					weight: Complex64::new(0.5, 0.0),
					offsets: vec![0],
				},
			];
			let source = ArithmeticStencil::new(
				vec![2],
				terms.clone(),
				boundary,
				scheme,
				PortfolioLimits::default(),
			)?;
			portfolio_support::whole_register(&source)?;
			let d = source.descriptor()?;
			let u = quest_qsvt::materialize_oracle(
				&source.replay_oracle(NumericalPolicy::default())?,
				NumericalPolicy::default(),
			)?;
			let s = d.left.logical_space::<quest_qsvt::Left>(
				d.layout.num_qubits,
				NumericalPolicy::default(),
			)?;
			for i in 0..4 {
				for j in 0..4 {
					let expected: Complex64 = terms
						.iter()
						.filter(|t| {
							let mapped = i64::try_from(j).unwrap() + t.offsets[0];
							if boundary == Boundary::Periodic {
								mapped.rem_euclid(4) == i64::try_from(i).unwrap()
							} else {
								mapped == i64::try_from(i).unwrap()
							}
						})
						.map(|t| t.weight)
						.sum();
					googletest::expect_true!(
						(u[(s.coordinate_at(i).unwrap(), s.coordinate_at(j).unwrap())]
							* d.normalization
							- expected)
							.norm()
							< 5e-13
					);
				}
			}
			let mut f = Vec::new();
			let mut r = Vec::new();
			source.visit_replay(false, &mut |g| {
				f.push(g);
				Ok(())
			})?;
			source.visit_replay(true, &mut |g| {
				r.push(g);
				Ok(())
			})?;
			googletest::expect_eq!(f.len(), r.len());
			for (a, b) in f.iter().rev().zip(r) {
				let kind = match a.kind {
					quest_qsvt::ReplayKind::Ry(x) => quest_qsvt::ReplayKind::Ry(-x),
					quest_qsvt::ReplayKind::Phase(x) => quest_qsvt::ReplayKind::Phase(-x),
					k => k,
				};
				googletest::expect_true!(
					kind == b.kind
						&& a.target == b.target
						&& a.control_mask == b.control_mask
						&& a.control_value == b.control_value
				);
			}
			googletest::expect_eq!(source.resources().elementary_gates, f.len());
		}
	}
	Ok(())
}
#[googletest::gtest]
fn tensor_zero_boundary_flags_union_of_invalid_axes_and_reports_actual_costs()
-> googletest::Result<()> {
	let source = ArithmeticStencil::new(
		vec![1, 1],
		vec![
			ArithmeticStencilTerm {
				weight: Complex64::new(0.3, 0.4),
				offsets: vec![-1, 1],
			},
			ArithmeticStencilTerm {
				weight: Complex64::new(-0.1, 0.0),
				offsets: vec![0, 0],
			},
		],
		Boundary::Zero,
		StructuredScheme::Prep,
		PortfolioLimits::default(),
	)?;
	portfolio_support::whole_register(&source)?;
	let d = source.descriptor()?;
	let u = quest_qsvt::materialize_oracle(
		&source.replay_oracle(NumericalPolicy::default())?,
		NumericalPolicy::default(),
	)?;
	let s = d
		.left
		.logical_space::<quest_qsvt::Left>(d.layout.num_qubits, NumericalPolicy::default())?;
	for i in 0..4 {
		for j in 0..4 {
			let expected = if i == j {
				Complex64::new(-0.1, 0.0)
			} else if j == 1 && i == 2 {
				Complex64::new(0.3, 0.4)
			} else {
				Complex64::default()
			};
			googletest::expect_true!(
				(u[(s.coordinate_at(i).unwrap(), s.coordinate_at(j).unwrap())] * d.normalization
					- expected)
					.norm()
					< 4e-13
			);
		}
	}
	googletest::expect_true!(!source.valid(0, 0)?);
	googletest::expect_true!(source.valid(0, 1)?);
	googletest::expect_true!(!source.valid(0, 3)?);
	for scheme in [StructuredScheme::Base, StructuredScheme::Prep] {
		let e = ArithmeticStencil::new(
			vec![2],
			vec![
				ArithmeticStencilTerm {
					weight: Complex64::new(0.0, 1.0),
					offsets: vec![1],
				},
				ArithmeticStencilTerm {
					weight: Complex64::new(-0.2, 0.1),
					offsets: vec![-1],
				},
				ArithmeticStencilTerm {
					weight: Complex64::new(0.5, 0.0),
					offsets: vec![0],
				},
			],
			Boundary::Periodic,
			scheme,
			PortfolioLimits::default(),
		)?;
		eprintln!(
			"PORTFOLIO_ARITHMETIC {scheme:?} alpha={} resources={:?}",
			e.descriptor()?.normalization,
			e.resources()
		);
	}
	let bad = vec![ArithmeticStencilTerm {
		weight: 1.0.into(),
		offsets: vec![1],
	}];
	for limits in [
		PortfolioLimits {
			max_bytes: 1,
			..Default::default()
		},
		PortfolioLimits {
			max_compile_work: 1,
			..Default::default()
		},
		PortfolioLimits {
			max_gates: 1,
			..Default::default()
		},
	] {
		googletest::expect_true!(
			ArithmeticStencil::new(
				vec![2],
				bad.clone(),
				Boundary::Zero,
				StructuredScheme::Base,
				limits
			)
			.is_err()
		);
	}
	Ok(())
}
