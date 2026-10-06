#![allow(
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	reason = "Bounded sparse matching reference"
)]
mod portfolio_support;
use quest_qsvt::{
	Complex64, MatchingEncoding, NumericalPolicy, ReplayEncoding,
	portfolio::{PerMatchingBounds, PortfolioLimits},
};
#[googletest::gtest]
fn per_color_bounds_reduce_normalization_and_preserve_complex_rectangular_operator()
-> googletest::Result<()> {
	let a = quest_numerics::SparseMatrix::from_triplets(
		2,
		3,
		quest_numerics::SparseFormat::Csr,
		vec![
			(0, 0, Complex64::new(0.0, 3.0)),
			(0, 1, Complex64::new(0.1, 0.0)),
			(0, 2, Complex64::new(-0.2, 0.0)),
			(1, 0, Complex64::new(0.0, -0.3)),
		],
		quest_numerics::SparseLimits::default(),
	)?;
	let base = MatchingEncoding::from_sparse(&a, NumericalPolicy::default())?;
	let e = PerMatchingBounds::new(&base, PortfolioLimits::default())?;
	portfolio_support::whole_register(&e)?;
	let d = e.descriptor()?;
	eprintln!(
		"PORTFOLIO_MATCHING base_alpha={} base_gates={} improved_alpha={} resources={:?}",
		base.normalization().get(),
		base.resources().replay_gates,
		d.normalization,
		e.resources()
	);
	googletest::expect_true!(d.normalization < base.normalization().get());
	let u = quest_qsvt::materialize_oracle(
		&e.replay_oracle(NumericalPolicy::default())?,
		NumericalPolicy::default(),
	)?;
	let l = d
		.left
		.logical_space::<quest_qsvt::Left>(d.layout.num_qubits, NumericalPolicy::default())?;
	let r = d
		.right
		.logical_space::<quest_qsvt::Right>(d.layout.num_qubits, NumericalPolicy::default())?;
	for i in 0..2 {
		for j in 0..3 {
			let expected = match (i, j) {
				(0, 0) => Complex64::new(0.0, 3.0),
				(0, 1) => Complex64::new(0.1, 0.0),
				(0, 2) => Complex64::new(-0.2, 0.0),
				(1, 0) => Complex64::new(0.0, -0.3),
				_ => Complex64::default(),
			};
			googletest::expect_true!(
				(u[(l.coordinate_at(i).unwrap(), r.coordinate_at(j).unwrap())] * d.normalization
					- expected)
					.norm()
					< 5e-13
			);
		}
	}
	googletest::expect_eq!(d.source_identity, base.source_identity());
	googletest::expect_true!(d.construction_identity != base.descriptor()?.construction_identity);
	Ok(())
}
