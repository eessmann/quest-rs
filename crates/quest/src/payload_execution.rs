//! Native resources for immutable numerical effects in the common program IR.
use crate::{
    Error, Register, RegisterKind, Result,
    error::BackendResult,
    execution::{self, MatrixCacheKey, MatrixPreparation, NativeMatrix},
    values::reserve_vec,
};
use cxx::UniquePtr;
use quest_compile::QuantumPayload;
use std::collections::{BTreeMap, BTreeSet};

enum Entry {
    Matrix {
        index: usize,
        controls: usize,
        width: usize,
    },
    Channel {
        index: usize,
        width: usize,
    },
}
pub struct PayloadCache {
    entries: BTreeMap<usize, Entry>,
    channels: Vec<UniquePtr<quest_sys::KrausMap>>,
}
impl PayloadCache {
    pub(crate) fn estimated_bytes(
        bank: &BTreeMap<usize, QuantumPayload>,
        gpu: bool,
        matrices: &mut BTreeSet<MatrixCacheKey>,
    ) -> Result<usize> {
        let mut bytes = bank.len().checked_mul(192).ok_or(Error::Overflow)?;
        let mut channels = BTreeSet::new();
        for payload in bank.values() {
            let extra = match payload {
                QuantumPayload::Matrix {
                    matrix,
                    control_states,
                } => execution::admit_matrix(matrix, control_states, gpu, matrices)?,
                QuantumPayload::Channel { kraus } => {
                    if !channels.insert(kraus.as_ptr().addr()) {
                        continue;
                    }
                    execution::kraus_bytes(kraus, gpu)?
                }
            };
            bytes = bytes.checked_add(extra).ok_or(Error::Overflow)?;
        }
        Ok(bytes)
    }
    pub(crate) fn prepare(
        bank: &BTreeMap<usize, QuantumPayload>,
        matrices: &mut MatrixPreparation,
    ) -> Result<Self> {
        let mut result = Self {
            entries: BTreeMap::new(),
            channels: reserve_vec(bank.len())?,
        };
        let mut channels = BTreeMap::new();
        for (&id, payload) in bank {
            let entry = match payload {
                QuantumPayload::Matrix {
                    matrix,
                    control_states,
                } => {
                    let index = matrices.include(matrix, control_states)?;
                    Entry::Matrix {
                        index,
                        controls: control_states.len(),
                        width: payload.num_wires(),
                    }
                }
                QuantumPayload::Channel { kraus } => {
                    let key = kraus.as_ptr().addr();
                    let index = if let Some(&index) = channels.get(&key) {
                        index
                    } else {
                        let index = result.channels.len();
                        result.channels.push(execution::prepare_kraus(kraus)?);
                        channels.insert(key, index);
                        index
                    };
                    Entry::Channel {
                        index,
                        width: payload.num_wires(),
                    }
                }
            };
            result.entries.insert(id, entry);
        }
        Ok(result)
    }
    pub(crate) const fn requires_density(&self) -> bool {
        !self.channels.is_empty()
    }
    pub(crate) fn execute<K: RegisterKind>(
        &self,
        id: usize,
        wires: &[usize],
        register: &mut Register<'_, K>,
        scratch: &mut Vec<i32>,
        matrices: &[NativeMatrix],
    ) -> Result<()> {
        let entry = self
            .entries
            .get(&id)
            .ok_or(Error::Value("unprepared numerical payload"))?;
        let (width, controls) = match entry {
            Entry::Matrix {
                width, controls, ..
            } => (*width, *controls),
            Entry::Channel { width, .. } => (*width, 0),
        };
        if wires.len() != width || width.max(1) > scratch.capacity() {
            return Err(Error::Value("numerical payload wire arity"));
        }
        scratch.clear();
        // Native matrix recipes embed targets in low bits, controls in high bits.
        for &wire in wires
            .get(controls..)
            .ok_or(Error::Overflow)?
            .iter()
            .chain(wires.get(..controls).ok_or(Error::Overflow)?)
        {
            scratch.push(register.check_qubit(wire)?);
        }
        if scratch.is_empty() {
            scratch.push(register.check_qubit(0)?);
        }
        match entry {
            Entry::Matrix { index, .. } => execution::execute_matrix(
                matrices
                    .get(*index)
                    .ok_or(Error::Value("matrix payload index"))?,
                register,
                scratch,
                false,
            ),
            Entry::Channel { index, .. } => {
                if !register.is_density() {
                    return Err(Error::RegisterMismatch);
                }
                quest_sys::mix_kraus_map(
                    register.pin(),
                    scratch,
                    self.channels
                        .get(*index)
                        .ok_or(Error::Value("channel payload index"))?,
                )
                .context("executing numerical channel")
            }
        }
    }
}
