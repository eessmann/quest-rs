//! Append-only rewrite history. Occurrences and history nodes have separate identities.
use crate::{Error, OccurrenceId, Result};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

static NEXT_NODE: AtomicU64 = AtomicU64::new(1);

/// A history node, checked against the owning immutable graph on every query.
/// Independent edits of a cloned program cannot alias newly appended nodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProvenanceId {
    index: usize,
    identity: u64,
}
#[derive(Debug, Clone)]
enum NodeKind {
    Source(OccurrenceId),
    Rewrite(std::ops::Range<usize>),
}
#[derive(Debug, Clone)]
struct Node {
    identity: u64,
    kind: NodeKind,
}
/// Borrowed immediate history, never an implicitly expanded source list.
#[derive(Debug, Clone, Copy)]
pub enum ProvenanceNode<'a> {
    Source(OccurrenceId),
    Rewrite(&'a [ProvenanceId]),
}
/// Limits for explicit source-leaf expansion. Leaves are unique and sorted by occurrence ID.
#[derive(Debug, Clone, Copy)]
pub struct ExpansionLimits {
    pub max_work: usize,
    pub max_leaves: usize,
    pub max_bytes: usize,
}
impl Default for ExpansionLimits {
    fn default() -> Self {
        Self {
            max_work: 4_000_000,
            max_leaves: 1_000_000,
            max_bytes: 64 * 1024 * 1024,
        }
    }
}
/// Immutable public history snapshot. Transformations append to a private owned
/// copy once per pass, then share the published graph with programs and reports.
#[derive(Debug, Clone, Default)]
pub struct ProvenanceGraph {
    nodes: Vec<Node>,
    inputs: Vec<ProvenanceId>,
    next_occurrence: usize,
}
impl ProvenanceGraph {
    #[must_use]
    pub const fn node_count(&self) -> usize {
        self.nodes.len()
    }
    #[must_use]
    pub const fn edge_count(&self) -> usize {
        self.inputs.len()
    }
    /// Retained graph allocation capacities and inline storage; excludes allocator metadata.
    /// # Errors
    /// Rejects byte-count overflow.
    pub fn retained_bytes(&self) -> Result<usize> {
        Self::bytes(self.nodes.capacity(), self.inputs.capacity())
    }
    fn bytes(nodes: usize, inputs: usize) -> Result<usize> {
        nodes
            .checked_mul(size_of::<Node>())
            .and_then(|bytes| {
                inputs
                    .checked_mul(size_of::<ProvenanceId>())
                    .and_then(|edges| bytes.checked_add(edges))
            })
            .and_then(|bytes| bytes.checked_add(size_of::<Self>()))
            .ok_or(Error::Budget("provenance storage"))
    }
    /// # Errors
    /// Rejects foreign, stale, or missing history identities.
    pub fn node(&self, id: ProvenanceId) -> Result<ProvenanceNode<'_>> {
        let node = self
            .nodes
            .get(id.index)
            .filter(|node| node.identity == id.identity)
            .ok_or(Error::InvalidId)?;
        match &node.kind {
            NodeKind::Source(source) => Ok(ProvenanceNode::Source(*source)),
            NodeKind::Rewrite(range) => Ok(ProvenanceNode::Rewrite(
                self.inputs.get(range.clone()).ok_or(Error::InvalidId)?,
            )),
        }
    }
    /// Expand source occurrences with bounded work and scratch, without recursion.
    /// # Errors
    /// Rejects foreign IDs, exhausted work/leaf/storage limits, and allocation failure.
    pub fn source_leaves(
        &self,
        root: ProvenanceId,
        limits: ExpansionLimits,
    ) -> Result<Vec<OccurrenceId>> {
        self.node(root)?;
        let mut remaining = limits
            .max_work
            .checked_sub(self.nodes.len())
            .ok_or(Error::Budget("provenance expansion work"))?;
        // Mark on enqueue, so each node occupies the pending stack at most once.
        let leaves = self.nodes.len().min(limits.max_leaves);
        let scratch = self
            .nodes
            .len()
            .checked_mul(const { size_of::<bool>() + size_of::<ProvenanceId>() })
            .and_then(|bytes| {
                leaves
                    .checked_mul(size_of::<OccurrenceId>())
                    .and_then(|extra| bytes.checked_add(extra))
            })
            .ok_or(Error::Budget("provenance expansion storage"))?;
        if scratch > limits.max_bytes {
            return Err(Error::Budget("provenance expansion storage"));
        }
        let mut seen = Vec::new();
        seen.try_reserve_exact(self.nodes.len())
            .map_err(|_| Error::Budget("provenance expansion allocation"))?;
        seen.resize(self.nodes.len(), false);
        let mut pending = Vec::new();
        pending
            .try_reserve_exact(self.nodes.len())
            .map_err(|_| Error::Budget("provenance expansion allocation"))?;
        let mut output = Vec::new();
        output
            .try_reserve_exact(leaves)
            .map_err(|_| Error::Budget("provenance expansion allocation"))?;
        *seen.get_mut(root.index).ok_or(Error::InvalidId)? = true;
        pending.push(root);
        while let Some(id) = pending.pop() {
            remaining = remaining
                .checked_sub(1)
                .ok_or(Error::Budget("provenance expansion work"))?;
            match self.node(id)? {
                ProvenanceNode::Source(source) => {
                    if output.len() >= limits.max_leaves {
                        return Err(Error::Budget("provenance expansion leaves"));
                    }
                    output.push(source);
                }
                ProvenanceNode::Rewrite(inputs) => {
                    for &input in inputs {
                        remaining = remaining
                            .checked_sub(1)
                            .ok_or(Error::Budget("provenance expansion work"))?;
                        let seen = seen.get_mut(input.index).ok_or(Error::InvalidId)?;
                        if !*seen {
                            *seen = true;
                            pending.push(input);
                        }
                    }
                }
            }
        }
        // Charge a conservative comparison-sort bound and the deduplication scan.
        let sort_work = output
            .len()
            .checked_mul(
                usize::try_from(output.len().checked_ilog2().unwrap_or(0))
                    .map_err(|_| Error::Budget("provenance expansion work"))?
                    .checked_add(2)
                    .ok_or(Error::Budget("provenance expansion work"))?,
            )
            .ok_or(Error::Budget("provenance expansion work"))?;
        if sort_work > remaining {
            return Err(Error::Budget("provenance expansion work"));
        }
        output.sort_unstable();
        output.dedup();
        Ok(output)
    }
    pub(crate) fn copy_work(&self) -> Result<usize> {
        self.nodes
            .len()
            .checked_add(self.inputs.len())
            .ok_or(Error::Budget("provenance work"))
    }
    pub(crate) fn edit(graph: Arc<Self>, max_bytes: usize) -> Result<Self> {
        if graph.retained_bytes()? > max_bytes {
            return Err(Error::Budget("provenance storage"));
        }
        match Arc::try_unwrap(graph) {
            Ok(graph) => Ok(graph),
            Err(graph) => {
                let mut result = Self {
                    next_occurrence: graph.next_occurrence,
                    ..Self::default()
                };
                result
                    .nodes
                    .try_reserve_exact(graph.nodes.len())
                    .map_err(|_| Error::Budget("provenance allocation"))?;
                result
                    .inputs
                    .try_reserve_exact(graph.inputs.len())
                    .map_err(|_| Error::Budget("provenance allocation"))?;
                result.nodes.extend_from_slice(&graph.nodes);
                result.inputs.extend_from_slice(&graph.inputs);
                Ok(result)
            }
        }
    }
    fn reserve(&mut self, extra_nodes: usize, extra_inputs: usize, max_bytes: usize) -> Result<()> {
        fn capacity<T>(items: &Vec<T>, extra: usize) -> Result<usize> {
            let needed = items
                .len()
                .checked_add(extra)
                .ok_or(Error::Budget("provenance storage"))?;
            if needed <= items.capacity() {
                return Ok(items.capacity());
            }
            Ok(needed
                .max(
                    items
                        .capacity()
                        .checked_mul(2)
                        .ok_or(Error::Budget("provenance storage"))?,
                )
                .max(4))
        }
        let nodes = capacity(&self.nodes, extra_nodes)?;
        let inputs = capacity(&self.inputs, extra_inputs)?;
        if Self::bytes(nodes, inputs)? > max_bytes {
            return Err(Error::Budget("provenance storage"));
        }
        self.nodes
            .try_reserve_exact(nodes.saturating_sub(self.nodes.len()))
            .map_err(|_| Error::Budget("provenance allocation"))?;
        self.inputs
            .try_reserve_exact(inputs.saturating_sub(self.inputs.len()))
            .map_err(|_| Error::Budget("provenance allocation"))?;
        if self.retained_bytes()? > max_bytes {
            return Err(Error::Budget("provenance storage"));
        }
        Ok(())
    }
    fn append(&mut self, kind: NodeKind) -> Result<ProvenanceId> {
        let identity = NEXT_NODE
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
            .map_err(|_| Error::Budget("provenance identities"))?;
        let id = ProvenanceId {
            index: self.nodes.len(),
            identity,
        };
        self.nodes.push(Node { identity, kind });
        Ok(id)
    }
    /// Stage source leaves as one atomic append, without cloning the existing graph.
    pub(crate) fn sources(
        &mut self,
        sources: &[OccurrenceId],
        max_bytes: usize,
    ) -> Result<Vec<ProvenanceId>> {
        let next_occurrence = sources
            .iter()
            .try_fold(self.next_occurrence, |next, source| {
                source
                    .index()
                    .checked_add(1)
                    .map(|after| next.max(after))
                    .ok_or(Error::Budget("occurrence identities"))
            })?;
        let count =
            u64::try_from(sources.len()).map_err(|_| Error::Budget("provenance identities"))?;
        let mut output = Vec::new();
        output
            .try_reserve_exact(sources.len())
            .map_err(|_| Error::Budget("provenance allocation"))?;
        self.reserve(sources.len(), 0, max_bytes)?;
        let first = NEXT_NODE
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |id| {
                id.checked_add(count)
            })
            .map_err(|_| Error::Budget("provenance identities"))?;
        // The atomic reservation proved this range cannot overflow.
        for (identity, &source) in (first..first.saturating_add(count)).zip(sources) {
            let id = ProvenanceId {
                index: self.nodes.len(),
                identity,
            };
            self.nodes.push(Node {
                identity,
                kind: NodeKind::Source(source),
            });
            output.push(id);
        }
        self.next_occurrence = next_occurrence;
        Ok(output)
    }
    pub(crate) fn source(
        &mut self,
        source: OccurrenceId,
        max_bytes: usize,
    ) -> Result<ProvenanceId> {
        self.reserve(1, 0, max_bytes)?;
        self.next_occurrence = self.next_occurrence.max(
            source
                .index()
                .checked_add(1)
                .ok_or(Error::Budget("occurrence identities"))?,
        );
        self.append(NodeKind::Source(source))
    }
    pub(crate) fn rewrite(
        &mut self,
        inputs: &[ProvenanceId],
        max_bytes: usize,
    ) -> Result<ProvenanceId> {
        for &id in inputs {
            self.node(id)?;
        }
        self.reserve(1, inputs.len(), max_bytes)?;
        let start = self.inputs.len();
        let end = start
            .checked_add(inputs.len())
            .ok_or(Error::Budget("provenance storage"))?;
        let id = self.append(NodeKind::Rewrite(start..end))?;
        self.inputs.extend_from_slice(inputs);
        Ok(id)
    }
    #[cfg(feature = "workers")]
    pub(crate) const fn next_occurrence(&self) -> usize {
        self.next_occurrence
    }
    #[cfg(feature = "workers")]
    pub(crate) fn retain_next_occurrence(&mut self, next: usize) {
        self.next_occurrence = self.next_occurrence.max(next);
    }
}
