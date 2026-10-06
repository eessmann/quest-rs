#![allow(
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	clippy::panic_in_result_fn,
	reason = "Bounded independent extracted blocks and callback visit counters"
)]
mod portfolio_support;
use quest_qsvt::{
	CompactProjector, Complex64, EncodingDescriptor, EncodingErrors, EncodingLayout,
	NumericalPolicy, ReplayEncoding, ReplayGate, ReplayKind, Result,
	portfolio::{
		KroneckerSum, LcuPlan, LcuPlanLimits, PortfolioLimits, TensorProduct, WeightedLcu,
	},
};
use std::sync::{
	Arc,
	atomic::{AtomicUsize, Ordering::Relaxed},
};

#[derive(Clone, Debug)]
struct GenericSource {
	descriptor: EncodingDescriptor,
	flip: bool,
	gates: usize,
	phase: f64,
	visits: Arc<AtomicUsize>,
	metadata_visits: Arc<AtomicUsize>,
}
impl GenericSource {
	fn new(alpha: f64, flip: bool, gates: usize) -> Result<Self> {
		let mask = if flip { 2 } else { 0 };
		let descriptor = EncodingDescriptor {
			rows: 2,
			cols: 2,
			normalization: alpha,
			layout: EncodingLayout {
				num_qubits: if flip { 2 } else { 1 },
				system_mask: 1,
				workspace_mask: mask,
				clean_workspace_mask: 0,
				clean_workspace_value: 0,
			},
			left: CompactProjector {
				fixed_mask: mask,
				fixed_value: mask,
				logical_range: 0..2,
			},
			right: CompactProjector {
				fixed_mask: mask,
				fixed_value: 0,
				logical_range: 0..2,
			},
			errors: EncodingErrors {
				preparation: Some(0.0),
				encoding: Some(0.0),
				binary64_parameters: false,
			},
			source_identity: alpha.to_bits(),
			construction_identity: alpha.to_bits()
				^ u64::try_from(mask)
					.map_err(|_| quest_qsvt::Error::Budget("test identity mask"))?,
		};
		descriptor.validate()?;
		Ok(Self {
			descriptor,
			flip,
			gates,
			phase: 0.0,
			visits: Arc::new(AtomicUsize::new(0)),
			metadata_visits: Arc::new(AtomicUsize::new(0)),
		})
	}
}
impl ReplayEncoding for GenericSource {
	fn descriptor(&self) -> Result<EncodingDescriptor> {
		self.metadata_visits.fetch_add(1, Relaxed);
		Ok(self.descriptor.clone())
	}
	fn retained_bytes(&self) -> Result<usize> {
		self.metadata_visits.fetch_add(1, Relaxed);
		Ok(size_of::<Self>())
	}
	fn visit_replay(
		&self,
		adjoint: bool,
		v: &mut dyn FnMut(ReplayGate) -> Result<()>,
	) -> Result<()> {
		for _ in 0..self.gates {
			self.visits.fetch_add(1, Relaxed);
			v(ReplayGate {
				kind: if self.flip {
					ReplayKind::X
				} else {
					ReplayKind::Phase(if adjoint { -self.phase } else { self.phase })
				},
				target: if self.flip { Some(1) } else { None },
				control_mask: 0,
				control_value: 0,
			})?;
		}
		Ok(())
	}
}
fn check_identity_block<E: ReplayEncoding>(e: &E, expected: f64) -> Result<()> {
	let d = e.descriptor()?;
	let p = NumericalPolicy::default();
	let left = d
		.left
		.logical_space::<quest_qsvt::Left>(d.layout.num_qubits, p)?;
	let right = d
		.right
		.logical_space::<quest_qsvt::Right>(d.layout.num_qubits, p)?;
	let u = quest_qsvt::materialize_oracle(&e.replay_oracle(p)?, p)?;
	for i in 0..d.rows {
		for j in 0..d.cols {
			assert!(
				(u[(
					left.coordinate_at(i)
						.ok_or(quest_qsvt::Error::Encoding("test left coordinate"))?,
					right
						.coordinate_at(j)
						.ok_or(quest_qsvt::Error::Encoding("test right coordinate"))?
				)] * d.normalization
					- Complex64::new(if i == j { expected } else { 0.0 }, 0.0))
				.norm()
					< 5e-13
			);
		}
	}
	Ok(())
}
#[googletest::gtest]
fn kronecker_sum_bridges_distinct_generic_left_and_right_workspaces() -> googletest::Result<()> {
	let a = GenericSource::new(1.0, true, 1)?;
	let b = GenericSource::new(1.0, true, 1)?;
	check_identity_block(&a, 1.0)?;
	let product = TensorProduct::new(a.clone(), b.clone(), PortfolioLimits::default())?;
	check_identity_block(&product, 1.0)?;
	let sum = KroneckerSum::new(a, b, PortfolioLimits::default())?;
	check_identity_block(&sum, 2.0)?;
	portfolio_support::whole_register(&sum)?;
	let mut forward = vec![];
	let mut reverse = vec![];
	sum.visit_replay(false, &mut |g| {
		forward.push(g);
		Ok(())
	})?;
	sum.visit_replay(true, &mut |g| {
		reverse.push(g);
		Ok(())
	})?;
	googletest::expect_eq!(forward.len(), sum.resources().elementary_gates);
	googletest::expect_eq!(reverse.len(), forward.len());
	for (f, r) in forward.iter().rev().zip(reverse) {
		let kind = match f.kind {
			ReplayKind::Ry(x) => ReplayKind::Ry(-x),
			ReplayKind::Phase(x) => ReplayKind::Phase(-x),
			kind => kind,
		};
		googletest::expect_true!(
			r.kind == kind
				&& r.target == f.target
				&& r.control_mask == f.control_mask
				&& r.control_value == f.control_value
		);
	}
	portfolio_support::whole_register(&product)?;
	let mixed = KroneckerSum::new(
		GenericSource::new(1.0, true, 1)?,
		GenericSource::new(1.0, false, 1)?,
		PortfolioLimits::default(),
	)?;
	check_identity_block(&mixed, 2.0)?;
	portfolio_support::whole_register(&mixed)?;
	Ok(())
}
#[googletest::gtest]
fn underflowed_lcu_mass_retains_and_charges_the_whole_unitary_branch() -> googletest::Result<()> {
	let first = GenericSource::new(1.0, false, 3)?;
	let mut tiny = GenericSource::new(1e-200, false, 3)?;
	tiny.phase = 0.37;
	let terms = vec![(1.0.into(), first), (Complex64::new(1e-200, 0.0), tiny)];
	let e = WeightedLcu::new(terms.clone(), PortfolioLimits::default())?;
	for adjoint in [false, true] {
		let mut actual = 0;
		e.visit_replay(adjoint, &mut |_| {
			actual += 1;
			Ok(())
		})?;
		googletest::expect_eq!(actual, 16);
		googletest::expect_eq!(e.resources().elementary_gates, actual);
	}
	check_identity_block(&e, 1.0)?;
	portfolio_support::whole_register(&e)?;
	googletest::expect_true!(
		WeightedLcu::new(
			terms,
			PortfolioLimits {
				max_gates: 12,
				..Default::default()
			}
		)
		.is_err()
	);
	Ok(())
}
#[googletest::gtest]
fn tensor_dry_runs_share_a_work_allowance_before_and_during_child_replay() -> googletest::Result<()>
{
	for allowance in [0, 7, 250] {
		let a = GenericSource::new(1.0, false, 100)?;
		let b = GenericSource::new(1.0, false, 100)?;
		let ac = Arc::clone(&a.visits);
		let bc = Arc::clone(&b.visits);
		let result = TensorProduct::new(
			a,
			b,
			PortfolioLimits {
				max_compile_work: allowance,
				..Default::default()
			},
		);
		googletest::expect_true!(result.is_err());
		let visited = ac.load(Relaxed) + bc.load(Relaxed);
		googletest::expect_true!(visited <= allowance + usize::from(allowance > 0));
		if allowance == 0 {
			googletest::expect_eq!(visited, 0);
		}
		if allowance == 250 {
			googletest::expect_eq!(ac.load(Relaxed), 200);
		}
	}
	let a = GenericSource::new(1.0, false, 100)?;
	let b = GenericSource::new(1.0, false, 100)?;
	let ac = Arc::clone(&a.visits);
	let bc = Arc::clone(&b.visits);
	let e = TensorProduct::new(
		a,
		b,
		PortfolioLimits {
			max_compile_work: 400,
			..Default::default()
		},
	)?;
	googletest::expect_eq!(ac.load(Relaxed) + bc.load(Relaxed), 400);
	googletest::expect_eq!(e.resources().compile_work, 400);
	Ok(())
}

