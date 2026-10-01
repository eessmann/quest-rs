//! One-request candidate generation process. Run through the bounded client.
#![forbid(unsafe_code)]
use quest_optimizer_protocol::{Outcome, Request, RequestEnvelope, ResponseEnvelope, VERSION};
use std::io::{Read, Write};

#[cfg(feature = "mitm")]
mod mitm;
#[cfg(feature = "synthesis")]
mod synthesis;
#[cfg(feature = "zx")]
mod zx;

fn dispatch(request: Request, seed: u64) -> Outcome {
    match request {
        Request::Capabilities => Outcome::Capabilities {
            synthesis: cfg!(feature = "synthesis"),
            zx: cfg!(feature = "zx"),
            mitm: cfg!(feature = "mitm"),
        },
        Request::Synthesize {
            target,
            epsilon_bits,
        } => synthesize(&target, epsilon_bits, seed),
        Request::Zx { sequence } => optimize_zx(&sequence, seed),
        Request::ZxBest { sequence } => optimize_zx_best(&sequence, seed),
        Request::ZxExpanded { sequence } => optimize_zx_expanded(&sequence, seed),
        Request::ExactMitm { target, limits } => exact_mitm(&target, limits),
        Request::ApproxMitm {
            target,
            epsilon_bits,
            limits,
        } => approx_mitm(&target, epsilon_bits, limits),
    }
}
#[cfg(feature = "mitm")]
fn exact_mitm(
    target: &quest_math::Sequence,
    limits: quest_optimizer_protocol::MitmLimits,
) -> Outcome {
    mitm::exact::search_exact(target, limits)
}
#[cfg(not(feature = "mitm"))]
fn exact_mitm(
    _target: &quest_math::Sequence,
    _limits: quest_optimizer_protocol::MitmLimits,
) -> Outcome {
    Outcome::Failure {
        code: "capability".into(),
        message: "MITM engine is not enabled".into(),
    }
}
#[cfg(feature = "mitm")]
fn approx_mitm(
    target: &quest_math::Target,
    epsilon_bits: u64,
    limits: quest_optimizer_protocol::MitmLimits,
) -> Outcome {
    mitm::approx::search_approx(target, epsilon_bits, limits)
}
#[cfg(not(feature = "mitm"))]
fn approx_mitm(
    _target: &quest_math::Target,
    _epsilon_bits: u64,
    _limits: quest_optimizer_protocol::MitmLimits,
) -> Outcome {
    Outcome::Failure {
        code: "capability".into(),
        message: "MITM engine is not enabled".into(),
    }
}
#[cfg(feature = "synthesis")]
fn synthesize(target: &quest_math::Target, epsilon_bits: u64, seed: u64) -> Outcome {
    match synthesis::synthesize(target, epsilon_bits, seed) {
        Ok((sequence, precision_bits)) => Outcome::Candidate {
            sequence,
            engine: format!("quest-synthesis-{}", quest_synthesis::ROTATION_ALGORITHM),
            precision_bits,
        },
        Err(error) => Outcome::Failure {
            code: synthesis::error_code(&error).into(),
            message: error.to_string(),
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
    match zx::optimize_baseline(sequence, seed) {
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
#[cfg(feature = "zx")]
fn optimize_zx_best(sequence: &quest_math::Sequence, seed: u64) -> Outcome {
    match zx::optimize(sequence, seed) {
        Ok(sequence) => Outcome::Candidate {
            sequence,
            engine: "quizx-0.3.0-best".into(),
            precision_bits: 0,
        },
        Err(message) => Outcome::Failure {
            code: "zx-best".into(),
            message,
        },
    }
}
#[cfg(not(feature = "zx"))]
fn optimize_zx_best(_sequence: &quest_math::Sequence, _seed: u64) -> Outcome {
    Outcome::Failure {
        code: "capability".into(),
        message: "ZX engine is not enabled".into(),
    }
}
#[cfg(feature = "zx")]
fn optimize_zx_expanded(sequence: &quest_math::Sequence, seed: u64) -> Outcome {
    match zx::optimize_expanded(sequence, seed) {
        Ok(sequence) => Outcome::Candidate {
            sequence,
            engine: "quizx-0.3.0-expanded".into(),
            precision_bits: 0,
        },
        Err(message) => Outcome::Failure {
            code: "zx-expanded".into(),
            message,
        },
    }
}
#[cfg(not(feature = "zx"))]
fn optimize_zx_expanded(_sequence: &quest_math::Sequence, _seed: u64) -> Outcome {
    Outcome::Failure {
        code: "capability".into(),
        message: "ZX engine is not enabled".into(),
    }
}
