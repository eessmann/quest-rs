//! Bounded `QuiZX` 0.3.0 candidate adapter with independent full-phase certification.
use quest_math::{Gate, Limits, Operation, Sequence};
use quizx::fscalar::Zero;
use quizx::{
	basic_rules,
	circuit::Circuit,
	extract::ToCircuit,
	gate::{GType, Gate as ZxGate},
	graph::{EType, GraphLike, VType},
	vec_graph::Graph,
};
use std::collections::BTreeSet;

const MAX_INPUT_GATES: usize = 128;
const MAX_OUTPUT_GATES: usize = 1_000;
const MAX_GRAPH_VERTICES: usize = 4_096;
const MAX_GRAPH_EDGES: usize = 16_384;
const MAX_RULE_EXAMINATIONS: usize = 100_000;
const MAX_RULE_REWRITES: usize = 4_096;
const MAX_RULE_SCRATCH_BYTES: usize = 64 * 1024 * 1024;

#[derive(Clone, Copy)]
#[expect(
	clippy::struct_field_names,
	reason = "Five independent hard ceilings share max terminology"
)]
struct ZxRuleLimits {
	max_vertices: usize,
	max_edges: usize,
	max_examinations: usize,
	max_rewrites: usize,
	max_scratch_bytes: usize,
}
impl Default for ZxRuleLimits {
	fn default() -> Self {
		Self {
			max_vertices: MAX_GRAPH_VERTICES,
			max_edges: MAX_GRAPH_EDGES,
			max_examinations: MAX_RULE_EXAMINATIONS,
			max_rewrites: MAX_RULE_REWRITES,
			max_scratch_bytes: MAX_RULE_SCRATCH_BYTES,
		}
	}
}
#[derive(Debug, Default, Clone, Copy)]
struct RuleUsage {
	examinations: usize,
	rewrites: usize,
	local_complements: usize,
	pivots: usize,
	gadget_fusions: usize,
}
impl RuleUsage {
	fn examine(&mut self, amount: usize, limits: ZxRuleLimits) -> Result<(), String> {
		self.examinations = self
			.examinations
			.checked_add(amount)
			.filter(|used| *used <= limits.max_examinations)
			.ok_or_else(|| "ZX rule examination budget exceeded".to_string())?;
		Ok(())
	}
	fn rewrite(&mut self, limits: ZxRuleLimits) -> Result<(), String> {
		self.rewrites = self
			.rewrites
			.checked_add(1)
			.filter(|used| *used <= limits.max_rewrites)
			.ok_or_else(|| "ZX rule rewrite budget exceeded".to_string())?;
		Ok(())
	}
	fn sort(&mut self, count: usize, limits: ZxRuleLimits) -> Result<(), String> {
		let depth = count.saturating_sub(1).checked_ilog2().map_or(1, |log| {
			usize::try_from(log).unwrap_or(usize::MAX).saturating_add(1)
		});
		let comparisons = count
			.checked_mul(depth)
			.and_then(|n| n.checked_mul(8))
			.ok_or_else(|| "ZX sort examination budget exceeded".to_string())?;
		self.examine(comparisons, limits)
	}
}
fn graph_bounds(graph: &Graph, limits: ZxRuleLimits) -> Result<(), String> {
	if graph.num_vertices() > limits.max_vertices {
		return Err("ZX graph vertex budget exceeded".into());
	}
	if graph.num_edges() > limits.max_edges {
		return Err("ZX graph edge budget exceeded".into());
	}
	let scratch = graph
		.num_vertices()
		.checked_mul(std::mem::size_of::<[usize; 2]>())
		.and_then(|bytes| {
			graph
				.num_edges()
				.checked_mul(std::mem::size_of::<[(usize, usize, EType); 2]>())
				.and_then(|edges| bytes.checked_add(edges))
		})
		.ok_or_else(|| "ZX rule scratch budget exceeded".to_string())?;
	if scratch > limits.max_scratch_bytes {
		return Err("ZX rule scratch budget exceeded".into());
	}
	Ok(())
}
fn preflight_growth(
	graph: &Graph,
	extra_edges: usize,
	scratch_vertices: usize,
	limits: ZxRuleLimits,
) -> Result<(), String> {
	if graph
		.num_edges()
		.checked_add(extra_edges)
		.is_none_or(|edges| edges > limits.max_edges)
	{
		return Err("ZX graph edge growth budget exceeded".into());
	}
	let scratch = scratch_vertices
		.checked_mul(std::mem::size_of::<[usize; 8]>())
		.ok_or_else(|| "ZX rule scratch budget exceeded".to_string())?;
	if scratch > limits.max_scratch_bytes {
		return Err("ZX rule scratch budget exceeded".into());
	}
	Ok(())
}
fn gadget_center(graph: &Graph, vertex: usize) -> bool {
	graph.vertex_type(vertex) == VType::Z
		&& graph.phase(vertex).is_zero()
		&& graph.degree(vertex) >= 2
		&& graph.incident_edges(vertex).any(|(neighbor, edge)| {
			edge == EType::H
				&& graph.vertex_type(neighbor) == VType::Z
				&& graph.degree(neighbor) == 1
		})
}
#[expect(
	clippy::too_many_lines,
	reason = "Keep deterministic rule priority and one shared examination ledger together"
)]
fn apply_bounded_rules(graph: &mut Graph, limits: ZxRuleLimits) -> Result<RuleUsage, String> {
	graph_bounds(graph, limits)?;
	let mut usage = RuleUsage::default();
	loop {
		usage.sort(graph.num_vertices(), limits)?;
		let mut vertices: Vec<_> = graph.vertices().collect();
		vertices.sort_unstable();
		let mut changed = false;
		// Gadget fusion precedes graph-like rules; its public predicate alone
		// does not require the two selected centers to be Z spiders.
		for (index, &first) in vertices.iter().enumerate() {
			usage.examine(graph.degree(first).saturating_add(1), limits)?;
			if !gadget_center(graph, first) {
				continue;
			}
			for &second in vertices.iter().skip(index.saturating_add(1)) {
				usage.examine(graph.degree(second).saturating_add(1), limits)?;
				if !gadget_center(graph, second) {
					continue;
				}
				// QuiZX 0.3.0's unchecked fusion assumes distinct non-leaf
				// centers. Adjacent centers can make its final degree(v0)
				// query refer to a vertex removed as v1's gadget leaf.
				if graph.edge_type_opt(first, second).is_some() {
					continue;
				}
				let degree = graph
					.degree(first)
					.checked_add(graph.degree(second))
					.ok_or_else(|| "ZX rule scratch budget exceeded".to_string())?;
				preflight_growth(graph, 0, degree, limits)?;
				usage.examine(degree, limits)?;
				if basic_rules::check_gadget_fusion(graph, first, second) {
					usage.rewrite(limits)?;
					basic_rules::gadget_fusion_unchecked(graph, first, second);
					usage.gadget_fusions = usage
						.gadget_fusions
						.checked_add(1)
						.ok_or_else(|| "ZX gadget count overflow".to_string())?;
					changed = true;
					break;
				}
			}
			if changed {
				break;
			}
		}
		if changed {
			graph_bounds(graph, limits)?;
			continue;
		}
		for &vertex in &vertices {
			let degree = graph.degree(vertex);
			usage.examine(degree.saturating_add(1), limits)?;
			if !basic_rules::check_local_comp(graph, vertex) {
				continue;
			}
			let clique_edges = degree
				.checked_mul(degree.saturating_sub(1))
				.and_then(|pairs| pairs.checked_div(2))
				.ok_or_else(|| "ZX graph edge growth budget exceeded".to_string())?;
			preflight_growth(graph, clique_edges, degree, limits)?;
			usage.examine(clique_edges, limits)?;
			usage.rewrite(limits)?;
			basic_rules::local_comp_unchecked(graph, vertex);
			usage.local_complements = usage
				.local_complements
				.checked_add(1)
				.ok_or_else(|| "ZX local complement count overflow".to_string())?;
			changed = true;
			break;
		}
		if changed {
			graph_bounds(graph, limits)?;
			continue;
		}
		usage.sort(graph.num_edges(), limits)?;
		let mut edges: Vec<_> = graph.edges().collect();
		edges.sort_unstable();
		for (first, second, _) in edges {
			let degrees = graph
				.degree(first)
				.checked_add(graph.degree(second))
				.ok_or_else(|| "ZX rule scratch budget exceeded".to_string())?;
			usage.examine(degrees.saturating_add(1), limits)?;
			if !basic_rules::check_pivot(graph, first, second) {
				continue;
			}
			let growth = graph
				.degree(first)
				.checked_mul(graph.degree(second))
				.ok_or_else(|| "ZX graph edge growth budget exceeded".to_string())?;
			preflight_growth(graph, growth, degrees, limits)?;
			usage.examine(growth, limits)?;
			usage.rewrite(limits)?;
			basic_rules::pivot_unchecked(graph, first, second);
			usage.pivots = usage
				.pivots
				.checked_add(1)
				.ok_or_else(|| "ZX pivot count overflow".to_string())?;
			changed = true;
			break;
		}
		if !changed {
			break;
		}
		graph_bounds(graph, limits)?;
	}
	Ok(usage)
}

