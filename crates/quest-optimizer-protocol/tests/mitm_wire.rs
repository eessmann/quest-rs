use googletest::{Result, prelude::*};
use quest_math::{Gate, Operation, Sequence};
use quest_optimizer_protocol::{
    MAX_REQUEST_BYTES, MitmLimits, Outcome, Request, RequestEnvelope, VERSION, decode, encode,
};

#[gtest]
fn versioned_mitm_request_round_trips_and_rejects_excess_limits() -> Result<()> {
    expect_eq!(VERSION, 2);
    let limits = MitmLimits::for_qubits(1)?;
    expect_eq!(limits.max_depth, 12);
    limits.validate(1)?;
    let target = Sequence {
        qubits: 1,
        operations: vec![Operation {
            gate: Gate::H,
            targets: vec![0],
            controls: vec![],
        }],
    };
    let request = RequestEnvelope {
        version: VERSION,
        seed: 7,
        request: Request::ExactMitm { target, limits },
    };
    let bytes = encode(&request, MAX_REQUEST_BYTES)?;
    let restored: RequestEnvelope = decode(&bytes, MAX_REQUEST_BYTES)?;
    expect_true!(matches!(restored.request, Request::ExactMitm { .. }));
    let mut invalid = limits;
    invalid.max_states = 32_769;
    expect_true!(invalid.validate(1).is_err());
    Ok(())
}

#[gtest]
fn mitm_terminal_outcomes_remain_distinct() -> Result<()> {
    let variants = [
        Outcome::NoCandidate { explored: 4 },
        Outcome::Incomplete {
            reason: "work".into(),
            explored: 5,
        },
        Outcome::Exhausted { explored: 6 },
        Outcome::Unresolved {
            precision_bits: 4096,
            explored: 7,
        },
    ];
    for outcome in variants {
        let bytes = encode(&outcome, quest_optimizer_protocol::MAX_OUTPUT_BYTES)?;
        let restored: Outcome = decode(&bytes, quest_optimizer_protocol::MAX_OUTPUT_BYTES)?;
        expect_eq!(
            std::mem::discriminant(&restored),
            std::mem::discriminant(&outcome)
        );
    }
    Ok(())
}
