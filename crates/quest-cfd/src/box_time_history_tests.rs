#![allow(
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	clippy::too_many_lines,
	clippy::panic_in_result_fn,
	clippy::suboptimal_flops,
	clippy::many_single_char_names,
	clippy::similar_names,
	reason = "Tiny independently chart-aligned time-history and MPI tests"
)]
use super::*;
use crate::{
	history::{HistoryDynamics, HistorySystem},
	physical_space::{
		BoundaryLimits, BoundaryTimeCoefficient, BoxConstraintOutcome, BoxConstraintRecipe,
		BoxForceRecipe, BoxTimeCoefficient, BoxTimeDataLimits, BoxTimeDataShard,
		ConstraintRecipeLimits, DistributedForceLimits, ForceRecipeLimits, PhysicalSpace,
		PolynomialBoundary,
	},
	simplex::BoxBoundary,
};
use quest::{
	collective::MpiRuntime,
	distributed_constraints::{ConstraintLimits, RankPolicy},
};
use quest_numerics::{SparseFormat, SparseLimits, SparseMatrix};
struct Reference<'a> {
	grid: &'a ConfigurationGrid,
	boundary: &'a PolynomialBoundary<'a>,
	rotation: &'a [Vec<f64>],
}
impl Reference<'_> {
	fn generator(&self, time: f64) -> Result<SparseMatrix, CfdError> {
		let m = self.rotation.len();
		self.grid.generator_from(
			m,
			|x| {
				let y = self
					.rotation
					.iter()
					.map(|r| r.iter().zip(x).map(|(a, b)| a * b).sum())
					.collect::<Vec<_>>();
				let dy = self.boundary.drift(time, &y)?;
				Ok((0..m)
					.map(|j| (0..m).map(|i| self.rotation[i][j] * dy[i]).sum())
					.collect())
			},
			SparseLimits::default(),
		)
	}
}
impl HistoryDynamics for Reference<'_> {
	fn dimension(&self) -> usize {
		self.grid.dimension()
	}
	fn max_generator_entries(&self) -> usize {
		self.grid.dimension() * self.grid.axes() * 3
	}
	fn retained_bytes(&self) -> Result<usize, CfdError> {
		Ok(1_000_000)
	}
	fn visit_generator(
		&self,
		time: f64,
		v: &mut dyn FnMut(usize, usize, Complex64) -> Result<(), CfdError>,
	) -> Result<(), CfdError> {
		for (i, j, x) in self.generator(time)?.entries() {
			v(i, j, x)?;
		}
		Ok(())
	}
	fn source(&self, _: f64, out: &mut [Complex64]) -> Result<(), CfdError> {
		out.fill(Complex64::from(0.));
		Ok(())
	}
}
#[test]
fn generated_time_history_uses_every_exact_quadrature_time()
-> Result<(), Box<dyn std::error::Error>> {
	if std::env::var_os("QUEST_TIME_HISTORY_CHILD").is_none() {
		for (ranks, split) in [(1, 0), (2, 0), (4, 0), (8, 0), (4, 1)] {
			assert!(
				quest_test_support::mpi::MpiTest::new(
					usize::try_from(ranks)?,
					std::time::Duration::from_secs(240)
				)?
				.args([
					"--exact",
					"box_kvn_history::time_tests::generated_time_history_uses_every_exact_quadrature_time",
					"--nocapture",
					"--test-threads=1"
				])
				.env("QUEST_TIME_HISTORY_CHILD", "1")
				.env("QUEST_TIME_HISTORY_SPLIT", split.to_string())
				.status()?
				.success()
			);
		}
		return Ok(());
	}
	let runtime = MpiRuntime::initialize()?;
	let mut world = runtime.world()?;
	quest_test_support::mpi::assert_rank_count(world.size()?)?;
	let comm = if std::env::var("QUEST_TIME_HISTORY_SPLIT").as_deref() == Ok("1") {
		world.split_power_of_two(2)?
	} else {
		world.duplicate()?
	};
	let env = CollectiveEnvironment::builder(&comm)?
		.memory_budget(quest::MemoryBudget::new(64 * 1024 * 1024))
		.build()?;
	for periodic in [false, true] {
		if periodic && env.size()? > 2 {
			continue;
		}
		let boundary = if periodic {
			BoxBoundary::Periodic
		} else {
			BoxBoundary::Cavity { lid_speed: 0. }
		};
		let geometry =
			BoxConstraintRecipe::new(2, 1, 1., boundary, 1, ConstraintRecipeLimits::default())?;
		let recipe = BoxForceRecipe::new(&geometry, 0.01, 0., ForceRecipeLimits::default())?;
		let BoxConstraintOutcome::Prepared(prepared) = geometry.prepare_collective(
			&env,
			ConstraintLimits::default(),
			RankPolicy::default(),
		)?
		else {
			return Err("ambiguous chart".into());
		};
		let dense = PhysicalSpace::box_mesh(2, 1, 1., 0.01, boundary, 1)?;
		let m = dense.dimension();
		let n = geometry.local_velocity_dimension();
		let rows = prepared.chart().local_row_range();
		let cells = rows.start / n..rows.end / n;
		let mut data = vec![BoxTimeCoefficient::zero(); 3 * cells.len()];
		let mut refs = (0..3)
			.map(|_| BoundaryTimeCoefficient::zero(&dense))
			.collect::<Result<Vec<_>, _>>()?;
		for k in 0..3 {
			for cell in 0..geometry.cell_count() {
				for i in 0..n {
					let index = cell * n + i;
					refs[k].lifting[index] = [0.01, -0.03, 0.02][k] * dense.chart()[m - 1][index];
					refs[k].body_force[index] = [0.02, 0.9, -0.1][k] * dense.chart()[m - 1][index];
					if cells.contains(&cell) {
						data[k * cells.len() + cell - cells.start].lifting[i] =
							refs[k].lifting[index];
						data[k * cells.len() + cell - cells.start].body_force[i] =
							refs[k].body_force[index];
					}
				}
			}
		}
		let data = BoxTimeDataShard::new(
			&geometry,
			cells,
			2,
			[0., 1.],
			data,
			BoxTimeDataLimits::default(),
		)?;
		let force = prepared.prepare_time_force(
			&recipe,
			&data,
			DistributedForceLimits {
				max_prepare_work: 2_000_000_000,
				..Default::default()
			},
		)?;
		let bounded = PolynomialBoundary::new(&dense, refs, BoundaryLimits::default())?;
		let mut rotation = vec![vec![0.; m]; m];
		let range = force.local_coordinate_range();
		for j in 0..m {
			let mut e = vec![0.; range.len()];
			if range.contains(&j) {
				e[j - range.start] = 1.;
			}
			let q = prepared.chart().lift_null(&e)?;
			let mut full = vec![0.; geometry.cell_count() * n];
			for peer in 0..env.size()? {
				let mut bounds = [0; 16];
				bounds[..8].copy_from_slice(&u64::try_from(rows.start)?.to_le_bytes());
				bounds[8..].copy_from_slice(&u64::try_from(rows.end)?.to_le_bytes());
				broadcast(&env, peer, &mut bounds)?;
				let a = usize::try_from(u64::from_le_bytes(bounds[..8].try_into()?))?;
				let b = usize::try_from(u64::from_le_bytes(bounds[8..].try_into()?))?;
				for (i, v) in full.iter_mut().enumerate().take(b).skip(a) {
					let x = if rows.contains(&i) {
						q.as_slice()[i - rows.start]
					} else {
						0.
					};
					let mut word = x.to_le_bytes();
					broadcast(&env, peer, &mut word)?;
					*v = f64::from_le_bytes(word);
				}
			}
			let v = dense.coordinates(&full)?;
			for i in 0..m {
				rotation[i][j] = v[i];
			}
		}
		let grid = ConfigurationGrid::uniform(m, -0.3, 0.3, 1, 2, 1024)?;
		let reference = Reference {
			grid: &grid,
			boundary: &bounded,
			rotation: &rotation,
		};
		let limits = BoxKvnHistoryLimits::default();
		let source = CollectiveDynamics::new(&env, &force, &grid, &limits)?;
		let g0 = reference.generator(0.)?;
		let g1 = reference.generator(0.4)?;
		let change = (0..grid.dimension())
			.map(|r| {
				let mut v = vec![Complex64::from(0.); grid.dimension()];
				for (j, a) in g0.row(r) {
					v[j] += a;
				}
				for (j, a) in g1.row(r) {
					v[j] -= a;
				}
				v.iter().map(Complex64::norm_sqr).sum::<f64>()
			})
			.sum::<f64>()
			.sqrt();
		assert!(change > 1e-3, "time dependence must be observable");
		for time_order in [1, 2] {
			let time = TemporalHistoryRecipe::new(
				&source,
				0.4,
				2,
				time_order,
				HistoryStreamLimits {
					max_work_per_visit: 1_000_000_000_000,
					..Default::default()
				},
			)?;
			let h = HistorySystem::assemble_dynamics(
				&reference,
				&vec![Complex64::from(1.); grid.dimension()],
				0.4,
				2,
				time_order,
				SparseLimits::default(),
			)?;
			let mut actual = Vec::new();
			for row in 0..time.dimension() {
				for entry in time.rows(row..row + 1)? {
					let entry = entry?;
					actual.push((entry.row, entry.column, entry.value));
				}
			}
			let actual = SparseMatrix::from_triplets(
				time.dimension(),
				time.dimension(),
				SparseFormat::Csr,
				actual,
				SparseLimits::default(),
			)?;
			for row in 0..time.dimension() {
				let mut v = vec![Complex64::from(0.); time.dimension()];
				for (j, a) in actual.row(row) {
					v[j] += a;
				}
				for (j, a) in h.operator().row(row) {
					v[j] -= a;
				}
				assert!(v.iter().all(|x| x.norm() < 2e-9));
			}
		}
		if !periodic {
			let initial = vec![Complex64::from(1.); grid.dimension()];
			let small = HistorySystem::assemble_dynamics(
				&reference,
				&initial,
				0.001,
				1,
				1,
				SparseLimits::default(),
			)?;
			let spectrum = small.spectral_bounds(SparseLimits::default())?;
			let outcome = prepare_time_box_kvn_history_inverse(
				&env,
				&force,
				&grid,
				0.001,
				1,
				1,
				|_| Ok(Complex64::from(1.)),
				&spectrum,
				&limits,
				DistributedHistoryLimits {
					max_rhs_query_work: 10_000_000_000_000,
					approximation_tolerance: 0.01,
					max_degree: 511,
					..Default::default()
				},
			)?;
			assert!(
				outcome
					.construction
					.is_some_and(|r| r.actual_drift_calls > 0)
			);
			assert!(matches!(
				outcome.inverse,
				DistributedHistoryOutcome::Prepared(_)
			));
		}
		let spectrum = quest_qsvt::reciprocal::SpectralBounds::new(
			0.4,
			2.,
			quest_qsvt::reciprocal::SpectralEvidence::CallerPremise {
				description: "unused zero-RHS factory fixture".into(),
			},
		)?;
		let zero = prepare_time_box_kvn_history_inverse(
			&env,
			&force,
			&grid,
			0.01,
			1,
			1,
			|_| Ok(Complex64::from(0.)),
			&spectrum,
			&limits,
			DistributedHistoryLimits {
				max_rhs_query_work: 10_000_000_000_000,
				..Default::default()
			},
		)?;
		assert!(zero.construction.is_none());
		assert!(
			prepare_time_box_kvn_history_inverse(
				&env,
				&force,
				&grid,
				2.,
				1,
				1,
				|_| Ok(Complex64::from(0.)),
				&spectrum,
				&limits,
				DistributedHistoryLimits::default()
			)
			.is_err()
		);
	}
	Ok(())
}
