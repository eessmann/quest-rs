//! All-at-once causal temporal DG, applied directly to the non-Hermitian system.
#![allow(
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	clippy::too_many_lines,
	clippy::as_conversions,
	clippy::suboptimal_flops,
	clippy::manual_midpoint,
	clippy::suspicious_operation_groupings,
	reason = "Checked dimensions bound DG tensor and slab indices"
)]
use crate::{CfdError, configuration::Element};
use quest_numerics::{Complex64, SparseFormat, SparseLimits, SparseMatrix};

/// Time-dependent lifted linear dynamics, `z' = G(t) z + f(t)`.
///
/// Recipes are visited once per temporal quadrature node. Implementations declare
/// their maximum emitted entries and retained kernel storage; these declarations
/// are charged before history allocation. Recipe-internal allocations and work
/// must obey the implementation's own construction policy. No global coefficient
/// tensor is required by this interface.
pub trait HistoryDynamics {
	/// Every lifted coordinate, including auxiliary and conserved coordinates.
	fn dimension(&self) -> usize;
	/// Maximum number of emitted entries at any one node, including duplicates.
	fn max_generator_entries(&self) -> usize;
	/// Owned and borrowed kernel storage live concurrently with assembly.
	/// # Errors
	/// Reports checked storage-accounting failures.
	fn retained_bytes(&self) -> Result<usize, CfdError>;
	/// Emit finite generator entries; coordinates and entry counts are checked.
	/// # Errors
	/// Propagates recipe and visitor failures.
	fn visit_generator(
		&self,
		time: f64,
		visitor: &mut dyn FnMut(usize, usize, Complex64) -> Result<(), CfdError>,
	) -> Result<(), CfdError>;
	/// Fill a dimension-sized, initially zero source buffer.
	/// # Errors
	/// Reports unsupported source evaluation or numerical domain failures.
	fn source(&self, time: f64, output: &mut [Complex64]) -> Result<(), CfdError>;
}
struct Autonomous<'a>(&'a SparseMatrix);
impl HistoryDynamics for Autonomous<'_> {
	fn dimension(&self) -> usize {
		self.0.rows()
	}
	fn max_generator_entries(&self) -> usize {
		self.0.nnz()
	}
	fn retained_bytes(&self) -> Result<usize, CfdError> {
		Ok(self.0.retained_bytes()?)
	}
	fn visit_generator(
		&self,
		_: f64,
		visitor: &mut dyn FnMut(usize, usize, Complex64) -> Result<(), CfdError>,
	) -> Result<(), CfdError> {
		for (row, column, value) in self.0.entries() {
			visitor(row, column, value)?;
		}
		Ok(())
	}
	fn source(&self, _: f64, _: &mut [Complex64]) -> Result<(), CfdError> {
		Ok(())
	}
}

