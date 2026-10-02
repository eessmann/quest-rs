use super::representatives::{ParetoIndex, Signature};
use dashu_int::IBig;
use quest_math::{
    DyadicBox8, ExactMatrix, Limits, Operation, Sequence, Target, adjoint_times_rotation_enclosure,
    certify_rotation, dyadic_from_bits,
};
use quest_optimizer_protocol::{MitmLimits, Outcome};

struct State {
    matrix: ExactMatrix,
    parent: Option<usize>,
    via: Option<usize>,
    depth: usize,
    signature: Signature,
    active: bool,
}
struct Budget {
    limits: MitmLimits,
    states: usize,
    bytes: usize,
    work: u64,
    truncated: bool,
}
impl Budget {
    const fn new(limits: MitmLimits) -> Self {
        Self {
            limits,
            states: 0,
            bytes: 4 * 1024 * 1024,
            work: 0,
            truncated: false,
        }
    }
    const fn work(&mut self, units: u64) -> bool {
        match self.work.checked_add(units) {
            Some(next) if next <= self.limits.max_work => {
                self.work = next;
                true
            }
            _ => {
                self.truncated = true;
                false
            }
        }
    }
    const fn state(&mut self, bytes: usize, left: bool) -> bool {
        let Some(next_states) = self.states.checked_add(1) else {
            self.truncated = true;
            return false;
        };
        let Some(next_bytes) = self.bytes.checked_add(bytes) else {
            self.truncated = true;
            return false;
        };
        let side_bytes = if left {
            self.limits
                .table_bytes
                .saturating_div(2)
                .saturating_add(4 * 1024 * 1024)
        } else {
            self.limits.table_bytes
        };
        let side_states = if left {
            self.limits.max_states / 2
        } else {
            self.limits.max_states
        };
        if next_states > side_states || next_bytes > side_bytes {
            self.truncated = true;
            return false;
        }
        self.states = next_states;
        self.bytes = next_bytes;
        true
    }
    fn explored(&self) -> u64 {
        u64::try_from(self.states).unwrap_or(u64::MAX)
    }
}

pub fn search_approx(target: &Target, epsilon_bits: u64, limits: MitmLimits) -> Outcome {
    match run(target, epsilon_bits, limits) {
        Ok(outcome) => outcome,
        Err(message) => Outcome::Failure {
            code: "approx-mitm".into(),
            message,
        },
    }
}

