//! Bounded canonical JSON encoding and hashing, delegated to Serde.
use super::{ArtifactError, ArtifactResult};
use serde::Serialize;
use sha2::{Digest, Sha256};

pub(super) fn bytes(value: &impl Serialize, limit: usize) -> ArtifactResult<Vec<u8>> {
	let mut output = Encoder::new(Sink::Bytes(Vec::new()), limit);
	output.serialize(value)?;
	match output.sink {
		Sink::Bytes(bytes) => Ok(bytes),
		Sink::Hash(_) => Err(ArtifactError::Invalid("JSON output sink")),
	}
}

pub(super) fn digest(value: &impl Serialize, limit: usize) -> ArtifactResult<[u8; 32]> {
	let mut output = Encoder::new(Sink::Hash(Sha256::new()), limit);
	output.serialize(value)?;
	match output.sink {
		Sink::Hash(hash) => Ok(hash.finalize().into()),
		Sink::Bytes(_) => Err(ArtifactError::Invalid("JSON digest sink")),
	}
}

enum Sink {
	Bytes(Vec<u8>),
	Hash(Sha256),
}
struct Encoder {
	sink: Sink,
	count: usize,
	limit: usize,
	exhausted: bool,
}
impl Encoder {
	const fn new(sink: Sink, limit: usize) -> Self {
		Self {
			sink,
			count: 0,
			limit,
			exhausted: false,
		}
	}
	fn serialize(&mut self, value: &impl Serialize) -> ArtifactResult<()> {
		serde_json::to_writer(&mut *self, value).map_err(|error| {
			if self.exhausted {
				ArtifactError::Budget("encoded JSON")
			} else {
				ArtifactError::Json(error)
			}
		})
	}
}
impl std::io::Write for Encoder {
	fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
		let Some(count) = self
			.count
			.checked_add(bytes.len())
			.filter(|n| *n <= self.limit)
		else {
			self.exhausted = true;
			return Err(std::io::Error::other("JSON byte limit"));
		};
		match &mut self.sink {
			Sink::Bytes(output) => {
				if count > output.capacity() {
					let capacity = output
						.capacity()
						.saturating_mul(2)
						.max(64)
						.max(count)
						.min(self.limit);
					output
						.try_reserve_exact(capacity.saturating_sub(output.len()))
						.map_err(|error| {
							self.exhausted = true;
							std::io::Error::other(error)
						})?;
				}
				output.extend_from_slice(bytes);
			}
			Sink::Hash(hash) => hash.update(bytes),
		}
		self.count = count;
		Ok(bytes.len())
	}
	fn flush(&mut self) -> std::io::Result<()> {
		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use googletest::prelude::*;
	use serde::ser::SerializeSeq;
	use std::cell::Cell;

	struct Counted<'a>(&'a Cell<usize>);
	impl Serialize for Counted<'_> {
		fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
			let mut sequence = serializer.serialize_seq(Some(100))?;
			for _ in 0..100 {
				self.0.set(self.0.get().saturating_add(1));
				sequence.serialize_element("escaped \"value\"\n😀")?;
			}
			sequence.end()
		}
	}

	#[gtest]
	fn serialization_stops_visiting_values_at_the_byte_limit() {
		let visited = Cell::new(0);
		expect_true!(matches!(
			bytes(&Counted(&visited), 32),
			Err(ArtifactError::Budget(_))
		));
		expect_true!(visited.get() < 100);
		visited.set(0);
		expect_true!(matches!(
			digest(&Counted(&visited), 32),
			Err(ArtifactError::Budget(_))
		));
		expect_true!(visited.get() < 100);
	}

	#[gtest]
	fn canonical_bytes_digest_and_exact_limit_are_preserved() -> googletest::Result<()> {
		let value =
			serde_json::json!({"words":[u64::MAX, (-0.0f64).to_bits(), 1], "text":"\"\n😀"});
		let expected = serde_json::to_vec(&value)?;
		expect_eq!(bytes(&value, expected.len())?, expected);
		let hash: [u8; 32] = Sha256::digest(&expected).into();
		expect_eq!(digest(&value, expected.len())?, hash);
		expect_true!(matches!(
			bytes(&value, expected.len().saturating_sub(1)),
			Err(ArtifactError::Budget(_))
		));
		Ok(())
	}

	#[gtest]
	fn incremental_output_grows_with_bounded_amortized_capacity() -> googletest::Result<()> {
		use std::io::Write;
		let limit = 10_000;
		let mut output = Encoder::new(Sink::Bytes(Vec::new()), limit);
		let mut previous_capacity = 0;
		let mut allocations = 0usize;
		for _ in 0..limit {
			output.write_all(b"x")?;
			if let Sink::Bytes(bytes) = &output.sink {
				if bytes.capacity() != previous_capacity {
					allocations = allocations.saturating_add(1);
					previous_capacity = bytes.capacity();
				}
				expect_le!(bytes.capacity(), limit);
			}
		}
		expect_le!(allocations, 20);
		expect_true!(output.write_all(b"x").is_err());
		Ok(())
	}
}
