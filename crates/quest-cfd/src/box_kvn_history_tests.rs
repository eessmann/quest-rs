#![allow(
	clippy::many_single_char_names,
	clippy::used_underscore_binding,
	clippy::as_conversions,
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	clippy::panic_in_result_fn,
	clippy::too_many_lines,
	clippy::suboptimal_flops,
	reason = "Bounded independent dense chart-aligned whole generator/history MPI tests"
)]
use super::*;
const RESIDUAL_TARGET: f64 = 1e-2;
use crate::{
	history::HistorySystem,
	physical_space::{
		BoxConstraintOutcome, BoxConstraintRecipe, BoxForceRecipe, ConstraintRecipeLimits,
		DistributedForceLimits, ForceRecipeLimits, PhysicalSpace,
	},
	simplex::BoxBoundary,
};
use quest::{
	collective::MpiRuntime,
	distributed_constraints::{ConstraintLimits, RankPolicy},
};
use quest_numerics::{Interval, SparseFormat, SparseLimits, SparseMatrix};

fn resolved_signal(signal: f64, error: f64, observed: f64, allowance: f64) -> bool {
	signal.is_finite()
		&& error.is_finite()
		&& observed.is_finite()
		&& allowance.is_finite()
		&& signal >= 4e-4
		&& error >= 0.
		&& allowance >= 0.
		&& error <= 2e-5
		&& signal >= 10. * (error + allowance)
		&& observed >= signal - error - allowance
}

#[test]
fn resolved_signal_rejects_the_historical_error_dominated_response() {
	assert!(!resolved_signal(1.1e-5, 0.001_158, 0.001_16, 1e-10));
	assert!(!resolved_signal(0., 0., 0., 0.));
	assert!(!resolved_signal(5e-4, 1e-5, 0., 1e-10));
	assert!(!resolved_signal(5e-4, 1e-5, 4.99e-4, f64::NAN));
	assert!(resolved_signal(5e-4, 1e-5, 4.99e-4, 1e-10));
}

// Outward residual of the rounded reference, independent of elimination.
fn residual_upper(h: &HistorySystem, x: &[Complex64]) -> Result<f64, CfdError> {
	let point = Interval::point;
	let mut squared = point(0.)?;
	for row in 0..h.operator().rows() {
		let mut re = point(-h.rhs()[row].re)?;
		let mut im = point(-h.rhs()[row].im)?;
		for (column, a) in h.operator().row(row) {
			let br = point(x[column].re)?;
			let bi = point(x[column].im)?;
			re = re.checked_add(
				point(a.re)?
					.checked_mul(br)?
					.checked_sub(point(a.im)?.checked_mul(bi)?)?,
			)?;
			im = im.checked_add(
				point(a.re)?
					.checked_mul(bi)?
					.checked_add(point(a.im)?.checked_mul(br)?)?,
			)?;
		}
		squared = squared.checked_add(re.square()?.checked_add(im.square()?)?)?;
	}
	Ok(squared.sqrt()?.upper())
}

fn norm_interval(x: &[Complex64]) -> Result<quest_numerics::Interval, CfdError> {
	let mut squared = Interval::point(0.)?;
	for z in x {
		squared = squared.checked_add(
			Interval::point(z.re)?
				.square()?
				.checked_add(Interval::point(z.im)?.square()?)?,
		)?;
	}
	Ok(squared.sqrt()?)
}

fn difference_norm(a: &[Complex64], b: &[Complex64]) -> Result<quest_numerics::Interval, CfdError> {
	assert_eq!(a.len(), b.len());
	let mut squared = Interval::point(0.)?;
	for (a, b) in a.iter().zip(b) {
		let re = Interval::point(a.re)?.checked_sub(Interval::point(b.re)?)?;
		let im = Interval::point(a.im)?.checked_sub(Interval::point(b.im)?)?;
		squared = squared.checked_add(re.square()?.checked_add(im.square()?)?)?;
	}
	Ok(squared.sqrt()?)
}

#[test]
fn resolved_reference_residual_and_complex_difference_are_outward() -> Result<(), CfdError> {
	let zero =
		SparseMatrix::from_triplets(1, 1, SparseFormat::Csr, vec![], SparseLimits::default())?;
	let h = HistorySystem::assemble(
		&zero,
		&[Complex64::from(1.)],
		0.1,
		1,
		1,
		SparseLimits::default(),
	)?;
	assert!(residual_upper(&h, &[Complex64::from(1.), Complex64::from(1.)])? < 1e-14);
	let residual = residual_upper(&h, &[Complex64::from(1.), Complex64::from(1.01)])?;
	assert!(residual > 0.007 && residual < 0.008);
	let a = [Complex64::new(4., 5.)];
	let b = [Complex64::new(1., 1.)];
	let norm = difference_norm(&a, &b)?;
	assert!(norm.lower() <= 5. && norm.upper() >= 5.);
	Ok(())
}

