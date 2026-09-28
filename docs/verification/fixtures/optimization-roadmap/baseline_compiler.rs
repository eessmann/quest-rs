//! Fixed compiler corpus for before/after measurements. Counts requested Rust allocator bytes, not native allocations or RSS.
use std::{
    alloc::{GlobalAlloc, Layout, System},
    sync::atomic::{AtomicUsize, Ordering::Relaxed},
    time::Instant,
};

struct Counting;
static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);
static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);
static REQUESTED: AtomicUsize = AtomicUsize::new(0);
fn added(bytes: usize) {
    ALLOCATIONS.fetch_add(1, Relaxed);
    REQUESTED.fetch_add(bytes, Relaxed);
    let live = LIVE.fetch_add(bytes, Relaxed) + bytes;
    PEAK.fetch_max(live, Relaxed);
}
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            added(layout.size());
        }
        pointer
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        LIVE.fetch_sub(layout.size(), Relaxed);
        unsafe {
            System.dealloc(pointer, layout);
        }
    }
    unsafe fn realloc(&self, pointer: *mut u8, old: Layout, size: usize) -> *mut u8 {
        let result = unsafe { System.realloc(pointer, old, size) };
        if !result.is_null() {
            LIVE.fetch_sub(old.size(), Relaxed);
            added(size);
        }
        result
    }
}
#[global_allocator]
static ALLOCATOR: Counting = Counting;
pub(crate) type Error = Box<dyn std::error::Error>;

use quest_circuit::{Angle, Control, ControlState, Gate, ProgramBuilder, ValidatedProgram};

