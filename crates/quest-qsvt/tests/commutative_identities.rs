#![allow(
	clippy::arithmetic_side_effects,
	clippy::panic_in_result_fn,
	reason = "Bounded independent integer maps, fixed public operator fixtures and explicit rejection assertions"
)]
use quest_qsvt::{
	Complex64, NumericalPolicy, ReplayEncoding, ShiftRegister, StructuredStencilEncoding,
	TensorShiftEncoding,
	portfolio::{
		ArithmeticStencil, ArithmeticStencilTerm, Boundary, LcuPlan, LcuPlanLimits,
		PortfolioLimits, StructuredScheme, WeightedLcu,
	},
};
fn shift(width: usize, offset: usize) -> quest_qsvt::Result<TensorShiftEncoding> {
	TensorShiftEncoding::new(
		width,
		vec![ShiftRegister::new(0, width, offset)?],
		NumericalPolicy::default(),
	)
}
#[test]
fn weighted_distinct_children_bind_signed_complex_weights() -> quest_qsvt::Result<()> {
	let positive = Complex64::new(0., 1.);
	let negative = Complex64::new(0., -1.);
	let terms = vec![(positive, shift(2, 0)?), (negative, shift(2, 1)?)];
	let opposite = vec![(negative, shift(2, 0)?), (positive, shift(2, 1)?)];
	let original = WeightedLcu::new(terms.clone(), PortfolioLimits::default())?;
	let changed = WeightedLcu::new(opposite, PortfolioLimits::default())?;
	let a = original.descriptor()?;
	let b = changed.descriptor()?;
	// Column zero has +i/-i at row zero; the other term maps to row one.
	assert_ne!(a.source_identity, b.source_identity);
	assert_ne!(a.construction_identity, b.construction_identity);
	let plan = |terms: Vec<(Complex64, TensorShiftEncoding)>| {
		LcuPlan::new(
			terms
				.into_iter()
				.map(|(w, child)| Ok((w, child.descriptor()?)))
				.collect::<quest_qsvt::Result<_>>()?,
			LcuPlanLimits::default(),
		)
	};
	let reordered = plan(terms.into_iter().rev().collect())?;
	assert_eq!(a.source_identity, reordered.descriptor().source_identity);
	assert_ne!(
		a.construction_identity,
		reordered.descriptor().construction_identity
	);
	Ok(())
}
#[test]
fn arithmetic_stencil_complex_signs_bind_base_and_prep() -> quest_qsvt::Result<()> {
	for scheme in [StructuredScheme::Base, StructuredScheme::Prep] {
		let terms = |sign: f64| {
			vec![
				ArithmeticStencilTerm {
					weight: Complex64::new(0., sign),
					offsets: vec![0],
				},
				ArithmeticStencilTerm {
					weight: Complex64::new(0., -sign),
					offsets: vec![2],
				},
			]
		};
		let make = |terms| {
			ArithmeticStencil::new(
				vec![2],
				terms,
				Boundary::Periodic,
				scheme,
				PortfolioLimits::default(),
			)
		};
		let a = make(terms(1.))?.descriptor()?;
		let b = make(terms(-1.))?.descriptor()?;
		assert_ne!(a.source_identity, b.source_identity);
		assert_ne!(a.construction_identity, b.construction_identity);
		let reordered = make(terms(1.).into_iter().rev().collect())?.descriptor()?;
		assert_eq!(a.source_identity, reordered.source_identity);
		assert_ne!(a.construction_identity, reordered.construction_identity);
	}
	Ok(())
}
#[test]
fn legacy_stencil_distinct_shifts_bind_complex_phases() -> quest_qsvt::Result<()> {
	let terms = |sign| -> quest_qsvt::Result<_> {
		Ok(vec![
			(Complex64::new(0., sign), shift(5, 12)?),
			(Complex64::new(0., -sign), shift(5, 16)?),
		])
	};
	let make = |terms| StructuredStencilEncoding::new(5, terms, NumericalPolicy::default());
	let a = make(terms(1.)?)?.descriptor()?;
	let b = make(terms(-1.)?)?.descriptor()?;
	// Column zero is +i/-i at row twelve, whereas the second term maps to sixteen.
	assert_ne!(a.source_identity, b.source_identity);
	assert_ne!(a.construction_identity, b.construction_identity);
	let reordered = make(terms(1.)?.into_iter().rev().collect())?.descriptor()?;
	assert_eq!(a.source_identity, reordered.source_identity);
	assert_ne!(a.construction_identity, reordered.construction_identity);
	Ok(())
}
#[test]
fn disjoint_shift_offset_cancellations_do_not_alias_permutations() -> quest_qsvt::Result<()> {
	let terms = |a, b| -> quest_qsvt::Result<_> {
		Ok(vec![
			ShiftRegister::new(0, 8, a)?,
			ShiftRegister::new(8, 8, b)?,
		])
	};
	let make = |terms| TensorShiftEncoding::new(16, terms, NumericalPolicy::default());
	let a = make(terms(1, 129)?)?.descriptor()?;
	let b = make(terms(129, 1)?)?.descriptor()?;
	// Applying the two explicit adders to zero gives 33025 versus 385.
	assert_ne!(a.source_identity, b.source_identity);
	assert_ne!(a.construction_identity, b.construction_identity);
	let reordered = make(terms(1, 129)?.into_iter().rev().collect())?.descriptor()?;
	assert_eq!(a.source_identity, reordered.source_identity);
	assert_ne!(a.construction_identity, reordered.construction_identity);
	Ok(())
}

