#![cfg(feature = "synthesis")]
use googletest::prelude::*;
use quest_compile::language::{
    GateKind,
    semantic::builder::{Bit, Modifier},
    vm::{GateRequest, Interpreter, QuantumBackend, RunInputs},
};
#[allow(unused_imports)]
use quest_compile::prelude::*;
use quest_compile::{
    Angle, Constructed, Control, ControlState, Gate, Program, ProgramBuilder, QuantumRegionBuilder,
    circuit,
};
#[derive(Default)]
struct Backend {
    trace: Vec<String>,
}
impl QuantumBackend for Backend {
    type Error = std::convert::Infallible;
    fn apply_gate(&mut self, r: GateRequest<'_>) -> Result<(), Self::Error> {
        self.trace
            .push(format!("{:?}:{:?}:{:?}", r.gate, r.targets, r.controls));
        Ok(())
    }
    fn measure(&mut self, q: usize) -> Result<bool, Self::Error> {
        self.trace.push(format!("measure:{q}"));
        Ok(true)
    }
    fn reset(&mut self, q: usize) -> Result<(), Self::Error> {
        self.trace.push(format!("reset:{q}"));
        Ok(())
    }
    fn barrier(&mut self, q: &[usize]) -> Result<(), Self::Error> {
        self.trace.push(format!("barrier:{q:?}"));
        Ok(())
    }
}
#[gtest]
fn finite_import_preserves_measurement_feedback_and_exact_target() -> Result<()> {
    let mut b = QuantumRegionBuilder::new(2, 1)?;
    let q = b.qubit(0)?;
    let other = b.qubit(1)?;
    let c = b.bit(0)?;
    b.gate(
        Gate::Rz(Angle::pi(1, 4)?),
        &[q],
        &[Control::new(other, ControlState::Zero)],
    )?;
    b.measure(q, c)?;
    b.gate_if(c, true, Gate::X, &[other], &[])?;
    b.reset(q)?;
    let p = Program::<Constructed>::from_region(b.finish()?, &[])?
        .verify()?
        .lower()?
        .plan()?;
    expect_eq!(p.exact_captures().len(), 1);
    let mut backend = Backend::default();
    let out =
        Interpreter::default().run(p.ssa(), &mut backend, &RunInputs::default(), p.captures())?;
    expect_eq!(backend.trace.len(), 4);
    expect_true!(out.outputs.contains_key("c0"));
    Ok(())
}
#[gtest]
fn exact_captures_are_typed_evaluated_once_and_kept_distinct_from_floats() -> Result<()> {
    let mut visits = Vec::new();
    let p=circuit!{qubit q;rz(${{visits.push(1);Angle::pi(1,4)?}}) q;rx(${{visits.push(2);0.25}}) q;rz(pi/4) q;}?.verify()?.lower()?.plan()?;
    expect_eq!(visits, vec![1, 2]);
    expect_eq!(p.exact_captures().len(), 1);
    Ok(())
}
#[gtest]
fn typed_builder_supports_effects_definitions_outputs_and_modifiers() -> Result<()> {
    let mut b = ProgramBuilder::new()?;
    let q = b.qubit("q", 1)?;
    let zero = b.bitstring::<1>("0")?;
    let out = b.output::<Bit<1>>("result", &zero)?;
    let turn = b.define_gate("turn", 1, 1, |b, args, qs| b.gate(GateKind::Rz, args, qs))?;
    let theta = b.floating::<64>(0.2)?;
    b.call_gate(
        &turn,
        &[theta],
        std::slice::from_ref(&q),
        &[Modifier::Adjoint],
    )?;
    b.measure(&q, &out)?;
    b.reset(&q)?;
    b.barrier(&[q])?;
    let p = b.finish()?.verify()?.lower()?.plan()?;
    let mut backend = Backend::default();
    Interpreter::default().run(p.ssa(), &mut backend, &RunInputs::default(), p.captures())?;
    expect_eq!(backend.trace.len(), 4);
    Ok(())
}
#[gtest]
fn static_recipes_preserve_runtime_inputs_and_checked_captures() -> Result<()> {
    let p =
        circuit! {input float theta;qubit[2] q;h q[0];ctrl @ rz(0.25) q[0],q[1];rx(theta) q[1];}?
            .verify()?
            .lower()?
            .plan()?;
    expect_eq!(p.dispatch().occurrences(), 2);
    expect_eq!(p.dispatch().gates().count(), 2);
    let mut inputs = RunInputs::default();
    inputs.insert(
        "theta",
        quest_compile::language::vm::ClassicalValue::Scalar(
            quest_compile::language::classical::ScalarValue::floating(
                quest_compile::language::classical::FloatWidth::F64,
                0.7,
            )?,
        ),
    )?;
    let mut a = Backend::default();
    let mut b = Backend::default();
    Interpreter::default().run(p.ssa(), &mut a, &inputs, p.captures())?;
    Interpreter::default().run_prepared(p.ssa(), p.dispatch(), &mut b, &inputs, p.captures())?;
    expect_eq!(a.trace, b.trace);
    Ok(())
}

