#![cfg(all(feature = "mpi", quest_native_mpi))]
#![allow(
	clippy::indexing_slicing,
	clippy::arithmetic_side_effects,
	clippy::too_many_lines,
	clippy::manual_let_else,
	clippy::suboptimal_flops,
	reason = "Fixed eight-row independent dense references deliberately retain separate arithmetic and explicit MPI admission scenarios"
)]
#[allow(
	unused_imports,
	reason = "Googletest prelude exports test macros across native MPI build configurations"
)]
use googletest::prelude::*;
use quest::collective::{CollectiveEnvironment, MpiRuntime};
use quest::distributed_constraints::ConstraintShard;
use quest::distributed_constraints::{ChartOutcome, ConstraintLimits, RankPolicy};
use quest_numerics::constraint_chart::CellInput;
#[gtest]
fn generated_chart_reconstructs_complete_null_coordinates() -> googletest::Result<()> {
	if std::env::var("QUEST_CHART_RANKS").is_err() {
		for (p, split) in [("1", "0"), ("2", "0"), ("4", "0"), ("8", "0"), ("4", "1")] {
			let s = quest_test_support::mpi::MpiTest::new(
				p.parse()?,
				std::time::Duration::from_secs(90),
			)?
			.args([
				"--exact",
				"generated_chart_reconstructs_complete_null_coordinates",
				"--nocapture",
				"--test-threads=1",
			])
			.env("QUEST_CHART_RANKS", p)
			.env("QUEST_CHART_SPLIT", split)
			.status()?;
			expect_true!(s.success());
		}
		return Ok(());
	}
	let rt = MpiRuntime::initialize()?;
	let mut world = rt.world()?;
	quest_test_support::mpi::assert_rank_count(world.size()?)?;
	let comm = if std::env::var("QUEST_CHART_SPLIT").as_deref() == Ok("1") {
		world.split_power_of_two(2)?
	} else {
		world.duplicate()?
	};
	let env = CollectiveEnvironment::builder(&comm)?.build()?;
	let chart = env.prepare_constraint_chart_from_fn(
		4,
		2,
		3,
		ConstraintLimits::default(),
		RankPolicy::default(),
		|cell, i, j| Ok(if i == j { number(cell + 1) } else { 0. }),
		|cell, i, j| {
			Ok(if j == 0 {
				if cell * 2 + i == 7 { 4. } else { 0. }
			} else if j == 1 {
				if cell * 2 + i == 0 { 1. } else { 0. }
			} else {
				if cell * 2 + i == 0 { 2. } else { 0. }
			})
		},
	)?;
	let chart = match chart {
		ChartOutcome::Prepared(c) => c,
		ChartOutcome::Ambiguous(_) => {
			return Err(quest::Error::Value("unexpected ambiguous rank").into());
		}
	};
	expect_eq!(chart.numerical_rank(), 2);
	expect_eq!(chart.nullity(), 6);
	let a = chart
		.local_null_range()
		.map(|i| number(i) + 1.)
		.collect::<Vec<_>>();
	let u = chart.lift_null(&a)?;
	let b = chart.lower_velocity(u.as_slice())?;
	for (x, y) in a.iter().zip(b.as_slice()) {
		expect_true!((x - y).abs() < 1e-11);
	}
	let g = chart.constraint_values(u.as_slice())?;
	for x in g.as_slice() {
		expect_true!(x.abs() < 1e-11);
	}
	Ok(())
}