#[test]
fn cached_legacy_ids_retain_original_hash_cost_and_admit_stack() -> quest_qsvt::Result<()> {
	let records = vec![ShiftRegister::new(0, 2, 1)?];
	let required = records.capacity() * size_of::<ShiftRegister>() * 2
		+ size_of::<TensorShiftEncoding>()
		+ quest_qsvt::RECORD_FINGERPRINT_SCRATCH_BYTES;
	assert!(
		TensorShiftEncoding::new(
			2,
			records.clone(),
			NumericalPolicy {
				max_bytes: required - 1
			}
		)
		.is_err()
	);
	let source = TensorShiftEncoding::new(
		2,
		records,
		NumericalPolicy {
			max_bytes: required,
		},
	)?;
	let clone = source.clone();
	assert_eq!(
		source.source_fingerprint_work(),
		quest_qsvt::record_fingerprint_work(3)?
	);
	assert_eq!(
		source.source_fingerprint_work(),
		clone.source_fingerprint_work()
	);
	assert_eq!(source.descriptor()?, clone.descriptor()?);
	assert_eq!(source.descriptor()?, source.descriptor()?);
	assert!(
		StructuredStencilEncoding::new(
			2,
			vec![(Complex64::new(0., 1.), source.clone())],
			NumericalPolicy {
				max_bytes: quest_qsvt::RECORD_FINGERPRINT_SCRATCH_BYTES
			},
		)
		.is_err()
	);
	let stencil = StructuredStencilEncoding::new(
		2,
		vec![(Complex64::new(0., 1.), source)],
		NumericalPolicy::default(),
	)?;
	assert_eq!(
		stencil.source_fingerprint_work(),
		quest_qsvt::record_fingerprint_work(3)?
	);
	let cloned_stencil = stencil.clone();
	let original_work = stencil.source_fingerprint_work();
	let original_descriptor = stencil.descriptor()?;
	drop(stencil);
	assert_eq!(original_work, cloned_stencil.source_fingerprint_work());
	assert_eq!(original_descriptor, cloned_stencil.descriptor()?);
	Ok(())
}
#[test]
fn record_hash_work_is_admitted_before_portfolio_compilation() -> quest_qsvt::Result<()> {
	let metadata = vec![
		(Complex64::new(0., 1.), shift(2, 0)?.descriptor()?),
		(Complex64::new(0., -1.), shift(2, 1)?.descriptor()?),
	];
	let hash_work = 2 * quest_qsvt::record_fingerprint_work(3)?;
	assert!(
		LcuPlan::new(
			metadata.clone(),
			LcuPlanLimits {
				max_compile_work: hash_work - 1,
				..LcuPlanLimits::default()
			}
		)
		.is_err()
	);
	let plan = LcuPlan::new(metadata, LcuPlanLimits::default())?;
	assert!(plan.resources().metadata_compile_work >= hash_work);
	let terms = || {
		vec![
			ArithmeticStencilTerm {
				weight: Complex64::new(0., 1.),
				offsets: vec![0],
			},
			ArithmeticStencilTerm {
				weight: Complex64::new(0., -1.),
				offsets: vec![2],
			},
		]
	};
	assert!(
		ArithmeticStencil::new(
			vec![2],
			terms(),
			Boundary::Periodic,
			StructuredScheme::Base,
			PortfolioLimits {
				max_compile_work: hash_work - 1,
				..PortfolioLimits::default()
			}
		)
		.is_err()
	);
	let arithmetic = ArithmeticStencil::new(
		vec![2],
		terms(),
		Boundary::Periodic,
		StructuredScheme::Base,
		PortfolioLimits::default(),
	)?;
	assert!(arithmetic.resources().compile_work >= hash_work);
	Ok(())
}

#[test]
fn all_offset_owners_are_admitted_before_first_arithmetic_record() {
	let mut later = Vec::with_capacity(8192);
	later.push(2);
	let result = ArithmeticStencil::new(
		vec![2],
		vec![
			ArithmeticStencilTerm {
				weight: Complex64::new(0., 1.),
				offsets: vec![4], // Invalid first record must not mask the known live input excess.
			},
			ArithmeticStencilTerm {
				weight: Complex64::new(0., -1.),
				offsets: later,
			},
		],
		Boundary::Periodic,
		StructuredScheme::Base,
		PortfolioLimits {
			max_bytes: 32768,
			..PortfolioLimits::default()
		},
	);
	assert!(matches!(result, Err(quest_qsvt::Error::Budget(_))));
}
