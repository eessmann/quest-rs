//! Partition-independent record fingerprint components.
//!
//! Hash each domain-separated record before wrapping addition. Summing byte-wise
//! FNV hashes directly permits systematic cancellation of ordinary phase signs.
//! Truncation and addition still provide only accidental-integrity provenance;
//! persisted cryptographic file/semantic digests remain a separate obligation.
use crate::{Error, Result};
use sha2::{Digest, Sha256};

/// Conservative fixed scratch allowance for the digest state, padded block and
/// compression schedule. No record vector or heap allocation is created here.
pub const RECORD_FINGERPRINT_SCRATCH_BYTES: usize = 1024;

/// Domain-separated SHA-256, truncated to its first eight bytes in LE order.
///
/// Feed fixed semantic fields in canonical order; reduce completed record hashes
/// by wrapping addition only afterward. This is not cryptographic authentication.
/// The scratch/work model covers the digest engine; arbitrary iterator callback
/// work, retained input and allocations remain the caller's separate obligation.
#[must_use]
pub fn record_fingerprint(domain: u64, words: impl IntoIterator<Item = u64>) -> u64 {
	let mut hash = Sha256::new();
	hash.update(domain.to_le_bytes());
	for word in words {
		hash.update(word.to_le_bytes());
	}
	let output = hash.finalize();
	let mut bytes = [0_u8; 8];
	for (byte, value) in bytes.iter_mut().zip(output) {
		*byte = value;
	}
	u64::from_le_bytes(bytes)
}

/// Conservative modeled scalar work for one record hash, including domain,
/// padding and finalization.
///
/// Each 64-byte SHA block is charged 8192 units:
/// 64 rounds at <=64 scalar operations, 48 expansion words at <=32 operations,
/// plus 2560 units for input, state and output handling. Hardware acceleration
/// does not reduce this deterministic logical charge; it is not measured cycles.
/// # Errors
/// Rejects byte/block/work arithmetic overflow.
pub fn record_fingerprint_work(words: usize) -> Result<usize> {
	words
		.checked_mul(8)
		.and_then(|n| n.checked_add(8 + 9 + 63))
		.and_then(|n| n.checked_div(64))
		.and_then(|n| n.checked_mul(8192))
		.ok_or(Error::Budget("record fingerprint work"))
}