#[googletest::gtest]
fn lcu_validates_zero_weight_input_descriptors_before_any_child_scan() -> googletest::Result<()> {
	let first = GenericSource::new(1.0, false, 100)?;
	let visits = Arc::clone(&first.visits);
	let mut invalid_zero = GenericSource::new(1.0, false, 1)?;
	invalid_zero.descriptor.rows = 3;
	let result = WeightedLcu::new(
		vec![(1.0.into(), first), (0.0.into(), invalid_zero)],
		PortfolioLimits::default(),
	);
	googletest::expect_true!(result.is_err());
	googletest::expect_eq!(visits.load(Relaxed), 0);
	Ok(())
}

#[googletest::gtest]
fn lcu_rejects_known_metadata_work_before_opaque_child_callbacks() -> googletest::Result<()> {
	let first = GenericSource::new(1.0, false, 100)?;
	let second = GenericSource::new(1.0, false, 100)?;
	let a = Arc::clone(&first.metadata_visits);
	let b = Arc::clone(&second.metadata_visits);
	let result = WeightedLcu::new(
		vec![(1.0.into(), first), (1.0.into(), second)],
		PortfolioLimits {
			max_compile_work: 1,
			..Default::default()
		},
	);
	googletest::expect_true!(result.is_err());
	googletest::expect_eq!(a.load(Relaxed) + b.load(Relaxed), 0);
	Ok(())
}