fn launch(name: &str) -> Result<bool, Box<dyn std::error::Error>> {
	if std::env::var_os("QUEST_BOX_HISTORY_CHILD").is_some() {
		return Ok(false);
	}
	for (ranks, split) in [(1, 0), (2, 0), (4, 0), (4, 1)] {
		let status = quest_test_support::mpi::MpiTest::new(
			usize::try_from(ranks)?,
			std::time::Duration::from_secs(240),
		)?
		.args(["--exact", name, "--nocapture", "--test-threads=1"])
		.env("QUEST_BOX_HISTORY_CHILD", "1")
		.env("QUEST_BOX_HISTORY_SPLIT", split.to_string())
		.status()?;
		assert!(status.success(), "history ranks={ranks} split={split}");
	}
	Ok(true)
}
#[test]
fn complete_generated_history_has_no_independent_collective_readers()
-> Result<(), Box<dyn std::error::Error>> {
	if launch(
		"box_kvn_history::tests::complete_generated_history_has_no_independent_collective_readers",
	)? {
		return Ok(());
	}
	let rt = MpiRuntime::initialize()?;
	let mut world = rt.world()?;
	quest_test_support::mpi::assert_rank_count(world.size()?)?;
	let split = std::env::var("QUEST_BOX_HISTORY_SPLIT").as_deref() == Ok("1");
	let comm = if split {
		world.split_power_of_two(2)?
	} else {
		world.duplicate()?
	};
	let env = CollectiveEnvironment::builder(&comm)?
		.memory_budget(quest::MemoryBudget::new(64 * 1024 * 1024))
		.build()?;
	let geometry = BoxConstraintRecipe::new(
		2,
		1,
		1.7,
		BoxBoundary::Periodic,
		1,
		ConstraintRecipeLimits::default(),
	)?;
	let BoxConstraintOutcome::Prepared(prepared) =
		geometry.prepare_collective(&env, ConstraintLimits::default(), RankPolicy::default())?
	else {
		return Err("ambiguous chart".into());
	};
	let f = BoxForceRecipe::new(&geometry, 0., 0., ForceRecipeLimits::default())?;
	let force = prepared.prepare_force(&f, DistributedForceLimits::default())?;
	assert_eq!(force.dimension(), 5);
	let rank = usize::try_from(env.rank()?)?;
	let parts = usize::try_from(env.size()?)?;
	let directory = std::env::temp_dir().join(format!(
		"quest-box-history-{}-{}",
		std::process::id(),
		world.rank()?
	));
	std::fs::create_dir(&directory)?;
	let resolved = std::env::var_os("QUEST_BOX_HISTORY_RESOLVED").is_some();
	let higher_budget = std::env::var_os("QUEST_BOX_HISTORY_HIGH_BUDGET").is_some();
	let horizon = if resolved { 0.1 } else { 0.001 };
	let tolerance = if resolved { 1e-6 } else { 0.03 };
	let l = BoxKvnHistoryLimits {
		max_node_bytes: if resolved {
			1024 * 1024 * 1024
		} else {
			usize::MAX
		},
		ranks_per_node: parts,
		scratch_directory: directory.clone(),
		..Default::default()
	};
	let dense = PhysicalSpace::box_mesh(2, 1, 1.7, 0., BoxBoundary::Periodic, 1)?;
	// Bounded reference only: gather native Q to align physical states with the
	// independent complete dense chart. Tensor grids are not rotation-covariant.
	let m = force.dimension();
	let rows = prepared.chart().local_row_range();
	let range = force.local_coordinate_range();
	let mut rotation = vec![vec![0.; m]; m];
	for j in 0..m {
		let mut e = vec![0.; range.len()];
		if range.contains(&j) {
			e[j - range.start] = 1.;
		}
		let q = prepared.chart().lift_null(&e)?;
		let mut full = vec![0.; geometry.cell_count() * geometry.local_velocity_dimension()];
		for peer in 0..env.size()? {
			let mut bounds = [0; 16];
			bounds[..8].copy_from_slice(&(rows.start as u64).to_le_bytes());
			bounds[8..].copy_from_slice(&(rows.end as u64).to_le_bytes());
			broadcast(&env, peer, &mut bounds)?;
			let a = usize::try_from(u64::from_le_bytes(bounds[..8].try_into()?))?;
			let z = usize::try_from(u64::from_le_bytes(bounds[8..].try_into()?))?;
			for (i, slot) in full.iter_mut().enumerate().take(z).skip(a) {
				let value = if rows.contains(&i) {
					q.as_slice()[i - rows.start]
				} else {
					0.
				};
				let mut b = value.to_le_bytes();
				broadcast(&env, peer, &mut b)?;
				*slot = f64::from_le_bytes(b);
			}
		}
		let coefficients = dense.coordinates(&full)?;
		for i in 0..m {
			rotation[i][j] = coefficients[i];
		}
	}
	for configuration_order in [1, 2] {
		if configuration_order == 1 && std::env::var_os("QUEST_BOX_HISTORY_INVERSE").is_some() {
			continue;
		}
		// One-cell DG1 has exact duplicate cancellation: protocol/degenerate test only.
		// Meaningful nonlinear DG2 configuration is exercised on 1/2 ranks here.
		if configuration_order == 2 && (parts > 2 || split) {
			continue;
		}
		let grid = ConfigurationGrid::uniform(m, -0.3, 0.3, 1, configuration_order, 1024)?;
		let g = grid.generator_from(
			m,
			|x| {
				let y = rotation
					.iter()
					.map(|row| row.iter().zip(x).map(|(a, b)| a * b).sum())
					.collect::<Vec<_>>();
				let dy = dense.drift(&y)?;
				Ok((0..m)
					.map(|j| (0..m).map(|i| rotation[i][j] * dy[i]).sum())
					.collect())
			},
			SparseLimits::default(),
		)?;
		let norm = g.entries().map(|(_, _, v)| v.norm_sqr()).sum::<f64>();
		if configuration_order == 2 {
			assert!(
				norm > 1e-5,
				"must prove nonzero nonlinear coupling before inverse claim"
			);
		} else {
			assert!(
				norm < 1e-22,
				"one-cell DG1 is the degenerate duplicate case"
			);
		}
		if std::env::var_os("QUEST_BOX_HISTORY_INVERSE").is_some() {
			let bump = crate::kvn_recipe::CompactBumpRecipe::new(
				&grid,
				vec![0.04, -0.03, 0.02, 0.05, -0.01],
				0.55,
				crate::kvn_recipe::KvnRecipeLimits::default(),
			)?;
			let initial = (0..grid.dimension())
				.map(|i| bump.amplitude(i))
				.collect::<Result<Vec<_>, _>>()?;
			// Independently isolate complete quadratic convection, never viscosity:
			// F2(a,a) = (F(a)+F(-a)-2F(0))/2 in the same native physical chart.
			let zero = dense.drift(&vec![0.; m])?;
			let nonlinear = grid.generator_from(
				m,
				|x| {
					let y = rotation
						.iter()
						.map(|row| row.iter().zip(x).map(|(a, b)| a * b).sum())
						.collect::<Vec<f64>>();
					let plus = dense.drift(&y)?;
					let minus = dense.drift(&y.iter().map(|v| -v).collect::<Vec<_>>())?;
					let quadratic = plus
						.iter()
						.zip(&minus)
						.zip(&zero)
						.map(|((p, n), z)| 0.5 * (p + n - 2. * z))
						.collect::<Vec<_>>();
					Ok((0..m)
						.map(|j| (0..m).map(|i| rotation[i][j] * quadratic[i]).sum())
						.collect())
				},
				SparseLimits::default(),
			)?;
			let action = nonlinear.matvec(&initial, SparseLimits::default())?;
			let action_norm = action.iter().map(Complex64::norm_sqr).sum::<f64>().sqrt();
			let initial_norm = initial.iter().map(Complex64::norm_sqr).sum::<f64>().sqrt();
			if rank == 0 {
				println!(
					"nonlinear RHS coupling: ||G_NL z0||={} relative={} center=[.04,-.03,.02,.05,-.01] width=.55 boundary_resolved=false",
					action_norm,
					action_norm / initial_norm
				);
			}
			assert!(
				action_norm > 1e-8 && action_norm / initial_norm > 1e-5,
				"actual RHS must excite complete nonlinear generator"
			);
			let h = HistorySystem::assemble(&g, &initial, horizon, 1, 1, SparseLimits::default())?;
			let spectrum = h.spectral_bounds(SparseLimits::default())?;
			let reference = crate::classical_history::solve_reference(
				&h,
				crate::classical_history::ReferenceBudget {
					max_work: 3_000_000_000,
					..Default::default()
				},
			)?;
			// Separate bounded reference stage: independently check all locally
			// owned original-H records, then bind the exact spool digest below.
			let mut comparison_receipt = None;
			let mut matrix_difference_upper = 0.;
			if resolved {
				let mut generator_difference_squared = 0.;
				for row in 0..grid.dimension() {
					let mut differences = vec![Complex64::from(0.); grid.dimension()];
					for (j, a) in g.row(row) {
						differences[j] = a;
					}
					for (j, a) in nonlinear.row(row) {
						differences[j] -= a;
					}
					generator_difference_squared +=
						differences.iter().map(Complex64::norm_sqr).sum::<f64>();
				}
				let generator_difference = generator_difference_squared.sqrt();
				assert!(
					generator_difference < 1e-10,
					"zero-viscosity source must equal complete quadratic drift"
				);
				let source = CollectiveDynamics::new(&env, &force, &grid, &l)?;
				let recipe = TemporalHistoryRecipe::new(
					&source,
					horizon,
					1,
					1,
					HistoryStreamLimits {
						max_work_per_visit: l.max_rank_work,
						..Default::default()
					},
				)?;
				let local_rows =
					rank * recipe.dimension() / parts..(rank + 1) * recipe.dimension() / parts;
				let (mut spool, compared) =
					build_spool(&env, &source, &recipe, local_rows.clone(), &l)?;
				let entries = spool
					.by_ref()
					.map(|entry| entry.map(|e| (e.row, e.column, e.value)))
					.collect::<Result<Vec<_>, _>>()?;
				let actual = SparseMatrix::from_triplets(
					recipe.dimension(),
					recipe.dimension(),
					SparseFormat::Csr,
					entries,
					SparseLimits {
						max_bytes: 128 * 1024 * 1024,
						max_dimension: 512,
						..Default::default()
					},
				)?;
				let mut difference_squared = Interval::point(0.)?;
				for row in local_rows {
					let mut differences =
						vec![(Interval::point(0.)?, Interval::point(0.)?); recipe.dimension()];
					for (j, a) in actual.row(row) {
						differences[j] = (Interval::point(a.re)?, Interval::point(a.im)?);
					}
					for (j, a) in h.operator().row(row) {
						differences[j].0 = differences[j].0.checked_sub(Interval::point(a.re)?)?;
						differences[j].1 = differences[j].1.checked_sub(Interval::point(a.im)?)?;
					}
					for a in differences {
						difference_squared = difference_squared
							.checked_add(a.0.square()?.checked_add(a.1.square()?)?)?;
					}
				}
				let mut global_squared = Interval::point(0.)?;
				for peer in 0..env.size()? {
					let mut bytes = difference_squared.upper().to_le_bytes();
					broadcast(&env, peer, &mut bytes)?;
					global_squared =
						global_squared.checked_add(Interval::point(f64::from_le_bytes(bytes))?)?;
				}
				matrix_difference_upper = global_squared.sqrt()?.upper();
				assert!(
					matrix_difference_upper < 1e-9,
					"complete generated H differs from aligned independent H"
				);
				comparison_receipt = Some(compared);
				drop(spool);
				drop(source);
			}

			let started = std::time::Instant::now();
			let construction = prepare_box_kvn_history_inverse(
				&env,
				&force,
				&grid,
				horizon,
				1,
				1,
				|i| bump.amplitude(i),
				&spectrum,
				&l,
				DistributedHistoryLimits {
					max_rhs_query_work: 64_000_000_000,
					max_local_bytes: if higher_budget {
						1024 * 1024 * 1024
					} else {
						512 * 1024 * 1024
					},
					max_node_bytes: if higher_budget {
						2 * 1024 * 1024 * 1024
					} else if resolved {
						1024 * 1024 * 1024
					} else {
						usize::MAX
					},
					ranks_per_node: parts,
					approximation_tolerance: tolerance,
					max_degree: if resolved { 1023 } else { 2047 },
					max_synthesis_bytes: if resolved {
						64 * 1024 * 1024
					} else {
						32 * 1024 * 1024
					},
					synthesis: {
						let mut policy = quest_qsp::Policy::default();
						if resolved {
							policy.limits.resources.max_work_units = 8_589_934_592;
							policy.limits.resources.max_peak_bytes = 64 * 1024 * 1024;
						}
						policy
					},
					certification: Some(quest_qsp::certification::CertificationPolicy {
						max_bytes: if higher_budget {
							256 * 1024 * 1024
						} else if resolved {
							64 * 1024 * 1024
						} else {
							32 * 1024 * 1024
						},
						..Default::default()
					}),
					producer: quest::qsvt::matching::preprocess::ProducerLimits {
						stream: quest_numerics::sparse_stream::StreamLimits {
							buffer_entries: 16384,
							..Default::default()
						},
						..Default::default()
					},
					initial_retained_bytes: bump.retained_bytes(),
					initial_query_bytes: bump.query_bytes(),
					initial_query_work: bump.query_work(),
					..Default::default()
				},
			)?;
			let receipt = construction
				.construction
				.ok_or("missing nonzero construction")?;
			if let Some(compared) = comparison_receipt {
				assert_eq!(
					compared.local_spool_sha256, receipt.local_spool_sha256,
					"checked H spool must equal actual producer input"
				);
				assert_eq!(compared.local_entries, receipt.local_entries);
			}
			let DistributedHistoryOutcome::Prepared(mut inverse) = construction.inverse else {
				return Err("unexpected zero RHS".into());
			};
			if rank == 0 {
				println!(
					"meaningful nonlinear inverse prepared: n={} m=5 calls={} degree={} qubits={} preparation_seconds={}",
					h.operator().rows(),
					receipt.actual_drift_calls,
					inverse.report().polynomial_degree,
					inverse.qubit_count().get(),
					started.elapsed().as_secs_f64()
				);
			}
			let mut state = env.state_vector_local(inverse.qubit_count())?;
			inverse.initialize_rhs(&mut state)?;
			inverse.apply_inverse(&mut state, false)?;
			let readout =
				inverse.reduce_observable(&state, |_| Ok((true, Complex64::new(1., 0.))))?;
			let norm = reference
				.solution
				.iter()
				.map(Complex64::norm_sqr)
				.sum::<f64>();
			assert!((readout.physical_selected_squared_norm - norm).abs() < 0.15 * norm);
			assert!((readout.total_probability - 1.).abs() < 1e-9);
			// Bounded reference only: gather 486 logical success coefficients, one
			// native amplitude per collective read. No complete native state is saved.
			let local = state.deployment().local_amplitudes();
			let mut recovered = Vec::with_capacity(h.operator().rows());
			for i in 0..h.operator().rows() {
				let owner = i32::try_from(2 * i / local)?;
				let one = state.read_local_amplitudes(2 * i % local, 1)?;
				let mut b = [0; 16];
				b[..8].copy_from_slice(&one[0].re.to_le_bytes());
				b[8..].copy_from_slice(&one[0].im.to_le_bytes());
				broadcast(&env, owner, &mut b)?;
				recovered.push(
					Complex64::new(
						f64::from_le_bytes(b[..8].try_into()?),
						f64::from_le_bytes(b[8..].try_into()?),
					) * inverse.report().physical_rescaling,
				);
			}
			let applied = h.operator().matvec(&recovered, SparseLimits::default())?;
			let rhs_norm = h.rhs().iter().map(Complex64::norm_sqr).sum::<f64>().sqrt();
			let residual = applied
				.iter()
				.zip(h.rhs())
				.map(|(a, b)| (*a - *b).norm_sqr())
				.sum::<f64>()
				.sqrt()
				/ rhs_norm;
			let solution_error = recovered
				.iter()
				.zip(&reference.solution)
				.map(|(a, b)| (*a - *b).norm_sqr())
				.sum::<f64>()
				.sqrt()
				/ norm.sqrt();
			if rank == 0 {
				println!(
					"nonlinear inverse accuracy: alpha={} spectral_lower={} spectral_upper={} evidence={:?} approximation_tolerance={} reciprocal_scale={} recovered_relative_residual={} residual_target={} relative_solution_error={} probability={}",
					inverse.report().alpha,
					spectrum.lower(),
					spectrum.upper(),
					inverse.report().spectral_evidence,
					tolerance,
					inverse.report().reciprocal_scale,
					residual,
					RESIDUAL_TARGET,
					solution_error,
					readout.inverse_success_probability
				);
			}
			assert!(
				residual.is_finite() && residual < RESIDUAL_TARGET,
				"full physical inverse residual {residual} exceeds fixed target"
			);
			assert!(solution_error < 0.02);
			let mut resolved_receipt = None;
			if resolved {
				let baseline = initial.iter().chain(&initial).copied().collect::<Vec<_>>();
				let baseline_norm = norm_interval(&baseline)?;
				let signal = difference_norm(&reference.solution, &baseline)?
					.checked_div(baseline_norm)?
					.lower();
				let error = difference_norm(&recovered, &reference.solution)?
					.checked_div(baseline_norm)?
					.upper();
				let change = difference_norm(&recovered, &baseline)?
					.checked_div(baseline_norm)?
					.lower();
				let zero_generator = SparseMatrix::from_triplets(
					grid.dimension(),
					grid.dimension(),
					SparseFormat::Csr,
					vec![],
					SparseLimits::default(),
				)?;
				let zero_history = HistorySystem::assemble(
					&zero_generator,
					&initial,
					horizon,
					1,
					1,
					SparseLimits::default(),
				)?;
				assert!(residual_upper(&zero_history, &baseline)? < 1e-15);
				drop(zero_history);
				drop(zero_generator);
				let reference_residual_upper = residual_upper(&h, &reference.solution)?;
				let actual_lower = Interval::point(spectrum.lower())?
					.checked_sub(Interval::point(matrix_difference_upper)?)?;
				assert!(actual_lower.lower() > 0.);
				let reference_allowance = Interval::point(reference_residual_upper)?
					.checked_add(
						Interval::point(matrix_difference_upper)?
							.checked_mul(norm_interval(&reference.solution)?)?,
					)?
					.checked_div(actual_lower)?
					.checked_div(baseline_norm)?
					.upper();
				assert!(residual < 1e-5, "fixed nonlinear inverse residual target");
				assert!(
					inverse.report().projector_response_bound.is_some(),
					"do not omit planned phase certificate"
				);
				assert!(
					resolved_signal(signal, error, change, reference_allowance),
					"unresolved nonlinear signal S={signal} E={error} Q={change} allowance={reference_allowance}"
				);
				let endpoint_signal =
					difference_norm(&reference.solution[grid.dimension()..], &initial)?
						.checked_div(norm_interval(&initial)?)?
						.lower();
				if rank == 0 {
					resolved_receipt = Some(serde_json::json!({
						"schema":"quest-cfd-resolved-nonlinear-box-v1", "ranks":parts,
						"layout":{"horizon":horizon,"configuration_order":configuration_order,"configuration_nodes":grid.dimension(),
							"physical_coordinates":m,"history_dimension":h.operator().rows(),"temporal_order":1,"time_slabs":1,
							"center":[0.04,-0.03,0.02,0.05,-0.01],"width":0.55},
						"scope":{"configuration_boundary_resolved":false,"same_physical_ensemble_across_rank_counts":false,
							"physical_convergence_established":false,"complete_nonlinear_generator":true,"resolved_discrete_nonlinear_response":true,
							"empirical_native_separation":true,"certified_total_quantum_error":false},
						"accuracy":{"signal_relative_lower":signal,"inverse_error_relative_upper":error,"observed_change_relative_lower":change,
							"endpoint_signal_relative_lower":endpoint_signal,"reference_forward_allowance_upper":reference_allowance,
							"reference_residual_norm_upper":reference_residual_upper,"generated_reference_matrix_difference_upper":matrix_difference_upper,
							"reference_relative_residual":reference.relative_residual,"inverse_relative_residual":residual},
						"encoding":{"alpha":inverse.report().alpha,"spectral_lower":spectrum.lower(),"spectral_upper":spectrum.upper(),
							"approximation_tolerance":tolerance,"approximation_error_bound":inverse.report().approximation_error_bound,
							"degree":inverse.report().polynomial_degree,"qubits":inverse.qubit_count().get(),
							"projector_response_bound":inverse.report().projector_response_bound,
							"execution_error_premise":inverse.report().execution_amplitude_error_premise,
							"physical_rescaling":inverse.report().physical_rescaling,"reciprocal_scale":inverse.report().reciprocal_scale},
						"source":{"source_identity":format!("{:016x}",inverse.report().source_identity),
							"recipe_metadata_sha256":receipt.recipe_metadata_sha256,
							"local_spool_sha256":receipt.local_spool_sha256,
							"actual_producer_source_drift_calls":receipt.actual_drift_calls,
							"additional_reference_source_drift_calls":comparison_receipt.map(|r| r.actual_drift_calls),
							"modeled_source_peak_bytes":receipt.maximum_rank_peak_bytes,"modeled_source_node_peak_bytes":receipt.node_peak_bytes},
						"budgets":{"native_memory_cap_bytes":64*1024*1024,"source_memory_cap_bytes":512*1024*1024,
							"source_node_memory_cap_bytes":1024*1024*1024,"history_memory_cap_bytes":if higher_budget {1024*1024*1024}else{512*1024*1024},
							"history_node_memory_cap_bytes":if higher_budget {2_u64*1024*1024*1024}else{1024*1024*1024},
							"certificate_bytes_per_owner":if higher_budget {256*1024*1024}else{64*1024*1024},
							"os_address_space_bytes_per_rank":if higher_budget {Some(3_u64*1024*1024*1024)}else{None},"bounded_reference_dimension_cap":512,
							"bounded_reference_byte_cap":128*1024*1024,"reference_modeled_peak_bytes":reference.modeled_peak_bytes},
						"measurement":{"success_probability_measured":readout.inverse_success_probability,"sampling_probability_lower_bound":null},
						"timings":{"rhs_compilation_seconds":inverse.report().timings.rhs_compilation_seconds, "streamed_preprocessing_seconds":inverse.report().timings.streamed_preprocessing_seconds, "reciprocal_construction_seconds":inverse.report().timings.reciprocal_construction_seconds,"phase_synthesis_seconds":inverse.report().timings.phase_synthesis_seconds,"projector_certification_seconds":inverse.report().timings.projector_certification_seconds},"first_inverse_elapsed_seconds":started.elapsed().as_secs_f64()
					}));
				}
			}

			// The reusable whole unitary is checked on deterministic nonzero values
			// in all failure/control/padding sectors, retaining only 64 amplitudes.
			let generated = |index: usize| -> Result<Complex64, CfdError> {
				let x = f64::from(u32::try_from(index).map_err(|_| overflow())?);
				let total = f64::from(u32::try_from(local * parts).map_err(|_| overflow())?).sqrt();
				Ok(Complex64::new((0.13 * x).sin(), (0.17 * x).cos()) / total)
			};
			for offset in (0..local).step_by(64) {
				let words = (offset..(offset + 64).min(local))
					.map(|i| generated(rank * local + i))
					.collect::<Result<Vec<_>, _>>()?;
				state.write_local_amplitudes(offset, &words)?;
			}
			inverse.apply_inverse(&mut state, false)?;
			inverse.apply_inverse(&mut state, true)?;
			for offset in (0..local).step_by(64) {
				for (i, a) in state
					.read_local_amplitudes(offset, 64.min(local - offset))?
					.iter()
					.enumerate()
				{
					assert!((*a - generated(rank * local + offset + i)?).norm() < 1e-9);
				}
			}
			if let Some(mut receipt) = resolved_receipt {
				receipt["all_sector_forward_adjoint_roundtrip_passed"] = serde_json::json!(true);
				receipt["native_inverse_cumulative_seconds"] =
					serde_json::json!(inverse.report().timings.cumulative_inverse_seconds);
				receipt["completed_inverse_replays"] =
					serde_json::json!(inverse.report().timings.completed_inverse_replays);
				receipt["rhs_initialization_seconds"] =
					serde_json::json!(inverse.report().timings.rhs_initialization_seconds);
				receipt["three_replays_elapsed_seconds"] =
					serde_json::json!(started.elapsed().as_secs_f64());
				println!("RESOLVED_NONLINEAR_BOX_JSON={receipt}");
			}

			if rank == 0 {
				println!(
					"nonlinear inverse executed: probability={} physical_norm={} reference_norm={} elapsed_seconds={}",
					readout.inverse_success_probability,
					readout.physical_selected_squared_norm,
					norm,
					started.elapsed().as_secs_f64()
				);
			}

			continue;
		}
		for time_order in [1, 2] {
			if configuration_order == 2 && time_order == 2 {
				continue;
			}
			let before = env.view().allocated_bytes();
			let source = CollectiveDynamics::new(&env, &force, &grid, &l)?;
			let recipe = TemporalHistoryRecipe::new(
				&source,
				0.001,
				1,
				time_order,
				HistoryStreamLimits {
					max_work_per_visit: l.max_rank_work,
					..Default::default()
				},
			)?;
			let local = rank * recipe.dimension() / parts..(rank + 1) * recipe.dimension() / parts;
			let (mut spool, receipt) = build_spool(&env, &source, &recipe, local.clone(), &l)?;
			assert_eq!(receipt.physical_coordinates, 5);
			assert_eq!(receipt.aggregate_work, parts * receipt.maximum_rank_work);
			assert!(receipt.actual_drift_calls <= receipt.maximum_drift_calls);
			let mut triplets = Vec::new();
			for entry in spool.by_ref() {
				let e = entry?;
				triplets.push((e.row, e.column, e.value));
			}
			let actual = SparseMatrix::from_triplets(
				recipe.dimension(),
				recipe.dimension(),
				SparseFormat::Csr,
				triplets,
				SparseLimits::default(),
			)?;
			let initial = (0..grid.dimension())
				.map(|i| Complex64::new(if i == 0 { 1. } else { 0. }, 0.))
				.collect::<Vec<_>>();
			let expected = HistorySystem::assemble(
				&g,
				&initial,
				0.001,
				1,
				time_order,
				SparseLimits::default(),
			)?;
			let mut max_error = 0_f64;
			for row in local {
				let mut reference = vec![Complex64::new(0., 0.); recipe.dimension()];
				for (j, v) in expected.operator().row(row) {
					reference[j] = v;
				}
				for (j, v) in actual.row(row) {
					max_error = max_error.max((v - reference[j]).norm());
					reference[j] = Complex64::new(0., 0.);
				}
				assert!(reference.iter().all(|v| v.norm() < 1e-10));
				assert!(
					(recipe.rhs_value(row, |i| Ok(initial[i]))? - expected.rhs()[row]).norm()
						< 1e-13
				);
			}
			assert!(max_error < 1e-10, "whole matrix error {max_error}");
			// Reader rewind is purely local and preserves immutable source digest.
			let mut reader = spool._store.open(&spool._file)?;
			let mut h = Sha256::new();
			let mut count = 0;
			while let Some(e) = reader.next_entry()? {
				hash_entry(&mut h, e)?;
				count += 1;
			}
			assert_eq!(count, receipt.local_entries);
			assert_eq!(<[u8; 32]>::from(h.finalize()), receipt.local_spool_sha256);
			// Close the independent handle before unlinking the spool, including on NFS.
			drop(reader);
			spool.reader = spool._store.open(&spool._file)?;
			spool.seen = 0;
			spool.last = None;
			spool.failed = false;
			spool.hash = Sha256::new();
			spool.expected[0] ^= 1;
			assert!(
				spool.by_ref().last().is_some_and(|e| e.is_err()),
				"immutable digest mismatch must reject at EOF"
			);
			if rank == 0 {
				println!(
					"configuration={} time_DG={} calls={}/{} rank_work={} aggregate={} global_transport={} local_disk={} matrix_error={} nonlinear_norm={}",
					grid.dimension(),
					time_order,
					receipt.actual_drift_calls,
					receipt.maximum_drift_calls,
					receipt.maximum_rank_work,
					receipt.aggregate_work,
					receipt.global_transport_bytes,
					receipt.local_disk_bytes,
					max_error,
					norm
				);
			}
			drop(spool);
			drop(source);
			assert_eq!(env.view().allocated_bytes(), before);
			assert_eq!(std::fs::read_dir(&directory)?.count(), 0);
		}
	}
	// Collective rejection before physics, including malformed per-rank policy.
	let grid = ConfigurationGrid::uniform(m, -0.3, 0.3, 1, 1, 1024)?;
	let source = CollectiveDynamics::new(&env, &force, &grid, &l)?;
	let recipe = TemporalHistoryRecipe::new(
		&source,
		0.001,
		1,
		1,
		HistoryStreamLimits {
			max_work_per_visit: l.max_rank_work,
			..Default::default()
		},
	)?;
	let range = rank * recipe.dimension() / parts..(rank + 1) * recipe.dimension() / parts;
	let mut calls = l.clone();
	calls.max_drift_calls = 0;
	assert!(build_spool(&env, &source, &recipe, range.clone(), &calls).is_err());
	assert_eq!(source.calls.get(), 0);
	let mut disk = l.clone();
	disk.max_global_disk_bytes = 0;
	assert!(build_spool(&env, &source, &recipe, range.clone(), &disk).is_err());
	let mut work = l.clone();
	work.max_aggregate_work = 0;
	assert!(build_spool(&env, &source, &recipe, range.clone(), &work).is_err());
	let mut transport = l.clone();
	transport.max_transport_bytes = 0;
	assert!(build_spool(&env, &source, &recipe, range.clone(), &transport).is_err());
	let mut mismatch = l.clone();
	if rank == 0 {
		mismatch.max_rank_work -= 1;
	}
	if parts > 1 {
		assert!(CollectiveDynamics::new(&env, &force, &grid, &mismatch).is_err());
	}
	let mut missing = l.clone();
	if rank == 0 {
		missing.scratch_directory = directory.join("absent");
	}
	assert!(build_spool(&env, &source, &recipe, range, &missing).is_err());
	assert_eq!(source.calls.get(), 0);
	// A one-owner write failure still completes exactly the current physics row.
	WRITE_FAILURE_ROW.with(|row| row.set((rank == 0).then_some(0)));
	assert!(
		build_spool(
			&env,
			&source,
			&recipe,
			rank * recipe.dimension() / parts..(rank + 1) * recipe.dimension() / parts,
			&l
		)
		.is_err()
	);
	WRITE_FAILURE_ROW.with(|row| row.set(None));
	assert_eq!(source.calls.get(), 21);
	assert_eq!(std::fs::read_dir(&directory)?.count(), 0);
	let mut validation = l.clone();
	validation.max_source_prepare_work = 0;
	assert!(CollectiveDynamics::new(&env, &force, &grid, &validation).is_err());
	let mut malformed = l.clone();
	malformed.ranks_per_node = 0;
	assert!(CollectiveDynamics::new(&env, &force, &grid, &malformed).is_err());
	assert!(
		TemporalHistoryRecipe::new(&source, f64::NAN, 1, 1, HistoryStreamLimits::default())
			.is_err()
	);
	assert!(
		TemporalHistoryRecipe::new(&source, 0.1, 0, 1, HistoryStreamLimits::default()).is_err()
	);
	let spectral = quest_qsvt::reciprocal::SpectralBounds::new(
		0.1,
		10.,
		quest_qsvt::reciprocal::SpectralEvidence::CallerPremise {
			description: "zero-RHS path only".into(),
		},
	)?;
	let zero = prepare_box_kvn_history_inverse(
		&env,
		&force,
		&grid,
		0.001,
		1,
		1,
		|_| Ok(Complex64::new(0., 0.)),
		&spectral,
		&l,
		DistributedHistoryLimits {
			max_rhs_query_work: 64_000_000_000,
			..Default::default()
		},
	)?;
	assert!(matches!(
		zero.inverse,
		DistributedHistoryOutcome::ZeroRhs { .. }
	));
	assert!(zero.construction.is_none());
	drop(source);
	std::fs::remove_dir(directory)?;
	Ok(())
}