/// Global history of mass-weighted configuration amplitudes.
#[derive(Clone, Debug)]
pub struct HistorySystem {
	operator: SparseMatrix,
	rhs: Vec<Complex64>,
	configuration_dimension: usize,
	times: Vec<f64>,
	temporal_weights: Vec<f64>,
	spectral_lower: Option<f64>,
	temporal_order: usize,
}
impl HistorySystem {
	/// Assemble the whole horizon with upwind temporal traces and lumped DG mass.
	///
	/// # Errors
	/// Rejects incompatible states, invalid horizons/order, overflow or sparse budgets.
	#[allow(
		clippy::cast_precision_loss,
		reason = "Temporal node counts are bounded by admitted sparse dimensions"
	)]
	pub fn assemble(
		generator: &SparseMatrix,
		initial: &[Complex64],
		horizon: f64,
		cells: usize,
		order: usize,
		limits: SparseLimits,
	) -> Result<Self, CfdError> {
		if generator.rows() != generator.cols() {
			return Err(CfdError::InvalidInput("history generator must be square"));
		}
		Self::assemble_dynamics(
			&Autonomous(generator),
			initial,
			horizon,
			cells,
			order,
			limits,
		)
	}

	/// Assemble a common causal DG history for `KvN` or Carleman dynamics.
	/// Forcing enters every temporal test equation through its quadrature mass;
	/// the initial trace is an additional source in the first slab.
	/// # Errors
	/// Rejects malformed recipes, nonfinite values, invalid dimensions and budgets.
	#[allow(
		clippy::cast_precision_loss,
		reason = "Node counts are bounded by sparse admission"
	)]
	pub fn assemble_dynamics(
		dynamics: &impl HistoryDynamics,
		initial: &[Complex64],
		horizon: f64,
		cells: usize,
		order: usize,
		limits: SparseLimits,
	) -> Result<Self, CfdError> {
		let n = dynamics.dimension();
		let max_generator_entries = dynamics.max_generator_entries();
		if n == 0
			|| n != initial.len()
			|| initial
				.iter()
				.any(|z| !z.re.is_finite() || !z.im.is_finite())
			|| cells == 0
			|| !horizon.is_finite()
			|| horizon <= 0.0
		{
			return Err(CfdError::InvalidInput(
				"invalid global history dimensions, horizon or initial state",
			));
		}
		let element = Element::new(order)?;
		let q = element.nodes.len();
		let temporal = cells
			.checked_mul(q)
			.ok_or(CfdError::InvalidInput("temporal dimension overflow"))?;
		let total = n
			.checked_mul(temporal)
			.ok_or(CfdError::InvalidInput("history dimension overflow"))?;
		let entries = total
			.checked_mul(q + 1)
			.and_then(|v| v.checked_add(temporal.checked_mul(max_generator_entries)?))
			.ok_or(CfdError::InvalidInput("history entry overflow"))?;
		let dt = horizon / (cells as f64);
		let minimum_mass = 0.5
			* dt
			* element
				.weights
				.iter()
				.copied()
				.fold(f64::INFINITY, f64::min);
		if !dt.is_finite() || dt <= 0.0 || !minimum_mass.is_finite() || minimum_mass <= 0.0 {
			return Err(CfdError::InvalidInput(
				"temporal quadrature mass underflow/overflow",
			));
		}
		let side_bytes = dynamics
			.retained_bytes()?
			.checked_add(size_of::<Self>())
			.and_then(|v| v.checked_add(initial.len().checked_mul(size_of::<Complex64>())?))
			.and_then(|v| v.checked_add(total.checked_mul(size_of::<Complex64>())?))
			.and_then(|v| v.checked_add(n.checked_mul(size_of::<Complex64>())?))
			.and_then(|v| v.checked_add(temporal.checked_mul(2 * size_of::<f64>())?))
			.ok_or(CfdError::InvalidInput(
				"history concurrent storage overflow",
			))?;
		let bytes = entries
			.checked_mul(size_of::<(usize, usize, Complex64)>())
			.and_then(|v| v.checked_add(side_bytes))
			.and_then(|v| v.checked_add(element.retained_bytes()))
			.ok_or(CfdError::InvalidInput("history storage overflow"))?;
		if total > limits.max_dimension
			|| entries > limits.max_entries
			|| bytes > limits.max_bytes
			|| entries > limits.max_work
		{
			return Err(CfdError::InvalidInput(
				"global history exceeds sparse admission",
			));
		}
		let mut triplets = Vec::new();
		triplets
			.try_reserve_exact(entries)
			.map_err(|_| CfdError::InvalidInput("history allocation failed"))?;
		let mut rhs = Vec::new();
		rhs.try_reserve_exact(total)
			.map_err(|_| CfdError::InvalidInput("history rhs allocation failed"))?;
		rhs.resize(total, Complex64::new(0.0, 0.0));
		rhs[..n].copy_from_slice(initial);
		let mut times = Vec::new();
		let mut temporal_weights = Vec::new();
		times
			.try_reserve_exact(temporal)
			.map_err(|_| CfdError::InvalidInput("history time allocation failed"))?;
		temporal_weights
			.try_reserve_exact(temporal)
			.map_err(|_| CfdError::InvalidInput("history weight allocation failed"))?;
		let mut source = Vec::new();
		source
			.try_reserve_exact(n)
			.map_err(|_| CfdError::InvalidInput("history source allocation failed"))?;
		source.resize(n, Complex64::new(0.0, 0.0));
		for cell in 0..cells {
			for a in 0..q {
				let block = cell * q + a;
				let mass = 0.5 * dt * element.weights[a];
				let time = dt * ((cell as f64) + 0.5 * (element.nodes[a] + 1.0));
				times.push(time);
				source.fill(Complex64::new(0.0, 0.0));
				dynamics.source(time, &mut source)?;
				for (i, value) in source.iter().enumerate() {
					let contribution = mass * value;
					if !contribution.re.is_finite()
						|| !contribution.im.is_finite()
						|| !value.re.is_finite()
						|| !value.im.is_finite()
					{
						return Err(CfdError::InvalidInput("nonfinite history source"));
					}
					rhs[block * n + i] += contribution;
					if !rhs[block * n + i].re.is_finite() || !rhs[block * n + i].im.is_finite() {
						return Err(CfdError::InvalidInput(
							"history source accumulation overflow",
						));
					}
				}
				temporal_weights.push(mass);
				for b in 0..q {
					let k = -element.derivative[b][a] * element.weights[b]
						+ f64::from(a == q - 1 && b == q - 1);
					if k != 0.0 {
						for i in 0..n {
							triplets.push((
								block * n + i,
								(cell * q + b) * n + i,
								Complex64::new(k, 0.0),
							));
						}
					}
				}
				if cell > 0 && a == 0 {
					for i in 0..n {
						triplets.push((
							block * n + i,
							((cell - 1) * q + q - 1) * n + i,
							Complex64::new(-1.0, 0.0),
						));
					}
				}
				let mut emitted = 0usize;
				dynamics.visit_generator(time, &mut |i, j, value| {
					emitted = emitted
						.checked_add(1)
						.ok_or(CfdError::InvalidInput("history recipe count overflow"))?;
					let scaled = -mass * value;
					if emitted > max_generator_entries
						|| i >= n
						|| j >= n
						|| !value.re.is_finite()
						|| !value.im.is_finite()
						|| !scaled.re.is_finite()
						|| !scaled.im.is_finite()
					{
						return Err(CfdError::InvalidInput("invalid history generator recipe"));
					}
					triplets.push((block * n + i, block * n + j, scaled));
					Ok(())
				})?;
			}
		}
		drop(element);
		let remaining = limits
			.max_bytes
			.checked_sub(side_bytes)
			.ok_or(CfdError::InvalidInput(
				"history concurrent storage exceeds budget",
			))?;
		let stage_limits = SparseLimits {
			max_bytes: remaining,
			..limits
		};
		let operator =
			SparseMatrix::from_triplets(total, total, SparseFormat::Csr, triplets, stage_limits)?;
		let spectral_lower = if order == 1 {
			match history_lower(&operator, n * q, cells, stage_limits) {
				Ok(bound) => bound,
				Err(CfdError::Numerical(
					quest_numerics::Error::Interval | quest_numerics::Error::Domain(_),
				)) => None,
				Err(error) => return Err(error),
			}
		} else {
			match history_lower_dg2(&operator, n, cells, stage_limits) {
				Ok(bound) => bound,
				Err(CfdError::Numerical(
					quest_numerics::Error::Interval | quest_numerics::Error::Domain(_),
				)) => None,
				Err(error) => return Err(error),
			}
		};
		Ok(Self {
			operator,
			rhs,
			configuration_dimension: n,
			times,
			temporal_weights,
			spectral_lower,
			temporal_order: order,
		})
	}
	/// Retained operator, RHS, temporal metadata and inline wrapper storage.
	/// # Errors
	/// Rejects resource-accounting overflow; allocator bookkeeping is excluded.
	pub fn retained_bytes(&self) -> Result<usize, CfdError> {
		self.operator
			.retained_bytes()?
			.checked_add(size_of::<Self>())
			.and_then(|n| n.checked_add(self.rhs.capacity().checked_mul(size_of::<Complex64>())?))
			.and_then(|n| n.checked_add(self.times.capacity().checked_mul(size_of::<f64>())?))
			.and_then(|n| {
				n.checked_add(
					self.temporal_weights
						.capacity()
						.checked_mul(size_of::<f64>())?,
				)
			})
			.ok_or(CfdError::InvalidInput("history retained byte overflow"))
	}
	/// Direct, non-Hermitian causal operator. No normal equations are formed.
	#[must_use]
	pub const fn operator(&self) -> &SparseMatrix {
		&self.operator
	}
	/// Initial trace plus temporal quadrature of the external lifted source.
	#[must_use]
	pub fn rhs(&self) -> &[Complex64] {
		&self.rhs
	}
	/// Number of configuration coefficients per temporal node.
	#[must_use]
	pub const fn configuration_dimension(&self) -> usize {
		self.configuration_dimension
	}
	/// Polynomial degree of the causal temporal DG approximation.
	#[must_use]
	pub const fn temporal_order(&self) -> usize {
		self.temporal_order
	}
	/// Times in slab-major, nodal order. Interface traces appear twice.
	#[must_use]
	pub fn times(&self) -> &[f64] {
		&self.times
	}
	/// Temporal quadrature weights; raw history amplitudes are not weighted by these.
	#[must_use]
	pub fn temporal_weights(&self) -> &[f64] {
		&self.temporal_weights
	}
}

