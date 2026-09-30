//! Conservative owned result storage, shared by batches and collective admission.
use crate::{Error, Result};
use quest_circuit::language::ssa::VerifiedProgram;
#[cfg(any(test, all(feature = "mpi", quest_native_mpi)))]
use quest_circuit::language::vm::{ClassicalValue, RunOutput};

pub fn estimated_bytes(program: &VerifiedProgram) -> Result<usize> {
    quest_circuit::language::vm::output_storage(program).ok_or(Error::Overflow)
}

/// Versioned, deterministic result contract; includes exact scalar bits and widths.
#[cfg(any(test, all(feature = "mpi", quest_native_mpi)))]
pub fn encode(output: &RunOutput, limit: usize) -> Result<Vec<u8>> {
    let mut encoder = Encoder {
        bytes: Vec::new(),
        limit,
    };
    encoder.word(1)?;
    for value in [
        output.completed_quantum,
        output.allocated_qubits,
        output.steps,
        output.outputs.len(),
    ] {
        encoder.word(u64::try_from(value).map_err(|_| Error::Overflow)?)?;
    }
    for (name, value) in &output.outputs {
        encoder.word(u64::try_from(name.len()).map_err(|_| Error::Overflow)?)?;
        encoder.append(name.as_bytes())?;
        encoder.value(value)?;
    }
    Ok(encoder.bytes)
}
#[cfg(any(test, all(feature = "mpi", quest_native_mpi)))]
struct Encoder {
    bytes: Vec<u8>,
    limit: usize,
}
#[cfg(any(test, all(feature = "mpi", quest_native_mpi)))]
impl Encoder {
    fn append(&mut self, bytes: &[u8]) -> Result<()> {
        let requested = self
            .bytes
            .len()
            .checked_add(bytes.len())
            .ok_or(Error::Overflow)?;
        if requested > self.limit {
            return Err(Error::Budget {
                requested,
                available: self.limit,
            });
        }
        self.bytes
            .try_reserve_exact(bytes.len())
            .map_err(|_| Error::Allocation)?;
        self.bytes.extend_from_slice(bytes);
        Ok(())
    }
    fn word(&mut self, value: u64) -> Result<()> {
        self.append(&value.to_le_bytes())
    }
    fn value(&mut self, value: &ClassicalValue) -> Result<()> {
        use quest_circuit::language::classical::{FloatWidth, ScalarType};
        match value {
            ClassicalValue::Array(values) => {
                self.word(0)?;
                self.word(u64::try_from(values.len()).map_err(|_| Error::Overflow)?)?;
                for item in values {
                    self.value(item)?;
                }
            }
            ClassicalValue::Scalar(value) => {
                let (tag, width) = match value.ty() {
                    ScalarType::Bool => (1, 1),
                    ScalarType::Bit(w) => (2, u64::from(w.value())),
                    ScalarType::Int(w) => (3, u64::from(w.value())),
                    ScalarType::Uint(w) => (4, u64::from(w.value())),
                    ScalarType::Angle(w) => (5, u64::from(w.value())),
                    ScalarType::Float(FloatWidth::F32) => (6, 32),
                    ScalarType::Float(FloatWidth::F64) => (6, 64),
                };
                self.word(tag)?;
                self.word(width)?;
                let bits = match value.ty() {
                    ScalarType::Bool => value.to_bool().map(u64::from),
                    ScalarType::Float(_) => value.to_f64().map(f64::to_bits),
                    _ => value.raw_bits(),
                }
                .map_err(|_| Error::Value("invalid collective scalar"))?;
                self.word(bits)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quest_circuit::language::classical::ScalarValue;
    #[test]
    fn collective_output_encoding_binds_values_and_execution_counts() {
        let first = RunOutput {
            outputs: [(
                "result".into(),
                ClassicalValue::Scalar(ScalarValue::boolean(false)),
            )]
            .into(),
            completed_quantum: 1,
            allocated_qubits: 1,
            steps: 8,
        };
        let mut other = first.clone();
        other.steps += 1;
        assert_ne!(encode(&first, 4096).unwrap(), encode(&other, 4096).unwrap());
        other = first.clone();
        other.outputs.insert(
            "result".into(),
            ClassicalValue::Scalar(ScalarValue::boolean(true)),
        );
        assert_ne!(encode(&first, 4096).unwrap(), encode(&other, 4096).unwrap());
        assert!(encode(&first, 1).is_err());
    }
    #[test]
    fn scalar_output_admission_includes_names_and_map_cells() {
        let program = quest_circuit::Program::parse("output int answer=42;", "outputs")
            .unwrap()
            .verify()
            .unwrap()
            .lower()
            .unwrap()
            .plan()
            .unwrap();
        assert!(
            estimated_bytes(program.ssa()).unwrap()
                > program.resources().classical_bytes + size_of::<RunOutput>()
        );
    }
}