#[test]
#[ignore = "Explicit bounded 243-node nonlinear quantum campaign; run with --ignored"]
fn nonlinear_generated_history_inverse() -> Result<(), Box<dyn std::error::Error>> {
	if std::env::var_os("QUEST_BOX_HISTORY_CHILD").is_some() {
		return complete_generated_history_has_no_independent_collective_readers();
	}
	for ranks in [1, 2] {
		let status = quest_test_support::mpi::MpiTest::new(
			usize::try_from(ranks)?,
			std::time::Duration::from_secs(180),
		)?
		.args([
			"--exact",
			"box_kvn_history::tests::nonlinear_generated_history_inverse",
			"--ignored",
			"--nocapture",
			"--test-threads=1",
		])
		.env("QUEST_BOX_HISTORY_CHILD", "1")
		.env("QUEST_BOX_HISTORY_INVERSE", "1")
		.status()?;
		assert!(
			status.success(),
			"meaningful inverse ranks={ranks} failed/timed out"
		);
	}
	Ok(())
}

#[test]
#[ignore = "Explicit fixed-budget T=.1 resolved nonlinear1/2rank inverse trial"]
fn resolved_nonlinear_generated_history_inverse() -> Result<(), Box<dyn std::error::Error>> {
	if std::env::var_os("QUEST_BOX_HISTORY_CHILD").is_some() {
		return complete_generated_history_has_no_independent_collective_readers();
	}
	for ranks in [1, 2] {
		let status = quest_test_support::mpi::MpiTest::new(
			usize::try_from(ranks)?,
			std::time::Duration::from_secs(600),
		)?
		.args([
			"--exact",
			"box_kvn_history::tests::resolved_nonlinear_generated_history_inverse",
			"--ignored",
			"--nocapture",
			"--test-threads=1",
		])
		.env("QUEST_BOX_HISTORY_CHILD", "1")
		.env("QUEST_BOX_HISTORY_INVERSE", "1")
		.env("QUEST_BOX_HISTORY_RESOLVED", "1")
		.status()?;
		assert!(
			status.success(),
			"fixed resolved inverse ranks={ranks} failed/timed out; budgets will not be relaxed"
		);
	}
	Ok(())
}