impl HistorySystem {
	/// Analytic singular-value bound for the exact stored global operator.
	/// Each diagonal slab has Hermitian part at least c I. The causal inverse is
	/// bounded by sum_{k=1}^{cells} c^(-k); the upper bound uses sparse row/column norms.
	/// No dense SVD or normal equations are used.
	/// # Errors
	/// Reports unavailable evidence when the admitted slab bound is insufficient.
	pub fn spectral_bounds(
		&self,
		limits: SparseLimits,
	) -> Result<quest_qsvt::reciprocal::SpectralBounds, CfdError> {
		let lower=self.spectral_lower.ok_or_else(||CfdError::Unsupported(format!("analytic temporal DG{} spectral bound unavailable: slab evidence and a finite representable causal inverse bound required", self.temporal_order)))?;
		let upper = self.operator.norms(limits)?.spectral_upper_bound;
		Ok(quest_qsvt::reciprocal::SpectralBounds::new(lower,upper,quest_qsvt::reciprocal::SpectralEvidence::Analytic {description: if self.temporal_order == 1 { "Temporal DG1 slab coercivity and finite causal inverse series, with outward sparse Hermitian-part Gershgorin lower bound" } else { "Temporal DG2 inverse-preconditioner interval residual and finite causal inverse series; no normality premise" }.to_owned()})?)
	}
}

