use googletest::prelude::*;
use quest::{Complex64, Environment, InterpreterLimits, Program, QubitCount, RunInputs, circuit};

fn isolated(name: &str, body: impl FnOnce() -> Result<()>) -> Result<()> {
	if std::env::var("QUEST_STRUCTURED_TEST").as_deref() == Ok(name) {
		return body();
	}
	let status = std::process::Command::new(std::env::current_exe()?)
		.args(["--exact", name, "--nocapture", "--test-threads=1"])
		.env("QUEST_STRUCTURED_TEST", name)
		.status()?;
	expect_true!(status.success());
	Ok(())
}
#[gtest]
fn structured_feedback_and_density_reset_preserve_observations() -> Result<()> {
	isolated(
		"structured_feedback_and_density_reset_preserve_observations",
		|| {
			let environment = Environment::builder().build().or_fail()?;
			let program = circuit! {
				qubit[2] q;
				output bit outcome;
				h q[0]; cx q[0],q[1];
				outcome = measure q[0];
				if (bool(outcome)) { x q[1]; }
				reset q[0];
			}?;
			let mut prepared = environment
				.prepare((program).verify()?.lower()?.plan()?)
				.or_fail()?;
			let mut state = environment
				.state_vector(QubitCount::new(2).or_fail()?)
				.or_fail()?;
			let result = prepared.run(&mut state, &RunInputs::default()).or_fail()?;
			expect_true!(result.outputs.contains_key("outcome"));
			expect_that!(state.amplitude(0).or_fail()?.re, near(1., 1e-13));
			let mut density = environment
				.density_matrix(QubitCount::new(2).or_fail()?)
				.or_fail()?;
			prepared
				.run(&mut density, &RunInputs::default())
				.or_fail()?;
			expect_that!(density.entry(0, 0).or_fail()?.re, near(1., 1e-13));
			Ok(())
		},
	)
}
#[gtest]
fn runtime_loop_capture_once_and_step_failure_report_prefix() -> Result<()> {
	isolated(
		"runtime_loop_capture_once_and_step_failure_report_prefix",
		|| {
			let mut captures = Vec::new();
			let program = circuit! {
				gate turn(a) q { rz(a) q; }
				qubit q; output int count=0;
				h q;
				while(count<3) { turn(${{captures.push(1); 0.25}}) q; count+=1; }
			}?;
			expect_eq!(captures, vec![1]);
			let (macro_diagnostic, source_diagnostic) = {
				let environment = Environment::builder().build().or_fail()?;
				let mut prepared = environment
					.prepare((program).verify()?.lower()?.plan()?)
					.or_fail()?;
				let mut state = environment
					.state_vector(QubitCount::new(1).or_fail()?)
					.or_fail()?;
				let result = prepared.run(&mut state, &RunInputs::default()).or_fail()?;
				expect_eq!(
					result
						.outputs
						.get("count")
						.and_then(|value| value.as_scalar())
						.map(quest::language::classical::ScalarValue::to_i128),
					Some(Ok(3))
				);
				expect_that!(
					state.amplitude(0).or_fail()?.im,
					near(-0.375_f64.sin() / 2_f64.sqrt(), 1e-13)
				);
				let looping = circuit! {qubit q; while(true){x q;}}?;
				let mut loop_plan = environment
					.prepare((looping).verify()?.lower()?.plan()?)
					.or_fail()?;
				let error = loop_plan
					.run_with_limits(
						&mut state,
						&RunInputs::default(),
						InterpreterLimits {
							steps: 100,
							..InterpreterLimits::default()
						},
					)
					.unwrap_err();
				let quest::Error::StructuredExecution(error) = error else {
					fail!("structured error expected")?;
					return Ok(());
				};
				expect_true!(matches!(
					error.cause,
					quest::language::vm::RuntimeCause::StepLimit
				));
				expect_gt!(error.completed_quantum, 0);
				expect_false!(error.context.is_empty());
				let macro_diagnostic = error.diagnostic().clone();
				expect_true!(macro_diagnostic.labels.is_empty());
				expect_true!(
					macro_diagnostic
						.notes
						.iter()
						.any(|note| note.contains("Rust source location:"))
				);
				expect_true!(matches!(
					macro_diagnostic.provenance.entity,
					Some(quest::language::Entity::Operation(_))
				));
				expect_false!(macro_diagnostic.provenance.execution.is_empty());

				let sourced = Program::parse(
					"qubit q; input int value; if (value > 0) { x q; }",
					"runtime.qasm",
				)?;
				let mut source_plan = environment
					.prepare((sourced).verify()?.lower()?.plan()?)
					.or_fail()?;
				let source_error = source_plan
					.run(&mut state, &RunInputs::default())
					.unwrap_err();
				let source_diagnostic = source_error.diagnostic().or_fail()?.clone();
				expect_eq!(source_diagnostic.labels.len(), 1);
				(macro_diagnostic, source_diagnostic)
			};
			expect_false!(quest_sys::is_quest_env_init());
			quest_sys::finalize_quest_env().or_fail()?;
			macro_diagnostic.validate_sources()?;
			source_diagnostic.validate_sources()?;
			#[cfg(feature = "codespan-reporting")]
			expect_true!(
				quest::language::render_plain(&source_diagnostic)?.contains("runtime.qasm")
			);
			expect_false!(
				source_diagnostic
					.sources
					.slice(source_diagnostic.labels[0].span)?
					.is_empty()
			);
			expect_eq!(
				source_diagnostic
					.sources
					.get(quest::language::SourceId::new(1))
					.or_fail()?
					.name(),
				"runtime.qasm"
			);
			Ok(())
		},
	)
}
#[gtest]
#[expect(
	clippy::arithmetic_side_effects,
	reason = "Independent finite two-qubit analytical matrix fixture"
)]
fn standard_cu_and_cx_preserve_full_phase_on_each_basis_column() -> Result<()> {
	isolated(
		"standard_cu_and_cx_preserve_full_phase_on_each_basis_column",
		|| {
			let environment = Environment::builder().build().or_fail()?;
			let program = circuit! {
				include "stdgates.inc";
				qubit[2] q;
				cu(0.7,-0.4,0.2,0.3) q[1],q[0];
				CX q[1],q[0];
			}?;
			let mut prepared = environment
				.prepare((program).verify()?.lower()?.plan()?)
				.or_fail()?;
			let mut state = environment
				.state_vector(QubitCount::new(2).or_fail()?)
				.or_fail()?;
			let cis = |x: f64| Complex64::new(x.cos(), x.sin());
			// The specification's active block is exp(i gamma) U3.1, then CX.
			// U3.1 includes exp(i theta/2) relative to the conventional Euler matrix.
			let c = 0.35_f64.cos();
			let s = 0.35_f64.sin();
			let u = [
				[cis(0.65) * c, -cis(0.85) * s],
				[cis(0.25) * s, cis(0.45) * c],
			];
			for column in 0..4 {
				state.init_zero().or_fail()?;
				if column & 1 != 0 {
					state.x(0).or_fail()?;
				}
				if column & 2 != 0 {
					state.x(1).or_fail()?;
				}
				prepared.run(&mut state, &RunInputs::default()).or_fail()?;
				for row in 0..4 {
					let expected = if column < 2 {
						if row == column {
							Complex64::new(1., 0.)
						} else {
							Complex64::new(0., 0.)
						}
					} else if row >= 2 {
						u[(row - 2) ^ 1][column - 2]
					} else {
						Complex64::new(0., 0.)
					};
					expect_that!(
						(state.amplitude(row).or_fail()? - expected).norm(),
						near(0., 1e-13)
					);
				}
			}
			Ok(())
		},
	)
}

