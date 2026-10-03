//! Portable checked frontend templates. Serialized witnesses are never trusted.
use super::{CompileLimits, SemanticError, TypedModule};
use crate::{ssa, syntax};

#[derive(serde::Serialize, serde::Deserialize)]
struct Template {
	version: u32,
	syntax: syntax::Module,
	program: ssa::Program,
}

/// Encode admitted syntax and its checked executable graph at macro expansion.
/// # Errors
/// Rejects encoding failures and the default template storage budget.
pub fn encode(module: &TypedModule) -> Result<String, SemanticError> {
	let template = Template {
		version: 2,
		syntax: module.syntax.clone(),
		program: module.program.program().clone(),
	};
	encode_json(&template, CompileLimits::default().storage_bytes)
}

fn encode_json(value: &impl serde::Serialize, limit: usize) -> Result<String, SemanticError> {
	struct Writer {
		bytes: Vec<u8>,
		limit: usize,
		exhausted: bool,
	}
	impl std::io::Write for Writer {
		fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
			let Some(length) = self
				.bytes
				.len()
				.checked_add(bytes.len())
				.filter(|n| *n <= self.limit)
			else {
				self.exhausted = true;
				return Err(std::io::Error::other("template byte limit"));
			};
			if length > self.bytes.capacity() {
				let capacity = self
					.bytes
					.capacity()
					.saturating_mul(2)
					.max(64)
					.max(length)
					.min(self.limit);
				self.bytes
					.try_reserve_exact(capacity.saturating_sub(self.bytes.len()))
					.map_err(|error| {
						self.exhausted = true;
						std::io::Error::other(error)
					})?;
			}
			self.bytes.extend_from_slice(bytes);
			Ok(bytes.len())
		}
		fn flush(&mut self) -> std::io::Result<()> {
			Ok(())
		}
	}
	let mut output = Writer {
		bytes: Vec::new(),
		limit,
		exhausted: false,
	};
	if let Err(error) = serde_json::to_writer(&mut output, value) {
		return Err(if output.exhausted {
			SemanticError::budget("template encoding storage")
		} else {
			SemanticError::invalid(format!("template encoding: {error}"))
		});
	}
	String::from_utf8(output.bytes)
		.map_err(|error| SemanticError::invalid(format!("template encoding: {error}")))
}

/// Materialize an emitted template without parsing or admitting source again.
/// The caller caches this immutable result; captures are bound per invocation.
/// # Errors
/// Rejects malformed encoding, unknown versions, invalid scalar values and IR.
pub fn load(encoded: &str, limits: CompileLimits) -> Result<TypedModule, SemanticError> {
	if encoded.len() > limits.storage_bytes {
		return Err(SemanticError::budget("template encoding storage"));
	}
	let template: Template = serde_json::from_str(encoded)
		.map_err(|error| SemanticError::invalid(format!("template decoding: {error}")))?;
	if template.version != 2 {
		return Err(SemanticError::invalid(
			"unsupported frontend template version",
		));
	}
	// Verification creates a new publication identity. The serialized snapshot
	// does not grant a capability, and no VerifiedProgram is deserializable.
	let program = template.program.verify(limits)?;
	let module = TypedModule {
		syntax: template.syntax,
		program,
	};
	if module.retained_bytes()? > limits.storage_bytes {
		return Err(SemanticError::budget("template retained storage"));
	}
	Ok(module)
}

#[cfg(test)]
mod tests {
	use super::*;
	use googletest::prelude::*;
	use serde::{Serialize, ser::SerializeSeq};
	use std::cell::Cell;

	struct Counted<'a>(&'a Cell<usize>);
	impl Serialize for Counted<'_> {
		fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
			let mut values = serializer.serialize_seq(Some(100))?;
			for _ in 0..100 {
				self.0.set(self.0.get().saturating_add(1));
				values.serialize_element("escaped \"text\"\n😀")?;
			}
			values.end()
		}
	}
	#[gtest]
	fn template_serialization_stops_at_the_byte_limit() {
		let visited = Cell::new(0);
		let error = encode_json(&Counted(&visited), 32);
		expect_true!(matches!(
			error,
			Err(SemanticError {
				kind: super::super::ErrorKind::Resource,
				..
			})
		));
		expect_true!(visited.get() < 100);
	}
	#[gtest]
	fn template_json_preserves_escaped_values_at_the_exact_limit() -> googletest::Result<()> {
		let value = serde_json::json!({"text":"\"\n😀", "word":u64::MAX});
		let expected = serde_json::to_string(&value)?;
		expect_eq!(encode_json(&value, expected.len())?, expected);
		expect_true!(encode_json(&value, expected.len().saturating_sub(1)).is_err());
		Ok(())
	}
}
