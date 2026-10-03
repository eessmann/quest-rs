use quest_math::{Cyclotomic, Limits, OmegaResidue, Sqrt2Exponent};

#[test]
fn eighth_root_phase_admission_and_extraction_are_typed() {
	for power in 0..8 {
		let phase = quest_math::EighthRootPhase::new(power).unwrap();
		assert_eq!(Cyclotomic::phase(phase).eighth_root_phase(), Some(phase));
		assert_eq!(phase.power(), power);
	}
	assert!(quest_math::EighthRootPhase::new(8).is_err());
	assert_eq!(Cyclotomic::zero().eighth_root_phase(), None);
}

#[test]
fn denominator_exponents_are_distinct_and_residues_are_exact() {
	let limits = Limits::default();
	let half_root =
		Cyclotomic::new([0.into(), 1.into(), 0.into(), (-1).into()], 1, limits).unwrap();
	assert_eq!(half_root.denominator_exponent(), 1);
	assert_eq!(
		half_root.least_sqrt2_exponent(limits).unwrap(),
		Sqrt2Exponent(1)
	);
	assert_eq!(
		half_root.residue_at(Sqrt2Exponent(1), limits).unwrap(),
		OmegaResidue::from_bits(1)
	);
	let half = Cyclotomic::new([1.into(), 0.into(), 0.into(), 0.into()], 1, limits).unwrap();
	assert_eq!(half.least_sqrt2_exponent(limits).unwrap(), Sqrt2Exponent(2));
	assert!(half.residue_at(Sqrt2Exponent(1), limits).is_err());
}

#[test]
fn residue_norm_classes_and_reducibility_match_the_ring() {
	for bits in 0..16 {
		let r = OmegaResidue::from_bits(bits);
		let x = Cyclotomic::new(
			std::array::from_fn(|i| ((bits >> i) & 1).into()),
			0,
			Limits::default(),
		)
		.unwrap();
		let norm = x
			.conjugated(Limits::default())
			.unwrap()
			.checked_mul(&x, Limits::default())
			.unwrap();
		assert_eq!(
			r.norm(),
			norm.residue_at(Sqrt2Exponent(0), Limits::default())
				.unwrap()
		);
		assert_eq!(r.reducible(), [0, 5, 10, 15].contains(&bits));
	}
}