fn history_lower(
	operator: &SparseMatrix,
	slab: usize,
	cells: usize,
	limits: SparseLimits,
) -> Result<Option<f64>, CfdError> {
	use quest_numerics::Interval;
	let mut work = operator
		.rows()
		.checked_mul(2)
		.ok_or(CfdError::InvalidInput("spectral proof work overflow"))?;
	for (_, col, _) in operator.entries() {
		let row_length = operator
			.major_segment(col)
			.map_or(0, |(indices, _)| indices.len());
		work = work
			.checked_add(row_length)
			.and_then(|n| n.checked_add(4))
			.ok_or(CfdError::InvalidInput("spectral proof work overflow"))?;
	}
	let bytes = operator
		.rows()
		.checked_mul(size_of::<Interval>())
		.and_then(|n| n.checked_add(operator.retained_bytes().ok()?))
		.and_then(|n| n.checked_add(size_of::<Vec<Interval>>()))
		.ok_or(CfdError::InvalidInput("spectral proof bytes overflow"))?;
	if work > limits.max_work || bytes > limits.max_bytes {
		return Err(CfdError::InvalidInput(
			"spectral proof exceeds sparse work/storage admission",
		));
	}
	let point = Interval::point;
	let mut rows = Vec::new();
	rows.try_reserve_exact(operator.rows())
		.map_err(|_| CfdError::InvalidInput("DG1 spectral allocation"))?;
	rows.resize(operator.rows(), point(0.0)?);
	for (row, col, value) in operator.entries() {
		if row / slab != col / slab {
			continue;
		}
		if row == col {
			rows[row] = rows[row].checked_add(point(value.re)?)?;
			continue;
		}
		let reverse = operator
			.row(col)
			.find(|(j, _)| *j == row)
			.map(|(_, z)| z.conj());
		let other = reverse.unwrap_or(Complex64::new(0.0, 0.0));
		let real = point(value.re)?.checked_add(point(other.re)?)?;
		let imag = point(value.im)?.checked_add(point(other.im)?)?;
		let bound = real
			.square()?
			.checked_add(imag.square()?)?
			.sqrt()?
			.checked_mul(point(0.5)?)?;
		rows[row] = rows[row].checked_sub(bound)?;
		if reverse.is_none() {
			rows[col] = rows[col].checked_sub(bound)?;
		}
	}
	// Gershgorin lower bound on (B+B†)/2 uses the sign of each diagonal.
	// Positive viscous dissipation therefore strengthens the coercivity proof.
	let c = rows.iter().map(|v| v.lower()).fold(f64::INFINITY, f64::min);
	if c <= 0.0 {
		return Ok(None);
	}
	let inverse = point(1.0)?.checked_div(point(c)?)?;
	let mut term = inverse;
	let mut sum = point(0.0)?;
	for step in 0..cells {
		sum = sum.checked_add(term)?;
		if step + 1 < cells {
			term = term.checked_mul(inverse)?;
		}
	}
	Ok(Some(point(1.0)?.checked_div(sum)?.lower()))
}