#[gtest]
#[expect(
	clippy::arithmetic_side_effects,
	reason = "Independent bounded two-qubit complex fixture"
)]
fn prepared_static_dispatch_is_reused_with_dynamic_inputs_and_full_phase() -> Result<()> {
	isolated(
		"prepared_static_dispatch_is_reused_with_dynamic_inputs_and_full_phase",
		|| {
			use quest::language::classical::{FloatWidth, ScalarValue};
			let environment = Environment::builder().build()?;
			let program = circuit! {
				input float theta;
				qubit[2] q;
				h q[0];
				negctrl @ gphase(${quest::Angle::pi(1, 4)?}) q[0];
				ctrl @ rz(0.25) q[0], q[1];
				rx(theta) q[1];
			}?
			.verify()?
			.lower()?
			.plan()?;
			let mut prepared = environment.prepare(program)?;
			expect_eq!(prepared.prepared_static_gates(), 3);
			let prepared_bytes = environment.allocated_bytes();
			let mut actual = environment.state_vector(QubitCount::new(2)?)?;
			for theta in [0.1, -0.7, 0.1] {
				actual.init_zero()?;
				let mut inputs = RunInputs::default();
				inputs.insert(
					"theta",
					quest::ClassicalValue::Scalar(ScalarValue::floating(FloatWidth::F64, theta)?),
				)?;
				let result = prepared.run(&mut actual, &inputs)?;
				expect_eq!(result.completed_quantum, 4);
				// Independent native matrix application checks the relative phase of
				// the negative control branch, including the full scalar phase.
				let phase = Complex64::from_polar(1.0, std::f64::consts::FRAC_PI_4);
				let inverse_sqrt_two = 2.0_f64.sqrt().recip();
				let c = (theta / 2.0).cos();
				let sine = Complex64::new(0.0, -(theta / 2.0).sin());
				let controlled = Complex64::from_polar(1.0, -0.125);
				let reference = [
					phase * inverse_sqrt_two * c,
					controlled * inverse_sqrt_two * c,
					phase * inverse_sqrt_two * sine,
					controlled * inverse_sqrt_two * sine,
				];
				for (index, value) in reference.into_iter().enumerate() {
					expect_that!((actual.amplitude(index)? - value).norm(), lt(2e-14));
				}
			}
			drop(actual);
			expect_eq!(environment.allocated_bytes(), prepared_bytes);
			expect_eq!(prepared.prepared_static_gates(), 3);
			Ok(())
		},
	)
}

