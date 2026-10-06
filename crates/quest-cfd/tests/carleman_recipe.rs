#![allow(
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	clippy::panic_in_result_fn,
	clippy::suboptimal_flops,
	reason = "Bounded independent reference hierarchies and explicit domain assertions"
)]
use mathcore::{
	RBig,
	exact::{Owner, Symbol},
	multivariate::{PolynomialLimits, SparsePolynomial},
};
use quest_cfd::{
	CfdError,
	carleman::{CarlemanLimits, OrderedCarlemanReference, SymmetricCarleman},
	carleman_recipe::{CarlemanRecipeLimits, StatelessCarleman},
	polynomial::PolynomialOde,
	stream_history::HistoryRowDynamics,
};
use std::sync::Arc;
fn ode() -> Result<Arc<PolynomialOde>, CfdError> {
	let limits = PolynomialLimits::default();
	let symbols = (0..4)
		.map(|i| Symbol::new(Owner::new(9120), i))
		.collect::<Vec<_>>();
	let f = SparsePolynomial::from_terms(
		symbols.clone(),
		[
			(vec![0, 0, 0, 1], RBig::ONE),
			(vec![1, 0, 0, 0], RBig::from(-2)),
			(vec![1, 1, 0, 0], RBig::from(3)),
		],
		limits,
	)?;
	let g = SparsePolynomial::from_terms(
		symbols.clone(),
		[
			(vec![0, 1, 0, 0], RBig::from(-1)),
			(vec![2, 0, 0, 0], RBig::from(2)),
		],
		limits,
	)?;
	let mean = SparsePolynomial::constant(symbols, RBig::ZERO, limits)?;
	Ok(Arc::new(PolynomialOde::from_polynomials(
		vec![f, g, mean],
		3,
		limits,
	)?))
}
#[test]
fn stateless_indices_rows_sources_and_lift_match_stored_normalization() -> Result<(), CfdError> {
	let ode = ode()?;
	let state = [0.21, -0.13, 0.37];
	for order in 1..=4 {
		let stored =
			SymmetricCarleman::new(Arc::clone(&ode), order, 0.7, CarlemanLimits::default())?;
		let recipe = StatelessCarleman::new(
			Arc::clone(&ode),
			order,
			0.7,
			CarlemanRecipeLimits::default(),
		)?;
		assert_eq!(stored.dimension(), recipe.dimension());
		for (row, powers) in stored.powers().iter().enumerate() {
			assert_eq!(&recipe.unrank(row)?, powers);
			assert_eq!(recipe.rank(powers)?, row);
			assert!(
				(recipe.lift_entry(row, &state)? - stored.lift_entry(row, &state)?).abs() < 1e-13
			);
			for time in [-0.4, 0., 0.37, 1.2] {
				let mut a = Vec::new();
				let mut b = Vec::new();
				stored.visit_row(time, row, &mut |j, z| {
					a.push((j, z));
					Ok(())
				})?;
				recipe.visit_row(time, row, &mut |j, z| {
					b.push((j, z));
					Ok(())
				})?;
				assert_eq!(a.len(), b.len());
				for ((ia, va), (ib, vb)) in a.iter().zip(b) {
					assert_eq!(*ia, ib);
					assert!((*va - vb).norm() < 1e-12);
				}
				assert!(
					(stored.source_entry(time, row)? - recipe.source_entry(time, row)?).norm()
						< 1e-12
				);
			}
		}
		for (axis, &value) in state.iter().enumerate() {
			assert_eq!(recipe.physical_coordinate_index(axis)?, axis);
			assert!(
				(recipe.recover_entry(axis, recipe.lift_entry(axis, &state)?)? - value).abs()
					< 1e-14
			);
		}
	}
	Ok(())
}
#[test]
fn stateless_generator_intertwines_independent_ordered_hierarchy() -> Result<(), CfdError> {
	let ode = ode()?;
	let stored = SymmetricCarleman::new(Arc::clone(&ode), 3, 0.7, CarlemanLimits::default())?;
	let ordered =
		OrderedCarlemanReference::new(Arc::clone(&ode), 3, 0.7, CarlemanLimits::default())?;
	let recipe = StatelessCarleman::new(ode, 3, 0.7, CarlemanRecipeLimits::default())?;
	let state = (0..recipe.dimension())
		.map(|i| f64::from(u32::try_from(i).unwrap_or_default() + 1) / 23.)
		.collect::<Vec<_>>();
	let mut drift = Vec::new();
	for row in 0..recipe.dimension() {
		let mut v = recipe.source_entry(0.37, row)?.re;
		recipe.visit_row(0.37, row, &mut |j, z| {
			v += z.re * state[j];
			Ok(())
		})?;
		drift.push(v);
	}
	let left = ordered.drift(0.37, &ordered.embed_symmetric(&stored, &state)?)?;
	let right = ordered.embed_symmetric(&stored, &drift)?;
	for (a, b) in left.iter().zip(right) {
		assert!((a - b).abs() < 1e-12);
	}
	Ok(())
}

