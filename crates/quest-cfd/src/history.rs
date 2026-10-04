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

/// Global history of mass-weighted configuration amplitudes.
#[derive(Clone, Debug)]
pub struct HistorySystem {
	operator: SparseMatrix,
	rhs: Vec<Complex64>,
	configuration_dimension: usize,
	times: Vec<f64>,
	temporal_weights: Vec<f64>,
	spectral_lower: Option<f64>,
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
		if generator.rows() != generator.cols()
			|| generator.rows() != initial.len()
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
		let n = generator.rows();
		let temporal = cells
			.checked_mul(q)
			.ok_or(CfdError::InvalidInput("temporal dimension overflow"))?;
		let total = n
			.checked_mul(temporal)
			.ok_or(CfdError::InvalidInput("history dimension overflow"))?;
		let entries = total
			.checked_mul(q + 1)
			.and_then(|v| v.checked_add(temporal.checked_mul(generator.nnz())?))
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
		let side_bytes = generator
			.retained_bytes()?
			.checked_add(size_of::<Self>())
			.and_then(|v| v.checked_add(initial.len().checked_mul(size_of::<Complex64>())?))
			.and_then(|v| v.checked_add(total.checked_mul(size_of::<Complex64>())?))
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
		for cell in 0..cells {
			for a in 0..q {
				let block = cell * q + a;
				let mass = 0.5 * dt * element.weights[a];
				times.push(dt * ((cell as f64) + 0.5 * (element.nodes[a] + 1.0)));
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
				for (i, j, value) in generator.entries() {
					triplets.push((block * n + i, block * n + j, -mass * value));
				}
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
			None
		};
		Ok(Self {
			operator,
			rhs,
			configuration_dimension: n,
			times,
			temporal_weights,
			spectral_lower,
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
	/// Initial trace source; only the first temporal test function has a source.
	#[must_use]
	pub fn rhs(&self) -> &[Complex64] {
		&self.rhs
	}
	/// Number of configuration coefficients per temporal node.
	#[must_use]
	pub const fn configuration_dimension(&self) -> usize {
		self.configuration_dimension
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
	/// Analytic p=1 singular-value bound for the exact stored global operator.
	/// Each diagonal slab has Hermitian part at least c I. The causal inverse is
	/// bounded by sum_{k=1}^{cells} c^(-k); the upper bound uses sparse row/column norms.
	/// No dense SVD or normal equations are used.
	/// # Errors
	/// Order two currently requires external spectral evidence; insufficient coercivity fails.
	pub fn spectral_bounds(
		&self,
		limits: SparseLimits,
	) -> Result<quest_qsvt::reciprocal::SpectralBounds, CfdError> {
		let lower=self.spectral_lower.ok_or_else(||CfdError::Unsupported("analytic temporal DG1 spectral bound unavailable: requires order one, positive slab coercivity and a finite representable causal inverse bound; external spectral evidence required".to_owned()))?;
		let upper = self.operator.norms(limits)?.spectral_upper_bound;
		Ok(quest_qsvt::reciprocal::SpectralBounds::new(lower,upper,quest_qsvt::reciprocal::SpectralEvidence::Analytic {description:"Temporal DG1 slab coercivity and finite causal inverse series, with outward sparse Hermitian-defect bound".to_owned()})?)
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
		.and_then(|n| {
			n.checked_add(
				operator
					.nnz()
					.checked_mul(size_of::<Complex64>() + size_of::<usize>())?,
			)
		})
		.and_then(|n| n.checked_add(operator.indptr().len().checked_mul(size_of::<usize>())?))
		.ok_or(CfdError::InvalidInput("spectral proof bytes overflow"))?;
	if work > limits.max_work || bytes > limits.max_bytes {
		return Err(CfdError::InvalidInput(
			"spectral proof exceeds sparse work/storage admission",
		));
	}
	let point = Interval::point;
	let mut rows = vec![point(0.0)?; operator.rows()];
	for (row, col, value) in operator.entries() {
		if row / slab != col / slab {
			continue;
		}
		let reverse = operator
			.row(col)
			.find(|(j, _)| *j == row)
			.map(|(_, z)| z.conj());
		let other = reverse.unwrap_or(Complex64::new(0.0, 0.0));
		let real = point(value.re)?
			.checked_add(point(other.re)?)?
			.checked_sub(point(f64::from(row == col))?)?;
		let imag = point(value.im)?.checked_add(point(other.im)?)?;
		let bound = real
			.square()?
			.checked_add(imag.square()?)?
			.sqrt()?
			.checked_mul(point(0.5)?)?;
		rows[row] = rows[row].checked_add(bound)?;
		if reverse.is_none() {
			rows[col] = rows[col].checked_add(bound)?;
		}
	}
	// Every DG1 diagonal is nonzero in assembled history; include missing diagonals defensively.
	for (row, sum) in rows.iter_mut().enumerate() {
		if !operator.row(row).any(|(col, _)| col == row) {
			*sum = sum.checked_add(point(0.5)?)?;
		}
	}
	let defect = rows.iter().map(|v| v.upper()).fold(0.0, f64::max);
	let c = point(0.5)?.checked_sub(point(defect)?)?.lower();
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
