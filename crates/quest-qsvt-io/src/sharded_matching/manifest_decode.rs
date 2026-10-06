//! Serde visitors with admission before receipt/string allocations.
use super::{CONSTRUCTION, HEADER_WORDS, MatchingBucketReceipt, MatchingManifest, ShardIoLimits};
use crate::{Error, Result};
use serde::{
	Deserialize,
	de::{self, DeserializeSeed, IgnoredAny, MapAccess, SeqAccess, Visitor},
};
use std::fmt;

// In addition to input/owned receipts, reserve two input lengths for Serde's
// geometric scratch growth (including numeric parsing) and bounded error text.
const DECODER_FIXED_BYTES: usize = 8192;
pub(super) fn base_bytes(input_capacity: usize) -> Result<usize> {
	input_capacity
		.checked_mul(3)
		.and_then(|n| n.checked_add(DECODER_FIXED_BYTES))
		.and_then(|n| n.checked_add(size_of::<MatchingManifest>()))
		.ok_or(Error::Budget("manifest decoder allowance"))
}

struct Budget {
	remaining: usize,
	buckets: usize,
	failure: Option<&'static str>,
}
impl Budget {
	fn charge<E: de::Error>(&mut self, bytes: usize) -> std::result::Result<(), E> {
		let Some(remaining) = self.remaining.checked_sub(bytes) else {
			return self.reject("manifest concurrent metadata allocation");
		};
		self.remaining = remaining;
		Ok(())
	}
	fn reject<T, E: de::Error>(&mut self, message: &'static str) -> std::result::Result<T, E> {
		self.failure = Some(message);
		Err(E::custom(message))
	}
}

pub(super) fn decode(
	bytes: &[u8],
	input_capacity: usize,
	limits: ShardIoLimits,
) -> Result<MatchingManifest> {
	// Every schema string has a fixed ASCII spelling. Reject escapes before Serde
	// can allocate an unescaping scratch buffer, even for otherwise borrowed strings
	// and field identifiers. Canonical writer output never needs an escape.
	if !bytes.is_ascii() || bytes.contains(&b'\\') {
		return Err(Error::Format(
			"matching manifests require unescaped ASCII strings",
		));
	}
	// Without escapes, odd quote-delimited segments are strings. Bound every
	// string, including unknown keys, before an error formatter can copy it.
	if bytes
		.split(|byte| *byte == b'"')
		.enumerate()
		.any(|(index, string)| index & 1 == 1 && string.len() > 32)
	{
		return Err(Error::Budget("manifest string/filename admission"));
	}
	let mut budget = Budget {
		remaining: limits.max_manifest_bytes,
		buckets: limits.max_buckets,
		failure: None,
	};
	let admitted = budget
		.charge::<serde_json::Error>(base_bytes(input_capacity)?)
		.and_then(|()| {
			let mut decoder = serde_json::Deserializer::from_slice(bytes);
			let result = ManifestSeed(&mut budget).deserialize(&mut decoder)?;
			decoder.end()?;
			Ok(result)
		});
	match (admitted, budget.failure) {
		(_, Some(failure)) => Err(Error::Budget(failure)),
		(result, None) => result.map_err(Error::from),
	}
}

#[derive(Deserialize)]
#[serde(field_identifier, rename_all = "snake_case")]
enum Field {
	SchemaVersion,
	Construction,
	Header,
	Buckets,
}

