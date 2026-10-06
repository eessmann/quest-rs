//! Compact sparse matching plans. No padded matrix or circuit stream is stored.
use crate::{
	Complex64, EncodingBuilder, Error, ExplicitUnitaryPremise, Left, LogicalSpace, Normalization,
	NumericalPolicy, ProjectedEncoding, Result, Right,
};
use quest_numerics::SparseMatrix;
use std::{
	ops::{Add, Div, Mul, Neg, Sub},
	sync::Arc,
};

/// A weighted directed matching edge from input column to output row.
#[derive(Clone, Copy, Debug)]
pub struct MatchingEdge {
	/// Output coordinate.
	pub row: usize,
	/// Input coordinate.
	pub col: usize,
	/// Original canonical coefficient.
	pub value: Complex64,
	/// Binary64 magnitude.
	pub magnitude: f64,
	/// Success-sector phase in radians.
	pub phase: f64,
	/// Flag rotation angle, 2 acos(magnitude / beta).
	pub theta: f64,
}
/// One partial matching and its reversible completion on touched coordinates.
#[derive(Clone, Debug)]
pub struct Matching {
	edges: Vec<MatchingEdge>,
	permutation: Vec<(usize, usize)>,
	inverse: Vec<(usize, usize)>,
	cycles: Vec<Vec<usize>>,
}
impl Matching {
	/// Edges in increasing input-column order.
	#[must_use]
	pub fn edges(&self) -> &[MatchingEdge] {
		&self.edges
	}
	/// Completed touched-coordinate mapping, including path closure edges, sorted by source.
	#[must_use]
	pub fn permutation(&self) -> &[(usize, usize)] {
		&self.permutation
	}
	/// Nontrivial directed cycles of the completed permutation.
	#[must_use]
	pub fn cycles(&self) -> &[Vec<usize>] {
		&self.cycles
	}
	/// Coefficient at this input column; absent columns have zero success amplitude.
	#[must_use]
	pub fn entry(&self, col: usize) -> Option<&MatchingEdge> {
		self.edges
			.binary_search_by_key(&col, |edge| edge.col)
			.ok()
			.and_then(|i| self.edges.get(i))
	}
	/// Completed forward permutation, identity on every untouched coordinate.
	#[must_use]
	pub fn permutation_forward(&self, index: usize) -> usize {
		self.permutation
			.binary_search_by_key(&index, |&(source, _)| source)
			.ok()
			.and_then(|i| self.permutation.get(i))
			.map_or(index, |&(_, dest)| dest)
	}
	/// Completed inverse permutation, identity on every untouched coordinate.
	#[must_use]
	pub fn permutation_inverse(&self, index: usize) -> usize {
		self.inverse
			.binary_search_by_key(&index, |&(destination, _)| destination)
			.ok()
			.and_then(|position| self.inverse.get(position))
			.map_or(index, |&(_, source)| source)
	}
}
/// Immutable, auditable construction and replay resource counts.
#[derive(Clone, Copy, Debug)]
pub struct MatchingResources {
	/// Retained plan payload, excluding allocator bookkeeping and caller-owned input.
	pub retained_bytes: usize,
	/// Conservative peak construction allowance, independent of padded dimension.
	pub construction_peak_bytes: usize,
	/// Number of forward replay primitives (equal for the adjoint).
	pub replay_gates: usize,
}
/// Numerical provenance; this is not an exact-symbolic or theorem certificate.
#[derive(Clone, Copy, Debug)]
pub struct MatchingErrorMetadata {
	/// Coefficients and trigonometric gate parameters use binary64 arithmetic.
	pub binary64_parameters: bool,
	/// Whole-oracle unitarity follows from controlled gate composition and permutation completion.
	pub gate_construction_premise: bool,
}
#[derive(Debug)]
struct Plan {
	rows: usize,
	cols: usize,
	system_qubits: usize,
	color_qubits: usize,
	num_colors: usize,
	beta: f64,
	alpha: Normalization,
	matchings: Vec<Matching>,
	source_identity: u64,
	resources: MatchingResources,
}
/// Owning coherent block encoding A/(K beta), with bounded lazy gate replay.
///
/// Physical bits are flag bit 0, system bits 1..n, and color bits n+1..n+m.
/// Ancilla-zero packed logical ranges retain every physical row and column.
#[derive(Clone, Debug)]
pub struct MatchingEncoding {
	plan: Arc<Plan>,
}

