#![cfg(feature = "distributed")]
#![allow(
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	clippy::too_many_lines,
	clippy::suboptimal_flops,
	clippy::panic_in_result_fn,
	reason = "Bounded complete independent physical reference and MPI schedule matrix"
)]
use quest::{
	collective::{CollectiveEnvironment, MpiRuntime},
	distributed_constraints::{ConstraintLimits, RankPolicy},
};
use quest_cfd::{
	physical_space::{
		BoundaryLimits, BoundaryTimeCoefficient, BoxConstraintOutcome, BoxConstraintRecipe,
		BoxForceRecipe, BoxTimeCoefficient, BoxTimeDataLimits, BoxTimeDataShard,
		ConstraintRecipeLimits, DistributedForceLimits, DistributedPressureLimits,
		ForceRecipeLimits, PhysicalSpace, PolynomialBoundary,
	},
	simplex::BoxBoundary,
};
#[test]
fn full_time_shards_collective_drift_pressure_and_failure_agreement()
-> Result<(), Box<dyn std::error::Error>> {
	if std::env::var_os("QUEST_TIME_CHILD").is_none() {
		for (ranks, split) in [(1, 0), (2, 0), (4, 0), (8, 0), (4, 1)] {
			assert!(
				quest_test_support::mpi::MpiTest::new(
					usize::try_from(ranks)?,
					std::time::Duration::from_secs(180)
				)?
				.args([
					"--exact",
					"full_time_shards_collective_drift_pressure_and_failure_agreement",
					"--nocapture",
					"--test-threads=1"
				])
				.env("QUEST_TIME_CHILD", "1")
				.env("QUEST_TIME_SPLIT", split.to_string())
				.status()?
				.success()
			);
		}
		return Ok(());
	}
	let runtime = MpiRuntime::initialize()?;
	let mut world = runtime.world()?;
	quest_test_support::mpi::assert_rank_count(world.size()?)?;
	let comm = if std::env::var("QUEST_TIME_SPLIT").as_deref() == Ok("1") {
		world.split_power_of_two(2)?
	} else {
		world.duplicate()?
	};
	let env = CollectiveEnvironment::builder(&comm)?.build()?;
	pressure_result_provenance(&env)?;
	for d in [2, 3] {
		for p in [1, 2] {
			for periodic in [false, true] {
				let boundary = if periodic {
					BoxBoundary::Periodic
				} else {
					BoxBoundary::Cavity { lid_speed: 0. }
				};
				let geometry = BoxConstraintRecipe::new(
					d,
					1,
					1.,
					boundary,
					p,
					ConstraintRecipeLimits::default(),
				)?;
				let recipe =
					BoxForceRecipe::new(&geometry, 0.03, 0., ForceRecipeLimits::default())?;
				let BoxConstraintOutcome::Prepared(prepared) = geometry.prepare_collective(
					&env,
					ConstraintLimits::default(),
					RankPolicy::default(),
				)?
				else {
					return Err("ambiguous chart".into());
				};
				let chart = prepared.chart();
				let rows = chart.local_row_range();
				let n = geometry.local_velocity_dimension();
				let range = rows.start / n..rows.end / n;
				let scalar = n / d;
				// The following dense objects are independent bounded test references only.
				let space = PhysicalSpace::box_mesh(d, 1, 1., 0.03, boundary, p)?;
				let nodes = space.velocity_nodes()?;
				let facets = space.boundary_facets()?;
				let field = |x: [f64; 3]| {
					if periodic {
						[0.2, -0.1, 0.]
					} else {
						[x[0], -x[1], 0.]
					}
				};
				let mut data = vec![BoxTimeCoefficient::zero(); 3 * range.len()];
				let mut refs = (0..3)
					.map(|_| BoundaryTimeCoefficient::zero(&space))
					.collect::<Result<Vec<_>, _>>()?;
				for k in 0..3 {
					let scale = [0.1, -0.2, 0.3][k];
					for cell in 0..geometry.cell_count() {
						let mut c = BoxTimeCoefficient::zero();
						for axis in 0..d {
							for node in 0..scalar {
								c.lifting[axis * scalar + node] =
									scale * field(nodes[cell][node])[axis];
								c.body_force[axis * scalar + node] = scale
									* (f64::from(u32::try_from(axis + 1)?)
										+ 0.31
											* space.chart()[space.dimension() - 1]
												[cell * n + axis * scalar + node]);
								c.lifting[axis * scalar + node] += scale
									* 0.1
									* space.chart()[space.dimension() - 1]
										[cell * n + axis * scalar + node];
							}
						}
						for (face, trace) in c.prescribed.iter_mut().enumerate().take(d + 1) {
							if geometry.facet_is_exterior(cell, face)? {
								for mode in 0..geometry.facet_mode_count() {
									let node = geometry.facet_velocity_node(face, mode)?;
									for axis in 0..d {
										trace[axis * scalar + node] =
											scale * field(nodes[cell][node])[axis];
									}
								}
							}
						}
						refs[k].lifting[cell * n..(cell + 1) * n].copy_from_slice(&c.lifting[..n]);
						refs[k].body_force[cell * n..(cell + 1) * n]
							.copy_from_slice(&c.body_force[..n]);
						if range.contains(&cell) {
							data[k * range.len() + cell - range.start] = c;
						}
					}
					for (f, face) in facets.iter().enumerate() {
						for (out, &x) in refs[k].prescribed[f].iter_mut().zip(&face.nodes) {
							let v = field(x);
							for axis in 0..d {
								out[axis] = scale * v[axis];
							}
						}
					}
				}
				let last = &space.chart()[space.dimension() - 1];
				let mut last_body = 0.;
				for cell in 0..geometry.cell_count() {
					for i in 0..n {
						for j in 0..n {
							last_body += last[cell * n + i]
								* geometry.mass_value(cell, i, j)?
								* refs[1].body_force[cell * n + j];
						}
					}
				}
				assert!(
					last_body.abs() > 1e-4,
					"last complete coordinate body source vanished"
				);
				let source = BoxTimeDataShard::new(
					&geometry,
					range,
					2,
					[-1., 1.],
					data,
					BoxTimeDataLimits::default(),
				)?;
				let before = env.view().allocated_bytes();
				let owner = prepared.prepare_time_force(
					&recipe,
					&source,
					DistributedForceLimits {
						max_prepare_work: 2_000_000_000,
						..Default::default()
					},
				)?;
				assert!(owner.maximum_coefficient_net_flux_defect < 1e-9);
				assert!(owner.interval_constraint_defect_envelope < 1e-8);
				let admitted = owner.admit_drift()?;
				assert_eq!(
					admitted.aggregate_work_ceiling,
					admitted.maximum_rank_work * usize::try_from(env.size()?)?
				);
				assert!(owner.prepare_aggregate_work_ceiling >= owner.prepare_maximum_rank_work);
				let bounded = PolynomialBoundary::new(&space, refs, BoundaryLimits::default())?;
				let state = (0..space.dimension())
					.map(|i| Ok(0.007 * f64::from(u32::try_from(i + 1)?).sin()))
					.collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?;
				let base = space.coefficients(&state)?;
				let local_state = chart.lower_velocity(&base[rows.clone()])?;
				for time in [-0.3, 0.17, 0.8] {
					let result = owner.drift_at(time, local_state.as_slice())?;
					let expected = bounded.momentum_force(time, &state)?;
					for (a, b) in result
						.local_momentum_force()
						.iter()
						.zip(&expected[rows.clone()])
					{
						assert!((a - b).abs() < 3e-8);
					}
					let actual = chart.lift_null(result.as_slice())?;
					let want = space.coefficients(&bounded.drift(time, &state)?)?;
					for (a, b) in actual.as_slice().iter().zip(&want[rows.clone()]) {
						assert!((a - b).abs() < 3e-7);
					}
					let pressure =
						owner.recover_pressure(&result, DistributedPressureLimits::default())?;
					let want = bounded.reconstruct_pressure(time, &state)?;
					let flat = want
						.pressure_coefficients
						.iter()
						.flatten()
						.copied()
						.collect::<Vec<_>>();
					for (i, &a) in pressure
						.pressure
						.pressure_range()
						.zip(pressure.pressure.pressure_coefficients())
					{
						assert!((a - flat[i]).abs() < 3e-7);
					}
					assert!(pressure.original_momentum_residual < 1e-7);
				}
				assert!(
					owner
						.drift_at(
							if env.rank()? == 0 { 2. } else { 0.2 },
							local_state.as_slice()
						)
						.is_err()
				);
				assert!(
					owner
						.drift_at(
							0.,
							if env.rank()? == 0 {
								&[f64::NAN]
							} else {
								local_state.as_slice()
							}
						)
						.is_err()
				);
				if env.size()? > 1 {
					assert!(
						owner
							.drift_at(
								if env.rank()? == 0 { 0.1 } else { 0.2 },
								local_state.as_slice()
							)
							.is_err()
					);
					let degree = if env.rank()? == 0 { 0 } else { source.degree() };
					let mut different = Vec::new();
					for k in 0..=degree {
						for cell in source.cell_range() {
							different.push(*source.coefficient(k, cell).ok_or("owned mode")?);
						}
					}
					let different = BoxTimeDataShard::new(
						&geometry,
						source.cell_range(),
						degree,
						source.time_interval(),
						different,
						BoxTimeDataLimits::default(),
					)?;
					assert!(
						prepared
							.prepare_time_force(
								&recipe,
								&different,
								DistributedForceLimits {
									max_prepare_work: 2_000_000_000,
									..Default::default()
								}
							)
							.is_err()
					);
				}
				let live = env.view().allocated_bytes();
				let first = owner.drift_at(0.17, local_state.as_slice())?;
				let second = owner.drift_at(0.17, local_state.as_slice())?;
				assert_eq!(first.local_momentum_force(), second.local_momentum_force());
				assert_eq!(first.as_slice(), second.as_slice());
				assert!(
					owner
						.recover_pressure(
							&first,
							DistributedPressureLimits {
								max_work: 0,
								..Default::default()
							}
						)
						.is_err()
				);
				drop(first);
				drop(second);
				assert_eq!(env.view().allocated_bytes(), live);
				let mut malformed = Vec::new();
				for k in 0..=source.degree() {
					for cell in source.cell_range() {
						malformed.push(
							*source
								.coefficient(k, cell)
								.ok_or("missing owned coefficient")?,
						);
					}
				}
				if env.rank()? == 0 {
					malformed[0].lifting[0] += 0.2;
				}
				let bad = BoxTimeDataShard::new(
					&geometry,
					source.cell_range(),
					source.degree(),
					source.time_interval(),
					malformed,
					BoxTimeDataLimits::default(),
				)?;
				assert!(
					prepared
						.prepare_time_force(
							&recipe,
							&bad,
							DistributedForceLimits {
								max_prepare_work: 2_000_000_000,
								..Default::default()
							}
						)
						.is_err()
				);
				for limits in [
					DistributedForceLimits {
						max_work: 0,
						max_prepare_work: 2_000_000_000,
						..Default::default()
					},
					DistributedForceLimits {
						max_prepare_work: 0,
						..Default::default()
					},
					DistributedForceLimits {
						max_local_bytes: 0,
						..Default::default()
					},
					DistributedForceLimits {
						max_node_bytes: 0,
						..Default::default()
					},
					DistributedForceLimits {
						max_transport_bytes: 0,
						..Default::default()
					},
				] {
					if let Ok(limited) = prepared.prepare_time_force(&recipe, &source, limits) {
						assert!(limited.drift_at(0.17, local_state.as_slice()).is_err());
					}
				}
				assert_eq!(env.view().allocated_bytes(), live);
				drop(local_state);
				drop(owner);
				assert_eq!(env.view().allocated_bytes(), before);
			}
		}
	}
	Ok(())
}