fn high_budget_process_metrics() -> Result<serde_json::Value, Box<dyn std::error::Error>> {
	let status = std::fs::read_to_string("/proc/self/status")?;
	let value = |label: &str| -> Result<u64, Box<dyn std::error::Error>> {
		let line = status
			.lines()
			.find(|line| line.starts_with(label))
			.ok_or("missing process-memory metric")?;
		Ok(line
			.split_whitespace()
			.nth(1)
			.ok_or("missing process-memory value")?
			.parse::<u64>()?
			.checked_mul(1024)
			.ok_or("process-memory metric overflow")?)
	};
	let limits = std::fs::read_to_string("/proc/self/limits")?;
	let line = limits
		.lines()
		.find_map(|line| line.strip_prefix("Max address space"))
		.ok_or("missing address-space limit")?;
	let mut tokens = line.split_whitespace();
	let soft = tokens
		.next()
		.ok_or("missing soft AS limit")?
		.parse::<u64>()?;
	let hard = tokens
		.next()
		.ok_or("missing hard AS limit")?
		.parse::<u64>()?;
	assert_eq!(soft, 3 * 1024 * 1024 * 1024, "approved per-rank AS limit");
	assert_eq!(hard, soft);
	let world_rank = [
		"SLURM_PROCID",
		"OMPI_COMM_WORLD_RANK",
		"PMI_RANK",
		"PMIX_RANK",
	]
	.iter()
	.find_map(|name| std::env::var(name).ok());
	Ok(
		serde_json::json!({"peak_rss_bytes":value("VmHWM:")?,"current_rss_bytes":value("VmRSS:")?,"soft_address_space_limit_bytes":soft,"hard_address_space_limit_bytes":hard,"world_rank":world_rank}),
	)
}