/// Q is the exact inverse of the three-node temporal derivative/trace matrix:
/// [[1,-1/2,1],[1,5/8,-1/2],[1,1,1]]. Its 1- and infinity-norms are <= 3.
/// We certify `||I-(Q tensor I) B_s||_2 < 1` for every stored diagonal slab `B_s`,
/// including its binary64 temporal coefficients. Thus ||B_s^-1|| <= 3/(1-delta).
/// Off-diagonal causal trace blocks have norm one. This argument does not use
/// eigenvalue decay, normality, an inverse of G, or normal equations.
fn history_lower_dg2(
	operator: &SparseMatrix,
	dimension: usize,
	cells: usize,
	limits: SparseLimits,
) -> Result<Option<f64>, CfdError> {
	use quest_numerics::Interval;
	let slab = dimension
		.checked_mul(3)
		.ok_or(CfdError::InvalidInput("DG2 slab overflow"))?;
	let work = operator
		.nnz()
		.checked_mul(24)
		.and_then(|n| n.checked_add(operator.rows().checked_mul(8)?))
		.ok_or(CfdError::InvalidInput("DG2 proof work overflow"))?;
	// Reusable indexed sparse accumulator: one stamp and complex interval per
	// column in a slab, plus a touched-column list. No tree allocation occurs
	// during the proof and each stored entry is visited at most three times.
	let bytes = slab
		.checked_mul(2 * size_of::<Interval>() + 2 * size_of::<usize>())
		.and_then(|n| n.checked_add(operator.rows().checked_mul(size_of::<Interval>())?))
		.and_then(|n| n.checked_add(operator.retained_bytes().ok()?))
		.and_then(|n| n.checked_add(5 * size_of::<Vec<usize>>()))
		.ok_or(CfdError::InvalidInput("DG2 proof storage overflow"))?;
	if work > limits.max_work || bytes > limits.max_bytes {
		return Err(CfdError::InvalidInput(
			"DG2 spectral proof exceeds sparse admission",
		));
	}
	let q = [[1.0, -0.5, 1.0], [1.0, 0.625, -0.5], [1.0, 1.0, 1.0]];
	let point = Interval::point;
	let zero = point(0.0)?;
	let mut columns = Vec::new();
	columns
		.try_reserve_exact(operator.cols())
		.map_err(|_| CfdError::InvalidInput("DG2 proof allocation"))?;
	columns.resize(operator.cols(), zero);
	let mut values = Vec::new();
	values
		.try_reserve_exact(slab)
		.map_err(|_| CfdError::InvalidInput("DG2 accumulator allocation"))?;
	values.resize(slab, (zero, zero));
	let mut stamps = Vec::new();
	stamps
		.try_reserve_exact(slab)
		.map_err(|_| CfdError::InvalidInput("DG2 stamp allocation"))?;
	stamps.resize(slab, usize::MAX);
	let mut touched = Vec::new();
	touched
		.try_reserve_exact(slab)
		.map_err(|_| CfdError::InvalidInput("DG2 touched allocation"))?;
	let mut max_row = 0.0_f64;
	for cell in 0..cells {
		for (i, coefficients) in q.iter().enumerate() {
			for coordinate in 0..dimension {
				let row = cell * slab + i * dimension + coordinate;
				let local_row = i * dimension + coordinate;
				touched.clear();
				stamps[local_row] = row;
				values[local_row] = (point(-1.0)?, zero);
				touched.push(local_row);
				for (j, coefficient) in coefficients.iter().enumerate() {
					let multiplier = point(*coefficient)?;
					for (column, value) in operator.row(cell * slab + j * dimension + coordinate) {
						if column / slab != cell {
							continue;
						}
						let local = column % slab;
						if stamps[local] != row {
							stamps[local] = row;
							values[local] = (zero, zero);
							touched.push(local);
						}
						let (real, imag) = &mut values[local];
						*real = real.checked_add(multiplier.checked_mul(point(value.re)?)?)?;
						*imag = imag.checked_add(multiplier.checked_mul(point(value.im)?)?)?;
					}
				}
				let mut sum = zero;
				for &column in &touched {
					let (real, imag) = values[column];
					let modulus = real.square()?.checked_add(imag.square()?)?.sqrt()?;
					sum = sum.checked_add(modulus)?;
					let global = cell * slab + column;
					columns[global] = columns[global].checked_add(modulus)?;
				}
				max_row = max_row.max(sum.upper());
			}
		}
	}
	let max_column = columns
		.iter()
		.map(|value| value.upper())
		.fold(0.0, f64::max);
	let delta = point(max_row)?.checked_mul(point(max_column)?)?.sqrt()?;
	let gap = point(1.0)?.checked_sub(delta)?;
	if gap.lower() <= 0.0 {
		return Ok(None);
	}
	let inverse = point(3.0)?.checked_div(gap)?;
	let mut term = inverse;
	let mut total = zero;
	for step in 0..cells {
		total = total.checked_add(term)?;
		if step + 1 < cells {
			term = term.checked_mul(inverse)?;
		}
	}
	Ok(Some(point(1.0)?.checked_div(total)?.lower()))
}