pub fn bit(index: usize) -> Result<usize> {
	1_usize
		.checked_shl(u32::try_from(index).map_err(|_| Error::Budget("matching bit width"))?)
		.ok_or(Error::Budget("matching bit width"))
}
pub fn reserve<T>(count: usize) -> Result<Vec<T>> {
	let mut v = Vec::new();
	v.try_reserve_exact(count)
		.map_err(|_| Error::Budget("matching allocation"))?;
	Ok(v)
}
pub fn admit(bytes: usize, policy: NumericalPolicy) -> Result<()> {
	if bytes > policy.max_bytes || isize::try_from(bytes).is_err() {
		Err(Error::Budget("matching storage"))
	} else {
		Ok(())
	}
}
fn complete(mut edges: Vec<MatchingEdge>) -> Result<Matching> {
	edges.sort_unstable_by_key(|edge| edge.col);
	let mut permutation = reserve(
		edges
			.len()
			.checked_mul(2)
			.ok_or(Error::Budget("matching completion"))?,
	)?;
	permutation.extend(edges.iter().map(|edge| (edge.col, edge.row)));
	for edge in &edges {
		if edges.iter().any(|other| other.row == edge.col) {
			continue;
		}
		let start = edge.col;
		let mut terminal = edge.row;
		while let Ok(position) = edges.binary_search_by_key(&terminal, |other| other.col) {
			terminal = edges
				.get(position)
				.ok_or(Error::Encoding("matching path"))?
				.row;
		}
		permutation.push((terminal, start));
	}
	permutation.sort_unstable_by_key(|&(source, _)| source);
	let mut visited = reserve(permutation.len())?;
	visited.resize(permutation.len(), false);
	let mut cycles = reserve(edges.len())?;
	for (position, &(start, _)) in permutation.iter().enumerate() {
		if visited
			.get(position)
			.copied()
			.ok_or(Error::Encoding("matching visit"))?
		{
			continue;
		}
		let mut len = 1_usize;
		let mut next = permutation
			.get(position)
			.ok_or(Error::Encoding("matching cycle"))?
			.1;
		while next != start {
			let p = permutation
				.binary_search_by_key(&next, |&(s, _)| s)
				.map_err(|_| Error::Encoding("matching closed cycle"))?;
			next = permutation
				.get(p)
				.ok_or(Error::Encoding("matching cycle"))?
				.1;
			len = len.checked_add(1).ok_or(Error::Budget("matching cycle"))?;
		}
		let mut cycle = reserve(len)?;
		let mut current = start;
		loop {
			let p = permutation
				.binary_search_by_key(&current, |&(s, _)| s)
				.map_err(|_| Error::Encoding("matching cycle"))?;
			*visited
				.get_mut(p)
				.ok_or(Error::Encoding("matching visit"))? = true;
			cycle.push(current);
			current = permutation
				.get(p)
				.ok_or(Error::Encoding("matching cycle"))?
				.1;
			if current == start {
				break;
			}
		}
		if cycle.len() > 1 {
			cycles.push(cycle);
		}
	}
	let mut inverse = reserve(permutation.len())?;
	inverse.extend(
		permutation
			.iter()
			.map(|&(source, destination)| (destination, source)),
	);
	inverse.sort_unstable_by_key(|&(destination, _)| destination);
	Ok(Matching {
		edges,
		permutation,
		inverse,
		cycles,
	})
}
type ColoredEntries = (Vec<(usize, usize, Complex64)>, Vec<usize>, usize, f64, u64);
fn color_entries(matrix: &SparseMatrix) -> Result<ColoredEntries> {
	let mut entries = reserve(matrix.nnz())?;
	entries.extend(matrix.entries());
	entries.sort_unstable_by_key(|&(r, c, _)| (r, c));
	let mut colors = reserve(entries.len())?;
	let mut num_matchings = 0_usize;
	let mut beta = 0.0_f64;
	let mut identity = 0xcbf2_9ce4_8422_2325_u64;
	for component in [
		u64::try_from(matrix.rows()).map_err(|_| Error::Budget("source identity"))?,
		u64::try_from(matrix.cols()).map_err(|_| Error::Budget("source identity"))?,
	] {
		identity = crate::owned_replay::fingerprint_word(identity, component);
	}
	for (position, &(row, col, value)) in entries.iter().enumerate() {
		let magnitude = value.norm();
		if !magnitude.is_finite() {
			return Err(Error::NonFinite);
		}
		beta = beta.max(magnitude);
		for component in [
			u64::try_from(row).map_err(|_| Error::Budget("source identity"))?,
			u64::try_from(col).map_err(|_| Error::Budget("source identity"))?,
			value.re.to_bits(),
			value.im.to_bits(),
		] {
			identity = crate::owned_replay::fingerprint_word(identity, component);
		}
		let color = (0..num_matchings)
			.find(|&candidate| {
				!entries
					.iter()
					.zip(&colors)
					.take(position)
					.any(|(&(r, c, _), &assigned)| assigned == candidate && (r == row || c == col))
			})
			.unwrap_or(num_matchings);
		if color == num_matchings {
			num_matchings = num_matchings
				.checked_add(1)
				.ok_or(Error::Budget("matching colors"))?;
		}
		colors.push(color);
	}
	Ok((entries, colors, num_matchings, beta, identity))
}
fn retained_storage(matchings: &Vec<Matching>) -> Result<usize> {
	matchings.iter().try_fold(
		size_of::<Plan>()
			.checked_add(
				matchings
					.capacity()
					.checked_mul(size_of::<Matching>())
					.ok_or(Error::Budget("matching retained"))?,
			)
			.ok_or(Error::Budget("matching retained"))?,
		|total, m| {
			let cycle_bytes = m.cycles.iter().try_fold(
				m.cycles
					.capacity()
					.checked_mul(size_of::<Vec<usize>>())
					.ok_or(Error::Budget("matching cycles"))?,
				|b, cycle| {
					b.checked_add(
						cycle
							.capacity()
							.checked_mul(size_of::<usize>())
							.ok_or(Error::Budget("matching cycles"))?,
					)
					.ok_or(Error::Budget("matching cycles"))
				},
			)?;
			total
				.checked_add(
					m.edges
						.capacity()
						.checked_mul(size_of::<MatchingEdge>())
						.ok_or(Error::Budget("matching edges"))?,
				)
				.and_then(|b| {
					b.checked_add(
						m.permutation
							.capacity()
							.checked_mul(size_of::<(usize, usize)>())?,
					)
				})
				.and_then(|b| {
					b.checked_add(
						m.inverse
							.capacity()
							.checked_mul(size_of::<(usize, usize)>())?,
					)
				})
				.and_then(|b| b.checked_add(cycle_bytes))
				.ok_or(Error::Budget("matching retained"))
		},
	)
}
impl MatchingEncoding {
	/// Greedily color canonical row-major edges and complete each partial permutation.
	/// Colors are deterministic and bounded by maximum row degree + maximum
	/// column degree - 1. K is the next power of two; absent colors are zero blocks.
	/// Zero matrices use K=beta=alpha=1 and a flag rotation with zero success block.
	///
	/// # Errors
	/// Rejects dimension/normalization overflow, nonfinite magnitudes, storage budgets and allocation failure.
	pub fn from_sparse(matrix: &SparseMatrix, policy: NumericalPolicy) -> Result<Self> {
		let n = matrix
			.rows()
			.max(matrix.cols())
			.checked_next_power_of_two()
			.ok_or(Error::Budget("matching system dimension"))?;
		let system_qubits =
			usize::try_from(n.ilog2()).map_err(|_| Error::Budget("matching width"))?;
		// Accounts original entries, assignments, colors, per-edge completion and scratch.
		let peak = matrix
			.nnz()
			.checked_mul(512)
			.and_then(|b| b.checked_add(512))
			.ok_or(Error::Budget("matching plan size"))?;
		admit(peak, policy)?;
		let (entries, colors, num_matchings, mut beta, identity) = color_entries(matrix)?;
		let num_colors = num_matchings
			.max(1)
			.checked_next_power_of_two()
			.ok_or(Error::Budget("matching padded colors"))?;
		let color_qubits = usize::try_from(num_colors.ilog2())
			.map_err(|_| Error::Budget("matching color width"))?;
		bit(system_qubits
			.checked_add(color_qubits)
			.and_then(|w| w.checked_add(1))
			.ok_or(Error::Budget("matching width"))?)?;
		if beta == 0.0 {
			beta = 1.0;
		}
		let alpha = Normalization::new(
			beta.mul(f64::from(
				u32::try_from(num_colors)
					.map_err(|_| Error::Budget("matching normalization colors"))?,
			)),
		)?;
		let mut matchings = reserve(num_matchings)?;
		for color in 0..num_matchings {
			let count = colors.iter().filter(|&&c| c == color).count();
			let mut edges = reserve(count)?;
			for (&(row, col, value), &assigned) in entries.iter().zip(&colors) {
				if assigned == color {
					let magnitude = value.norm();
					edges.push(MatchingEdge {
						row,
						col,
						value,
						magnitude,
						phase: value.arg(),
						theta: magnitude.div(beta).clamp(0.0, 1.0).acos().mul(2.0),
					});
				}
			}
			matchings.push(complete(edges)?);
		}
		let retained_bytes = retained_storage(&matchings)?;
		admit(retained_bytes, policy)?;
		let mut result = Self {
			plan: Arc::new(Plan {
				rows: matrix.rows(),
				cols: matrix.cols(),
				system_qubits,
				color_qubits,
				num_colors,
				beta,
				alpha,
				matchings,
				source_identity: identity,
				resources: MatchingResources {
					retained_bytes,
					construction_peak_bytes: peak,
					replay_gates: 0,
				},
			}),
		};
		let mut count = 0_usize;
		result.visit_gates(false, |_| {
			count = count
				.checked_add(1)
				.ok_or(Error::Budget("matching gate count"))?;
			Ok(())
		})?;
		Arc::get_mut(&mut result.plan)
			.ok_or(Error::Encoding("matching plan ownership"))?
			.resources
			.replay_gates = count;
		Ok(result)
	}
	/// Logical output dimension.
	#[must_use]
	pub fn rows(&self) -> usize {
		self.plan.rows
	}
	/// Logical input dimension.
	#[must_use]
	pub fn cols(&self) -> usize {
		self.plan.cols
	}
	/// Total physical oracle width.
	#[must_use]
	pub fn num_qubits(&self) -> usize {
		self.plan
			.system_qubits
			.saturating_add(self.plan.color_qubits)
			.saturating_add(1)
	}
	/// System register width.
	#[must_use]
	pub fn system_qubits(&self) -> usize {
		self.plan.system_qubits
	}
	/// Color register width.
	#[must_use]
	pub fn color_qubits(&self) -> usize {
		self.plan.color_qubits
	}
	/// Padded number of colors, including zero dummy colors.
	#[must_use]
	pub fn num_colors(&self) -> usize {
		self.plan.num_colors
	}
	/// Entry magnitude normalization.
	#[must_use]
	pub fn beta(&self) -> f64 {
		self.plan.beta
	}
	/// Physical normalization K beta.
	#[must_use]
	pub fn normalization(&self) -> Normalization {
		self.plan.alpha
	}
	/// Compact nonempty matching plans.
	#[must_use]
	pub fn matchings(&self) -> &[Matching] {
		&self.plan.matchings
	}
	/// Deterministic noncryptographic FNV identity of dimensions and canonical coefficient bits.
	#[must_use]
	pub fn source_identity(&self) -> u64 {
		self.plan.source_identity
	}
	/// Immutable plan and replay resource counts.
	#[must_use]
	pub fn resources(&self) -> MatchingResources {
		self.plan.resources
	}
	/// Numerical construction provenance, without an exact error certificate.
	#[must_use]
	pub const fn error_metadata(&self) -> MatchingErrorMetadata {
		MatchingErrorMetadata {
			binary64_parameters: true,
			gate_construction_premise: true,
		}
	}
	/// Build a bounded conventional projected encoding from the lazy replay.
	///
	/// # Errors
	/// Rejects circuit storage, compact interface admission and oracle construction failures.
	pub fn projected_encoding(&self, policy: NumericalPolicy) -> Result<ProjectedEncoding> {
		let dimension = bit(self.num_qubits())?;
		let system_mask = bit(self.system_qubits())?
			.checked_sub(1)
			.and_then(|mask| mask.checked_mul(2))
			.ok_or(Error::Budget("matching system mask"))?;
		let mask = dimension
			.checked_sub(1)
			.ok_or(Error::Budget("matching projection mask"))?
			& !system_mask;
		EncodingBuilder::new().oracle(self.to_oracle(policy)?)
            .left(LogicalSpace::<Left>::constrained_range(dimension,mask,0,0..self.rows(),policy)?)
            .right(LogicalSpace::<Right>::constrained_range(dimension,mask,0,0..self.cols(),policy)?)
            .normalization(self.normalization().get())?.policy(policy)
            .unitarity_assumption(ExplicitUnitaryPremise::new("Sparse matching oracle composes controlled H, Ry, phase and X gates with completed permutation cycles; floating gate parameters are not an exact-symbolic certificate")?).build()
	}
	/// Apply the identical whole matching unitary as a budgeted scalar reference.
	/// This includes both flag sectors and dummy colors; it is never a solver fallback.
	///
	/// # Errors
	/// Rejects shape, storage, allocation and nonfinite state/amplitudes.
	pub fn apply_reference(
		&self,
		state: &mut [Complex64],
		adjoint: bool,
		policy: NumericalPolicy,
	) -> Result<()> {
		let dimension = bit(self.num_qubits())?;
		if state.len() != dimension {
			return Err(Error::Encoding("matching reference state dimension"));
		}
		admit(
			dimension
				.checked_mul(size_of::<Complex64>())
				.and_then(|b| b.checked_mul(2))
				.and_then(|b| b.checked_add(self.resources().retained_bytes))
				.ok_or(Error::Budget("matching reference state"))?,
			policy,
		)?;
		if state.iter().any(|v| !v.re.is_finite() || !v.im.is_finite()) {
			return Err(Error::NonFinite);
		}
		let mut scratch = reserve(dimension)?;
		scratch.resize(dimension, Complex64::new(0.0, 0.0));
		self.hadamard_colors(state)?;
		for color in 0..self.num_colors() {
			self.apply_color_reference(color, state, &mut scratch, adjoint)?;
		}
		if !adjoint {
			state.copy_from_slice(&scratch);
		}
		self.hadamard_colors(state)?;
		if state.iter().any(|v| !v.re.is_finite() || !v.im.is_finite()) {
			return Err(Error::NonFinite);
		}
		Ok(())
	}
	fn apply_color_reference(
		&self,
		color: usize,
		state: &mut [Complex64],
		scratch: &mut [Complex64],
		adjoint: bool,
	) -> Result<()> {
		let size = bit(self.system_qubits())?;
		let base = color
			.checked_mul(size)
			.and_then(|n| n.checked_mul(2))
			.ok_or(Error::Budget("matching reference offset"))?;
		let matching = self.matchings().get(color);
		if adjoint {
			for system in 0..size {
				let dest = matching.map_or(system, |m| m.permutation_inverse(system));
				for flag in 0..2 {
					let src = base
						.checked_add(
							system
								.checked_mul(2)
								.ok_or(Error::Budget("matching offset"))?,
						)
						.and_then(|n| n.checked_add(flag))
						.ok_or(Error::Budget("matching offset"))?;
					let output_index = base
						.checked_add(
							dest.checked_mul(2)
								.ok_or(Error::Budget("matching offset"))?,
						)
						.and_then(|n| n.checked_add(flag))
						.ok_or(Error::Budget("matching offset"))?;
					*scratch
						.get_mut(output_index)
						.ok_or(Error::Encoding("matching reference destination"))? = *state
						.get(src)
						.ok_or(Error::Encoding("matching reference source"))?;
				}
			}
		}
		for system in 0..size {
			let index = base
				.checked_add(
					system
						.checked_mul(2)
						.ok_or(Error::Budget("matching reference offset"))?,
				)
				.ok_or(Error::Budget("matching reference offset"))?;
			let other = index
				.checked_add(1)
				.ok_or(Error::Budget("matching flag offset"))?;
			let entry = matching.and_then(|m| m.entry(system));
			let (sin, cos) = entry.map_or((1.0, 0.0), |e| e.theta.div(2.0).sin_cos());
			let phase = entry.map_or(Complex64::new(1.0, 0.0), |e| {
				Complex64::from_polar(1.0, e.phase)
			});
			let input = if adjoint { &scratch[..] } else { &state[..] };
			let a = *input
				.get(index)
				.ok_or(Error::Encoding("matching reference flag"))?;
			let b = *input
				.get(other)
				.ok_or(Error::Encoding("matching reference flag"))?;
			let (a, b) = if adjoint {
				(
					a.mul(phase.conj()).mul(cos).add(b.mul(sin)),
					a.mul(phase.conj()).mul(sin).neg().add(b.mul(cos)),
				)
			} else {
				(
					a.mul(cos).sub(b.mul(sin)).mul(phase),
					a.mul(sin).add(b.mul(cos)),
				)
			};
			let dest = if adjoint {
				system
			} else {
				matching.map_or(system, |m| m.permutation_forward(system))
			};
			let output = base
				.checked_add(
					dest.checked_mul(2)
						.ok_or(Error::Budget("matching reference output"))?,
				)
				.ok_or(Error::Budget("matching reference output"))?;
			let output_other = output
				.checked_add(1)
				.ok_or(Error::Budget("matching reference output"))?;
			if adjoint {
				*state
					.get_mut(output)
					.ok_or(Error::Encoding("matching output"))? = a;
				*state
					.get_mut(output_other)
					.ok_or(Error::Encoding("matching output"))? = b;
			} else {
				*scratch
					.get_mut(output)
					.ok_or(Error::Encoding("matching output"))? = a;
				*scratch
					.get_mut(output_other)
					.ok_or(Error::Encoding("matching output"))? = b;
			}
		}
		Ok(())
	}
	fn hadamard_colors(&self, state: &mut [Complex64]) -> Result<()> {
		let width = self
			.system_qubits()
			.checked_add(1)
			.ok_or(Error::Budget("matching color offset"))?;
		for color_bit in 0..self.color_qubits() {
			let mask = bit(width
				.checked_add(color_bit)
				.ok_or(Error::Budget("matching color bit"))?)?;
			for index in 0..state.len() {
				if index & mask != 0 {
					continue;
				}
				let other = index | mask;
				let a = *state
					.get(index)
					.ok_or(Error::Encoding("matching H state"))?;
				let b = *state
					.get(other)
					.ok_or(Error::Encoding("matching H state"))?;
				*state
					.get_mut(index)
					.ok_or(Error::Encoding("matching H state"))? =
					a.add(b).mul(std::f64::consts::FRAC_1_SQRT_2);
				*state
					.get_mut(other)
					.ok_or(Error::Encoding("matching H state"))? =
					a.sub(b).mul(std::f64::consts::FRAC_1_SQRT_2);
			}
		}
		Ok(())
	}
}
