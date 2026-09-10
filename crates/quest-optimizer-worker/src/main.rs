//! One-request candidate generation process. Run through the bounded client.
#![forbid(unsafe_code)]
use quest_optimizer_protocol::{Outcome, Request, RequestEnvelope, ResponseEnvelope, VERSION};
use std::io::{Read, Write};

#[cfg(feature = "synthesis")]
mod synthesis;
#[cfg(feature = "zx")]
mod zx;

fn dispatch(request: Request, seed: u64) -> Outcome {
    match request {
        Request::Capabilities => Outcome::Capabilities {
            synthesis: cfg!(feature = "synthesis"),
            zx: cfg!(feature = "zx"),
        },
        Request::Synthesize {
            target,
            epsilon_bits,
        } => synthesize(&target, epsilon_bits, seed),
        Request::Zx { sequence } => optimize_zx(&sequence, seed),
    }
}
#[cfg(feature = "synthesis")]
fn synthesize(target: &quest_math::Target, epsilon_bits: u64, seed: u64) -> Outcome {
    match synthesis::synthesize(target, epsilon_bits, seed) {
        Ok((sequence, precision_bits)) => Outcome::Candidate {
            sequence,
            engine: "rsgridsynth-0.2.2-quest.1".into(),
            precision_bits,
        },
        Err(message) => Outcome::Failure {
            code: "synthesis".into(),
            message,
        },
    }
}
#[cfg(not(feature = "synthesis"))]
fn synthesize(_target: &quest_math::Target, _epsilon_bits: u64, _seed: u64) -> Outcome {
    Outcome::Failure {
        code: "capability".into(),
        message: "synthesis engine is not enabled".into(),
    }
}
fn main() -> color_eyre::Result<()> {
    color_eyre::install()?;
    let capacity = quest_optimizer_protocol::MAX_REQUEST_BYTES
        .checked_add(1)
        .ok_or_else(|| color_eyre::eyre::eyre!("request budget overflow"))?;
    let mut bytes = Vec::new();
    bytes.try_reserve_exact(capacity)?;
    std::io::stdin()
        .lock()
        .take(u64::try_from(capacity)?)
        .read_to_end(&mut bytes)?;
    let request: RequestEnvelope =
        quest_optimizer_protocol::decode(&bytes, quest_optimizer_protocol::MAX_REQUEST_BYTES)?;
    let outcome = if request.version == VERSION {
        dispatch(request.request, request.seed)
    } else {
        Outcome::Failure {
            code: "version".into(),
            message: "unsupported protocol version".into(),
        }
    };
    let response = ResponseEnvelope {
        version: VERSION,
        seed: request.seed,
        outcome,
    };
    let bytes =
        quest_optimizer_protocol::encode(&response, quest_optimizer_protocol::MAX_OUTPUT_BYTES)?;
    std::io::stdout().lock().write_all(&bytes)?;
    Ok(())
}

#[cfg(feature = "zx")]
fn optimize_zx(sequence: &quest_math::Sequence, seed: u64) -> Outcome {
    match zx::optimize(sequence, seed) {
        Ok(sequence) => Outcome::Candidate {
            sequence,
            engine: "quizx-0.3.0".into(),
            precision_bits: 0,
        },
        Err(message) => Outcome::Failure {
            code: "zx".into(),
            message,
        },
    }
}
#[cfg(not(feature = "zx"))]
fn optimize_zx(_sequence: &quest_math::Sequence, _seed: u64) -> Outcome {
    Outcome::Failure {
        code: "capability".into(),
        message: "ZX engine is not enabled".into(),
    }
}