#[allow(clippy::too_many_lines)] // The bounded search keeps its admission and terminal states in one place.
fn run(target: &Target, epsilon_bits: u64, limits: MitmLimits) -> Result<Outcome, String> {
    limits.validate(1).map_err(|error| error.to_string())?;
    if limits.table_bytes < 4 * 1024 * 1024 {
        return Ok(Outcome::Incomplete {
            reason: "interval scratch".into(),
            explored: 0,
        });
    }
    let proof_limits = Limits {
        precision_bits: limits.precision_bits,
        bytes: 4 * 1024 * 1024,
        ..Limits::default()
    };
    if let Err(error) = quest_math::admit_rotation_target(target, proof_limits) {
        return admission_error(&error, "target admission");
    }
    let epsilon = match dyadic_from_bits(epsilon_bits, proof_limits) {
        Ok(epsilon) => epsilon,
        Err(error) => return admission_error(&error, "epsilon admission"),
    };
    if epsilon <= quest_math::RBig::from(0) || epsilon >= quest_math::RBig::from(1) {
        return Err("MITM epsilon must be positive and less than one".into());
    }
    let table_limits = Limits {
        qubits: 1,
        gates: 1,
        coefficient_bits: limits.coefficient_bits,
        bytes: limits.table_bytes,
        ..proof_limits
    };
    let alphabet = super::exact::alphabet(1);
    let mut budget = Budget::new(limits);
    let mut gates = Vec::new();
    if gates.try_reserve_exact(alphabet.len()).is_err() {
        return Ok(Outcome::Incomplete {
            reason: "gate allocation".into(),
            explored: budget.explored(),
        });
    }
    for operation in &alphabet {
        if !budget.work(64) || !budget.state(8 * 1024, true) {
            return Ok(Outcome::Incomplete {
                reason: "gate table".into(),
                explored: budget.explored(),
            });
        }
        match ExactMatrix::for_operation(1, operation, table_limits) {
            Ok(matrix) => gates.push(matrix),
            Err(error) => {
                return Ok(Outcome::Incomplete {
                    reason: format!("gate table: {error}"),
                    explored: budget.explored(),
                });
            }
        }
    }
    let identity = match ExactMatrix::identity(1, table_limits) {
        Ok(matrix) => matrix,
        Err(error) => {
            return Ok(Outcome::Incomplete {
                reason: format!("identity table: {error}"),
                explored: budget.explored(),
            });
        }
    };
    let left_depth = limits.max_depth / 2;
    let right_depth = limits
        .max_depth
        .checked_sub(left_depth)
        .ok_or("depth overflow")?;
    let mut left = Vec::new();
    let mut left_index = ParetoIndex::new();
    let identity_key = identity
        .full_phase_key(table_limits)
        .map_err(|error| error.to_string())?;
    if !budget.state(64 * 1024, true) {
        return Ok(Outcome::Incomplete {
            reason: "left table".into(),
            explored: budget.explored(),
        });
    }
    left.push(State {
        matrix: identity.clone(),
        parent: None,
        via: None,
        depth: 0,
        signature: Signature::identity(1),
        active: true,
    });
    if left_index
        .commit(identity_key.clone(), Signature::identity(1), 0, |_| {})
        .is_err()
    {
        return Ok(Outcome::Incomplete {
            reason: "left index allocation".into(),
            explored: budget.explored(),
        });
    }
    if let Err(error) = expand(
        &mut left,
        &mut left_index,
        left_depth,
        &gates,
        &alphabet,
        table_limits,
        64 * 1024,
        true,
        &mut budget,
    ) {
        return Ok(Outcome::Incomplete {
            reason: format!("left table: {error}"),
            explored: budget.explored(),
        });
    }
    let mut right = Vec::new();
    let mut right_index = ParetoIndex::new();
    if !budget.state(8 * 1024, false) {
        return Ok(Outcome::Incomplete {
            reason: "right table".into(),
            explored: budget.explored(),
        });
    }
    right.push(State {
        matrix: identity,
        parent: None,
        via: None,
        depth: 0,
        signature: Signature::identity(1),
        active: true,
    });
    if right_index
        .commit(identity_key, Signature::identity(1), 0, |_| {})
        .is_err()
    {
        return Ok(Outcome::Incomplete {
            reason: "right index allocation".into(),
            explored: budget.explored(),
        });
    }
    if let Err(error) = expand(
        &mut right,
        &mut right_index,
        right_depth,
        &gates,
        &alphabet,
        table_limits,
        8 * 1024,
        false,
        &mut budget,
    ) {
        return Ok(Outcome::Incomplete {
            reason: format!("right table: {error}"),
            explored: budget.explored(),
        });
    }
    let mut attempted = std::collections::HashSet::new();
    let mut shortlisted = 0usize;
    let mut unresolved = false;
    let mut last_precision = 0usize;
    for bits in [128, 256, 512, 1024, 2048, 4096] {
        if bits > limits.precision_bits {
            break;
        }
        last_precision = bits;
        let mut round_unresolved = false;
        let mut left_boxes = Vec::new();
        let mut active_left = Vec::new();
        if left_boxes.try_reserve_exact(left.len()).is_err() {
            return Ok(Outcome::Incomplete {
                reason: "interval allocation".into(),
                explored: budget.explored(),
            });
        }
        if active_left.try_reserve_exact(left.len()).is_err() {
            return Ok(Outcome::Incomplete {
                reason: "interval index allocation".into(),
                explored: budget.explored(),
            });
        }
        for (index, state) in left.iter().enumerate() {
            if !state.active {
                continue;
            }
            if !budget.work(64) {
                return Ok(Outcome::Incomplete {
                    reason: "interval work".into(),
                    explored: budget.explored(),
                });
            }
            match DyadicBox8::from_exact(&state.matrix, bits, proof_limits) {
                Ok(box_) => {
                    left_boxes.push(box_);
                    active_left.push(index);
                }
                Err(quest_math::Error::NotCertified) => {
                    return Ok(Outcome::Unresolved {
                        precision_bits: bits,
                        explored: budget.explored(),
                    });
                }
                Err(quest_math::Error::Budget { .. } | quest_math::Error::Resource(_)) => {
                    return Ok(Outcome::Incomplete {
                        reason: "interval resources".into(),
                        explored: budget.explored(),
                    });
                }
                Err(error) => return Err(error.to_string()),
            }
        }
        let log = usize::BITS
            .checked_sub(left_boxes.len().leading_zeros())
            .ok_or("index work")?;
        let index_work = u64::try_from(left_boxes.len())
            .ok()
            .and_then(|n| n.checked_mul(u64::from(log)))
            .and_then(|n| n.checked_mul(u64::from(log)))
            .and_then(|n| n.checked_mul(8))
            .ok_or("index work")?;
        if !budget.work(index_work) {
            return Ok(Outcome::Incomplete {
                reason: "index work".into(),
                explored: budget.explored(),
            });
        }
        let tree = match Tree::build(&left_boxes, 8) {
            Ok(tree) => tree,
            Err(quest_math::Error::Budget { .. } | quest_math::Error::Resource(_)) => {
                return Ok(Outcome::Incomplete {
                    reason: "index resources".into(),
                    explored: budget.explored(),
                });
            }
            Err(error) => return Err(error.to_string()),
        };
        let epsilon_squared = std::ops::Mul::mul(&epsilon, &epsilon);
        let scale_squared = std::ops::Shl::shl(
            IBig::from(1),
            bits.checked_mul(2).ok_or("precision overflow")?,
        );
        let threshold = std::ops::Div::div(
            std::ops::Mul::mul(epsilon_squared.numerator(), scale_squared),
            epsilon_squared.denominator(),
        );
        let mut any_hit = false;
        for (right_index, state) in right.iter().enumerate() {
            if !state.active {
                continue;
            }
            if !budget.work(64) {
                return Ok(Outcome::Incomplete {
                    reason: "query work".into(),
                    explored: budget.explored(),
                });
            }
            let query =
                match adjoint_times_rotation_enclosure(&state.matrix, target, bits, proof_limits) {
                    Ok(box_) => box_,
                    Err(quest_math::Error::NotCertified) => {
                        round_unresolved = true;
                        continue;
                    }
                    Err(quest_math::Error::Budget { .. } | quest_math::Error::Resource(_)) => {
                        return Ok(Outcome::Incomplete {
                            reason: "query resources".into(),
                            explored: budget.explored(),
                        });
                    }
                    Err(error) => return Err(error.to_string()),
                };
            let mut hits = Vec::new();
            let complete = match tree.query(&query, &threshold, &mut hits, &mut budget) {
                Ok(complete) => complete,
                Err(quest_math::Error::Budget { .. } | quest_math::Error::Resource(_)) => {
                    return Ok(Outcome::Incomplete {
                        reason: "range resources".into(),
                        explored: budget.explored(),
                    });
                }
                Err(error) => return Err(error.to_string()),
            };
            if !complete {
                return Ok(Outcome::Incomplete {
                    reason: "range work".into(),
                    explored: budget.explored(),
                });
            }
            hits.sort_unstable();
            any_hit |= !hits.is_empty();
            for box_index in hits {
                let left_index = *active_left.get(box_index).ok_or("interval index")?;
                if attempted.try_reserve(1).is_err() {
                    return Ok(Outcome::Incomplete {
                        reason: "shortlist allocation".into(),
                        explored: budget.explored(),
                    });
                }
                if !attempted.insert((left_index, right_index)) {
                    continue;
                }
                if shortlisted >= limits.shortlist {
                    return Ok(Outcome::Exhausted {
                        explored: budget.explored(),
                    });
                }
                shortlisted = shortlisted.checked_add(1).ok_or("shortlist overflow")?;
                if !budget.work(128) {
                    return Ok(Outcome::Incomplete {
                        reason: "certificate work".into(),
                        explored: budget.explored(),
                    });
                }
                let mut operations = path(&left, left_index, &alphabet)?;
                operations.extend(path(&right, right_index, &alphabet)?);
                let candidate = Sequence {
                    qubits: 1,
                    operations,
                };
                match certify_rotation(&candidate, target, epsilon_bits, proof_limits) {
                    Ok(proof) => {
                        return Ok(Outcome::Candidate {
                            sequence: proof.candidate().clone(),
                            engine: "mitm-interval-v1".into(),
                            precision_bits: proof.precision_bits(),
                        });
                    }
                    Err(quest_math::Error::NotCertified) => unresolved = true,
                    Err(quest_math::Error::Budget { .. } | quest_math::Error::Resource(_)) => {
                        return Ok(Outcome::Incomplete {
                            reason: "certificate resources".into(),
                            explored: budget.explored(),
                        });
                    }
                    Err(error) => return Err(error.to_string()),
                }
            }
        }
        if !any_hit && !round_unresolved {
            return Ok(if budget.truncated {
                Outcome::Incomplete {
                    reason: "state enumeration".into(),
                    explored: budget.explored(),
                }
            } else {
                Outcome::NoCandidate {
                    explored: budget.explored(),
                }
            });
        }
        unresolved |= round_unresolved;
    }
    Ok(if budget.truncated {
        Outcome::Incomplete {
            reason: "state enumeration".into(),
            explored: budget.explored(),
        }
    } else if unresolved {
        Outcome::Unresolved {
            precision_bits: last_precision,
            explored: budget.explored(),
        }
    } else {
        Outcome::NoCandidate {
            explored: budget.explored(),
        }
    })
}