#[test]
#[ignore = "Separately approved256MiB certificate/3GiB AS1->2rank nonlinear inverse trial"]
fn higher_budget_resolved_nonlinear_generated_history_inverse()
-> Result<(), Box<dyn std::error::Error>> {
	if std::env::var_os("QUEST_BOX_HISTORY_CHILD").is_some() {
		let started = std::time::Instant::now();
		let result = complete_generated_history_has_no_independent_collective_readers();
		let metrics = high_budget_process_metrics()?;
		println!(
			"HIGH_BUDGET_RANK_METRICS_JSON={}",
			serde_json::json!({"metrics":metrics,"rank_trial_seconds":started.elapsed().as_secs_f64(),"completed":result.is_ok()})
		);
		return result;
	}
	for ranks in [1, 2] {
		let status = quest_test_support::mpi::MpiTest::new(
			usize::try_from(ranks)?,
			std::time::Duration::from_secs(600),
		)?
		.executable("/usr/bin/time")
		.args([
			"-f",
			"HIGH_BUDGET_PROCESS_METRICS elapsed_seconds=%e max_rss_kib=%M exit_code=%x",
			"/bin/sh",
			"-c",
			"ulimit -v 3145728 || exit 99; exec \"$@\"",
			"quest-box-high-budget",
		])
		.arg(std::env::current_exe()?)
		.args([
			"--exact",
			"box_kvn_history::tests::higher_budget_resolved_nonlinear_generated_history_inverse",
			"--ignored",
			"--nocapture",
			"--test-threads=1",
		])
		.env("QUEST_BOX_HISTORY_CHILD", "1")
		.env("QUEST_BOX_HISTORY_INVERSE", "1")
		.env("QUEST_BOX_HISTORY_RESOLVED", "1")
		.env("QUEST_BOX_HISTORY_HIGH_BUDGET", "1")
		.status()?;
		assert!(
			status.success(),
			"separately approved inverse ranks={ranks} failed/timed out; further limits will not be relaxed"
		);
	}
	Ok(())
}