#[gtest]
fn coherent_extraction_preserves_phase_controls_and_rejects_effects() -> Result<()> {
    let p = circuit! {qubit[2] q;negctrl @ gphase(0.3) q[0];ctrl @ h q[0],q[1];}?
        .verify()?
        .lower()?
        .plan()?;
    let region = p.try_coherent_region()?;
    expect_eq!(region.instructions().len(), 2);
    expect_true!(
        matches!(region.instructions()[0].operation(),quest_compile::Operation::GlobalPhase{controls,..} if controls[0].state()==ControlState::Zero)
    );
    let effect = circuit! {qubit q;reset q;}?.verify()?.lower()?.plan()?;
    expect_true!(effect.try_coherent_region().is_err());
    let dynamic = circuit! {input float t;qubit q;rz(t) q;}?
        .verify()?
        .lower()?
        .plan()?;
    expect_true!(dynamic.try_coherent_region().is_err());
    Ok(())
}

#[gtest]
fn builder_array_reference_call_preserves_mutation_and_returns_value() -> Result<()> {
    use quest_compile::Int;
    let mut b = ProgramBuilder::new()?;
    let initial = b.integer::<32>(4)?;
    let array = b.array("values", &[initial])?;
    let function =
        b.define_array_function::<Int<32>, Int<32>>("increment", 1, true, |b, array| {
            let index = b.integer::<64>(0)?;
            let value = b.array_read(&array, &index)?;
            let one = b.integer::<32>(1)?;
            let sum = value.add(&one)?;
            b.array_write(&array, &index, &sum)?;
            Ok(sum)
        })?;
    let result = b.call_array_function(&function, &array)?;
    b.output("result", &result)?;
    let p = b.finish()?.verify()?.lower()?.plan()?;
    let mut backend = Backend::default();
    let output =
        Interpreter::default().run(p.ssa(), &mut backend, &RunInputs::default(), p.captures())?;
    expect_true!(output.outputs.contains_key("result"));
    Ok(())
}

#[gtest]
fn typed_builder_embeds_finite_exact_feedback_in_common_program() -> Result<()> {
    let mut finite = QuantumRegionBuilder::new(1, 1)?;
    let q = finite.qubit(0)?;
    let bit = finite.bit(0)?;
    finite.gate(Gate::Rz(Angle::pi(1, 4)?), &[q], &[])?;
    finite.measure(q, bit)?;
    finite.gate_if(bit, true, Gate::X, &[q], &[])?;
    let region = finite.finish()?.bind(&[])?;
    let mut b = ProgramBuilder::new()?;
    let q = b.qubit("q", 1)?;
    let zero = b.bitstring::<1>("0")?;
    let bit = b.output("result", &zero)?;
    b.region(region, &[q], &[bit])?;
    let p = b.finish()?.verify()?.lower()?.plan()?;
    expect_eq!(p.exact_captures().len(), 1);
    expect_eq!(p.embedded_regions().len(), 1);
    let mut backend = Backend::default();
    Interpreter::default().run(p.ssa(), &mut backend, &RunInputs::default(), p.captures())?;
    expect_eq!(backend.trace.len(), 3);
    Ok(())
}
#[gtest]
fn prepared_capture_identity_distinguishes_signed_zero() -> Result<()> {
    let p = circuit! {qubit q;rz(${0.0_f64}) q;}?
        .verify()?
        .lower()?
        .plan()?;
    let other = [quest_compile::language::classical::ScalarValue::floating(
        quest_compile::language::classical::FloatWidth::F64,
        -0.0,
    )?];
    let mut backend = Backend::default();
    expect_true!(
        Interpreter::default()
            .run_prepared(
                p.ssa(),
                p.dispatch(),
                &mut backend,
                &RunInputs::default(),
                &other
            )
            .is_err()
    );
    expect_true!(backend.trace.is_empty());
    Ok(())
}
#[gtest]
fn qasm_zero_division_stays_a_runtime_trap_after_prior_effects() -> Result<()> {
    let p = circuit! {qubit q;x q;rx(pi/0) q;}?
        .verify()?
        .lower()?
        .plan()?;
    let mut backend = Backend::default();
    expect_true!(
        Interpreter::default()
            .run_prepared(
                p.ssa(),
                p.dispatch(),
                &mut backend,
                &RunInputs::default(),
                p.captures()
            )
            .is_err()
    );
    expect_eq!(backend.trace.len(), 1);
    Ok(())
}

