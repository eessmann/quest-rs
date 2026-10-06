#![allow(
	clippy::arithmetic_side_effects,
	clippy::panic_in_result_fn,
	clippy::indexing_slicing,
	clippy::suboptimal_flops,
	reason = "Small independent history reference and large dimension admission"
)]
use quest_cfd::{
	CfdError,
	history::{HistoryDynamics, HistorySystem},
	stream_history::{HistoryRowDynamics, HistoryStreamLimits, TemporalHistoryRecipe},
};
use quest_numerics::{Complex64, SparseFormat, SparseLimits, SparseMatrix};
struct Dynamics;
impl HistoryRowDynamics for Dynamics {
	fn dimension(&self) -> usize {
		2
	}
	fn max_row_entries(&self) -> usize {
		2
	}
	fn retained_bytes(&self) -> Result<usize, CfdError> {
		Ok(size_of::<Self>())
	}
	fn row_query_bytes(&self) -> usize {
		0
	}
	fn row_query_work(&self) -> usize {
		16
	}
	fn visit_row(
		&self,
		time: f64,
		row: usize,
		visit: &mut dyn FnMut(usize, Complex64) -> Result<(), CfdError>,
	) -> Result<(), CfdError> {
		visit(row, Complex64::new(-1. - time, 0.2))?;
		if row == 0 {
			visit(1, 2.0.into())?;
		}
		Ok(())
	}
	fn source_entry(&self, time: f64, row: usize) -> Result<Complex64, CfdError> {
		Ok(if row == 0 {
			time.into()
		} else {
			(-time * time).into()
		})
	}
}
impl HistoryDynamics for Dynamics {
	fn dimension(&self) -> usize {
		2
	}
	fn max_generator_entries(&self) -> usize {
		3
	}
	fn retained_bytes(&self) -> Result<usize, CfdError> {
		HistoryRowDynamics::retained_bytes(self)
	}
	fn visit_generator(
		&self,
		time: f64,
		visit: &mut dyn FnMut(usize, usize, Complex64) -> Result<(), CfdError>,
	) -> Result<(), CfdError> {
		for row in 0..2 {
			self.visit_row(time, row, &mut |col, v| visit(row, col, v))?;
		}
		Ok(())
	}
	fn source(&self, time: f64, out: &mut [Complex64]) -> Result<(), CfdError> {
		for (row, v) in out.iter_mut().enumerate() {
			*v = self.source_entry(time, row)?;
		}
		Ok(())
	}
}
#[test]
fn row_stream_matches_forced_time_dependent_stored_history_and_rank_independent_ordinals()
-> Result<(), Box<dyn std::error::Error>> {
	let initial = [Complex64::new(0.2, -0.1), Complex64::new(0.3, 0.4)];
	for order in [1, 2] {
		let stored = HistorySystem::assemble_dynamics(
			&Dynamics,
			&initial,
			0.1,
			3,
			order,
			SparseLimits::default(),
		)?;
		let recipe =
			TemporalHistoryRecipe::new(&Dynamics, 0.1, 3, order, HistoryStreamLimits::default())?;
		let serial = recipe
			.rows(0..recipe.dimension())?
			.collect::<Result<Vec<_>, _>>()?;
		for parts in [1, 2, 4, 8] {
			let mut partitioned = Vec::new();
			for rank in 0..parts {
				let start = rank * recipe.dimension() / parts;
				let end = (rank + 1) * recipe.dimension() / parts;
				partitioned.extend(recipe.rows(start..end)?.collect::<Result<Vec<_>, _>>()?);
			}
			assert_eq!(serial, partitioned);
		}
		let rebuilt = SparseMatrix::from_triplets(
			recipe.dimension(),
			recipe.dimension(),
			SparseFormat::Csr,
			serial
				.into_iter()
				.map(|e| (e.row, e.column, e.value))
				.collect(),
			SparseLimits::default(),
		)?;
		assert_eq!(
			stored.operator().entries().collect::<Vec<_>>(),
			rebuilt.entries().collect::<Vec<_>>()
		);
		for (row, &expected) in stored.rhs().iter().enumerate() {
			assert_eq!(recipe.rhs_value(row, |i| Ok(initial[i]))?, expected);
		}
	}
	Ok(())
}
#[cfg(target_pointer_width = "64")]
#[test]
fn very_long_history_queries_only_local_rows_without_full_rhs_or_operator()
-> Result<(), Box<dyn std::error::Error>> {
	let recipe = TemporalHistoryRecipe::new(
		&Dynamics,
		1.,
		1_000_000_000,
		2,
		HistoryStreamLimits::default(),
	)?;
	assert_eq!(recipe.dimension(), 6_000_000_000);
	assert!(recipe.resources().peak_managed_bytes < 4096);
	let entries = recipe
		.rows(5_999_999_998..6_000_000_000)?
		.collect::<Result<Vec<_>, _>>()?;
	assert_ne!(entries, []);
	assert!(entries.iter().all(|e| e.row >= 5_999_999_998));
	assert!(recipe.rows(0..usize::MAX).is_err());
	assert!(
		TemporalHistoryRecipe::new(
			&Dynamics,
			1.,
			10,
			2,
			HistoryStreamLimits {
				max_bytes: 1,
				..HistoryStreamLimits::default()
			}
		)
		.is_err()
	);
	assert!(
		TemporalHistoryRecipe::new(
			&Dynamics,
			1.,
			10,
			2,
			HistoryStreamLimits {
				max_work_per_visit: 0,
				..HistoryStreamLimits::default()
			}
		)?
		.rows(0..1)
		.is_err()
	);
	Ok(())
}