fn diagonal_ode(m: usize) -> Result<Arc<PolynomialOde>, CfdError> {
	let limits = PolynomialLimits::default();
	let symbols = (0..m)
		.map(|i| {
			Ok(Symbol::new(
				Owner::new(9121),
				u64::try_from(i).map_err(|_| CfdError::InvalidInput("test symbol width"))?,
			))
		})
		.collect::<Result<Vec<_>, CfdError>>()?;
	let mut components = Vec::new();
	for axis in 0..m {
		let mut p = vec![0; m];
		p[axis] = 1;
		components.push(SparsePolynomial::from_terms(
			symbols.clone(),
			[(p, RBig::from(-1))],
			limits,
		)?);
	}
	Ok(Arc::new(PolynomialOde::from_polynomials(
		components, m, limits,
	)?))
}
#[test]
fn massive_index_space_queries_retain_fixed_memory_and_roundtrip() -> Result<(), CfdError> {
	use dashu_int::UBig;
	use quest_cfd::{
		carleman::symmetric_dimension,
		stream_history::{HistoryStreamLimits, TemporalHistoryRecipe},
	};
	let ode = diagonal_ode(64)?;
	let first = StatelessCarleman::new(Arc::clone(&ode), 1, 2., CarlemanRecipeLimits::default())?;
	let recipe = StatelessCarleman::new(ode, 10, 2., CarlemanRecipeLimits::default())?;
	assert_eq!(
		recipe.dimension(),
		usize::try_from(symmetric_dimension(&UBig::from(64usize), 10)?)
			.map_err(|_| CfdError::InvalidInput("test large dimension"))?
	);
	assert!(recipe.dimension() > 1_000_000_000);
	assert_eq!(
		first.resources().owned_bytes,
		recipe.resources().owned_bytes
	);
	assert_eq!(
		first.resources().retained_bytes,
		recipe.resources().retained_bytes
	);
	assert_eq!(
		first.resources().query_bytes,
		recipe.resources().query_bytes
	);
	assert!(recipe.resources().query_bytes < 4096);
	println!("sampled stateless resources: {:?}", recipe.resources());
	assert_eq!(
		recipe.resources().borrowed_ode_bytes,
		first.resources().borrowed_ode_bytes
	);
	let dimension = u64::try_from(recipe.dimension())
		.map_err(|_| CfdError::InvalidInput("test dimension width"))?;
	let mut random = 71_u64;
	for sample in 0..256 {
		random = random
			.wrapping_mul(6_364_136_223_846_793_005)
			.wrapping_add(1_442_695_040_888_963_407);
		let row = if sample == 0 {
			0
		} else if sample == 1 {
			recipe.dimension() - 1
		} else {
			usize::try_from(random % dimension)
				.map_err(|_| CfdError::InvalidInput("sample width"))?
		};
		let powers = recipe.unrank(row)?;
		assert_eq!(recipe.rank(&powers)?, row);
		let degree = powers.iter().sum::<u32>();
		let mut count = 0;
		let mut diagonal = 0.;
		recipe.visit_row(0.7, row, &mut |column, value| {
			assert_eq!(column, row);
			diagonal += value.re;
			count += 1;
			Ok(())
		})?;
		assert!(count <= 10);
		assert!((diagonal + f64::from(degree)).abs() < 1e-12);
		assert!(recipe.source_entry(0.7, row)?.norm() < 1e-15);
	}
	let initial = vec![0.5; 64];
	let last = recipe.lift_entry(recipe.dimension() - 1, &initial)?;
	assert!((last - 0.25_f64.powi(10)).abs() < 1e-20);
	let history = TemporalHistoryRecipe::new(&recipe, 0.1, 2, 1, HistoryStreamLimits::default())?;
	let global = history.dimension() - 1;
	let row = history
		.rows(global..global + 1)?
		.collect::<Result<Vec<_>, _>>()?;
	assert_ne!(row.len(), 0);
	assert!(row.len() <= history.resources().maximum_row_entries);
	Ok(())
}
#[test]
fn constructor_and_scalar_queries_reject_budgets_domains_and_overflow() -> Result<(), CfdError> {
	let ode = ode()?;
	let valid = StatelessCarleman::new(Arc::clone(&ode), 3, 0.7, CarlemanRecipeLimits::default())?;
	let r = valid.resources();
	for which in 0..8 {
		let mut limits = CarlemanRecipeLimits::default();
		match which {
			0 => limits.max_dimension = r.dimension - 1,
			1 => limits.max_physical_dimension = 2,
			2 => limits.max_order = 2,
			3 => limits.max_terms = r.maximum_row_entries - 1,
			4 => limits.max_bytes = r.peak_bytes - 1,
			5 => limits.max_construct_work = r.construct_work - 1,
			6 => limits.max_index_work = r.index_work - 1,
			_ => limits.max_query_work = r.query_work - 1,
		}
		assert!(StatelessCarleman::new(Arc::clone(&ode), 3, 0.7, limits).is_err());
	}
	for scale in [0., -1., f64::INFINITY, f64::NAN] {
		assert!(
			StatelessCarleman::new(Arc::clone(&ode), 3, scale, CarlemanRecipeLimits::default())
				.is_err()
		);
	}
	assert!(
		StatelessCarleman::new(Arc::clone(&ode), 0, 1., CarlemanRecipeLimits::default()).is_err()
	);
	assert!(
		StatelessCarleman::new(Arc::clone(&ode), 65, 1., CarlemanRecipeLimits::default()).is_err()
	);
	for p in [vec![], vec![0, 0, 0], vec![5, 0, 0], vec![u32::MAX, 1, 0]] {
		assert!(valid.rank(&p).is_err());
	}
	assert!(valid.unrank(valid.dimension()).is_err());
	assert!(valid.physical_coordinate_index(3).is_err());
	assert!(valid.recover_entry(0, f64::NAN).is_err());
	assert!(valid.lift_entry(0, &[0., 0.]).is_err());
	assert!(valid.lift_entry(0, &[0., 0., f64::NAN]).is_err());
	assert!(valid.lift_entry(0, &[f64::MAX, 0., 0.]).is_err());
	assert!(valid.source_entry(f64::NAN, 0).is_err());
	assert!(valid.source_entry(0., valid.dimension()).is_err());
	assert!(
		valid
			.visit_row(f64::INFINITY, 0, &mut |_, _| Ok(()))
			.is_err()
	);
	assert!(
		valid
			.visit_row(0., 0, &mut |_, _| Err(CfdError::InvalidInput(
				"visitor stopped"
			)))
			.is_err()
	);
	let large = diagonal_ode(64)?;
	assert!(StatelessCarleman::new(large, 64, 1., CarlemanRecipeLimits::default()).is_err());
	let tiny = StatelessCarleman::new(
		ode,
		3,
		f64::MIN_POSITIVE / 16.,
		CarlemanRecipeLimits::default(),
	)?;
	assert!(tiny.source_entry(1., 0).is_err());
	Ok(())
}