#[gtest]
#[expect(
	clippy::arithmetic_side_effects,
	reason = "Bounded complex differences in the native specialization equivalence fixture"
)]
fn immutable_specialization_reuses_dispatch_and_rejects_runtime_rebinding() -> Result<()> {
	isolated(
		"immutable_specialization_reuses_dispatch_and_rejects_runtime_rebinding",
		|| {
			use quest::language::classical::{FloatWidth, ScalarValue};
			let environment = Environment::builder().build()?;
			let source =
				"input float theta; qubit q; output float observed = theta; h q; rx(theta) q;";
			let mut inputs = RunInputs::default();
			inputs.insert(
				"theta",
				quest::ClassicalValue::Scalar(ScalarValue::floating(FloatWidth::F64, -0.7)?),
			)?;
			let mut dynamic = environment.prepare(
				Program::parse(source, "dynamic.qasm")?
					.verify()?
					.lower()?
					.plan()?,
			)?;
			let specialized = Program::parse(source, "specialized.qasm")?
				.verify()?
				.specialize(&inputs)?
				.lower()?
				.plan()?;
			expect_eq!(specialized.input_specializations().len(), 1);
			let mut fixed = environment.prepare(specialized)?;
			expect_eq!(dynamic.prepared_static_gates(), 1);
			expect_eq!(fixed.prepared_static_gates(), 2);
			let mut expected = environment.state_vector(QubitCount::new(1)?)?;
			let mut actual = environment.state_vector(QubitCount::new(1)?)?;
			let reference = dynamic.run(&mut expected, &inputs)?;
			for _ in 0..3 {
				actual.init_zero()?;
				let output = fixed.run(&mut actual, &RunInputs::default())?;
				expect_eq!(output.outputs, reference.outputs);
				expect_eq!(output.completed_quantum, reference.completed_quantum);
				for index in 0..2 {
					expect_that!(
						(actual.amplitude(index)? - expected.amplitude(index)?).norm(),
						lt(2e-14)
					);
				}
			}
			// A supplied value cannot override the immutable specialization. Input
			// admission must reject it before any quantum effect changes the register.
			actual.init_zero()?;
			expect_true!(fixed.run(&mut actual, &inputs).is_err());
			expect_eq!(actual.amplitude(0)?, Complex64::new(1.0, 0.0));
			expect_eq!(actual.amplitude(1)?, Complex64::new(0.0, 0.0));
			expect_eq!(fixed.prepared_static_gates(), 2);
			Ok(())
		},
	)
}
