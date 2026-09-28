use googletest::prelude::*;
use quest_language::{
    GateKind, SourceId, SourceSnapshot,
    classical::{ScalarValue, Width},
    semantic::{CompileLimits, admit},
    syntax::parse_source,
    vm::{
        ClassicalValue, GateRequest, Interpreter, InterpreterLimits, QuantumBackend, RunInputs,
        RuntimeCause,
    },
};

fn compile(text: &str) -> Result<quest_language::ssa::VerifiedProgram> {
    let source = SourceSnapshot::new(SourceId::new(41), "vm", text);
    Ok(admit(parse_source(&source)?, CompileLimits::default())?.into_ssa()?)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RecordedGate {
    gate: GateKind,
    parameters: Vec<u64>,
    targets: Vec<usize>,
    controls: Vec<(usize, bool)>,
    inverse: bool,
}

#[derive(Debug, thiserror::Error)]
#[error("recording backend failure")]
struct BackendError;

#[derive(Default)]
struct RecordingBackend {
    gates: Vec<RecordedGate>,
    measurements: Vec<bool>,
    resets: Vec<usize>,
    barriers: Vec<Vec<usize>>,
    fail_after: Option<usize>,
}

impl QuantumBackend for RecordingBackend {
    type Error = BackendError;

    fn apply_gate(&mut self, request: GateRequest<'_>) -> std::result::Result<(), Self::Error> {
        if self.fail_after == Some(self.gates.len()) {
            return Err(BackendError);
        }
        self.gates.push(RecordedGate {
            gate: request.gate,
            parameters: request
                .parameters
                .iter()
                .map(|value| value.to_bits())
                .collect(),
            targets: request.targets.to_vec(),
            controls: request
                .controls
                .iter()
                .map(|control| (control.qubit, control.positive))
                .collect(),
            inverse: request.inverse,
        });
        Ok(())
    }

    fn measure(&mut self, _qubit: usize) -> std::result::Result<bool, Self::Error> {
        Ok(if self.measurements.is_empty() {
            false
        } else {
            self.measurements.remove(0)
        })
    }

    fn reset(&mut self, qubit: usize) -> std::result::Result<(), Self::Error> {
        self.resets.push(qubit);
        Ok(())
    }

    fn barrier(&mut self, qubits: &[usize]) -> std::result::Result<(), Self::Error> {
        self.barriers.push(qubits.to_vec());
        Ok(())
    }
}

fn signed(value: i128) -> Result<ClassicalValue> {
    Ok(ClassicalValue::Scalar(ScalarValue::signed(
        Width::new(64)?,
        value,
    )?))
}

#[gtest]
fn shared_builder_expressions_preserve_materialized_evaluation_and_occurrences() -> Result<()> {
    let mut builder = quest_language::semantic::builder::Builder::new()?;
    let q = builder.qubit("q", 1)?;
    let mut angle = builder.floating::<64>(0.125)?;
    for _ in 0..3 {
        angle = angle.add(&angle)?;
    }
    for _ in 0..2 {
        builder.gate(
            GateKind::Rx,
            std::slice::from_ref(&angle),
            std::slice::from_ref(&q),
        )?;
    }
    let program = builder.finish(CompileLimits::default())?.into_ssa()?;
    let mut backend = RecordingBackend::default();
    Interpreter::default().run(&program, &mut backend, &RunInputs::default(), &[])?;
    expect_eq!(backend.gates.len(), 2);
    for gate in &backend.gates {
        expect_eq!(gate.gate, GateKind::Rx);
        expect_eq!(&gate.parameters, &vec![1.0f64.to_bits()]);
        expect_eq!(&gate.targets, &vec![0]);
    }
    Ok(())
}

fn output_i128(output: &quest_language::vm::RunOutput, name: &str) -> Result<i128> {
    Ok(output
        .outputs
        .get(name)
        .ok_or_else(|| std::io::Error::other("missing output"))?
        .as_scalar()
        .ok_or_else(|| std::io::Error::other("output is not scalar"))?
        .to_i128()?)
}

#[gtest]
fn duplicate_input_insertion_preserves_the_original_value() -> Result<()> {
    let mut inputs = RunInputs::default();
    inputs.insert("flag", ClassicalValue::Scalar(ScalarValue::boolean(true)))?;
    verify_true!(
        inputs
            .insert("flag", ClassicalValue::Scalar(ScalarValue::boolean(false)))
            .is_err()
    )?;
    expect_true!(
        inputs
            .get("flag")
            .and_then(ClassicalValue::as_scalar)
            .ok_or_else(|| std::io::Error::other("missing retained input"))?
            .to_bool()?
    );
    Ok(())
}

fn item<T>(values: &[T], index: usize) -> Result<&T> {
    values
        .get(index)
        .ok_or_else(|| std::io::Error::other("missing test fixture item").into())
}

fn failed<T, E>(result: std::result::Result<T, E>) -> Result<E> {
    result
        .err()
        .ok_or_else(|| std::io::Error::other("expected runtime failure").into())
}

#[gtest]
fn executes_cfg_backedges_branches_returns_and_owned_outputs() -> Result<()> {
    let program = compile(
        "def add(int n) -> int { return n + 1; } input bool flag; int n = 0; while (n < 3) { n += 1; } if (flag) { n = add(n); } output int answer = n;",
    )?;
    for (flag, expected) in [(false, 3), (true, 4)] {
        let mut inputs = RunInputs::default();
        inputs.insert("flag", ClassicalValue::Scalar(ScalarValue::boolean(flag)))?;
        let mut backend = RecordingBackend::default();
        let output = Interpreter::default().run(&program, &mut backend, &inputs, &[])?;
        expect_eq!(output_i128(&output, "answer")?, expected);
        expect_true!(backend.gates.is_empty());
    }
    Ok(())
}

#[gtest]
fn executes_break_continue_nested_calls_and_scalar_builtins() -> Result<()> {
    let program = compile(
        "def inner(int n) -> int { return n + 1; } def outer(int n) -> int { return inner(n); } int sum=0; for int i in [0:1:6] { if (i == 2) { continue; } if (i == 5) { break; } sum += i; } output int answer=outer(sum)+popcount(uint(15));",
    )?;
    let mut backend = RecordingBackend::default();
    let output = Interpreter::default().run(&program, &mut backend, &RunInputs::default(), &[])?;
    expect_eq!(output_i128(&output, "answer")?, 13);
    expect_true!(backend.gates.is_empty());
    Ok(())
}

#[gtest]
fn arrays_mutable_references_and_runtime_alias_checks_use_actual_indices() -> Result<()> {
    let program = compile(
        "def bump(mutable array[int,2] xs) { xs[1] += 4; } array[int,2] values = {1,2}; bump(values); output array[int,2] answer = values;",
    )?;
    let mut backend = RecordingBackend::default();
    let output = Interpreter::default().run(&program, &mut backend, &RunInputs::default(), &[])?;
    let values = output.outputs["answer"]
        .as_array()
        .ok_or_else(|| std::io::Error::other("output is not array"))?;
    expect_eq!(
        item(values, 0)?
            .as_scalar()
            .ok_or_else(|| std::io::Error::other("array item is not scalar"))?
            .to_i128()?,
        1
    );
    expect_eq!(
        item(values, 1)?
            .as_scalar()
            .ok_or_else(|| std::io::Error::other("array item is not scalar"))?
            .to_i128()?,
        6
    );

    let alias = compile(
        "def touch(mutable array[int,2] a, mutable array[int,2] b) { a[0] = 1; b[0] = 2; } input int i; input int j; array[int,2,2] values = {{0,0},{0,0}}; touch(values[i], values[j]);",
    )?;
    let mut inputs = RunInputs::default();
    inputs.insert("i", signed(1)?)?;
    inputs.insert("j", signed(1)?)?;
    let error = failed(Interpreter::default().run(&alias, &mut backend, &inputs, &[]))?;
    verify_true!(matches!(error.cause, RuntimeCause::Alias))?;
    Ok(())
}

#[gtest]
fn captures_dynamic_parameters_and_user_gate_adjoint_powers_are_exactly_ordered() -> Result<()> {
    let mut candidate = compile(
        "gate pair(a) q { rx(a) q; x q; } qubit[2] q; float theta = 0.25; negctrl @ inv @ pow(2) @ pair(theta) q[0], q[1];",
    )?
    .into_unverified();
    let constant = candidate
        .blocks
        .iter_mut()
        .flat_map(|block| &mut block.instructions)
        .find(|item| matches!(item.kind, quest_language::ssa::InstructionKind::Constant(value) if value.ty() == quest_language::classical::ScalarType::Float(quest_language::classical::FloatWidth::F64)))
        .ok_or_else(|| std::io::Error::other("missing float constant"))?;
    constant.kind = quest_language::ssa::InstructionKind::Capture {
        index: 0,
        ty: item(&constant.results, 0)?.ty.clone(),
    };
    constant.effect = constant.kind.effect();
    constant.accesses = constant.kind.accesses();
    let program = candidate.verify(CompileLimits::default())?;
    let capture = ScalarValue::floating(quest_language::classical::FloatWidth::F64, 0.375)?;
    let mut backend = RecordingBackend::default();
    let output =
        Interpreter::default().run(&program, &mut backend, &RunInputs::default(), &[capture])?;
    expect_eq!(output.completed_quantum, 4);
    expect_eq!(backend.gates.len(), 4);
    expect_eq!(item(&backend.gates, 0)?.gate, GateKind::X);
    expect_true!(item(&backend.gates, 0)?.inverse);
    expect_eq!(item(&backend.gates, 1)?.gate, GateKind::Rx);
    expect_true!(item(&backend.gates, 1)?.inverse);
    expect_eq!(item(&backend.gates, 2)?, item(&backend.gates, 0)?);
    expect_eq!(item(&backend.gates, 3)?, item(&backend.gates, 1)?);
    for gate in &backend.gates {
        expect_eq!(gate.controls, vec![(0, false)]);
    }
    expect_eq!(
        item(&backend.gates, 1)?.parameters,
        vec![0.375f64.to_bits()]
    );
    Ok(())
}

#[gtest]
fn intrinsic_and_explicit_controls_measurement_reset_and_barrier_are_dispatched() -> Result<()> {
    let program = compile(
        "qubit[3] q; ctrl @ cx q[0], q[1], q[2]; bit result = measure q[2]; if (bool(result)) { reset q[1]; } barrier q[2], q[0]; output bit answer = result;",
    )?;
    let mut backend = RecordingBackend {
        measurements: vec![true],
        ..RecordingBackend::default()
    };
    let output = Interpreter::default().run(&program, &mut backend, &RunInputs::default(), &[])?;
    expect_eq!(backend.gates.len(), 1);
    expect_eq!(item(&backend.gates, 0)?.gate, GateKind::Cx);
    expect_eq!(item(&backend.gates, 0)?.targets, vec![2]);
    expect_eq!(
        item(&backend.gates, 0)?.controls,
        vec![(0, true), (1, true)]
    );
    expect_eq!(backend.resets, vec![1]);
    expect_eq!(backend.barriers, vec![vec![2, 0]]);
    expect_eq!(
        output
            .outputs
            .get("answer")
            .and_then(ClassicalValue::as_scalar)
            .ok_or_else(|| std::io::Error::other("missing answer"))?
            .raw_bits()?,
        1
    );
    Ok(())
}

#[gtest]
fn broadcasts_user_gates_and_vector_controls_in_lane_order() -> Result<()> {
    let program = compile(
        "gate turn(a) q { rx(a) q; } qubit[2] controls; qubit[2] targets; negctrl @ turn(0.5) controls, targets;",
    )?;
    let mut backend = RecordingBackend::default();
    let output = Interpreter::default().run(&program, &mut backend, &RunInputs::default(), &[])?;
    expect_eq!(output.allocated_qubits, 4);
    expect_eq!(backend.gates.len(), 2);
    expect_eq!(item(&backend.gates, 0)?.targets, vec![2]);
    expect_eq!(item(&backend.gates, 0)?.controls, vec![(0, false)]);
    expect_eq!(item(&backend.gates, 1)?.targets, vec![3]);
    expect_eq!(item(&backend.gates, 1)?.controls, vec![(1, false)]);
    Ok(())
}

#[gtest]
fn physically_allocates_verified_partially_initialized_arrays_and_bitstrings() -> Result<()> {
    let program = compile(
        "array[int,2] values; values[0]=3; values[1]=4; bit[2] bits; bits[0]=bit(true); bits[1]=bit(false); output int answer=values[0]+values[1]; output bit[2] flags=bits;",
    )?;
    let mut backend = RecordingBackend::default();
    let output = Interpreter::default().run(&program, &mut backend, &RunInputs::default(), &[])?;
    expect_eq!(output_i128(&output, "answer")?, 7);
    expect_eq!(
        output
            .outputs
            .get("flags")
            .and_then(ClassicalValue::as_scalar)
            .ok_or_else(|| std::io::Error::other("missing flags"))?
            .raw_bits()?,
        1
    );
    Ok(())
}

#[gtest]
fn dynamic_indices_and_all_runtime_limits_fail_before_unsafe_progress() -> Result<()> {
    let indexed = compile("input int i; array[int,2] xs = {1,2}; output int answer = xs[i];")?;
    let mut inputs = RunInputs::default();
    inputs.insert("i", signed(2)?)?;
    let mut backend = RecordingBackend::default();
    let error = failed(Interpreter::default().run(&indexed, &mut backend, &inputs, &[]))?;
    verify_true!(matches!(error.cause, RuntimeCause::Value(_)))?;

    let endless = compile("while (true) { }")?;
    let error = failed(
        Interpreter::new(InterpreterLimits {
            steps: 8,
            ..InterpreterLimits::default()
        })
        .run(&endless, &mut backend, &RunInputs::default(), &[]),
    )?;
    verify_true!(matches!(error.cause, RuntimeCause::StepLimit))?;

    let calls = compile("def c() { } def b() { c(); } def a() { b(); } a();")?;
    let error = failed(
        Interpreter::new(InterpreterLimits {
            call_frames: 2,
            ..InterpreterLimits::default()
        })
        .run(&calls, &mut backend, &RunInputs::default(), &[]),
    )?;
    verify_true!(matches!(error.cause, RuntimeCause::FrameLimit))?;

    let error = failed(
        Interpreter::new(InterpreterLimits {
            storage_bytes: 1,
            ..InterpreterLimits::default()
        })
        .run(&indexed, &mut backend, &inputs, &[]),
    )?;
    verify_true!(matches!(error.cause, RuntimeCause::StorageLimit))?;

    let powered = compile("gate pair q { x q; h q; } qubit q; pow(100) @ pair q;")?;
    let error = failed(
        Interpreter::new(InterpreterLimits {
            steps: 20,
            ..InterpreterLimits::default()
        })
        .run(&powered, &mut backend, &RunInputs::default(), &[]),
    )?;
    verify_true!(matches!(error.cause, RuntimeCause::StepLimit))?;
    expect_true!(backend.gates.is_empty());
    Ok(())
}

#[gtest]
fn backend_failures_retain_location_context_and_completed_prefix() -> Result<()> {
    let text = "qubit[2] q; x q[0]; h q[1];";
    let source = SourceSnapshot::new(SourceId::new(41), "vm", text);
    let program = compile(text)?;
    let mut backend = RecordingBackend {
        fail_after: Some(1),
        ..RecordingBackend::default()
    };
    let error =
        failed(Interpreter::default().run(&program, &mut backend, &RunInputs::default(), &[]))?;
    verify_true!(matches!(error.cause, RuntimeCause::Backend(_)))?;
    expect_eq!(error.completed_quantum, 1);
    expect_true!(error.block.is_some());
    expect_true!(error.instruction.is_some());
    expect_true!(error.span.is_some());
    expect_false!(error.context.is_empty());
    expect_eq!(backend.gates.len(), 1);
    let mut sources = quest_language::SourceMap::default();
    sources.insert(source)?;
    let diagnostic = error.diagnostic(&sources);
    drop(program);
    drop(sources);
    diagnostic.validate_sources()?;
    expect_eq!(diagnostic.stage, quest_language::Stage::Execution);
    expect_true!(matches!(
        diagnostic.provenance.entity,
        Some(quest_language::Entity::Operation(_))
    ));
    expect_false!(diagnostic.provenance.execution.is_empty());
    expect_eq!(diagnostic.labels.len(), 1);
    expect_true!(diagnostic.notes[0].contains("1 quantum operations"));
    Ok(())
}

#[gtest]
fn range_advance_handles_signed_and_unsigned_boundaries_and_zero_step_assertion() -> Result<()> {
    let ranges = compile(
        "input int start; input int step; input int end; int count = 0; for int i in [start:step:end] { count += 1; } output int answer = count;",
    )?;
    let mut backend = RecordingBackend::default();
    let mut inputs = RunInputs::default();
    inputs.insert(
        "start",
        signed(
            i128::from(i64::MAX)
                .checked_sub(1)
                .ok_or_else(|| std::io::Error::other("fixture subtraction overflow"))?,
        )?,
    )?;
    inputs.insert("step", signed(2)?)?;
    inputs.insert("end", signed(i128::from(i64::MAX))?)?;
    let output = Interpreter::default().run(&ranges, &mut backend, &inputs, &[])?;
    expect_eq!(output_i128(&output, "answer")?, 1);

    let unsigned = compile(
        "input uint start; input uint step; input uint end; int count = 0; for uint i in [start:step:end] { count += 1; } output int answer = count;",
    )?;
    let width = Width::new(64)?;
    let mut unsigned_inputs = RunInputs::default();
    unsigned_inputs.insert(
        "start",
        ClassicalValue::Scalar(ScalarValue::unsigned(width, u64::MAX - 1)?),
    )?;
    unsigned_inputs.insert(
        "step",
        ClassicalValue::Scalar(ScalarValue::unsigned(width, 2)?),
    )?;
    unsigned_inputs.insert(
        "end",
        ClassicalValue::Scalar(ScalarValue::unsigned(width, u64::MAX)?),
    )?;
    let output = Interpreter::default().run(&unsigned, &mut backend, &unsigned_inputs, &[])?;
    expect_eq!(output_i128(&output, "answer")?, 1);

    let mut zero_inputs = RunInputs::default();
    zero_inputs.insert("start", signed(0)?)?;
    zero_inputs.insert("step", signed(0)?)?;
    zero_inputs.insert("end", signed(1)?)?;
    let error = failed(Interpreter::default().run(&ranges, &mut backend, &zero_inputs, &[]))?;
    verify_true!(matches!(error.cause, RuntimeCause::Assertion(_)))?;
    expect_eq!(error.completed_quantum, 0);
    Ok(())
}

#[gtest]
fn integer_gate_powers_preserve_signed_adjoint_and_runtime_unsigned_counts() -> Result<()> {
    let program = compile(
        "gate pair q { h q; s q; } input uint[8] count; qubit q; pow(-1) @ pair q; pow(count) @ x q; pow(int(true)) @ z q; pow(uint[1](bit(true))) @ h q;",
    )?;
    let mut inputs = RunInputs::default();
    inputs.insert(
        "count",
        ClassicalValue::Scalar(ScalarValue::unsigned(Width::new(8)?, 3)?),
    )?;
    let mut backend = RecordingBackend::default();
    Interpreter::default().run(&program, &mut backend, &inputs, &[])?;
    verify_eq!(backend.gates.len(), 7)?;
    verify_eq!(item(&backend.gates, 0)?.gate, GateKind::S)?;
    verify_eq!(item(&backend.gates, 1)?.gate, GateKind::H)?;
    verify_that!(
        backend.gates.iter().take(2).all(|gate| gate.inverse),
        eq(true)
    )?;
    verify_that!(
        backend
            .gates
            .iter()
            .skip(2)
            .take(3)
            .all(|gate| gate.gate == GateKind::X && !gate.inverse),
        eq(true)
    )?;
    verify_eq!(item(&backend.gates, 5)?.gate, GateKind::Z)?;
    verify_eq!(item(&backend.gates, 6)?.gate, GateKind::H)?;
    Ok(())
}

#[gtest]
fn user_gate_angle_arguments_use_radians_without_permitting_float_casts() -> Result<()> {
    let program = compile(
        "gate turn(theta) q { rx(theta) q; } input angle[8] theta; qubit q; turn(theta) q;",
    )?;
    let mut inputs = RunInputs::default();
    inputs.insert(
        "theta",
        ClassicalValue::Scalar(ScalarValue::angle_bits(Width::new(8)?, 128)?),
    )?;
    let mut backend = RecordingBackend::default();
    Interpreter::default().run(&program, &mut backend, &inputs, &[])?;
    verify_eq!(backend.gates.len(), 1)?;
    verify_eq!(
        item(&backend.gates, 0)?.parameters.as_slice(),
        &[std::f64::consts::PI.to_bits()]
    )?;
    Ok(())
}