struct ManifestSeed<'a>(&'a mut Budget);
impl<'de> DeserializeSeed<'de> for ManifestSeed<'_> {
	type Value = MatchingManifest;
	fn deserialize<D: de::Deserializer<'de>>(
		self,
		decoder: D,
	) -> std::result::Result<Self::Value, D::Error> {
		decoder.deserialize_map(self)
	}
}
impl<'de> Visitor<'de> for ManifestSeed<'_> {
	type Value = MatchingManifest;
	fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.write_str("a bounded matching manifest")
	}
	fn visit_map<A: MapAccess<'de>>(
		self,
		mut map: A,
	) -> std::result::Result<Self::Value, A::Error> {
		let (mut schema, mut construction, mut header, mut buckets) = (None, None, None, None);
		while let Some(field) = map.next_key::<Field>()? {
			match field {
				Field::SchemaVersion if schema.is_none() => schema = Some(map.next_value::<u32>()?),
				Field::Construction if construction.is_none() => {
					let text: &str = map.next_value()?;
					if text != CONSTRUCTION {
						return Err(de::Error::custom("unsupported construction"));
					}
					self.0.charge(text.len())?;
					let mut owned = String::new();
					owned
						.try_reserve_exact(text.len())
						.map_err(|_| de::Error::custom("construction allocation"))?;
					#[cfg(test)]
					super::capacity_tests::inflate_string(&mut owned).map_err(de::Error::custom)?;
					self.0.charge(
						owned
							.capacity()
							.checked_sub(text.len())
							.ok_or_else(|| de::Error::custom("construction capacity"))?,
					)?;
					#[cfg(test)]
					super::capacity_tests::fill();
					owned.push_str(text);
					construction = Some(owned);
				}
				Field::Header if header.is_none() => {
					header = Some(map.next_value::<[u64; HEADER_WORDS]>()?);
				}
				Field::Buckets if buckets.is_none() => {
					buckets = Some(map.next_value_seed(BucketsSeed(self.0))?);
				}
				_ => return Err(de::Error::custom("duplicate manifest field")),
			}
		}
		Ok(MatchingManifest {
			schema_version: schema.ok_or_else(|| de::Error::missing_field("schema_version"))?,
			construction: construction.ok_or_else(|| de::Error::missing_field("construction"))?,
			header: header.ok_or_else(|| de::Error::missing_field("header"))?,
			buckets: buckets.ok_or_else(|| de::Error::missing_field("buckets"))?,
		})
	}
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BorrowedReceipt<'a> {
	bucket: usize,
	buckets: usize,
	file: &'a str,
	records: usize,
	size_bytes: u64,
	sha256: [u8; 32],
	semantic_sha256: [u8; 32],
}
struct BucketsSeed<'a>(&'a mut Budget);
impl<'de> DeserializeSeed<'de> for BucketsSeed<'_> {
	type Value = Vec<MatchingBucketReceipt>;
	fn deserialize<D: de::Deserializer<'de>>(
		self,
		decoder: D,
	) -> std::result::Result<Self::Value, D::Error> {
		decoder.deserialize_seq(self)
	}
}
impl<'de> Visitor<'de> for BucketsSeed<'_> {
	type Value = Vec<MatchingBucketReceipt>;
	fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.write_str("bounded bucket receipts")
	}
	fn visit_seq<A: SeqAccess<'de>>(
		self,
		mut sequence: A,
	) -> std::result::Result<Self::Value, A::Error> {
		let mut records = Vec::new();
		loop {
			if records.len() == self.0.buckets {
				if sequence.next_element::<IgnoredAny>()?.is_some() {
					return self.0.reject("bucket metadata admission");
				}
				break;
			}
			let Some(receipt) = sequence.next_element::<BorrowedReceipt<'de>>()? else {
				break;
			};
			// Canonical filenames have fixed bounded spelling; borrow untrusted JSON
			// strings so rejection never first allocates their decoded contents.
			if receipt.file.len() > 32 {
				return self.0.reject("bucket filename admission");
			}
			let before = records.capacity();
			let needed = records
				.len()
				.checked_add(1)
				.ok_or_else(|| de::Error::custom("receipt length"))?;
			let requested = needed.saturating_sub(before);
			self.0.charge(
				requested
					.checked_mul(size_of::<MatchingBucketReceipt>())
					.ok_or_else(|| de::Error::custom("receipt bytes"))?,
			)?;
			self.0.charge(receipt.file.len())?;
			records
				.try_reserve_exact(1)
				.map_err(|_| de::Error::custom("bucket metadata allocation"))?;
			#[cfg(test)]
			super::capacity_tests::inflate(&mut records, "receipts").map_err(de::Error::custom)?;
			let excess = records
				.capacity()
				.checked_sub(before)
				.and_then(|n| n.checked_sub(requested))
				.and_then(|n| n.checked_mul(size_of::<MatchingBucketReceipt>()))
				.ok_or_else(|| de::Error::custom("receipt capacity"))?;
			self.0.charge(excess)?;
			let mut file = String::new();
			file.try_reserve_exact(receipt.file.len())
				.map_err(|_| de::Error::custom("filename allocation"))?;
			self.0.charge(
				file.capacity()
					.checked_sub(receipt.file.len())
					.ok_or_else(|| de::Error::custom("filename capacity"))?,
			)?;
			#[cfg(test)]
			super::capacity_tests::fill();
			file.push_str(receipt.file);
			records.push(MatchingBucketReceipt {
				bucket: receipt.bucket,
				buckets: receipt.buckets,
				file,
				records: receipt.records,
				size_bytes: receipt.size_bytes,
				sha256: receipt.sha256,
				semantic_sha256: receipt.semantic_sha256,
			});
		}
		Ok(records)
	}
}
