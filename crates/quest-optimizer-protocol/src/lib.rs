//! Untrusted, versioned candidate messages. Deserialization never certifies a circuit.
#![forbid(unsafe_code)]
use quest_math::{Sequence, Target};
use serde::{Deserialize, Serialize};

/// Supported wire version.
pub const VERSION: u16 = 1;
/// Maximum encoded request bytes.
pub const MAX_REQUEST_BYTES: usize = 65_536;
/// Maximum encoded response bytes.
pub const MAX_OUTPUT_BYTES: usize = 65_536;

/// One request per child process, with a reproducible seed.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequestEnvelope {
    pub version: u16,
    pub seed: u64,
    pub request: Request,
}
/// Optional candidate operation.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Request {
    Synthesize { target: Target, epsilon_bits: u64 },
    Zx { sequence: Sequence },
    Capabilities,
}
/// Untrusted child response, checked again by the parent.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResponseEnvelope {
    pub version: u16,
    pub seed: u64,
    pub outcome: Outcome,
}
/// A candidate is not an equivalence or accuracy certificate.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Outcome {
    Candidate {
        sequence: Sequence,
        engine: String,
        precision_bits: usize,
    },
    Capabilities {
        synthesis: bool,
        zx: bool,
    },
    Failure {
        code: String,
        message: String,
    },
}

/// A byte-limited encoder/decoder failure.
#[derive(Debug, thiserror::Error)]
pub enum WireError {
    #[error("optimizer message exceeds its byte budget")]
    Budget,
    #[error("malformed optimizer message: {0}")]
    Json(#[from] serde_json::Error),
}
struct BoundedBytes {
    bytes: Vec<u8>,
    limit: usize,
}
impl std::io::Write for BoundedBytes {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let length = self
            .bytes
            .len()
            .checked_add(bytes.len())
            .filter(|length| *length <= self.limit)
            .ok_or_else(|| std::io::Error::other("message byte budget"))?;
        self.bytes
            .try_reserve_exact(length.saturating_sub(self.bytes.len()))
            .map_err(std::io::Error::other)?;
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
/// Encode without first allocating an arbitrarily large serialized message.
/// # Errors
/// Rejects serialization errors, allocation failure, and the byte limit.
pub fn encode(value: &impl Serialize, limit: usize) -> Result<Vec<u8>, WireError> {
    let mut output = BoundedBytes {
        bytes: Vec::new(),
        limit,
    };
    serde_json::to_writer(&mut output, value)?;
    Ok(output.bytes)
}
/// Decode only a bounded complete message, retaining serde's nesting limit.
/// # Errors
/// Rejects oversized, malformed, or trailing input.
pub fn decode<T: serde::de::DeserializeOwned>(bytes: &[u8], limit: usize) -> Result<T, WireError> {
    if bytes.len() > limit {
        return Err(WireError::Budget);
    }
    Ok(serde_json::from_slice(bytes)?)
}