pub(crate) fn fixture(name: &str) -> Result<ValidatedProgram, Error> {
    fixture_with_width(name, 6)
}
pub(crate) fn fixture_with_width(name: &str, width: usize) -> Result<ValidatedProgram, Error> {
    if width < 6 {
        return Err("corpus requires six active wires".into());
    }
    let mut b = ProgramBuilder::new(width, 0)?;
    let q = (0..6).map(|i| b.qubit(i)).collect::<Result<Vec<_>, _>>()?;
    match name {
        "qft6" => {
            for target in 0..6 {
                b.gate(Gate::H, &[q[target]], &[])?;
                for control in target + 1..6 {
                    b.gate(
                        Gate::Phase(Angle::pi(1, 1_i64 << (control - target))?),
                        &[q[target]],
                        &[Control::new(q[control], ControlState::One)],
                    )?;
                }
            }
            for i in 0..3 {
                b.gate(Gate::Swap, &[q[i], q[5 - i]], &[])?;
            }
        }
        "pauli_evolution6" => {
            for step in 0..8 {
                for wire in 0..5 {
                    b.gate(Gate::H, &[q[wire]], &[])?;
                    b.gate(
                        Gate::X,
                        &[q[wire + 1]],
                        &[Control::new(q[wire], ControlState::One)],
                    )?;
                    b.gate(Gate::Rz(Angle::pi(step + 1, 37)?), &[q[wire + 1]], &[])?;
                    b.gate(
                        Gate::X,
                        &[q[wire + 1]],
                        &[Control::new(q[wire], ControlState::One)],
                    )?;
                    b.gate(Gate::H, &[q[wire]], &[])?;
                }
            }
        }
        "qsvt_oracle6" => {
            // An alternating U/U-adjoint sequence and projector phases, with
            // a retained coherent signal oracle. This is a circuit workload,
            // not a claim about a particular approximated polynomial.
            let mut signal = ProgramBuilder::new(5, 0)?;
            for i in 0..5 {
                signal.gate(Gate::H, &[signal.qubit(i)?], &[])?;
                signal.gate(
                    Gate::Rz(Angle::pi(i as i64 + 1, 19)?),
                    &[signal.qubit(i)?],
                    &[],
                )?;
            }
            let oracle = quest_circuit::OracleFragment::builder(signal.finish()?.bind(&[])?)
                .matrix_tolerance(1e-12)?
                .build()?;
            for i in 0..12 {
                b.gate(Gate::Phase(Angle::pi(2 * i + 1, 31)?), &[q[0]], &[])?;
                let call = if i % 2 == 0 {
                    oracle.clone()
                } else {
                    oracle.adjoint()
                };
                b.oracle(&call, &q[1..], &[Control::new(q[0], ControlState::One)])?;
            }
        }
        "clifford_t6" => {
            let mut state = 0x51_2026_0928_u64;
            for _ in 0..128 {
                state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
                let target = ((state >> 32) % 6) as usize;
                let gate = match state % 4 {
                    0 => Gate::H,
                    1 => Gate::T,
                    2 => Gate::Tdg,
                    _ => Gate::S,
                };
                b.gate(gate, &[q[target]], &[])?;
                if state & 8 != 0 {
                    b.gate(
                        Gate::X,
                        &[q[(target + 1) % 6]],
                        &[Control::new(q[target], ControlState::One)],
                    )?;
                }
            }
        }
        "signed_controls6" => {
            for i in 0..32 {
                let target = i % 3;
                let controls = [
                    Control::new(q[3], ControlState::Zero),
                    Control::new(q[4], ControlState::One),
                ];
                b.gate(
                    Gate::U {
                        theta: Angle::pi(1, 13)?,
                        phi: Angle::pi(-1, 7)?,
                        lambda: Angle::pi(1, 11)?,
                    },
                    &[q[target]],
                    &controls,
                )?;
                b.gate(
                    Gate::Sx,
                    &[q[target]],
                    &[Control::new(q[5], ControlState::Zero)],
                )?;
                b.gate(
                    Gate::Sxdg,
                    &[q[target]],
                    &[Control::new(q[5], ControlState::Zero)],
                )?;
            }
        }
        _ => return Err("unknown fixed corpus".into()),
    }
    Ok(b.finish()?)
}
pub(crate) fn measure<T>(
    case: &str,
    stage: &str,
    sample: u32,
    action: impl FnOnce() -> Result<T, Error>,
) -> bool {
    measure_report(case, stage, sample, action, |_| serde_json::Value::Null)
}
pub(crate) fn measure_report<T>(
    case: &str,
    stage: &str,
    sample: u32,
    action: impl FnOnce() -> Result<T, Error>,
    describe: impl FnOnce(&T) -> serde_json::Value,
) -> bool {
    measure_keep(case, stage, sample, action, describe).is_ok()
}
pub(crate) fn measure_keep<T>(
    case: &str,
    stage: &str,
    sample: u32,
    action: impl FnOnce() -> Result<T, Error>,
    describe: impl FnOnce(&T) -> serde_json::Value,
) -> Result<T, Error> {
    let baseline = LIVE.load(Relaxed);
    PEAK.store(baseline, Relaxed);
    ALLOCATIONS.store(0, Relaxed);
    REQUESTED.store(0, Relaxed);
    let start = Instant::now();
    let result = action();
    let nanos = start.elapsed().as_nanos();
    let allocations = ALLOCATIONS.load(Relaxed);
    let requested = REQUESTED.load(Relaxed);
    let retained = LIVE.load(Relaxed).saturating_sub(baseline);
    let peak = PEAK.load(Relaxed).saturating_sub(baseline);
    let ok = result.is_ok();
    let error = result.as_ref().err().map(ToString::to_string);
    let detail = result.as_ref().ok().map(describe);
    println!(
        "{}",
        serde_json::json!({"corpus_version":1,"case":case,"stage":stage,
        "sample":sample,"status":if ok {"complete"} else {"failed"},"error":error,
        "allocations":allocations,"requested_bytes":requested,"retained_bytes":retained,
        "peak_bytes":peak,"elapsed_ns":nanos,"detail":detail})
    );
    std::hint::black_box(result)
}
fn main() -> Result<(), Error> {
    println!(
        "{}",
        serde_json::json!({"metadata":"Rust requested allocator bytes; excludes allocator metadata, native allocations and RSS; input clones occur outside measured stage"})
    );
    let mut failures = 0;
    for name in [
        "qft6",
        "pauli_evolution6",
        "qsvt_oracle6",
        "clifford_t6",
        "signed_controls6",
    ] {
        for sample in 0..5 {
            if !measure(name, "construction", sample, || fixture(name)) {
                failures += 1;
            }
        }
        let original = fixture(name)?;
        for stage in [
            "unchanged",
            "exact",
            "linear",
            "parity",
            "fusion",
            "existing_combined",
        ] {
            for sample in 0..5 {
                let input = original.clone();
                if !measure(name, stage, sample, || {
                    let result = match stage {
                        "exact" => input.optimize_exact()?.0.bind(&[])?,
                        "linear" => input
                            .optimize_linear(quest_circuit::LinearOptions::default())?
                            .0
                            .bind(&[])?,
                        "parity" => input
                            .optimize_parity(quest_circuit::ParityOptions::default())?
                            .0
                            .bind(&[])?,
                        "fusion" => {
                            input
                                .bind(&[])?
                                .fuse(quest_circuit::FusionOptions::default())?
                                .0
                        }
                        "existing_combined" => {
                            input
                                .optimize_exact()?
                                .0
                                .optimize_linear(quest_circuit::LinearOptions::default())?
                                .0
                                .optimize_parity(quest_circuit::ParityOptions::default())?
                                .0
                                .bind(&[])?
                                .fuse(quest_circuit::FusionOptions::default())?
                                .0
                        }
                        _ => input.bind(&[])?,
                    };
                    Ok(result.plan()?)
                }) {
                    failures += 1;
                }
            }
        }
    }
    let source = format!(
        "input bool flag; qubit[6] q; int i=0; while(i<3) {{ {} i+=1; }}",
        "if(flag){h q[0];x q[2];h q[0];}else{cx q[1],q[3];cx q[1],q[3];} ".repeat(32)
    );
    for sample in 0..5 {
        if !measure("branch_heavy6", "construction", sample, || {
            Ok(quest_circuit::StructuredProgram::parse(&source, "corpus.qasm")?.verify()?)
        }) {
            failures += 1;
        }
    }
    let program = quest_circuit::StructuredProgram::parse(&source, "corpus.qasm")?.verify()?;
    for stage in ["unchanged", "classical", "quantum", "existing_combined"] {
        for sample in 0..5 {
            let input = program.clone();
            if !measure("branch_heavy6", stage, sample, || {
                let output = match stage {
                    "classical" => input
                        .optimize_classical(
                            quest_circuit::language::ssa::optimization::OptimizationLimits::default(
                            ),
                        )?
                        .0,
                    "quantum" => {
                        input
                            .optimize_quantum(quest_circuit::StructuredQuantumOptions::default())?
                            .0
                    }
                    "existing_combined" => input
                        .optimize_classical(
                            quest_circuit::language::ssa::optimization::OptimizationLimits::default(
                            ),
                        )?
                        .0
                        .optimize_quantum(quest_circuit::StructuredQuantumOptions::default())?
                        .0,
                    _ => input,
                };
                Ok(output.lower()?.plan()?)
            }) {
                failures += 1;
            }
        }
    }
    if failures != 0 {
        return Err(format!("{failures} failed samples (retained in JSONL)").into());
    }
    Ok(())
}
