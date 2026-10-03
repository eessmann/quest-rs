//! Deterministic bounded linear reversible synthesis. The PMH candidate uses
//! duplicate block-pattern elimination, then repeats on the transpose.
//! Algorithm reference: <https://arxiv.org/abs/quant-ph/0302002>.
//!
//! `Cnot` inputs are untrusted ordered control/target indices. Both candidates
//! are replayed from the identity binary matrix and compared with the source
//! map. The chosen sequence retains the original on a gate-count tie; Gaussian
//! wins a tie with PMH. This is deterministic bounded synthesis, not a claim of
//! minimum CNOT count. A fixed caller block size does not claim the asymptotic
//! bound of a size-dependent block schedule.
//!
//! Program passes preserve occurrence ownership and union input provenance.
//! Effects and other gates split contiguous windows. Any explicit user ordering
//! edge disables the pass. The dependency graph is rebuilt before returning.
//! Work counts source scanning, matrix row inspections and replay operations;
//! byte limits forecast output/provenance copies and bounded matrix/list scratch.
//! These are logical resource bounds, not process-RSS or wall-clock guarantees.
//! Unsupported width or exhausted budgets return an error without publishing
//! a partially replaced program.
use crate::{Control, ControlState, Error, Gate, OccurrenceId, QuantumRegion, QubitId, Result};
use crate::{ProvenanceGraph, ProvenanceId};
use quest_language::quantum::model::{Occurrence, SemanticOperation};
use std::{collections::BTreeMap, sync::Arc};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cnot {
	pub control: usize,
	pub target: usize,
}
#[derive(Debug, Clone, Copy)]
pub struct LinearOptions {
	pub max_qubits: usize,
	pub max_window_operations: usize,
	pub block_size: usize,
	pub max_work: usize,
	pub max_bytes: usize,
}
/// Algebraic candidate choice; comparison belongs to the enclosing search.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinearCandidateStrategy {
	Gaussian,
	Pmh,
}
impl Default for LinearOptions {
	fn default() -> Self {
		Self {
			max_qubits: 64,
			max_window_operations: 1024,
			block_size: 3,
			max_work: 1_000_000,
			max_bytes: 64 * 1024 * 1024,
		}
	}
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinearSynthesis {
	gates: Vec<Cnot>,
	gaussian: Vec<Cnot>,
	pmh: Vec<Cnot>,
}
impl LinearSynthesis {
	#[must_use]
	pub fn gates(&self) -> &[Cnot] {
		&self.gates
	}
	#[must_use]
	pub fn gaussian(&self) -> &[Cnot] {
		&self.gaussian
	}
	#[must_use]
	pub fn pmh(&self) -> &[Cnot] {
		&self.pmh
	}
}
#[derive(Debug, Clone)]
pub struct LinearRewrite {
	pub inputs: Vec<ProvenanceId>,
	pub provenance: ProvenanceId,
	pub outputs: Vec<OccurrenceId>,
}
#[derive(Debug, Clone, Default)]
pub struct LinearReport {
	pub before_operations: usize,
	pub after_operations: usize,
	pub considered_windows: usize,
	pub accepted_windows: usize,
	pub gaussian_operations: usize,
	pub pmh_operations: usize,
	pub work: usize,
	pub rewrites: Vec<LinearRewrite>,
	pub provenance: Arc<ProvenanceGraph>,
}

pub struct Work {
	pub used: usize,
	pub maximum: usize,
}
impl Work {
	pub fn charge(&mut self, amount: usize) -> Result<()> {
		self.used = self
			.used
			.checked_add(amount)
			.ok_or(Error::Budget("linear work"))?;
		if self.used > self.maximum {
			return Err(Error::Budget("linear work"));
		}
		Ok(())
	}
}
pub fn bit(index: usize) -> Result<u64> {
	1u64.checked_shl(u32::try_from(index).map_err(|_| Error::Budget("linear width"))?)
		.ok_or(Error::Budget("linear width"))
}
pub fn identity(width: usize) -> Result<Vec<u64>> {
	(0..width).map(bit).collect()
}
pub fn row_add(rows: &mut [u64], gate: Cnot) -> Result<()> {
	let source = *rows.get(gate.control).ok_or(Error::InvalidId)?;
	*rows.get_mut(gate.target).ok_or(Error::InvalidId)? ^= source;
	Ok(())
}
pub fn matrix(width: usize, gates: &[Cnot], work: &mut Work) -> Result<Vec<u64>> {
	let mut rows = identity(width)?;
	for &gate in gates {
		work.charge(1)?;
		if gate.control == gate.target {
			return Err(Error::DuplicateOperand);
		}
		row_add(&mut rows, gate)?;
	}
	Ok(rows)
}
pub fn preflight(width: usize, count: usize, options: LinearOptions) -> Result<()> {
	if width > options.max_qubits.min(64) {
		return Err(Error::Budget("linear width"));
	}
	if options.block_size == 0 || options.block_size > 64 {
		return Err(Error::Budget("linear block width"));
	}
	if count > options.max_window_operations {
		return Err(Error::Budget("linear window"));
	}
	// Three elimination lists plus input, selected output, maps and block table.
	let bytes = width
		.checked_mul(width)
		.and_then(|n| n.checked_mul(128))
		.and_then(|n| n.checked_add(count.checked_mul(64)?))
		.and_then(|n| n.checked_add(4096))
		.ok_or(Error::Budget("linear bytes"))?;
	if bytes > options.max_bytes {
		return Err(Error::Budget("linear bytes"));
	}
	Ok(())
}
fn eliminate(rows: &mut [u64], block_size: usize, work: &mut Work) -> Result<Vec<Cnot>> {
	let width = rows.len();
	let mut gates = vec![];
	for start in (0..width).step_by(block_size) {
		let end = start.saturating_add(block_size).min(width);
		if block_size > 1 {
			let mut patterns = BTreeMap::new();
			let mask =
				(start..end).try_fold(0, |mask, index| Ok::<_, Error>(mask | bit(index)?))?;
			for row in start..width {
				work.charge(1)?;
				let pattern = *rows.get(row).ok_or(Error::InvalidId)? & mask;
				if pattern == 0 {
					continue;
				}
				if let Some(&control) = patterns.get(&pattern) {
					let gate = Cnot {
						control,
						target: row,
					};
					row_add(rows, gate)?;
					gates.push(gate);
				} else {
					patterns.insert(pattern, row);
				}
			}
		}
		for pivot in start..end {
			let mask = bit(pivot)?;
			if rows.get(pivot).ok_or(Error::InvalidId)? & mask == 0 {
				let mut source = None;
				for row in pivot.saturating_add(1)..width {
					work.charge(1)?;
					if rows.get(row).ok_or(Error::InvalidId)? & mask != 0 {
						source = Some(row);
						break;
					}
				}
				let gate = Cnot {
					control: source.ok_or(Error::NotUnitary)?,
					target: pivot,
				};
				row_add(rows, gate)?;
				gates.push(gate);
			}
			for row in pivot.saturating_add(1)..width {
				work.charge(1)?;
				if rows.get(row).ok_or(Error::InvalidId)? & mask != 0 {
					let gate = Cnot {
						control: pivot,
						target: row,
					};
					row_add(rows, gate)?;
					gates.push(gate);
				}
			}
		}
	}
	Ok(gates)
}
fn transpose(rows: &[u64], work: &mut Work) -> Result<Vec<u64>> {
	let mut result = vec![0; rows.len()];
	for (row, value) in rows.iter().enumerate() {
		for (column, output) in result.iter_mut().enumerate() {
			work.charge(1)?;
			if value & bit(column)? != 0 {
				*output |= bit(row)?;
			}
		}
	}
	Ok(result)
}
fn candidate(rows: &[u64], block: usize, work: &mut Work) -> Result<Vec<Cnot>> {
	let mut reduced = rows.to_vec();
	let lower = eliminate(&mut reduced, block, work)?;
	let mut transposed = transpose(&reduced, work)?;
	let upper = eliminate(&mut transposed, block, work)?;
	if transposed != identity(rows.len())? {
		return Err(Error::NotUnitary);
	}
	Ok(upper
		.into_iter()
		.map(|gate| Cnot {
			control: gate.target,
			target: gate.control,
		})
		.chain(lower.into_iter().rev())
		.collect())
}
pub fn synthesize_rows(
	rows: &[u64],
	options: LinearOptions,
	work: &mut Work,
) -> Result<(Vec<Cnot>, Vec<Cnot>)> {
	let gaussian = candidate(rows, 1, work)?;
	let pmh = candidate(rows, options.block_size, work)?;
	// Replay both candidates from identity; no elimination state grants acceptance.
	if matrix(rows.len(), &gaussian, work)? != rows || matrix(rows.len(), &pmh, work)? != rows {
		return Err(Error::NotUnitary);
	}
	Ok((gaussian, pmh))
}
/// Synthesize and independently replay both candidates; retain input on ties.
/// # Errors
/// Rejects invalid operands and exhausted width, window, work or allocation limits.
pub fn synthesize_cnot(
	width: usize,
	input: &[Cnot],
	options: LinearOptions,
) -> Result<LinearSynthesis> {
	preflight(width, input.len(), options)?;
	let mut work = Work {
		used: 0,
		maximum: options.max_work,
	};
	let rows = matrix(width, input, &mut work)?;
	let (gaussian, pmh) = synthesize_rows(&rows, options, &mut work)?;
	let mut gates = input.to_vec();
	if gaussian.len() < gates.len() {
		gates.clone_from(&gaussian);
	}
	if pmh.len() < gates.len() {
		gates.clone_from(&pmh);
	}
	Ok(LinearSynthesis {
		gates,
		gaussian,
		pmh,
	})
}
pub fn cnot(operation: &SemanticOperation) -> Option<Cnot> {
	match operation {
		SemanticOperation::Gate {
			gate: Gate::X,
			targets,
			controls,
		} => match (targets.as_ref(), controls.as_ref()) {
			([target], [control]) if control.state() == ControlState::One => Some(Cnot {
				control: control.qubit().index(),
				target: target.index(),
			}),
			_ => None,
		},
		_ => None,
	}
}
pub fn cnot_operation(gate: Cnot, owner: u64) -> SemanticOperation {
	SemanticOperation::Gate {
		gate: Gate::X,
		targets: vec![QubitId {
			owner,
			index: gate.target,
		}]
		.into(),
		controls: vec![Control::new(
			QubitId {
				owner,
				index: gate.control,
			},
			ControlState::One,
		)]
		.into(),
	}
}
pub fn replacement(
	window: &[Occurrence],
	operations: Vec<SemanticOperation>,
	options: LinearOptions,
	provenance: &mut ProvenanceGraph,
) -> Result<(Vec<Occurrence>, LinearRewrite)> {
	if operations.len() > window.len() {
		return Err(Error::Budget("replacement occurrence IDs"));
	}
	let inputs: Vec<_> = window.iter().map(|item| item.provenance).collect();
	let history = provenance.rewrite(&inputs, options.max_bytes)?;
	let mut output = vec![];
	for (original, operation) in window.iter().zip(operations) {
		output.push(Occurrence {
			id: original.id,
			provenance: history,
			source: original.source.clone(),
			operation,
		});
	}
	let rewrite = LinearRewrite {
		inputs,
		provenance: history,
		outputs: output.iter().map(|item| item.id).collect(),
	};
	Ok((output, rewrite))
}
/// Bounded expansion uses fresh execution occurrences and immediate history.
pub fn replacement_candidate(
	window: &[Occurrence],
	operations: Vec<SemanticOperation>,
	max_output_operations: usize,
	options: LinearOptions,
	provenance: &mut ProvenanceGraph,
) -> Result<(Vec<Occurrence>, LinearRewrite)> {
	if operations.len() > max_output_operations {
		return Err(Error::Budget("candidate output operations"));
	}
	let payload = operations.iter().try_fold(0usize, |sum, operation| {
		sum.checked_add(operation.retained_bytes()?)
			.ok_or(Error::Budget("candidate payload storage"))
	})?;
	let bytes = operations
		.len()
		.checked_mul(1024)
		.and_then(|n| {
			window
				.len()
				.checked_mul(size_of::<ProvenanceId>())
				.and_then(|m| n.checked_add(m))
		})
		.and_then(|n| n.checked_add(payload))
		.ok_or(Error::Budget("candidate replacement storage"))?;
	let graph_limit = options
		.max_bytes
		.checked_sub(bytes)
		.ok_or(Error::Budget("candidate replacement storage"))?;
	let first = window.first().ok_or(Error::InvalidId)?;
	let inputs: Vec<_> = window.iter().map(|item| item.provenance).collect();
	let history = provenance.rewrite(&inputs, graph_limit)?;
	let mut output = Vec::new();
	output
		.try_reserve_exact(operations.len())
		.map_err(|_| Error::Budget("candidate allocation"))?;
	let mut next = provenance.next_occurrence();
	for operation in operations {
		let id = OccurrenceId {
			owner: first.id.owner,
			index: next,
		};
		next = next
			.checked_add(1)
			.ok_or(Error::Budget("candidate occurrence identities"))?;
		output.push(Occurrence {
			id,
			provenance: history,
			source: first.source.clone(),
			operation,
		});
	}
	provenance.retain_next_occurrence(next);
	let rewrite = LinearRewrite {
		inputs,
		provenance: history,
		outputs: output.iter().map(|item| item.id).collect(),
	};
	Ok((output, rewrite))
}
// Reserve output/report storage before cloning occurrences; history is copied only once.
pub fn program_preflight(program: &QuantumRegion, options: LinearOptions) -> Result<LinearOptions> {
	if program
		.occurrences()
		.len()
		.checked_add(program.provenance_arc().copy_work()?)
		.is_none_or(|work| work > options.max_work)
	{
		return Err(Error::Budget("linear scan work"));
	}
	// One root per operation and one immediate-input edge per window member.
	// Reports and rewritten occurrences share history instead of copying leaves.
	let bytes = program
		.occurrences()
		.len()
		.checked_mul(1024)
		.and_then(|bytes| {
			program
				.provenance_arc()
				.retained_bytes()
				.ok()
				.and_then(|history| bytes.checked_add(history))
		})
		.ok_or(Error::Budget("program output bytes"))?;
	let max_bytes = options
		.max_bytes
		.checked_sub(bytes)
		.ok_or(Error::Budget("program output bytes"))?;
	Ok(LinearOptions {
		max_bytes: max_bytes.min(program.limits().max_provenance_bytes),
		..options
	})
}
pub fn program_work(program: &QuantumRegion, options: LinearOptions) -> Result<Work> {
	let mut work = Work {
		used: 0,
		maximum: options.max_work,
	};
	work.charge(program.occurrences().len())?;
	work.charge(program.provenance_arc().copy_work()?)?;
	Ok(work)
}
/// Compiler extension over shared semantic capabilities.
pub trait LinearPasses: Sized {
	/// # Errors
	/// Rejects invalid semantic candidates, incompatible interfaces, and configured resource limits.
	fn optimize_linear(self, options: LinearOptions) -> Result<(Self, LinearReport)>;
	/// # Errors
	/// Rejects invalid semantic candidates, incompatible interfaces, and configured resource limits.
	fn resynthesize_linear_candidate(
		self,
		options: LinearOptions,
		strategy: LinearCandidateStrategy,
		max_output_operations: usize,
	) -> Result<(Self, LinearReport)>;
	/// # Errors
	/// Rejects invalid semantic candidates, incompatible interfaces, and configured resource limits.
	fn linear_pass(
		self,
		options: LinearOptions,
		strategy: Option<LinearCandidateStrategy>,
		max_output: usize,
	) -> Result<(Self, LinearReport)>;
}
impl LinearPasses for QuantumRegion {
	/// Resynthesize contiguous positive CNOT windows; explicit user edges disable rewriting.
	/// Every candidate is replayed as a complete binary map before a shorter replacement.
	/// # Errors
	/// Rejects exhausted pass budgets or an invalid rebuilt dependency graph.
	fn optimize_linear(self, options: LinearOptions) -> Result<(Self, LinearReport)> {
		let max_output = self.limits().max_operations;
		self.linear_pass(options, None, max_output)
	}
	/// Generate a replay-verified algebraic candidate, even when it is longer.
	/// Each positive-CNOT window is independent; mandatory edges disable rewriting.
	/// # Errors
	/// Rejects exhausted output, work or retained-storage limits transactionally.
	fn resynthesize_linear_candidate(
		self,
		options: LinearOptions,
		strategy: LinearCandidateStrategy,
		max_output_operations: usize,
	) -> Result<(Self, LinearReport)> {
		if max_output_operations > self.limits().max_operations
			|| max_output_operations < self.occurrences().len()
		{
			return Err(Error::Budget("linear candidate output"));
		}
		let output_bytes = max_output_operations
			.checked_mul(1024)
			.ok_or(Error::Budget("linear candidate storage"))?;
		let max_bytes = options
			.max_bytes
			.checked_sub(output_bytes)
			.ok_or(Error::Budget("linear candidate storage"))?;
		self.linear_pass(
			LinearOptions {
				max_bytes,
				..options
			},
			Some(strategy),
			max_output_operations,
		)
	}
	#[expect(
		clippy::too_many_lines,
		reason = "Keep bounded synthesis, independent replay and transactional candidate publication together"
	)]
	fn linear_pass(
		self,
		options: LinearOptions,
		strategy: Option<LinearCandidateStrategy>,
		max_output: usize,
	) -> Result<(Self, LinearReport)> {
		preflight(self.num_qubits(), 0, options)?;
		if options.max_window_operations == 0 {
			return Err(Error::Budget("linear window"));
		}
		let mut report = LinearReport {
			before_operations: self.occurrences().len(),
			after_operations: self.occurrences().len(),
			provenance: Arc::clone(self.provenance_arc()),
			..LinearReport::default()
		};
		if !self.explicit_edges().is_empty() {
			return Ok((self, report));
		}
		let options = program_preflight(&self, options)?;
		let mut provenance = ProvenanceGraph::edit(
			Arc::clone(self.provenance_arc()),
			options.max_bytes.min(self.limits().max_provenance_bytes),
		)?;
		let mut work = program_work(&self, options)?;
		let mut output = vec![];
		let mut offset = 0;
		while let Some(current) = self.occurrences().get(offset) {
			if cnot(&current.operation).is_none() {
				if output.len() >= max_output {
					return Err(Error::Budget("linear candidate output"));
				}
				output.push(current.clone());
				offset = offset
					.checked_add(1)
					.ok_or(Error::Budget("linear offset"))?;
				continue;
			}
			let end = self
				.occurrences()
				.iter()
				.skip(offset)
				.take(options.max_window_operations)
				.take_while(|item| cnot(&item.operation).is_some())
				.count()
				.checked_add(offset)
				.ok_or(Error::Budget("linear offset"))?;
			let window = self
				.occurrences()
				.get(offset..end)
				.ok_or(Error::InvalidId)?;
			preflight(self.num_qubits(), window.len(), options)?;
			let input: Vec<_> = window
				.iter()
				.filter_map(|item| cnot(&item.operation))
				.collect();
			let rows = matrix(self.num_qubits(), &input, &mut work)?;
			let (gaussian, pmh) = synthesize_rows(&rows, options, &mut work)?;
			report.considered_windows = report
				.considered_windows
				.checked_add(1)
				.ok_or(Error::Budget("linear report"))?;
			report.gaussian_operations = report
				.gaussian_operations
				.checked_add(gaussian.len())
				.ok_or(Error::Budget("linear report"))?;
			report.pmh_operations = report
				.pmh_operations
				.checked_add(pmh.len())
				.ok_or(Error::Budget("linear report"))?;
			let best = match strategy {
				Some(LinearCandidateStrategy::Pmh) => pmh,
				None if pmh.len() < gaussian.len() => pmh,
				Some(LinearCandidateStrategy::Gaussian) | None => gaussian,
			};
			if strategy.is_some() || best.len() < input.len() {
				work.charge(window.len())?;
				let operations = best
					.into_iter()
					.map(|gate| cnot_operation(gate, self.owner()))
					.collect();
				let (replacement, rewrite) = if strategy.is_some() {
					replacement_candidate(
						window,
						operations,
						max_output
							.checked_sub(output.len())
							.ok_or(Error::Budget("linear candidate output"))?,
						options,
						&mut provenance,
					)?
				} else {
					replacement(window, operations, options, &mut provenance)?
				};
				output.extend(replacement);
				report.rewrites.push(rewrite);
				report.accepted_windows = report
					.accepted_windows
					.checked_add(1)
					.ok_or(Error::Budget("linear report"))?;
			} else {
				output.extend_from_slice(window);
			}
			offset = end;
			if output.len() > max_output {
				return Err(Error::Budget("linear candidate output"));
			}
		}
		report.work = work.used;
		report.after_operations = output.len();
		report.provenance = Arc::new(provenance);
		let result = Self::from_parts(
			self.owner(),
			self.num_qubits(),
			self.num_bits(),
			self.parameter_storage().clone(),
			output,
			self.explicit_edges().clone(),
			self.limits(),
			Arc::clone(&report.provenance),
		)?;
		Ok((result, report))
	}
}