#[cfg(feature = "synthesis")]
#[gtest]
fn explicit_native_synthesis_uses_exact_capture_target_and_preserves_cancellation() -> Result<()> {
    use quest_compile::{
        CancellationToken, GenerationError, NativeSynthesis, StructuredWorkerError, SynthesisError,
        SynthesisOptions,
    };
    let program = circuit! {qubit q;rz(${Angle::pi(0,1)?}) q;}?.verify()?;
    let (compiled, report) = program.clone().synthesize_rotations(
        &NativeSynthesis::default(),
        0.1,
        17,
        quest_compile::certified::Limits::default(),
    )?;
    expect_eq!(report.rotations.len(), 1);
    expect_true!(matches!(
        report.rotations[0].certificate.base().target().angle,
        quest_compile::certified::AngleTarget::RationalPi { .. }
    ));
    let _ = compiled.lower()?.plan()?;
    let token = CancellationToken::default();
    token.cancel();
    let backend = NativeSynthesis::new(SynthesisOptions {
        cancellation: Some(token),
        ..SynthesisOptions::default()
    });
    expect_true!(matches!(
        program.synthesize_rotations(
            &backend,
            0.1,
            17,
            quest_compile::certified::Limits::default()
        ),
        Err(StructuredWorkerError::Generator(GenerationError::Native(
            SynthesisError::Cancelled
        )))
    ));
    Ok(())
}

#[gtest]
fn typed_mixed_signature_preserves_reference_alias_effects_and_scalar_types() -> Result<()> {
    use quest_compile::{ArrayRef, Int, ValueParameter};
    let mut b = ProgramBuilder::new()?;
    let function = b.define_subroutine(
        "update",
        (
            ArrayRef::<Int<64>, 2>::mutable(),
            (
                ValueParameter::<Int<64>>::new(),
                ValueParameter::<Int<64>>::new(),
            ),
        ),
        |b, (array, (index, value))| {
            b.array_write(&array, &index, &value)?;
            b.array_read(&array, &index)
        },
    )?;
    let zero = b.integer::<64>(0)?;
    let array = b.array("a", &[zero.clone(), zero])?;
    let index = b.integer::<64>(1)?;
    let value = b.integer::<64>(7)?;
    let result = b.call_subroutine(&function, (array.clone(), (index.clone(), value)))?;
    let _out = b.output("result", &result)?;
    let after = b.array_read(&array, &index)?;
    let _after = b.output("after", &after)?;
    let p = b.finish()?.verify()?.lower()?.plan()?;
    let mut backend = Backend::default();
    let run =
        Interpreter::default().run(p.ssa(), &mut backend, &RunInputs::default(), p.captures())?;
    expect_eq!(run.outputs.get("result"), run.outputs.get("after"));
    expect_true!(format!("{:?}", run.outputs.get("after")).contains('7'));
    Ok(())
}
#[gtest]
fn typed_mixed_signature_rejects_readonly_write_and_foreign_arguments() -> Result<()> {
    use quest_compile::{ArrayRef, Int, ValueParameter};
    let mut b = ProgramBuilder::new()?;
    let _bad = b.define_subroutine(
        "bad",
        (
            ArrayRef::<Int<64>, 1>::readonly(),
            ValueParameter::<Int<64>>::new(),
        ),
        |b, (array, value)| {
            let zero = b.integer::<64>(0)?;
            b.array_write(&array, &zero, &value)?;
            Ok(value)
        },
    )?;
    expect_true!(b.finish().is_err());
    let mut b = ProgramBuilder::new()?;
    let f = b.define_subroutine("identity", ValueParameter::<Int<64>>::new(), |_, v| Ok(v))?;
    let other = ProgramBuilder::new()?;
    expect_true!(b.call_subroutine(&f, other.integer::<64>(1)?).is_err());
    Ok(())
}

