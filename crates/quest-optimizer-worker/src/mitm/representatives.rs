use quest_math::{Gate, MatrixKey, Operation};
use std::collections::HashMap;

/// Additive search resources and the max-plus transfer from incoming wire depths.
/// Componentwise transfer dominance is preserved under every gate continuation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Signature {
    length: usize,
    t_count: usize,
    two_qubit_count: usize,
    transfer: [[Option<usize>; 2]; 2],
}

impl Signature {
    pub(super) const fn identity(width: usize) -> Self {
        let second = if width == 2 { Some(0) } else { None };
        Self {
            length: 0,
            t_count: 0,
            two_qubit_count: 0,
            transfer: [[Some(0), None], [None, second]],
        }
    }

    pub(super) fn after(self, operation: &Operation) -> Option<Self> {
        let mut next = Self {
            length: self.length.checked_add(1)?,
            t_count: self
                .t_count
                .checked_add(usize::from(matches!(operation.gate, Gate::T | Gate::Tdg)))?,
            two_qubit_count: self
                .two_qubit_count
                .checked_add(usize::from(operation.gate.target_count() == 2))?,
            transfer: self.transfer,
        };
        match operation.gate.target_count() {
            0 => {}
            1 => {
                let row = *operation.targets.first()?;
                for entry in next.transfer.get_mut(row)? {
                    *entry = match *entry {
                        Some(value) => Some(value.checked_add(1)?),
                        None => None,
                    };
                }
            }
            2 => {
                let a = *operation.targets.first()?;
                let b = *operation.targets.get(1)?;
                if a == b {
                    return None;
                }
                let old_a = *next.transfer.get(a)?;
                let old_b = *next.transfer.get(b)?;
                let mut joined = [None; 2];
                for ((entry, x), y) in joined.iter_mut().zip(old_a).zip(old_b) {
                    *entry = match x.max(y) {
                        Some(value) => Some(value.checked_add(1)?),
                        None => None,
                    };
                }
                *next.transfer.get_mut(a)? = joined;
                *next.transfer.get_mut(b)? = joined;
            }
            _ => return None,
        }
        Some(next)
    }

    pub(super) fn dominates(&self, other: &Self) -> bool {
        self.length <= other.length
            && self.t_count <= other.t_count
            && self.two_qubit_count <= other.two_qubit_count
            && self
                .transfer
                .iter()
                .zip(other.transfer.iter())
                .all(|(a, b)| a.iter().zip(b.iter()).all(|(x, y)| x <= y))
    }
}

#[derive(Clone, Copy)]
struct Representative {
    state: usize,
    signature: Signature,
}

pub(super) struct ParetoIndex {
    by_matrix: HashMap<MatrixKey, Vec<Representative>>,
}

impl ParetoIndex {
    pub(super) fn new() -> Self {
        Self {
            by_matrix: HashMap::new(),
        }
    }

    pub(super) fn is_dominated(&self, key: &MatrixKey, signature: &Signature) -> bool {
        self.by_matrix
            .get(key)
            .is_some_and(|bucket| bucket.iter().any(|old| old.signature.dominates(signature)))
    }

    pub(super) fn bucket_len(&self, key: &MatrixKey) -> usize {
        self.by_matrix.get(key).map_or(0, Vec::len)
    }

    pub(super) fn first(&self, key: &MatrixKey) -> Option<usize> {
        self.by_matrix.get(key)?.first().map(|record| record.state)
    }