fn admission_error(error: &quest_math::Error, reason: &'static str) -> Result<Outcome, String> {
    let resource_limited = matches!(error, quest_math::Error::Resource(_))
        || matches!(error, quest_math::Error::Budget { resource, .. } if resource == "allocation bytes");
    if resource_limited {
        Ok(Outcome::Incomplete {
            reason: reason.into(),
            explored: 0,
        })
    } else {
        Err(error.to_string())
    }
}

#[allow(clippy::too_many_arguments)] // Both bounded half-table walks share one admission and index routine.
fn expand(
    states: &mut Vec<State>,
    index: &mut ParetoIndex,
    max_depth: usize,
    gates: &[ExactMatrix],
    alphabet: &[Operation],
    limits: Limits,
    state_bytes: usize,
    left: bool,
    budget: &mut Budget,
) -> quest_math::Result<()> {
    let mut cursor = 0usize;
    while cursor < states.len() {
        let current = states
            .get(cursor)
            .ok_or_else(|| quest_math::Error::Resource("MITM state cursor".into()))?;
        if !current.active {
            cursor = cursor
                .checked_add(1)
                .ok_or_else(|| quest_math::Error::Resource("MITM state cursor".into()))?;
            continue;
        }
        if current.depth < max_depth {
            for (via, (gate, operation)) in gates.iter().zip(alphabet).enumerate() {
                if !budget.work(64) {
                    return Ok(());
                }
                let current = states
                    .get(cursor)
                    .ok_or_else(|| quest_math::Error::Resource("MITM state cursor".into()))?;
                let signature = current
                    .signature
                    .after(operation)
                    .ok_or_else(|| quest_math::Error::Resource("MITM signature overflow".into()))?;
                let matrix = gate.multiply(&current.matrix, limits)?;
                let depth = current
                    .depth
                    .checked_add(1)
                    .ok_or_else(|| quest_math::Error::Resource("MITM depth".into()))?;
                let key = matrix.full_phase_key(limits)?;
                if !budget.work(
                    u64::try_from(index.bucket_len(&key))
                        .unwrap_or(u64::MAX)
                        .saturating_mul(2),
                ) {
                    return Ok(());
                }
                if index.is_dominated(&key, &signature) {
                    continue;
                }
                if !budget.state(state_bytes, left) {
                    return Ok(());
                }
                states
                    .try_reserve(1)
                    .map_err(|_| quest_math::Error::Resource("MITM state allocation".into()))?;
                states.push(State {
                    matrix,
                    parent: Some(cursor),
                    via: Some(via),
                    depth,
                    signature,
                    active: true,
                });
                let next = states
                    .len()
                    .checked_sub(1)
                    .ok_or_else(|| quest_math::Error::Resource("MITM state index".into()))?;
                index
                    .commit(key, signature, next, |retired| {
                        if let Some(state) = states.get_mut(retired) {
                            state.active = false;
                        }
                    })
                    .map_err(|()| quest_math::Error::Resource("MITM index allocation".into()))?;
            }
        }
        cursor = cursor
            .checked_add(1)
            .ok_or_else(|| quest_math::Error::Resource("MITM state cursor".into()))?;
    }
    Ok(())
}
fn path(
    states: &[State],
    mut index: usize,
    alphabet: &[Operation],
) -> Result<Vec<Operation>, String> {
    let mut operations = Vec::new();
    while let Some(via) = states.get(index).and_then(|state| state.via) {
        operations
            .try_reserve(1)
            .map_err(|error| error.to_string())?;
        operations.push(alphabet.get(via).ok_or("predecessor operation")?.clone());
        index = states
            .get(index)
            .and_then(|state| state.parent)
            .ok_or("predecessor state")?;
    }
    operations.reverse();
    Ok(operations)
}