/// Generate a bounded extracted Clifford+T candidate and recover its exact scalar phase.
///
/// Flow simplification is deterministic; the protocol seed is accepted but no stochastic pass runs.
/// # Errors
/// Rejects malformed interfaces, unsupported controls, engine/extraction failures and failed exact certificates.
pub fn optimize(sequence: &Sequence, _seed: u64) -> Result<Sequence, String> {
	admit(sequence)?;
	let (baseline, expanded) =
		std::panic::catch_unwind(|| generate_variants(sequence, ZxRuleLimits::default(), true))
			.map_err(|_| "QuiZX candidate engine panicked".to_string())??;
	let certify = |generated: &Sequence| {
		quest_math::recover_eighth_root_phase(
			generated,
			sequence,
			Limits {
				gates: MAX_OUTPUT_GATES,
				..Limits::default()
			},
		)
		.map(|proof| proof.sequence().clone())
	};
	let baseline = certify(&baseline);
	let expanded = expanded.as_ref().map(certify);
	match (baseline, expanded) {
		(Ok(baseline), Some(Ok(expanded)))
			if expanded.operations.len() < baseline.operations.len() =>
		{
			Ok(expanded)
		}
		(Ok(baseline), _) => Ok(baseline),
		(Err(_), Some(Ok(expanded))) => Ok(expanded),
		(Err(error), _) => Err(format!(
			"extracted candidate failed full-phase certification: {error}"
		)),
	}
}
/// Extract the existing flow-simplification baseline without running the
/// additional bounded graph rules.
pub fn optimize_baseline(sequence: &Sequence, _seed: u64) -> Result<Sequence, String> {
	admit(sequence)?;
	let (baseline, _) =
		std::panic::catch_unwind(|| generate_variants(sequence, ZxRuleLimits::default(), false))
			.map_err(|_| "QuiZX candidate engine panicked".to_string())??;
	quest_math::recover_eighth_root_phase(
		&baseline,
		sequence,
		Limits {
			gates: MAX_OUTPUT_GATES,
			..Limits::default()
		},
	)
	.map(|proof| proof.sequence().clone())
	.map_err(|error| format!("baseline candidate failed full-phase certification: {error}"))
}
/// Return the candidate extracted after the additional bounded ZX rules, even
/// when it uses more operations than baseline flow simplification. The parent
/// still independently certifies the complete phase.
pub fn optimize_expanded(sequence: &Sequence, _seed: u64) -> Result<Sequence, String> {
	admit(sequence)?;
	let (_, expanded) =
		std::panic::catch_unwind(|| generate_variants(sequence, ZxRuleLimits::default(), true))
			.map_err(|_| "QuiZX candidate engine panicked".to_string())??;
	let expanded =
		expanded.ok_or_else(|| "expanded ZX extraction failed or exceeded bounds".to_string())?;
	quest_math::recover_eighth_root_phase(
		&expanded,
		sequence,
		Limits {
			gates: MAX_OUTPUT_GATES,
			..Limits::default()
		},
	)
	.map(|proof| proof.sequence().clone())
	.map_err(|error| format!("expanded candidate failed full-phase certification: {error}"))
}
fn admit(sequence: &Sequence) -> Result<(), String> {
	if sequence.qubits > 4 {
		return Err("ZX capability is limited to four qubits".into());
	}
	if sequence.operations.len() > MAX_INPUT_GATES {
		return Err("ZX input exceeds 128 gates".into());
	}
	for operation in &sequence.operations {
		if operation.targets.len() != operation.gate.target_count() {
			return Err("gate target arity mismatch".into());
		}
		let mut seen = BTreeSet::new();
		for qubit in operation
			.targets
			.iter()
			.copied()
			.chain(operation.controls.iter().map(|control| control.qubit))
		{
			if qubit >= sequence.qubits || !seen.insert(qubit) {
				return Err("gate interface has an invalid or repeated qubit".into());
			}
		}
	}
	Ok(())
}
fn generate_variants(
	sequence: &Sequence,
	limits: ZxRuleLimits,
	expand: bool,
) -> Result<(Sequence, Option<Sequence>), String> {
	let mut circuit = Circuit::new(sequence.qubits);
	let mut scalar = 0usize;
	for operation in &sequence.operations {
		if operation.gate == Gate::W && operation.controls.is_empty() {
			scalar = scalar.saturating_add(1) % 8;
			continue;
		}
		for control in &operation.controls {
			if !control.positive {
				circuit.push(ZxGate::new(GType::NOT, vec![control.qubit]));
			}
		}
		translate(&mut circuit, operation)?;
		for control in operation.controls.iter().rev() {
			if !control.positive {
				circuit.push(ZxGate::new(GType::NOT, vec![control.qubit]));
			}
		}
	}
	let mut graph: Graph = circuit.to_graph();
	graph_bounds(&graph, limits)?;
	quizx::simplify::flow_simp(&mut graph);
	graph_bounds(&graph, limits)?;
	let graph_bytes = graph
		.num_vertices()
		.checked_mul(256)
		.and_then(|n| {
			graph
				.num_edges()
				.checked_mul(512)
				.and_then(|m| n.checked_add(m))
		})
		.ok_or_else(|| "ZX baseline clone budget exceeded".to_string())?;
	let overlap = graph_bytes
		.checked_mul(2)
		.and_then(|n| {
			MAX_OUTPUT_GATES
				.checked_mul(2048)
				.and_then(|m| n.checked_add(m))
		})
		.ok_or_else(|| "ZX baseline clone budget exceeded".to_string())?;
	if overlap > limits.max_scratch_bytes {
		return Err("ZX baseline clone budget exceeded".into());
	}
	let mut baseline_graph = graph.clone();
	let baseline = extract(&mut baseline_graph, sequence, scalar, limits)?;
	drop(baseline_graph);
	if !expand {
		return Ok((baseline, None));
	}
	let expanded = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
		apply_bounded_rules(&mut graph, limits)?;
		quizx::simplify::flow_simp(&mut graph);
		graph_bounds(&graph, limits)?;
		extract(&mut graph, sequence, scalar, limits)
	}))
	.ok()
	.and_then(Result::ok);
	Ok((baseline, expanded))
}
fn extract(
	graph: &mut Graph,
	sequence: &Sequence,
	scalar: usize,
	limits: ZxRuleLimits,
) -> Result<Sequence, String> {
	graph_bounds(graph, limits)?;
	if graph.inputs().len() != sequence.qubits || graph.outputs().len() != sequence.qubits {
		return Err("ZX simplification changed the quantum interface".into());
	}
	// Default extraction includes the final wire permutation; up_to_perm is intentionally not used.
	let extracted = graph
		.to_circuit_mut()
		.map_err(|error| format!("QuiZX extraction failed: {error}"))?;
	if extracted.gates.len() > MAX_OUTPUT_GATES {
		return Err("ZX extraction output gate budget exceeded".into());
	}
	if extracted.num_qubits() != sequence.qubits {
		return Err("ZX extraction changed the qubit count".into());
	}
	let mut output = Sequence {
		qubits: sequence.qubits,
		operations: Vec::new(),
	};
	for gate in &extracted.gates {
		from_gate(&mut output, gate)?;
	}
	for _ in 0..scalar {
		push(&mut output, Gate::W, &[])?;
	}
	admit_candidate(&output)?;
	Ok(output)
}
fn translate(circuit: &mut Circuit, operation: &Operation) -> Result<(), String> {
	let controls = operation
		.controls
		.iter()
		.map(|control| control.qubit)
		.collect::<Vec<_>>();
	let targets = &operation.targets;
	let kind = match (operation.gate, controls.len()) {
		(Gate::H, 0) => GType::HAD,
		(Gate::X, 0) => GType::NOT,
		(Gate::Z, 0) => GType::Z,
		(Gate::S, 0) => GType::S,
		(Gate::Sdg, 0) => GType::Sdg,
		(Gate::T, 0) | (Gate::W, 1) => GType::T,
		(Gate::Tdg, 0) => GType::Tdg,
		(Gate::Cx, 0) | (Gate::X, 1) => GType::CNOT,
		(Gate::Cz, 0) | (Gate::Z, 1) => GType::CZ,
		(Gate::Swap, 0) => {
			let [first, second] = targets.as_slice() else {
				return Err("swap arity mismatch".into());
			};
			// QuiZX 0.3.0 tracks SWAP only in an internal wire map and omits a
			// trailing permutation from graph outputs. Explicit CNOTs preserve it.
			for qubits in [
				vec![*first, *second],
				vec![*second, *first],
				vec![*first, *second],
			] {
				circuit.push(ZxGate::new(GType::CNOT, qubits));
			}
			return Ok(());
		}
		(Gate::X, 2) | (Gate::Cx, 1) => GType::TOFF,
		(Gate::Z, 2) | (Gate::Cz, 1) => GType::CCZ,
		(Gate::Y, 0) => {
			for kind in [GType::Sdg, GType::NOT, GType::S] {
				circuit.push(ZxGate::new(kind, targets.clone()));
			}
			return Ok(());
		}
		_ => return Err("ZX capability does not exactly support this controlled gate".into()),
	};
	let qubits = controls
		.into_iter()
		.chain(targets.iter().copied())
		.collect();
	circuit.push(ZxGate::new(kind, qubits));
	Ok(())
}
fn from_gate(output: &mut Sequence, gate: &ZxGate) -> Result<(), String> {
	if gate.vars != quizx::params::Parity::default() {
		return Err("ZX extraction produced classical parameters".into());
	}
	let kind = match gate.t {
		GType::HAD => Gate::H,
		GType::NOT => Gate::X,
		GType::Z => Gate::Z,
		GType::S => Gate::S,
		GType::Sdg => Gate::Sdg,
		GType::T => Gate::T,
		GType::Tdg => Gate::Tdg,
		GType::CNOT => Gate::Cx,
		GType::CZ => Gate::Cz,
		GType::SWAP => Gate::Swap,
		GType::ZPhase | GType::XPhase => {
			if gate.qs.len() != 1 {
				return Err("ZX phase gate has invalid arity".into());
			}
			let phase = gate.phase.to_rational();
			let numerator = phase
				.numer()
				.checked_mul(4)
				.ok_or_else(|| "ZX phase numerator overflow".to_string())?;
			let denominator = *phase.denom();
			if denominator <= 0 || numerator.checked_rem(denominator) != Some(0) {
				return Err("ZX extracted phase is outside Clifford+T".into());
			}
			let power = numerator
				.checked_div(denominator)
				.ok_or_else(|| "ZX phase division failed".to_string())?
				.rem_euclid(8);
			if gate.t == GType::XPhase {
				push(output, Gate::H, &gate.qs)?;
			}
			let gates: &[Gate] = match power {
				0 => &[],
				1 => &[Gate::T],
				2 => &[Gate::S],
				3 => &[Gate::S, Gate::T],
				4 => &[Gate::Z],
				5 => &[Gate::Z, Gate::T],
				6 => &[Gate::Sdg],
				7 => &[Gate::Tdg],
				_ => return Err("invalid normalized ZX phase".into()),
			};
			for gate_type in gates {
				push(output, *gate_type, &gate.qs)?;
			}
			if gate.t == GType::XPhase {
				push(output, Gate::H, &gate.qs)?;
			}
			return Ok(());
		}
		_ => return Err("ZX extraction produced an unsupported gate or ancilla operation".into()),
	};
	push(output, kind, &gate.qs)
}
fn push(output: &mut Sequence, gate: Gate, targets: &[usize]) -> Result<(), String> {
	if output.operations.len() >= MAX_OUTPUT_GATES {
		return Err("ZX output gate budget exceeded".into());
	}
	output.operations.push(Operation {
		gate,
		targets: targets.to_vec(),
		controls: Vec::new(),
	});
	Ok(())
}
fn admit_candidate(sequence: &Sequence) -> Result<(), String> {
	if sequence.qubits > 4 {
		return Err("ZX candidate added qubits".into());
	}
	if sequence.operations.len() > MAX_OUTPUT_GATES {
		return Err("ZX candidate output budget exceeded".into());
	}
	for gate in &sequence.operations {
		if gate.targets.len() != gate.gate.target_count()
			|| gate.targets.iter().any(|qubit| *qubit >= sequence.qubits)
		{
			return Err("ZX candidate gate has invalid interface".into());
		}
	}
	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;
	use googletest::prelude::*;
	use quest_math::{Control, Gate, Limits, Operation, Sequence};
	use quizx::{
		graph::{EType, VType},
		phase::Phase,
	};

	fn local_complement_graph() -> Graph {
		let mut graph = Graph::new();
		let center = graph.add_vertex_with_phase(VType::Z, Phase::from_f64(0.5));
		let neighbors = (0..3)
			.map(|_| graph.add_vertex(VType::Z))
			.collect::<Vec<_>>();
		for neighbor in neighbors {
			graph.add_edge_with_type(center, neighbor, EType::H);
		}
		graph
	}

	fn pivot_graph() -> Graph {
		let mut graph = Graph::new();
		let first = graph.add_vertex(VType::Z);
		let second = graph.add_vertex(VType::Z);
		let left = graph.add_vertex(VType::Z);
		let right = graph.add_vertex(VType::Z);
		for (a, b) in [(first, second), (first, left), (second, right)] {
			graph.add_edge_with_type(a, b, EType::H);
		}
		graph
	}

	fn gadget_graph() -> Graph {
		let mut graph = Graph::new();
		let shared = graph.add_vertex(VType::Z);
		for phase in [0.25, 0.75] {
			let center = graph.add_vertex(VType::Z);
			let leaf = graph.add_vertex_with_phase(VType::Z, Phase::from_f64(phase));
			graph.add_edge_with_type(center, shared, EType::H);
			graph.add_edge_with_type(center, leaf, EType::H);
		}
		graph
	}

	#[gtest]
	fn bounded_local_complement_preflights_clique_growth_before_mutation() -> Result<()> {
		let mut graph = local_complement_graph();
		let original = graph.clone();
		let limits = ZxRuleLimits {
			max_edges: 5,
			..ZxRuleLimits::default()
		};
		verify_that!(apply_bounded_rules(&mut graph, limits), err(anything()))?;
		verify_eq!(graph, original)?;
		let usage = apply_bounded_rules(&mut graph, ZxRuleLimits::default())
			.map_err(std::io::Error::other)?;
		verify_that!(usage.local_complements, ge(1))?;
		Ok(())
	}

	#[gtest]
	fn rule_limits_reject_vertices_edges_examinations_and_rewrites() -> Result<()> {
		let original = local_complement_graph();
		for limits in [
			ZxRuleLimits {
				max_vertices: 3,
				..ZxRuleLimits::default()
			},
			ZxRuleLimits {
				max_edges: 2,
				..ZxRuleLimits::default()
			},
			ZxRuleLimits {
				max_examinations: 0,
				..ZxRuleLimits::default()
			},
			ZxRuleLimits {
				max_rewrites: 0,
				..ZxRuleLimits::default()
			},
			ZxRuleLimits {
				max_scratch_bytes: 0,
				..ZxRuleLimits::default()
			},
		] {
			let mut graph = original.clone();
			verify_that!(apply_bounded_rules(&mut graph, limits), err(anything()))?;
			verify_eq!(graph, original)?;
		}
		Ok(())
	}
	#[gtest]
	fn pivot_and_gadget_rules_are_exercised_under_bounds() -> Result<()> {
		let mut pivot = pivot_graph();
		let pivot_usage = apply_bounded_rules(&mut pivot, ZxRuleLimits::default())
			.map_err(std::io::Error::other)?;
		verify_that!(pivot_usage.pivots, ge(1))?;
		let mut gadget = gadget_graph();
		let gadget_usage = apply_bounded_rules(&mut gadget, ZxRuleLimits::default())
			.map_err(std::io::Error::other)?;
		verify_that!(gadget_usage.gadget_fusions, ge(1))?;
		Ok(())
	}
	#[gtest]
	fn pivot_cross_neighborhood_growth_is_rejected_before_mutation() -> Result<()> {
		let mut graph = pivot_graph();
		let original = graph.clone();
		let limits = ZxRuleLimits {
			max_edges: original.num_edges(),
			..ZxRuleLimits::default()
		};
		verify_that!(apply_bounded_rules(&mut graph, limits), err(anything()))?;
		verify_eq!(graph, original)?;
		Ok(())
	}
	#[gtest]
	fn examination_budget_charges_sorting_and_clique_before_rewrite() -> Result<()> {
		let mut graph = local_complement_graph();
		let original = graph.clone();
		let limits = ZxRuleLimits {
			max_examinations: 80,
			..ZxRuleLimits::default()
		};
		verify_that!(apply_bounded_rules(&mut graph, limits), err(anything()))?;
		verify_eq!(graph, original)?;
		let mut pivot = pivot_graph();
		let original_pivot = pivot.clone();
		let limits = ZxRuleLimits {
			max_examinations: 140,
			..ZxRuleLimits::default()
		};
		verify_that!(apply_bounded_rules(&mut pivot, limits), err(anything()))?;
		verify_eq!(pivot, original_pivot)?;
		Ok(())
	}
	#[gtest]
	fn flow_baseline_remains_certifiable_when_optional_rules_exhaust() -> Result<()> {
		let original = Sequence {
			qubits: 1,
			operations: vec![op(Gate::H, &[0]), op(Gate::T, &[0])],
		};
		let limits = ZxRuleLimits {
			max_examinations: 0,
			..ZxRuleLimits::default()
		};
		let (baseline, expanded) =
			generate_variants(&original, limits, true).map_err(std::io::Error::other)?;
		verify_that!(expanded, none())?;
		let recovered =
			quest_math::recover_eighth_root_phase(&baseline, &original, Limits::default())?;
		quest_math::verify_exact(recovered.sequence(), &original, Limits::default())?;
		Ok(())
	}
	#[gtest]
	fn output_cap_accepts_exactly_the_limit_and_rejects_the_next_gate() -> Result<()> {
		let mut output = Sequence {
			qubits: 1,
			operations: Vec::new(),
		};
		for _ in 0..MAX_OUTPUT_GATES {
			push(&mut output, Gate::X, &[0]).map_err(std::io::Error::other)?;
		}
		verify_eq!(output.operations.len(), MAX_OUTPUT_GATES)?;
		verify_that!(push(&mut output, Gate::X, &[0]), err(anything()))?;
		Ok(())
	}
	fn op(gate: Gate, targets: &[usize]) -> Operation {
		Operation {
			gate,
			targets: targets.to_vec(),
			controls: Vec::new(),
		}
	}
	#[gtest]
	fn real_engine_cancels_gates_and_preserves_idle_interface() -> Result<()> {
		let original = Sequence {
			qubits: 3,
			operations: vec![
				op(Gate::H, &[0]),
				op(Gate::H, &[0]),
				op(Gate::T, &[2]),
				op(Gate::Tdg, &[2]),
			],
		};
		let candidate = optimize(&original, 7).map_err(std::io::Error::other)?;
		verify_eq!(candidate.qubits, 3)?;
		verify_that!(candidate.operations.len(), lt(original.operations.len()))?;
		quest_math::verify_exact(&candidate, &original, Limits::default())?;
		Ok(())
	}
	#[gtest]
	fn extracted_scalar_phase_is_recovered_with_candidate_target_order() -> Result<()> {
		let original = Sequence {
			qubits: 1,
			operations: vec![
				op(Gate::X, &[0]),
				op(Gate::Z, &[0]),
				op(Gate::X, &[0]),
				op(Gate::Z, &[0]),
				op(Gate::W, &[]),
			],
		};
		let mut missing_phase = generate_variants(&original, ZxRuleLimits::default(), true)
			.map_err(std::io::Error::other)?
			.0;
		// A negative certificate fixture derived from a real extraction: deliberately omit W.
		missing_phase
			.operations
			.retain(|operation| operation.gate != Gate::W);
		verify_that!(
			quest_math::verify_exact(&missing_phase, &original, Limits::default()),
			err(anything())
		)?;
		let phase_recovery =
			quest_math::recover_eighth_root_phase(&missing_phase, &original, Limits::default())?;
		// Graph rules can move a minus sign into the graph scalar before
		// extraction; only the independently recovered full phase is stable.
		quest_math::verify_exact(phase_recovery.sequence(), &original, Limits::default())?;
		let candidate = optimize(&original, 0).map_err(std::io::Error::other)?;
		let recovery =
			quest_math::recover_eighth_root_phase(&candidate, &original, Limits::default())?;
		verify_eq!(recovery.certificate().target(), &original)?;
		verify_eq!(recovery.certificate().candidate(), recovery.sequence())?;
		quest_math::verify_exact(&candidate, &original, Limits::default())?;
		verify_that!(
			quest_math::verify_exact(
				&candidate,
				&Sequence {
					qubits: 1,
					operations: Vec::new()
				},
				Limits::default()
			),
			err(anything())
		)?;
		Ok(())
	}
	#[gtest]
	fn exactly_supported_signed_controls_survive_extraction() -> Result<()> {
		for positive in [true, false] {
			let original = Sequence {
				qubits: 2,
				operations: vec![Operation {
					gate: Gate::X,
					targets: vec![1],
					controls: vec![Control { qubit: 0, positive }],
				}],
			};
			let candidate = optimize(&original, 4).map_err(std::io::Error::other)?;
			quest_math::verify_exact(&candidate, &original, Limits::default())?;
		}
		Ok(())
	}
	#[gtest]
	fn unsupported_controls_and_malformed_interfaces_return_errors() -> Result<()> {
		let unsupported = Sequence {
			qubits: 2,
			operations: vec![Operation {
				gate: Gate::T,
				targets: vec![1],
				controls: vec![Control {
					qubit: 0,
					positive: true,
				}],
			}],
		};
		verify_that!(optimize(&unsupported, 0), err(anything()))?;
		for sequence in [
			Sequence {
				qubits: 5,
				operations: Vec::new(),
			},
			Sequence {
				qubits: 1,
				operations: vec![op(Gate::X, &[1])],
			},
			Sequence {
				qubits: 1,
				operations: vec![op(Gate::X, &[0]); 129],
			},
			Sequence {
				qubits: 2,
				operations: vec![op(Gate::Cx, &[0, 0])],
			},
		] {
			verify_that!(optimize(&sequence, 0), err(anything()))?;
		}
		Ok(())
	}
	#[gtest]
	fn complete_uncontrolled_gate_set_and_exact_control_extensions_certify() -> Result<()> {
		for gate in [
			Gate::H,
			Gate::X,
			Gate::Y,
			Gate::Z,
			Gate::S,
			Gate::Sdg,
			Gate::T,
			Gate::Tdg,
			Gate::Cx,
			Gate::Cz,
			Gate::Swap,
			Gate::W,
		] {
			let targets = (0..gate.target_count()).collect::<Vec<_>>();
			let original = Sequence {
				qubits: 3,
				operations: vec![op(gate, &targets)],
			};
			let candidate = optimize(&original, 9)
				.map_err(|error| std::io::Error::other(format!("{gate:?}: {error}")))?;
			quest_math::verify_exact(&candidate, &original, Limits::default())?;
		}
		for (gate, targets, control_wires) in [
			(Gate::X, vec![2], vec![0, 1]),
			(Gate::Z, vec![2], vec![0, 1]),
			(Gate::Cx, vec![1, 2], vec![0]),
			(Gate::Cz, vec![1, 2], vec![0]),
			(Gate::W, vec![], vec![0]),
		] {
			for positive in [true, false] {
				let original = Sequence {
					qubits: 3,
					operations: vec![Operation {
						gate,
						targets: targets.clone(),
						controls: control_wires
							.iter()
							.map(|qubit| Control {
								qubit: *qubit,
								positive,
							})
							.collect(),
					}],
				};
				let candidate = optimize(&original, 9)
					.map_err(|error| std::io::Error::other(format!("{gate:?}: {error}")))?;
				quest_math::verify_exact(&candidate, &original, Limits::default())?;
			}
		}
		Ok(())
	}
	#[gtest]
	fn zero_qubit_scalar_and_deterministic_seed_interface_are_preserved() -> Result<()> {
		let original = Sequence {
			qubits: 0,
			operations: vec![op(Gate::W, &[]); 3],
		};
		let candidate = optimize(&original, 1).map_err(std::io::Error::other)?;
		quest_math::verify_exact(&candidate, &original, Limits::default())?;
		verify_eq!(
			&candidate,
			&optimize(&original, 99).map_err(std::io::Error::other)?
		)?;
		Ok(())
	}
	#[gtest]
	fn extracted_ancilla_and_changed_interfaces_are_not_admitted() -> Result<()> {
		let mut candidate = Sequence {
			qubits: 1,
			operations: Vec::new(),
		};
		verify_that!(
			from_gate(&mut candidate, &ZxGate::new(GType::InitAncilla, vec![1])),
			err(anything())
		)?;
		let original = Sequence {
			qubits: 2,
			operations: Vec::new(),
		};
		verify_that!(
			quest_math::recover_eighth_root_phase(&candidate, &original, Limits::default()),
			err(anything())
		)?;
		Ok(())
	}
}