    pub(super) fn commit(
        &mut self,
        key: MatrixKey,
        signature: Signature,
        state: usize,
        mut retire: impl FnMut(usize),
    ) -> Result<(), ()> {
        self.by_matrix.try_reserve(1).map_err(|_| ())?;
        let bucket = self.by_matrix.entry(key).or_default();
        bucket.try_reserve(1).map_err(|_| ())?;
        bucket.retain(|old| {
            if signature.dominates(&old.signature) {
                retire(old.state);
                false
            } else {
                true
            }
        });
        bucket.push(Representative { state, signature });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{ParetoIndex, Signature};
    use googletest::{OrFail, Result, prelude::*};
    use quest_math::{ExactMatrix, Gate, Limits, Operation};

    fn op(gate: Gate, targets: &[usize]) -> Operation {
        Operation {
            gate,
            targets: targets.to_vec(),
            controls: vec![],
        }
    }

    fn depth(mut wires: [usize; 2], word: &[Operation]) -> Result<[usize; 2]> {
        for op in word {
            match op.gate.target_count() {
                0 => {}
                1 => {
                    let depth = wires.get_mut(*op.targets.first().or_fail()?).or_fail()?;
                    *depth = depth.checked_add(1).or_fail()?;
                }
                2 => {
                    let next = wires[0].max(wires[1]).checked_add(1).or_fail()?;
                    wires = [next, next];
                }
                _ => fail!("unexpected operation arity in two-wire reference")?,
            }
        }
        Ok(wires)
    }

    #[gtest]
    fn same_full_phase_state_keeps_incomparable_wire_transfers() -> Result<()> {
        let limits = Limits::default();
        let key = ExactMatrix::identity(2, limits)?.full_phase_key(limits)?;
        let h0 = op(Gate::H, &[0]);
        let h1 = op(Gate::H, &[1]);
        let first = Signature::identity(2)
            .after(&h0)
            .or_fail()?
            .after(&h0)
            .or_fail()?;
        let second = Signature::identity(2)
            .after(&h1)
            .or_fail()?
            .after(&h1)
            .or_fail()?;
        expect_false!(first.dominates(&second));
        expect_false!(second.dominates(&first));
        let mut index = ParetoIndex::new();
        let mut active = vec![true, true];
        index
            .commit(key.clone(), first, 0, |i| active[i] = false)
            .or_fail()?;
        expect_false!(index.is_dominated(&key, &second));
        index
            .commit(key, second, 1, |i| active[i] = false)
            .or_fail()?;
        expect_eq!(active, [true, true]);
        expect_eq!(
            depth([0, 0], &[h0.clone(), h0.clone(), h0.clone()])?
                .iter()
                .max(),
            Some(&3)
        );
        expect_eq!(depth([0, 0], &[h1.clone(), h1, h0])?.iter().max(), Some(&2));
        Ok(())
    }

    #[gtest]
    fn exhaustive_small_words_preserve_every_continuation_cost() -> Result<()> {
        let alphabet = [
            op(Gate::H, &[0]),
            op(Gate::H, &[1]),
            op(Gate::Cx, &[0, 1]),
            op(Gate::W, &[]),
        ];
        let limits = Limits::default();
        let identity = ExactMatrix::identity(2, limits)?;
        let gates: Vec<_> = alphabet
            .iter()
            .map(|op| ExactMatrix::for_operation(2, op, limits))
            .collect::<quest_math::Result<_>>()?;
        let mut words = vec![Vec::<usize>::new()];
        for _ in 0..3 {
            let latest = words.last().or_fail()?.len();
            let previous: Vec<_> = words
                .iter()
                .filter(|word| word.len() == latest)
                .cloned()
                .collect();
            for word in previous {
                for i in 0..alphabet.len() {
                    let mut next = word.clone();
                    next.push(i);
                    words.push(next);
                }
            }
        }
        let mut keys = Vec::new();
        let mut signatures = Vec::new();
        let mut index = ParetoIndex::new();
        let mut active = Vec::<bool>::new();
        for word in &words {
            let mut matrix = identity.clone();
            let mut signature = Signature::identity(2);
            for &i in word {
                matrix = gates[i].multiply(&matrix, limits)?;
                signature = signature.after(&alphabet[i]).or_fail()?;
            }
            let key = matrix.full_phase_key(limits)?;
            let retained = !index.is_dominated(&key, &signature);
            active.push(retained);
            if retained {
                index
                    .commit(key.clone(), signature, keys.len(), |i| active[i] = false)
                    .or_fail()?;
            }
            keys.push(key);
            signatures.push(signature);
        }
        expect_true!(active.iter().any(|keep| !keep));
        let suffixes: Vec<_> = std::iter::once(vec![])
            .chain((0..alphabet.len()).map(|i| vec![i]))
            .chain((0..alphabet.len()).flat_map(|i| (0..alphabet.len()).map(move |j| vec![i, j])))
            .collect();
        for (i, word) in words.iter().enumerate().filter(|(i, _)| !active[*i]) {
            let witnesses: Vec<_> = words
                .iter()
                .enumerate()
                .filter(|(j, _)| active[*j] && keys[*j] == keys[i])
                .collect();
            let mut safe = false;
            'witness: for (j, replacement) in witnesses {
                if !signatures[j].dominates(&signatures[i]) {
                    continue;
                }
                for suffix in &suffixes {
                    let continuation: Vec<_> =
                        suffix.iter().map(|&k| alphabet[k].clone()).collect();
                    let original: Vec<_> = word
                        .iter()
                        .map(|&k| alphabet[k].clone())
                        .chain(continuation.clone())
                        .collect();
                    let substitute: Vec<_> = replacement
                        .iter()
                        .map(|&k| alphabet[k].clone())
                        .chain(continuation)
                        .collect();
                    if substitute.len() > original.len()
                        || substitute
                            .iter()
                            .filter(|op| matches!(op.gate, Gate::T | Gate::Tdg))
                            .count()
                            > original
                                .iter()
                                .filter(|op| matches!(op.gate, Gate::T | Gate::Tdg))
                                .count()
                        || substitute
                            .iter()
                            .filter(|op| op.gate.target_count() == 2)
                            .count()
                            > original
                                .iter()
                                .filter(|op| op.gate.target_count() == 2)
                                .count()
                    {
                        continue 'witness;
                    }
                    for incoming in [[0, 0], [1, 3], [5, 0]] {
                        let a = depth(incoming, &original)?;
                        let b = depth(incoming, &substitute)?;
                        if b[0] > a[0] || b[1] > a[1] {
                            continue 'witness;
                        }
                    }
                }
                safe = true;
                break;
            }
            if !safe {
                return fail!("discarded word {i} lacks a safe representative");
            }
        }
        Ok(())
    }
}