// Zero autonomous source and constant periodic velocities make these deliberately
// indistinguishable by pressure residual alone: metadata must reject mixed queries.
fn pressure_result_provenance(
	env: &CollectiveEnvironment<'_, '_>,
) -> Result<(), Box<dyn std::error::Error>> {
	let geometry = BoxConstraintRecipe::new(
		2,
		1,
		1.,
		BoxBoundary::Periodic,
		1,
		ConstraintRecipeLimits::default(),
	)?;
	let recipe = BoxForceRecipe::new(&geometry, 0.03, 0., ForceRecipeLimits::default())?;
	let BoxConstraintOutcome::Prepared(prepared) =
		geometry.prepare_collective(env, ConstraintLimits::default(), RankPolicy::default())?
	else {
		return Err("ambiguous provenance fixture".into());
	};
	let chart = prepared.chart();
	let rows = chart.local_row_range();
	let n = geometry.local_velocity_dimension();
	let cells = rows.start / n..rows.end / n;
	let source = BoxTimeDataShard::new(
		&geometry,
		cells.clone(),
		0,
		[0., 1.],
		vec![BoxTimeCoefficient::zero(); cells.len()],
		BoxTimeDataLimits::default(),
	)?;
	let owner = prepared.prepare_time_force(
		&recipe,
		&source,
		DistributedForceLimits {
			max_prepare_work: 2_000_000_000,
			..Default::default()
		},
	)?;
	let physical = rows
		.map(|i| if i % n < n / 2 { 0.2 } else { -0.1 })
		.collect::<Vec<_>>();
	let state = chart.lower_velocity(&physical)?;
	let opposite = state.as_slice().iter().map(|a| -a).collect::<Vec<_>>();
	let first = owner.drift_at(0.1, state.as_slice())?;
	let repeated = owner.drift_at(0.1, state.as_slice())?;
	let different_time = owner.drift_at(0.2, state.as_slice())?;
	let different_state = owner.drift_at(0.1, &opposite)?;
	let rank = env.rank()?;
	let choose = |other| {
		if rank == 0 { &first } else { other }
	};
	owner.recover_pressure(choose(&repeated), DistributedPressureLimits::default())?;
	if env.size()? > 1 {
		assert!(
			owner
				.recover_pressure(
					choose(&different_time),
					DistributedPressureLimits::default()
				)
				.is_err(),
			"mixed times accepted"
		);
		assert!(
			owner
				.recover_pressure(
					choose(&different_state),
					DistributedPressureLimits::default()
				)
				.is_err(),
			"mixed states accepted"
		);
	}
	Ok(())
}