#[gtest]
fn typed_effectful_procedure_accepts_quantum_and_classical_reference_parameters() -> Result<()> {
    use quest_compile::{ArrayRef, QubitArrayParameter, QubitParameter};
    let mut b = ProgramBuilder::new()?;
    let procedure = b.define_procedure(
        "observe",
        (
            QubitParameter,
            (QubitArrayParameter::<2>, ArrayRef::<Bit<1>, 1>::mutable()),
        ),
        |b, (q, (register, result))| {
            let zero = b.bitstring::<1>("0")?;
            let bit = b.local("measured", &zero)?;
            b.measure(&q, &bit)?;
            b.reset(&register)?;
            let value = b.read(&bit)?;
            let index = b.integer::<64>(0)?;
            b.array_write(&result, &index, &value)
        },
    )?;
    let one = b.qubit("single", 1)?;
    let qs = b.qubit("pair", 2)?;
    let index = b.integer::<64>(0)?;
    let one = b.index(&one, &index)?;
    let zero = b.bitstring::<1>("0")?;
    let result = b.array("result", &[zero])?;
    b.call_procedure(&procedure, (one, (qs, result.clone())))?;
    let answer = b.array_read(&result, &index)?;
    b.output("answer", &answer)?;
    let program = b.finish()?.verify()?.lower()?.plan()?;
    let mut backend = Backend::default();
    Interpreter::default().run(
        program.ssa(),
        &mut backend,
        &RunInputs::default(),
        program.captures(),
    )?;
    expect_eq!(backend.trace, vec!["measure:0", "reset:1", "reset:2"]);
    Ok(())
}

#[gtest]
fn typed_procedure_quantum_shape_and_unitary_context_are_checked() -> Result<()> {
    use quest_compile::{QubitArrayParameter, QubitParameter};
    let mut b = ProgramBuilder::new()?;
    let procedure = b.define_procedure("clear", QubitArrayParameter::<2>, |b, q| b.reset(&q))?;
    let q = b.qubit("q", 3)?;
    b.call_procedure(&procedure, q)?;
    expect_true!(b.finish().is_err());
    let mut b = ProgramBuilder::new()?;
    let effect = b.define_procedure("effect", QubitParameter, |b, q| b.reset(&q))?;
    b.define_gate("invalid", 0, 1, |b, _, qs| {
        for q in qs {
            b.call_procedure(&effect, q.clone())?;
        }
        Ok(())
    })?;
    expect_true!(b.finish().is_err());
    Ok(())
}

#[gtest]
fn immutable_input_specialization_is_typed_explicit_and_reuses_static_dispatch() -> Result<()> {
    use quest_compile::language::{
        classical::{FloatWidth, ScalarValue},
        vm::ClassicalValue,
    };
    let p=circuit!{input float theta;input int count;qubit q;rx(theta) q;int zero=0;int trap=count/zero;}?.verify()?;
    let mut inputs = RunInputs::default();
    inputs.insert(
        "theta",
        ClassicalValue::Scalar(ScalarValue::floating(FloatWidth::F64, 0.25)?),
    )?;
    expect_true!(p.clone().specialize(&inputs).is_err());
    let specialized = p.clone().specialize_partial(&inputs)?.lower()?.plan()?;
    expect_eq!(specialized.dispatch().gates().count(), 1);
    expect_eq!(specialized.input_specializations().len(), 1);
    let mut runtime = RunInputs::default();
    runtime.insert(
        "count",
        ClassicalValue::Scalar(ScalarValue::signed(
            quest_compile::language::classical::Width::new(64)?,
            1,
        )?),
    )?;
    let mut backend = Backend::default();
    expect_true!(
        Interpreter::default()
            .run_prepared(
                specialized.ssa(),
                specialized.dispatch(),
                &mut backend,
                &runtime,
                specialized.captures()
            )
            .is_err()
    );
    expect_eq!(backend.trace.len(), 1);
    let mut bad = RunInputs::default();
    bad.insert("theta", ClassicalValue::Scalar(ScalarValue::boolean(true)))?;
    expect_true!(p.specialize_partial(&bad).is_err());
    let loaded = quest_compile::Program::<quest_compile::Executable>::load_compiled(
        &specialized.export_compiled(quest_compile::ArtifactLimits::default())?,
        quest_compile::ArtifactLimits::default(),
    )?;
    expect_eq!(loaded.input_specializations().len(), 1);
    Ok(())
}