#[googletest::gtest]
fn lcu_metadata_preparation_and_child_scans_share_compile_and_gate_allowances()
-> googletest::Result<()> {
	for gates in [false, true] {
		let first = GenericSource::new(1.0, false, 100)?;
		let second = GenericSource::new(1.0, false, 100)?;
		let plan = LcuPlan::new(
			vec![
				(1.0.into(), first.descriptor()?),
				(1.0.into(), second.descriptor()?),
			],
			LcuPlanLimits::default(),
		)?;
		let a = Arc::clone(&first.visits);
		let b = Arc::clone(&second.visits);
		let limits = if gates {
			PortfolioLimits {
				max_gates: plan.resources().primitive_gates + 100,
				..Default::default()
			}
		} else {
			PortfolioLimits {
				max_compile_work: plan.resources().compile_work + 250,
				..Default::default()
			}
		};
		googletest::expect_true!(
			WeightedLcu::new(vec![(1.0.into(), first), (1.0.into(), second)], limits).is_err()
		);
		googletest::expect_eq!(a.load(Relaxed), 200);
		googletest::expect_eq!(b.load(Relaxed), if gates { 1 } else { 51 });
	}
	let first = GenericSource::new(1.0, false, 100)?;
	let second = GenericSource::new(1.0, false, 100)?;
	let plan = LcuPlan::new(
		vec![
			(1.0.into(), first.descriptor()?),
			(1.0.into(), second.descriptor()?),
		],
		LcuPlanLimits::default(),
	)?;
	let exact = plan.resources().compile_work + 400;
	let source = WeightedLcu::new(
		vec![(1.0.into(), first), (1.0.into(), second)],
		PortfolioLimits {
			max_compile_work: exact,
			..Default::default()
		},
	)?;
	googletest::expect_eq!(source.resources().compile_work, exact);
	Ok(())
}