#[test]
fn temporal_rows_and_scalar_rhs_match_stored_dg1_and_dg2() -> Result<(), CfdError> {
	use quest_cfd::stream_history::{HistoryStreamLimits, TemporalHistoryRecipe};
	let ode = ode()?;
	let stored = SymmetricCarleman::new(Arc::clone(&ode), 3, 0.7, CarlemanLimits::default())?;
	let generated = StatelessCarleman::new(ode, 3, 0.7, CarlemanRecipeLimits::default())?;
	let initial = [0.21, -0.13, 0.37];
	for order in [1, 2] {
		let a = TemporalHistoryRecipe::new(&stored, 0.9, 3, order, HistoryStreamLimits::default())?;
		let b =
			TemporalHistoryRecipe::new(&generated, 0.9, 3, order, HistoryStreamLimits::default())?;
		assert_eq!(a.dimension(), b.dimension());
		let ea = a.rows(0..a.dimension())?.collect::<Result<Vec<_>, _>>()?;
		let eb = b.rows(0..b.dimension())?.collect::<Result<Vec<_>, _>>()?;
		assert_eq!(ea.len(), eb.len());
		for (x, y) in ea.iter().zip(eb) {
			assert_eq!(x.row, y.row);
			assert_eq!(x.column, y.column);
			assert_eq!(x.ordinal, y.ordinal);
			assert!((x.value - y.value).norm() < 1e-12);
		}
		for row in 0..a.dimension() {
			let fa = a.rhs_value(row, |i| Ok(stored.lift_entry(i, &initial)?.into()))?;
			let fb = b.rhs_value(row, |i| Ok(generated.lift_entry(i, &initial)?.into()))?;
			assert!((fa - fb).norm() < 1e-12);
		}
	}
	Ok(())
}
#[test]
fn scalar_order64_and_high_axis_compositions_have_no_recursive_depth() -> Result<(), CfdError> {
	for m in [1, 2, 7] {
		let ode = diagonal_ode(m)?;
		let order = if m == 7 { 7 } else { 64 };
		let recipe = StatelessCarleman::new(ode, order, 1., CarlemanRecipeLimits::default())?;
		let dimension = u64::try_from(recipe.dimension())
			.map_err(|_| CfdError::InvalidInput("test index width"))?;
		let mut random = 37_u64;
		for _ in 0..256 {
			random = random
				.wrapping_mul(6_364_136_223_846_793_005)
				.wrapping_add(1);
			let row = usize::try_from(random % dimension)
				.map_err(|_| CfdError::InvalidInput("test sampled row width"))?;
			let p = recipe.unrank(row)?;
			assert_eq!(recipe.rank(&p)?, row);
			let mut sum = 0.;
			recipe.visit_row(0., row, &mut |j, z| {
				assert_eq!(j, row);
				sum += z.re;
				Ok(())
			})?;
			assert!((sum + f64::from(p.iter().sum::<u32>())).abs() < 1e-12);
		}
	}
	Ok(())
}

#[test]
fn lift_depth_uses_recipe_budget_after_physical_kernels_are_compiled() -> Result<(), CfdError> {
	let limits = PolynomialLimits {
		max_degree: 2,
		..PolynomialLimits::default()
	};
	let symbols = vec![Symbol::new(Owner::new(9122), 0)];
	let equation = SparsePolynomial::from_terms(symbols, [(vec![1], RBig::from(-1))], limits)?;
	let physical = Arc::new(PolynomialOde::from_polynomials(vec![equation], 1, limits)?);
	let recipe = StatelessCarleman::new(physical, 3, 1., CarlemanRecipeLimits::default())?;
	assert_eq!(recipe.dimension(), 3);
	let mut value = 0.;
	recipe.visit_row(0., 2, &mut |j, z| {
		assert_eq!(j, 2);
		value += z.re;
		Ok(())
	})?;
	assert!((value + 3.).abs() < 1e-14);
	Ok(())
}
