//! Native resources for immutable numerical effects in the common program IR.
use crate::{
    Error, Register, RegisterKind, Result,
    error::BackendResult,
    execution::{self, MatrixCacheKey, NativeMatrix},
    values::{bytes_for, reserve_vec},
};
use cxx::UniquePtr;
use quest_circuit::{QuantumPayload, dispatch_recipe::MatrixRecipe};
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
    matrices: Vec<NativeMatrix>,
    channels: Vec<UniquePtr<quest_sys::KrausMap>>,
}
impl PayloadCache {
    pub(crate) fn estimated_bytes(
        bank: &BTreeMap<usize, QuantumPayload>,
        gpu: bool,
    ) -> Result<usize> {
        let mut bytes = bank.len().checked_mul(192).ok_or(Error::Overflow)?;
        let mut matrices = BTreeSet::new();
        let mut channels = BTreeSet::new();
        for payload in bank.values() {
            let extra = match payload {
                QuantumPayload::Matrix {
                    matrix,
                    control_states,
                } => {
                    let key = execution::matrix_cache_key(matrix, control_states.iter().copied());
                    if !matrices.insert(key) {
                        continue;
                    }
                    let recipe = MatrixRecipe::new(matrix, control_states)?;
                    let dim = recipe.dimension();
                    let count = if recipe.is_diagonal() {
                        dim
                    } else {
                        dim.checked_mul(dim).ok_or(Error::Overflow)?
                    };
                    bytes_for(count, if gpu { 16 } else { 12 })?
                }
                QuantumPayload::Channel { kraus } => {
                    if !channels.insert(kraus.as_ptr().addr()) {
                        continue;
                    }
                    let dim = kraus
                        .first()
                        .ok_or(Error::Value("empty Kraus channel"))?
                        .dimension()
                        .max(2);
                    let square = dim.checked_mul(dim).ok_or(Error::Overflow)?;
                    let count = square
                        .checked_mul(square)
                        .and_then(|n| n.checked_mul(if gpu { 8 } else { 4 }))
                        .and_then(|n| {
                            square
                                .checked_mul(kraus.len())
                                .and_then(|m| m.checked_mul(6))
                                .and_then(|m| n.checked_add(m))
                        })
                        .ok_or(Error::Overflow)?;
                    bytes_for(count, 1)?
                }
            };
            bytes = bytes.checked_add(extra).ok_or(Error::Overflow)?;
        }
        Ok(bytes)
    }
    pub(crate) fn prepare(bank: &BTreeMap<usize, QuantumPayload>) -> Result<Self> {
        let mut result = Self {
            entries: BTreeMap::new(),
            matrices: reserve_vec(bank.len())?,
            channels: reserve_vec(bank.len())?,
        };
        let mut matrices: BTreeMap<MatrixCacheKey, usize> = BTreeMap::new();
        let mut channels = BTreeMap::new();
        for (&id, payload) in bank {
            let entry = match payload {
                QuantumPayload::Matrix {
                    matrix,
                    control_states,
                } => {
                    let key = execution::matrix_cache_key(matrix, control_states.iter().copied());
                    let index = if let Some(&index) = matrices.get(&key) {
                        index
                    } else {
                        let index = result.matrices.len();
                        result
                            .matrices
                            .push(execution::prepare_numerical(matrix, control_states)?);
                        matrices.insert(key, index);
                        index
                    };
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
                self.matrices
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