struct Node {
    bounds: DyadicBox8,
    start: usize,
    end: usize,
    left: Option<usize>,
    right: Option<usize>,
}
struct Tree<'a> {
    boxes: &'a [DyadicBox8],
    order: Vec<usize>,
    nodes: Vec<Node>,
}
impl<'a> Tree<'a> {
    fn build(boxes: &'a [DyadicBox8], leaf_size: usize) -> quest_math::Result<Self> {
        if boxes.is_empty() || leaf_size == 0 {
            return Err(quest_math::Error::Invalid("empty MITM tree".into()));
        }
        let mut order = Vec::new();
        order
            .try_reserve_exact(boxes.len())
            .map_err(|_| quest_math::Error::Resource("MITM index allocation".into()))?;
        order.extend(0..boxes.len());
        let mut nodes = Vec::new();
        nodes
            .try_reserve_exact(
                boxes
                    .len()
                    .checked_mul(2)
                    .ok_or_else(|| quest_math::Error::Resource("MITM node count".into()))?,
            )
            .map_err(|_| quest_math::Error::Resource("MITM node allocation".into()))?;
        let mut tree = Self {
            boxes,
            order,
            nodes,
        };
        tree.add_node(0, boxes.len(), leaf_size)?;
        Ok(tree)
    }
    fn add_node(
        &mut self,
        start: usize,
        end: usize,
        leaf_size: usize,
    ) -> quest_math::Result<usize> {
        let first = *self
            .order
            .get(start)
            .ok_or_else(|| quest_math::Error::Invalid("MITM node range".into()))?;
        let mut bounds = self
            .boxes
            .get(first)
            .ok_or_else(|| quest_math::Error::Invalid("MITM box".into()))?
            .clone();
        let range = self
            .order
            .get(start..end)
            .ok_or_else(|| quest_math::Error::Invalid("MITM node range".into()))?;
        for &index in range {
            bounds.include(
                self.boxes
                    .get(index)
                    .ok_or_else(|| quest_math::Error::Invalid("MITM box".into()))?,
            )?;
        }
        let node_index = self.nodes.len();
        self.nodes.push(Node {
            bounds,
            start,
            end,
            left: None,
            right: None,
        });
        if end
            .checked_sub(start)
            .ok_or_else(|| quest_math::Error::Invalid("MITM node range".into()))?
            <= leaf_size
        {
            return Ok(node_index);
        }
        let mut split_axis = 0usize;
        let mut widest = IBig::from(-1);
        for axis in 0..8 {
            let mut minimum: Option<IBig> = None;
            let mut maximum: Option<IBig> = None;
            for &index in self
                .order
                .get(start..end)
                .ok_or_else(|| quest_math::Error::Invalid("MITM node range".into()))?
            {
                let (lower, upper) = self
                    .boxes
                    .get(index)
                    .ok_or_else(|| quest_math::Error::Invalid("MITM box".into()))?
                    .coordinate(axis)?;
                let middle_twice = std::ops::Add::add(lower, upper);
                minimum = Some(minimum.map_or_else(
                    || middle_twice.clone(),
                    |value| value.min(middle_twice.clone()),
                ));
                maximum = Some(maximum.map_or_else(
                    || middle_twice.clone(),
                    |value| value.max(middle_twice.clone()),
                ));
            }
            let spread = std::ops::Sub::sub(
                maximum.ok_or_else(|| quest_math::Error::Invalid("MITM range".into()))?,
                minimum.ok_or_else(|| quest_math::Error::Invalid("MITM range".into()))?,
            );
            if spread > widest {
                widest = spread;
                split_axis = axis;
            }
        }
        self.order
            .get_mut(start..end)
            .ok_or_else(|| quest_math::Error::Invalid("MITM node range".into()))?
            .sort_unstable_by(|&left, &right| {
                let middle = |index: usize| {
                    self.boxes
                        .get(index)
                        .and_then(|box_| box_.coordinate(split_axis).ok())
                        .map_or_else(
                            || IBig::from(0),
                            |(lower, upper)| std::ops::Add::add(lower, upper),
                        )
                };
                (middle(left), left).cmp(&(middle(right), right))
            });
        let middle = start
            .checked_add(
                end.checked_sub(start)
                    .ok_or_else(|| quest_math::Error::Invalid("MITM median".into()))?
                    / 2,
            )
            .ok_or_else(|| quest_math::Error::Resource("MITM median".into()))?;
        let left = self.add_node(start, middle, leaf_size)?;
        let right = self.add_node(middle, end, leaf_size)?;
        let node = self
            .nodes
            .get_mut(node_index)
            .ok_or_else(|| quest_math::Error::Invalid("MITM node".into()))?;
        node.left = Some(left);
        node.right = Some(right);
        Ok(node_index)
    }
    fn query(
        &self,
        box_: &DyadicBox8,
        radius_squared: &IBig,
        hits: &mut Vec<usize>,
        budget: &mut Budget,
    ) -> quest_math::Result<bool> {
        self.query_node(0, box_, radius_squared, hits, budget)
    }
    fn query_node(
        &self,
        index: usize,
        box_: &DyadicBox8,
        radius_squared: &IBig,
        hits: &mut Vec<usize>,
        budget: &mut Budget,
    ) -> quest_math::Result<bool> {
        if !budget.work(16) {
            return Ok(false);
        }
        let node = self
            .nodes
            .get(index)
            .ok_or_else(|| quest_math::Error::Invalid("MITM node".into()))?;
        if node.bounds.gap_squared(box_)? > *radius_squared {
            return Ok(true);
        }
        if let (Some(left), Some(right)) = (node.left, node.right) {
            if !self.query_node(left, box_, radius_squared, hits, budget)? {
                return Ok(false);
            }
            self.query_node(right, box_, radius_squared, hits, budget)
        } else {
            for &entry in self
                .order
                .get(node.start..node.end)
                .ok_or_else(|| quest_math::Error::Invalid("MITM node range".into()))?
            {
                if !budget.work(8) {
                    return Ok(false);
                }
                if self
                    .boxes
                    .get(entry)
                    .ok_or_else(|| quest_math::Error::Invalid("MITM box".into()))?
                    .gap_squared(box_)?
                    <= *radius_squared
                {
                    hits.try_reserve(1)
                        .map_err(|_| quest_math::Error::Resource("MITM hits allocation".into()))?;
                    hits.push(entry);
                }
            }
            Ok(true)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Budget, Tree, admission_error, search_approx};
    use googletest::{Result, prelude::*};
    use quest_math::{AngleTarget, Axis, DyadicBox8, Limits, Target, certify_rotation};
    use quest_optimizer_protocol::{MitmLimits, Outcome};

    #[gtest]
    fn admission_distinguishes_resource_exhaustion_from_request_caps() {
        expect_true!(matches!(
            admission_error(&quest_math::Error::Resource("scratch".into()), "target"),
            Ok(Outcome::Incomplete { explored: 0, .. })
        ));
        expect_true!(matches!(
            admission_error(
                &quest_math::Error::Budget {
                    resource: "allocation bytes".into(),
                    requested: 5,
                    limit: 4
                },
                "target"
            ),
            Ok(Outcome::Incomplete { explored: 0, .. })
        ));
        expect_true!(
            admission_error(
                &quest_math::Error::Budget {
                    resource: "coefficient bits".into(),
                    requested: 5,
                    limit: 4
                },
                "target"
            )
            .is_err()
        );
        expect_true!(
            admission_error(&quest_math::Error::Invalid("nonfinite".into()), "epsilon").is_err()
        );
    }

    #[gtest]
    fn approximate_mitm_certifies_zero_rotation() -> Result<()> {
        let target = Target {
            axis: Axis::Z,
            angle: AngleTarget::RationalPi {
                numerator: 0.into(),
                denominator: 1.into(),
            },
        };
        let mut limits = MitmLimits::for_qubits(1)?;
        limits.max_depth = 2;
        match search_approx(&target, 1e-8f64.to_bits(), limits) {
            Outcome::Candidate { sequence, .. } => {
                certify_rotation(&sequence, &target, 1e-8f64.to_bits(), Limits::default())?;
            }
            other => {
                return fail!("expected approximate candidate, got {other:?}");
            }
        }
        Ok(())
    }

    #[gtest]
    fn interval_tree_keeps_boundary_equality_and_prunes_strictly_far_boxes() -> Result<()> {
        let origin = DyadicBox8::point(4, std::array::from_fn(|_| 0.into()))?;
        let at_radius = DyadicBox8::point(
            4,
            [
                16.into(),
                0.into(),
                0.into(),
                0.into(),
                0.into(),
                0.into(),
                0.into(),
                0.into(),
            ],
        )?;
        let outside = DyadicBox8::point(
            4,
            [
                17.into(),
                0.into(),
                0.into(),
                0.into(),
                0.into(),
                0.into(),
                0.into(),
                0.into(),
            ],
        )?;
        let boxes = [at_radius, outside];
        let tree = Tree::build(&boxes, 8)?;
        let mut hits = Vec::new();
        let mut budget = Budget::new(MitmLimits::for_qubits(1)?);
        expect_true!(tree.query(&origin, &256.into(), &mut hits, &mut budget)?);
        expect_eq!(hits, vec![0]);
        Ok(())
    }

    #[gtest]
    fn interval_tree_radius_matches_exhaustive_boxes_across_splits() -> Result<()> {
        let mut boxes = Vec::new();
        for index in 0usize..25 {
            let mut box_ = DyadicBox8::point(
                8,
                std::array::from_fn(|axis| {
                    dashu_int::IBig::from(index.saturating_mul(axis.saturating_add(1)))
                }),
            )?;
            if index % 3 == 0 {
                let widened = DyadicBox8::point(
                    8,
                    std::array::from_fn(|axis| {
                        dashu_int::IBig::from(
                            index
                                .saturating_mul(axis.saturating_add(1))
                                .saturating_add(2),
                        )
                    }),
                )?;
                box_.include(&widened)?;
            }
            boxes.push(box_);
        }
        let tree = Tree::build(&boxes, 8)?;
        let query = DyadicBox8::point(
            8,
            std::array::from_fn(|axis| {
                dashu_int::IBig::from(7usize.saturating_mul(axis.saturating_add(1)))
            }),
        )?;
        for threshold in [0, 1, 20, 400, 10_000] {
            let radius = threshold.into();
            let mut actual = Vec::new();
            let mut budget = Budget::new(MitmLimits::for_qubits(1)?);
            expect_true!(tree.query(&query, &radius, &mut actual, &mut budget)?);
            actual.sort_unstable();
            let mut expected = Vec::new();
            for (index, box_) in boxes.iter().enumerate() {
                if box_.gap_squared(&query)? <= radius {
                    expected.push(index);
                }
            }
            expect_eq!(actual, expected);
        }
        Ok(())
    }

    #[gtest]
    fn approximate_search_distinguishes_complete_absence_and_capacity_cutoff() -> Result<()> {
        let target = Target {
            axis: Axis::Z,
            angle: AngleTarget::RationalPi {
                numerator: 1.into(),
                denominator: 7.into(),
            },
        };
        let mut limits = MitmLimits::for_qubits(1)?;
        limits.max_depth = 1;
        expect_true!(matches!(
            search_approx(&target, 1e-12f64.to_bits(), limits),
            Outcome::NoCandidate { .. }
        ));
        limits.max_states = 1;
        expect_true!(matches!(
            search_approx(&target, 1e-12f64.to_bits(), limits),
            Outcome::Incomplete { .. }
        ));
        Ok(())
    }

    #[gtest]
    fn huge_finite_dyadic_angle_can_be_unresolved_without_becoming_invalid() -> Result<()> {
        let target = Target {
            axis: Axis::Z,
            angle: AngleTarget::DyadicRadians {
                bits: f64::MAX.to_bits(),
            },
        };
        let mut limits = MitmLimits::for_qubits(1)?;
        limits.max_depth = 1;
        limits.precision_bits = 128;
        expect_true!(matches!(
            search_approx(&target, 1e-12f64.to_bits(), limits),
            Outcome::Unresolved { .. } | Outcome::NoCandidate { .. } | Outcome::Incomplete { .. }
        ));
        Ok(())
    }
}