#[gtest]
fn immutable_array_binding_checks_shape_and_preserves_dynamic_bounds_traps() -> Result<()> {
    use quest_compile::language::{
        classical::{FloatWidth, ScalarValue},
        vm::ClassicalValue,
    };
    let p=Program::<Constructed>::parse("input array[float[64], 2] angles; input int index; qubit q; rx(angles[0]) q; rx(angles[index]) q;","array-input")?.verify()?;
    let angle = ClassicalValue::Scalar(ScalarValue::floating(FloatWidth::F64, 0.25)?);
    let mut bad = RunInputs::default();
    bad.insert("angles", ClassicalValue::Array(vec![angle.clone()]))?;
    expect_true!(p.clone().specialize(&bad).is_err());
    let mut good = RunInputs::default();
    good.insert("angles", ClassicalValue::Array(vec![angle.clone(), angle]))?;
    good.insert(
        "index",
        ClassicalValue::Scalar(ScalarValue::signed(
            quest_compile::language::classical::Width::new(64)?,
            3,
        )?),
    )?;
    let bound = p.specialize(&good)?.lower()?.plan()?;
    expect_eq!(bound.dispatch().gates().count(), 1);
    let mut backend = Backend::default();
    expect_true!(
        Interpreter::default()
            .run_prepared(
                bound.ssa(),
                bound.dispatch(),
                &mut backend,
                &RunInputs::default(),
                bound.captures()
            )
            .is_err()
    );
    expect_eq!(backend.trace.len(), 1);
    Ok(())
}

#[gtest]
fn payload_register_shape_is_rejected_before_any_executable_effects() -> Result<()> {
    use quest_compile::{BoundGate, MatrixPolicy};
    for channel in [false, true] {
        let mut b = ProgramBuilder::new()?;
        let register = b.qubit("q", 2)?;
        let zero = b.integer::<64>(0)?;
        let scalar = b.index(&register, &zero)?;
        b.gate(GateKind::H, &[], std::slice::from_ref(&scalar))?;
        let x = BoundGate::X.matrix(MatrixPolicy::default())?;
        if channel {
            b.channel(vec![x], &[register], 1e-12, MatrixPolicy::default())?;
        } else {
            b.matrix(x, &[register], &[])?;
        }
        // No checked executable can be published, so the preceding H cannot run.
        expect_true!(b.finish().and_then(Program::verify).is_err());
        let mut b = ProgramBuilder::new()?;
        let register = b.qubit("q", 2)?;
        let zero = b.integer::<64>(0)?;
        let scalar = b.index(&register, &zero)?;
        let x = BoundGate::X.matrix(MatrixPolicy::default())?;
        if channel {
            b.channel(vec![x], &[scalar], 1e-12, MatrixPolicy::default())?;
        } else {
            b.matrix(x, &[scalar], &[])?;
        }
        b.finish()?.verify()?.lower()?.plan()?;
    }
    Ok(())
}