#[test]
fn complete_burgers_hierarchy_stream_matches_materialized_history()
-> Result<(), Box<dyn std::error::Error>> {
	use quest_cfd::{
		burgers::BurgersDg,
		carleman::{CarlemanLimits, SymmetricCarleman},
	};
	let physical = BurgersDg::new(4, 1, 0.1)?;
	let initial = physical.initial_state(0.01)?;
	let hierarchy =
		SymmetricCarleman::new(physical.polynomial_ode(), 2, 0.1, CarlemanLimits::default())?;
	let lifted = hierarchy
		.lift(&initial)?
		.into_iter()
		.map(Complex64::from)
		.collect::<Vec<_>>();
	let stored =
		HistorySystem::assemble_dynamics(&hierarchy, &lifted, 0.1, 2, 2, SparseLimits::default())?;
	let recipe = TemporalHistoryRecipe::new(&hierarchy, 0.1, 2, 2, HistoryStreamLimits::default())?;
	let entries = recipe
		.rows(0..recipe.dimension())?
		.map(|entry| entry.map(|e| (e.row, e.column, e.value)))
		.collect::<Result<Vec<_>, _>>()?;
	let rebuilt = SparseMatrix::from_triplets(
		recipe.dimension(),
		recipe.dimension(),
		SparseFormat::Csr,
		entries,
		SparseLimits::default(),
	)?;
	assert_eq!(
		stored.operator().entries().collect::<Vec<_>>(),
		rebuilt.entries().collect::<Vec<_>>()
	);
	for (row, &expected) in stored.rhs().iter().enumerate() {
		assert_eq!(
			recipe.rhs_value(row, |i| Ok(Complex64::from(
				hierarchy.lift_entry(i, &initial)?
			)))?,
			expected
		);
	}
	Ok(())
}

struct Malformed;
impl HistoryRowDynamics for Malformed {
	fn dimension(&self) -> usize {
		2
	}
	fn max_row_entries(&self) -> usize {
		0
	}
	fn retained_bytes(&self) -> Result<usize, CfdError> {
		Ok(0)
	}
	fn row_query_bytes(&self) -> usize {
		0
	}
	fn row_query_work(&self) -> usize {
		1
	}
	fn visit_row(
		&self,
		_: f64,
		row: usize,
		visitor: &mut dyn FnMut(usize, Complex64) -> Result<(), CfdError>,
	) -> Result<(), CfdError> {
		visitor(row, 1.0.into())
	}
	fn source_entry(&self, _: f64, _: usize) -> Result<Complex64, CfdError> {
		Ok(Complex64::new(f64::NAN, 0.))
	}
}
#[test]
fn malformed_recipe_fails_once_and_never_publishes_a_partial_row()
-> Result<(), Box<dyn std::error::Error>> {
	let recipe = TemporalHistoryRecipe::new(&Malformed, 0.1, 2, 1, HistoryStreamLimits::default())?;
	let mut rows = recipe.rows(0..recipe.dimension())?;
	assert!(rows.next().is_some_and(|v| v.is_err()));
	assert!(rows.next().is_none());
	assert!(recipe.rhs_value(0, |_| Ok(1.0.into())).is_err());
	Ok(())
}