fn number(value: usize) -> f64 {
	f64::from(u32::try_from(value).unwrap_or_default())
}
fn c(row: usize, col: usize, group: usize) -> f64 {
	let independent = match col {
		0 => {
			if row == 7 {
				9.
			} else {
				0.2 * number((row + group) % 3)
			}
		}
		1 => {
			if row == 0 {
				3.
			} else {
				0.1 * number(row % 2)
			}
		}
		_ => {
			if row == 4 {
				2.
			} else {
				0.15 * number((row + 1) % 3)
			}
		}
	};
	if col == 3 {
		c(row, 0, group) + 2. * c(row, 1, group)
	} else {
		independent
	}
}
fn mass(cell: usize, i: usize, j: usize, group: usize) -> f64 {
	if i == j {
		number(2 + cell + i + group)
	} else {
		0.3
	}
}
fn solve(mut a: Vec<Vec<f64>>, mut b: Vec<f64>) -> Vec<f64> {
	for k in 0..b.len() {
		let pivot = (k..b.len())
			.max_by(|&i, &j| a[i][k].abs().total_cmp(&a[j][k].abs()))
			.unwrap_or(k);
		a.swap(k, pivot);
		b.swap(k, pivot);
		for i in k + 1..b.len() {
			let q = a[i][k] / a[k][k];
			for j in k..b.len() {
				a[i][j] -= q * a[k][j];
			}
			b[i] -= q * b[k];
		}
	}
	for k in (0..b.len()).rev() {
		for j in k + 1..b.len() {
			b[k] -= a[k][j] * b[j];
		}
		b[k] /= a[k][k];
	}
	b
}
#[allow(
	clippy::suspicious_operation_groupings,
	reason = "The two-by-two SPD determinant is a*d-b*b"
)]
fn inverse_mass(v: &[f64], group: usize) -> Vec<f64> {
	let mut out = vec![0.; 8];
	for cell in 0..4 {
		let (a, b, d) = (mass(cell, 0, 0, group), 0.3, mass(cell, 1, 1, group));
		let det = a * d - b * b;
		out[cell * 2] = (d * v[cell * 2] - b * v[cell * 2 + 1]) / det;
		out[cell * 2 + 1] = (a * v[cell * 2 + 1] - b * v[cell * 2]) / det;
	}
	out
}
fn global_sum(comm: &quest_sys::mpi::MpiCommunicator<'_>, value: f64) -> googletest::Result<f64> {
	let mut lane = comm.collective_lane()?;
	let mut sum = 0.;
	for peer in 0..comm.size()? {
		let mut packet = value.to_le_bytes();
		lane.broadcast_bytes(peer, &mut packet)?;
		sum += f64::from_le_bytes(packet);
	}
	Ok(sum)
}
#[gtest]
fn full_chart_matches_independent_dense_projection_and_all_null_directions()
-> googletest::Result<()> {
	if std::env::var("QUEST_CHART_DENSE").is_err() {
		for (p, split) in [("1", "0"), ("2", "0"), ("4", "0"), ("8", "0"), ("4", "1")] {
			let s = quest_test_support::mpi::MpiTest::new(
				p.parse()?,
				std::time::Duration::from_secs(90),
			)?
			.args([
				"--exact",
				"full_chart_matches_independent_dense_projection_and_all_null_directions",
				"--nocapture",
				"--test-threads=1",
			])
			.env("QUEST_CHART_DENSE", p)
			.env("QUEST_CHART_SPLIT", split)
			.status()?;
			expect_true!(s.success());
		}
		return Ok(());
	}
	let rt = MpiRuntime::initialize()?;
	let mut world = rt.world()?;
	quest_test_support::mpi::assert_rank_count(world.size()?)?;
	let split = std::env::var("QUEST_CHART_SPLIT").as_deref() == Ok("1");
	let group = if split {
		usize::try_from(world.rank()?)? / 2
	} else {
		0
	};
	let comm = if split {
		world.split_power_of_two(2)?
	} else {
		world.duplicate()?
	};
	let env = CollectiveEnvironment::builder(&comm)?.build()?;
	let before = env.view().allocated_bytes();
	let mut mass_calls = 0;
	let mut constraint_calls = 0;
	let chart = match env.prepare_constraint_chart_from_fn(
		4,
		2,
		4,
		ConstraintLimits::default(),
		RankPolicy::default(),
		|cell, i, j| {
			mass_calls += 1;
			Ok(mass(cell, i, j, group))
		},
		|cell, i, j| {
			constraint_calls += 1;
			Ok(c(cell * 2 + i, j, group))
		},
	)? {
		ChartOutcome::Prepared(c) => c,
		ChartOutcome::Ambiguous(_) => {
			return Err(quest::Error::Value("ambiguous dense chart").into());
		}
	};
	expect_eq!(chart.numerical_rank(), 3);
	expect_eq!(chart.nullity(), 5);
	expect_eq!(chart.pivots()[0], 3);
	let rows = chart.local_row_range();
	let cols = chart.local_constraint_range();
	expect_eq!(mass_calls, rows.len() * 2);
	expect_eq!(constraint_calls, rows.len() * 4);
	if comm.size()? > 1 {
		expect_true!(rows.len() < 8);
	}
	expect_eq!(
		env.view().allocated_bytes() - before,
		chart.resources().retained_bytes
	);
	let f = (0..8).map(|i| 0.7 + number(i) * 0.3).collect::<Vec<_>>();
	let mass_inverse_force = inverse_mass(&f, group);
	let ci = (0..3)
		.map(|j| (0..8).map(|i| c(i, j, group)).collect::<Vec<_>>())
		.collect::<Vec<_>>();
	let mass_inverse_constraints = ci
		.iter()
		.map(|v| inverse_mass(v, group))
		.collect::<Vec<_>>();
	let gram = (0..3)
		.map(|i| {
			(0..3)
				.map(|j| {
					(0..8)
						.map(|k| ci[i][k] * mass_inverse_constraints[j][k])
						.sum()
				})
				.collect()
		})
		.collect();
	let rhs = (0..3)
		.map(|i| (0..8).map(|k| ci[i][k] * mass_inverse_force[k]).sum())
		.collect();
	let lambda = solve(gram, rhs);
	let expected = (0..8)
		.map(|i| {
			mass_inverse_force[i]
				- (0..3)
					.map(|j| mass_inverse_constraints[j][i] * lambda[j])
					.sum::<f64>()
		})
		.collect::<Vec<_>>();
	let a = chart.project_force_to_null(&f[rows.clone()])?;
	let projected = chart.lift_null(a.as_slice())?;
	for (i, &u) in rows.clone().zip(projected.as_slice()) {
		expect_true!((u - expected[i]).abs() < 2e-11);
	}
	let multipliers = chart.multipliers_from_force(&f[rows.clone()])?;
	expect_eq!(
		multipliers.gauge,
		quest::distributed_constraints::MultiplierGauge::PivotedDependentZero
	);
	let pressure_force = chart.multiplier_force(multipliers.values.as_slice())?;
	for (i, &x) in rows.clone().zip(pressure_force.as_slice()) {
		let expected_f = (0..3).map(|j| ci[j][i] * lambda[j]).sum::<f64>();
		expect_true!((x - expected_f).abs() < 2e-10);
	}
	let known = (0..8).map(|i| 0.2 * number(i + 1)).collect::<Vec<_>>();
	let g = chart.constraint_values(&known[rows.clone()])?;
	for (j, &x) in cols.clone().zip(g.as_slice()) {
		let expected = (0..8).map(|i| c(i, j, group) * known[i]).sum::<f64>();
		expect_true!((x - expected).abs() < 1e-11);
	}
	let lift = chart.lift_constraints(g.as_slice())?;
	expect_true!(lift.compatibility_residual < 1e-11);
	let back = chart.constraint_values(lift.velocity.as_slice())?;
	for (x, y) in g.as_slice().iter().zip(back.as_slice()) {
		expect_true!((x - y).abs() < 2e-10);
	}
	let mut incompatible = g.as_slice().to_vec();
	if cols.contains(&3) {
		incompatible[3 - cols.start] += 1.;
	}
	expect_true!(chart.lift_constraints(&incompatible).is_err());
	let mut directions = Vec::new();
	for k in 0..5 {
		let tail = chart
			.local_null_range()
			.map(|i| f64::from(i == k))
			.collect::<Vec<_>>();
		directions.push(chart.lift_null(&tail)?);
	}
	for i in 0..5 {
		let lowered = chart.lower_velocity(directions[i].as_slice())?;
		for (k, &x) in chart.local_null_range().zip(lowered.as_slice()) {
			expect_true!((x - f64::from(k == i)).abs() < 1e-11);
		}
		let constraints = chart.constraint_values(directions[i].as_slice())?;
		expect_true!(constraints.as_slice().iter().all(|x| x.abs() < 1e-11));
		for j in 0..5 {
			let mut dot = 0.;
			for (cell, (u, v)) in directions[i]
				.as_slice()
				.as_chunks::<2>()
				.0
				.iter()
				.zip(directions[j].as_slice().as_chunks::<2>().0.iter())
				.enumerate()
			{
				let global_cell = rows.start / 2 + cell;
				dot += u[0] * (mass(global_cell, 0, 0, group) * v[0] + 0.3 * v[1])
					+ u[1] * (0.3 * v[0] + mass(global_cell, 1, 1, group) * v[1]);
			}
			expect_true!((global_sum(&comm, dot)? - f64::from(i == j)).abs() < 2e-11);
		}
	}
	drop(directions);
	drop(back);
	drop(lift);
	drop(g);
	drop(pressure_force);
	drop(multipliers);
	drop(projected);
	drop(a);
	drop(chart);
	expect_eq!(env.view().allocated_bytes(), before);
	Ok(())
}