#[gtest]
fn ranked_builder_arrays_preserve_io_shapes_and_mixed_reference_effects() -> Result<()> {
    use quest_compile::language::{
        classical::{ScalarValue, Width},
        vm::ClassicalValue,
    };
    use quest_compile::{Int, RankedArrayRef};
    let mut b = ProgramBuilder::new()?;
    let update = b.define_procedure(
        "update",
        (
            RankedArrayRef::<Int<64>, 2>::readonly([2, 2]),
            RankedArrayRef::<Int<64>, 2>::mutable([2, 2]),
        ),
        |b, (source, target)| {
            let zero = b.integer::<64>(0)?;
            let one = b.integer::<64>(1)?;
            let value = b.ranked_read(&source, &[zero.clone(), one.clone()])?;
            b.ranked_write(&target, &[one, zero], &value)
        },
    )?;
    let input = b.input_array::<Int<64>, 2>("values", [2, 2])?;
    let zero = b.integer::<64>(0)?;
    let local = b.ranked_array(
        "local",
        [2, 2],
        &[zero.clone(), zero.clone(), zero.clone(), zero],
    )?;
    b.call_procedure(&update, (input.clone(), local.clone()))?;
    let echo = b.define_array_subroutine(
        "echo",
        RankedArrayRef::<Int<64>, 2>::readonly([2, 2]),
        [2, 2],
        |_, a| Ok(a),
    )?;
    let returned = b.call_array_subroutine(&echo, input)?;
    b.output_array("original", &returned)?;
    b.output_array("changed", &local)?;
    let p = b.finish()?.verify()?.lower()?.plan()?;
    let source = p.export_source(quest_compile::qasm::ExportLimits::default())?;
    let p = Program::<Constructed>::parse(&source, "ranked-builder-roundtrip")?
        .verify()?
        .lower()?
        .plan()?;
    let scalar = |n| -> Result<ClassicalValue> {
        Ok(ClassicalValue::Scalar(ScalarValue::signed(
            Width::new(64)?,
            n,
        )?))
    };
    let initial = ClassicalValue::Array(vec![
        ClassicalValue::Array(vec![scalar(1)?, scalar(2)?]),
        ClassicalValue::Array(vec![scalar(3)?, scalar(4)?]),
    ]);
    let mut inputs = RunInputs::default();
    inputs.insert("values", initial.clone())?;
    let result =
        Interpreter::default().run(p.ssa(), &mut Backend::default(), &inputs, p.captures())?;
    expect_eq!(result.outputs.get("original"), Some(&initial));
    expect_eq!(
        result.outputs.get("changed"),
        Some(&ClassicalValue::Array(vec![
            ClassicalValue::Array(vec![scalar(0)?, scalar(0)?]),
            ClassicalValue::Array(vec![scalar(2)?, scalar(0)?])
        ]))
    );
    Ok(())
}

#[gtest]
fn ranked_array_slices_preserve_alias_checks_and_array_return_values() -> Result<()> {
    use quest_compile::{Int, RankedArrayRef};
    let mut b = ProgramBuilder::new()?;
    let echo = b.define_array_subroutine(
        "echo",
        RankedArrayRef::<Int<64>, 2>::readonly([2, 2]),
        [2, 2],
        |_, a| Ok(a),
    )?;
    let zero = b.integer::<64>(0)?;
    let a = b.ranked_array(
        "a",
        [2, 2],
        &[zero.clone(), zero.clone(), zero.clone(), zero.clone()],
    )?;
    let returned = b.call_array_subroutine(&echo, a.clone())?;
    b.output_array("result", &returned)?;
    let row = b.ranked_slice::<Int<64>, 2, 1, 64>(&a, std::slice::from_ref(&zero))?;
    let wrong = b.ranked_array("wrong", [3], &[zero.clone(), zero.clone(), zero])?;
    let touch = b.define_procedure(
        "touch",
        (
            RankedArrayRef::<Int<64>, 1>::mutable([2]),
            RankedArrayRef::<Int<64>, 1>::mutable([2]),
        ),
        |b, (first, second)| {
            let zero = b.integer::<64>(0)?;
            let indices = [zero.clone()];
            b.ranked_write(&first, &indices, &zero)?;
            b.ranked_write(&second, &indices, &zero)
        },
    )?;
    expect_true!(b.call_procedure(&touch, (row.clone(), wrong)).is_err());
    b.call_procedure(&touch, (row.clone(), row))?;
    expect_true!(b.finish().is_err());
    Ok(())
}

#[gtest]
fn ranked_array_admission_rejects_readonly_writes_and_invalid_dimensions() -> Result<()> {
    use quest_compile::{Int, RankedArrayRef};
    let mut b = ProgramBuilder::new()?;
    expect_true!(b.input_array::<Int<64>, 0>("empty", []).is_err());
    expect_true!(b.input_array::<Int<64>, 2>("zero", [2, 0]).is_err());
    b.define_procedure(
        "invalid",
        RankedArrayRef::<Int<64>, 2>::readonly([2, 2]),
        |b, a| {
            let zero = b.integer::<64>(0)?;
            b.ranked_write(&a, &[zero.clone(), zero.clone()], &zero)
        },
    )?;
    expect_true!(b.finish().is_err());
    Ok(())
}