#[test]
fn source_capacity_overflow_agrees_before_rank_broadcast() -> Result<(), Box<dyn std::error::Error>>
{
	if std::env::var_os("QUEST_BOX_OVERFLOW_CHILD").is_none() {
		let status = quest_test_support::mpi::MpiTest::new(2, std::time::Duration::from_secs(20))?
			.args([
				"--exact",
				"box_kvn_history::tests::source_capacity_overflow_agrees_before_rank_broadcast",
				"--nocapture",
				"--test-threads=1",
			])
			.env("QUEST_BOX_OVERFLOW_CHILD", "1")
			.status()?;
		assert!(
			status.success(),
			"rank-local source overflow must agree instead of hanging"
		);
		return Ok(());
	}
	let rt = MpiRuntime::initialize()?;
	let comm = rt.world()?;
	quest_test_support::mpi::assert_rank_count(comm.size()?)?;
	let env = CollectiveEnvironment::builder(&comm)?
		.memory_budget(quest::MemoryBudget::new(usize::MAX))
		.build()?;
	let geometry = BoxConstraintRecipe::new(
		2,
		1,
		1.,
		BoxBoundary::Cavity { lid_speed: 0. },
		1,
		ConstraintRecipeLimits::default(),
	)?;
	let BoxConstraintOutcome::Prepared(chart) = geometry.prepare_collective(
		&env,
		ConstraintLimits {
			max_local_bytes: usize::MAX,
			node_budget: quest::MemoryBudget::new(usize::MAX),
			ranks_per_node: 1,
			..Default::default()
		},
		RankPolicy::default(),
	)?
	else {
		return Err("ambiguous chart".into());
	};
	assert_eq!(chart.chart().nullity(), 1);
	let f = BoxForceRecipe::new(&geometry, 0.01, 0., ForceRecipeLimits::default())?;
	let force = chart.prepare_force(
		&f,
		DistributedForceLimits {
			max_local_bytes: usize::MAX,
			max_node_bytes: usize::MAX,
			ranks_per_node: 1,
			..Default::default()
		},
	)?;
	let grid = ConfigurationGrid::uniform(1, -0.3, 0.3, 300, 1, 1024)?;
	// Accounting-only reservation: no enormous vector/process allocation is made.
	let external = if env.rank()? == 0 {
		usize::MAX - env.view().allocated_bytes() - 32768
	} else {
		0
	};
	let guard = env.reserve_external_bytes(external)?;
	force.admit_drift()?;
	let limits = BoxKvnHistoryLimits {
		max_local_bytes: usize::MAX,
		max_node_bytes: usize::MAX,
		ranks_per_node: 1,
		..Default::default()
	};
	assert!(CollectiveDynamics::new(&env, &force, &grid, &limits).is_err());
	drop(guard);
	Ok(())
}