#[gtest]
fn numerical_rank_outcomes_and_collective_admission_are_explicit() -> googletest::Result<()> {
	if std::env::var("QUEST_CHART_ADMISSION").is_err() {
		let s = quest_test_support::mpi::MpiTest::new(4, std::time::Duration::from_secs(90))?
			.args([
				"--exact",
				"numerical_rank_outcomes_and_collective_admission_are_explicit",
				"--nocapture",
				"--test-threads=1",
			])
			.env("QUEST_CHART_ADMISSION", "1")
			.status()?;
		expect_true!(s.success());
		return Ok(());
	}
	let rt = MpiRuntime::initialize()?;
	let comm = rt.world()?;
	quest_test_support::mpi::assert_rank_count(comm.size()?)?;
	let env = CollectiveEnvironment::builder(&comm)?.build()?;
	let rank = usize::try_from(comm.rank()?)?;
	let before = env.view().allocated_bytes();
	let create = |m: usize, limits: ConstraintLimits, policy: RankPolicy| {
		env.prepare_constraint_chart_from_fn(
			4,
			1,
			m,
			limits,
			policy,
			|_, _, _| Ok(1.),
			|cell, _, j| Ok(if cell == j { 1. } else { 0. }),
		)
	};
	let zero = match create(0, ConstraintLimits::default(), RankPolicy::default())? {
		ChartOutcome::Prepared(c) => c,
		ChartOutcome::Ambiguous(_) => return Err(quest::Error::Value("zero rank ambiguous").into()),
	};
	expect_eq!(zero.numerical_rank(), 0);
	expect_eq!(zero.nullity(), 4);
	let tail = vec![2.; zero.local_null_range().len()];
	let v = zero.lift_null(&tail)?;
	expect_eq!(v.as_slice(), tail);
	drop(v);
	drop(zero);
	let full = match create(4, ConstraintLimits::default(), RankPolicy::default())? {
		ChartOutcome::Prepared(c) => c,
		ChartOutcome::Ambiguous(_) => return Err(quest::Error::Value("full rank ambiguous").into()),
	};
	expect_eq!(full.numerical_rank(), 4);
	expect_eq!(full.nullity(), 0);
	let v = full.lift_null(&[])?;
	expect_true!(v.as_slice().iter().all(|x| *x == 0.));
	drop(v);
	drop(full);
	let ambiguous = env.prepare_constraint_chart_from_fn(
		4,
		1,
		1,
		ConstraintLimits::default(),
		RankPolicy::default(),
		|_, _, _| Ok(1.),
		|cell, _, _| Ok(if cell == 3 { 5e-11 } else { 0. }),
	)?;
	match ambiguous {
		ChartOutcome::Ambiguous(e) => {
			expect_eq!(e.candidate_rank, 0);
			expect_true!((e.observed_norm - 5e-11).abs() < 1e-25);
		}
		ChartOutcome::Prepared(_) => {
			return Err(quest::Error::Value("expected numerical rank band").into());
		}
	}
	let mut limits_cases = Vec::new();
	for which in 0..8 {
		let mut limits = ConstraintLimits::default();
		match which {
			0 => limits.max_rows = 3,
			1 => limits.max_constraints = 0,
			2 => limits.max_construct_work = 0,
			3 => limits.max_query_work = 0,
			4 => limits.max_transport_bytes = 0,
			5 => limits.max_local_bytes = 0,
			6 => limits.node_budget = quest::MemoryBudget::new(0),
			_ => limits.ranks_per_node = 0,
		}
		limits_cases.push(limits);
	}
	for limits in limits_cases {
		expect_true!(create(1, limits, RankPolicy::default()).is_err());
		expect_eq!(env.view().allocated_bytes(), before);
	}
	expect_true!(
		create(
			1,
			ConstraintLimits::default(),
			RankPolicy {
				zero_at_most: 1.,
				nonzero_at_least: 0.,
				compatibility_tolerance: 1e-10
			}
		)
		.is_err()
	);
	expect_true!(
		env.prepare_constraint_chart_from_fn(
			if rank == 0 { 3 } else { 4 },
			1,
			1,
			ConstraintLimits::default(),
			RankPolicy::default(),
			|_, _, _| Ok(1.),
			|_, _, _| Ok(0.)
		)
		.is_err()
	);
	expect_true!(
		env.prepare_constraint_chart_from_fn(
			4,
			1,
			1,
			ConstraintLimits::default(),
			RankPolicy::default(),
			|_, _, _| if rank == 0 {
				Err(quest::Error::Allocation)
			} else {
				Ok(1.)
			},
			|_, _, _| Ok(0.)
		)
		.is_err()
	);
	expect_true!(
		env.prepare_constraint_chart_from_fn(
			4,
			1,
			1,
			ConstraintLimits::default(),
			RankPolicy::default(),
			|_, _, _| Ok(if rank == 0 { -1. } else { 1. }),
			|_, _, _| Ok(0.)
		)
		.is_err()
	);
	expect_true!(
		env.prepare_constraint_chart_from_fn(
			4,
			1,
			1,
			ConstraintLimits::default(),
			RankPolicy::default(),
			|_, _, _| Ok(1.),
			|_, _, _| Ok(if rank == 0 { f64::NAN } else { 0. })
		)
		.is_err()
	);
	let fake = ConstraintShard::from_parts(
		4,
		4,
		1,
		0,
		4,
		vec![CellInput {
			cell_id: 0,
			dimension: 1,
			mass: vec![1.],
			constraints_transpose: vec![0.],
		}],
	)?;
	expect_true!(
		env.prepare_constraint_chart(fake, ConstraintLimits::default(), RankPolicy::default())
			.is_err()
	);
	let mut mass = Vec::with_capacity(if rank == 0 { 10000 } else { 1 });
	mass.push(1.);
	let shard = ConstraintShard::from_parts(
		4,
		4,
		1,
		rank,
		4,
		vec![CellInput {
			cell_id: rank,
			dimension: 1,
			mass,
			constraints_transpose: vec![0.],
		}],
	)?;
	expect_true!(
		env.prepare_constraint_chart(
			shard,
			ConstraintLimits {
				max_local_bytes: 30000,
				..ConstraintLimits::default()
			},
			RankPolicy::default()
		)
		.is_err()
	);
	let chart = match create(1, ConstraintLimits::default(), RankPolicy::default())? {
		ChartOutcome::Prepared(c) => c,
		ChartOutcome::Ambiguous(_) => return Err(quest::Error::Value("unexpected chart").into()),
	};
	let stored = env.view().allocated_bytes();
	expect_true!(
		chart
			.lift_null(if rank == 0 { &[1., 2.] } else { &[] })
			.is_err()
	);
	expect_eq!(env.view().allocated_bytes(), stored);
	let input = vec![if rank == 0 { f64::NAN } else { 0. }; chart.local_row_range().len()];
	expect_true!(chart.lower_velocity(&input).is_err());
	drop(chart);
	expect_eq!(env.view().allocated_bytes(), before);
	let bounded = match create(
		1,
		ConstraintLimits {
			max_local_bytes: 12000,
			..ConstraintLimits::default()
		},
		RankPolicy::default(),
	)? {
		ChartOutcome::Prepared(c) => c,
		ChartOutcome::Ambiguous(_) => {
			return Err(quest::Error::Value("bounded chart ambiguous").into());
		}
	};
	let tail = vec![0.; bounded.local_null_range().len()];
	let first = bounded.lift_null(&tail)?;
	let held = env.view().allocated_bytes();
	expect_true!(bounded.lift_null(&tail).is_err());
	expect_eq!(env.view().allocated_bytes(), held);
	drop(first);
	let repeat = bounded.lift_null(&tail)?;
	drop(repeat);
	let row = vec![0.; bounded.local_row_range().len()];
	let mismatch = if rank == 0 {
		bounded.lift_null(&tail)
	} else {
		bounded.lower_velocity(&row)
	};
	expect_true!(mismatch.is_err());
	drop(bounded);
	expect_eq!(env.view().allocated_bytes(), before);
	expect_true!(
		env.prepare_constraint_chart_from_fn(
			4,
			1,
			1,
			ConstraintLimits::default(),
			RankPolicy::default(),
			|_, _, _| Ok(1.),
			|_, _, _| Ok(1e308)
		)
		.is_err()
	);
	expect_eq!(env.view().allocated_bytes(), before);
	Ok(())
}
#[gtest]
fn malformed_cell_coverage_is_rejected_locally() {
	expect_true!(ConstraintShard::from_parts(2, 4, 1, 0, 1, vec![]).is_err());
	expect_true!(ConstraintShard::from_parts(2, 5, 1, 0, 1, vec![]).is_err());
	expect_true!(
		ConstraintShard::from_parts(
			1,
			2,
			1,
			0,
			1,
			vec![CellInput {
				cell_id: 0,
				dimension: 1,
				mass: vec![1.],
				constraints_transpose: vec![0.]
			}]
		)
		.is_err()
	);
	expect_true!(
		ConstraintShard::from_parts(
			1,
			1,
			1,
			0,
			1,
			vec![CellInput {
				cell_id: 1,
				dimension: 1,
				mass: vec![1.],
				constraints_transpose: vec![0.]
			}]
		)
		.is_err()
	);
}
