//! Canonical compressed sparse matrices, independent of file formats and runtimes.
use crate::{
	Complex64, Error, Interval, Result,
	policy::{check_limit, finite, zeros},
};

/// Compressed storage orientation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SparseFormat {
	/// Compressed sparse rows.
	#[default]
	Csr,
	/// Compressed sparse columns.
	Csc,
}

/// Independent sparse construction and operation admission limits.
#[derive(Clone, Copy, Debug)]
pub struct SparseLimits {
	/// Maximum number of rows or columns.
	pub max_dimension: usize,
	/// Maximum input or retained nonzero count.
	pub max_entries: usize,
	/// Peak wrapper-owned storage, including retained input capacities.
	pub max_bytes: usize,
	/// Conservative deterministic work model, including validation and sorting.
	/// Construction uses 16*entries*(floor(log2(entries))+2)+8*pointers
	/// units; these are admission units, not measured machine instructions.
	pub max_work: usize,
}
impl Default for SparseLimits {
	fn default() -> Self {
		Self {
			max_dimension: 1_048_576,
			max_entries: 1_048_576,
			max_bytes: 536_870_912,
			max_work: usize::MAX,
		}
	}
}

/// Upper enclosures of entry, induced row/column, and spectral norms.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SparseNorms {
	/// Largest entry magnitude.
	pub max_abs: f64,
	/// Maximum absolute row sum (infinity norm).
	pub max_row_sum: f64,
	/// Maximum absolute column sum (one norm).
	pub max_col_sum: f64,
	/// sqrt(infinity norm * one norm), rounded outward.
	pub spectral_upper_bound: f64,
}

/// Builder state before compressed storage is supplied.
#[derive(Debug)]
pub struct MissingEntries;
/// Builder state containing caller-owned compressed buffers.
#[derive(Debug)]
pub struct SuppliedEntries {
	data: Vec<Complex64>,
	indices: Vec<usize>,
	indptr: Vec<usize>,
}
/// Validating compressed storage builder.
#[derive(Debug)]
pub struct SparseMatrixBuilder<S = MissingEntries> {
	rows: usize,
	cols: usize,
	format: SparseFormat,
	index_base: usize,
	state: S,
}
/// Immutable canonical sparse matrix: zero based, sorted, duplicate free, no explicit zeros.
///
/// Duplicate coefficients are summed in original input order in binary64.
/// Canonicalization therefore preserves deterministic floating-point semantics,
/// rather than claiming exact arithmetic for input duplicates.
#[derive(Clone, Debug)]
pub struct SparseMatrix {
	rows: usize,
	cols: usize,
	format: SparseFormat,
	data: Vec<Complex64>,
	indices: Vec<usize>,
	indptr: Vec<usize>,
}

fn bytes(values: usize, indices: usize, pointers: usize) -> Result<usize> {
	values
		.checked_mul(size_of::<Complex64>())
		.and_then(|n| {
			indices
				.checked_mul(size_of::<usize>())
				.and_then(|i| n.checked_add(i))
		})
		.and_then(|n| {
			pointers
				.checked_mul(size_of::<usize>())
				.and_then(|p| n.checked_add(p))
		})
		.ok_or(Error::Overflow)
}
fn reserve<T>(count: usize) -> Result<Vec<T>> {
	let mut result = Vec::new();
	result
		.try_reserve_exact(count)
		.map_err(|_| Error::Allocation)?;
	Ok(result)
}
fn admit_bytes(count: usize, limits: SparseLimits) -> Result<()> {
	isize::try_from(count).map_err(|_| Error::Overflow)?;
	check_limit("sparse bytes", count, limits.max_bytes)
}
fn dimensions(rows: usize, cols: usize, limits: SparseLimits) -> Result<()> {
	if rows == 0 || cols == 0 {
		return Err(Error::Length("sparse dimensions must be positive"));
	}
	check_limit("sparse dimension", rows.max(cols), limits.max_dimension)
}
fn sorting_work(entries: usize, pointers: usize) -> Result<usize> {
	let levels = usize::try_from(entries.checked_ilog2().unwrap_or(0))
		.map_err(|_| Error::Overflow)?
		.checked_add(2)
		.ok_or(Error::Overflow)?;
	entries
		.checked_mul(levels)
		.and_then(|n| n.checked_mul(16))
		.and_then(|n| pointers.checked_mul(8).and_then(|p| n.checked_add(p)))
		.ok_or(Error::Overflow)
}
impl SparseMatrix {
	/// Start a compressed row builder.
	#[must_use]
	pub const fn builder(rows: usize, cols: usize) -> SparseMatrixBuilder {
		SparseMatrixBuilder {
			rows,
			cols,
			format: SparseFormat::Csr,
			index_base: 0,
			state: MissingEntries,
		}
	}
	/// Number of rows.
	#[must_use]
	pub const fn rows(&self) -> usize {
		self.rows
	}
	/// Number of columns.
	#[must_use]
	pub const fn cols(&self) -> usize {
		self.cols
	}
	/// Number of retained nonzero entries.
	#[must_use]
	pub const fn nnz(&self) -> usize {
		self.data.len()
	}
	/// Storage orientation.
	#[must_use]
	pub const fn format(&self) -> SparseFormat {
		self.format
	}
	/// Compressed values.
	#[must_use]
	pub fn data(&self) -> &[Complex64] {
		&self.data
	}
	/// Sorted zero-based minor indices.
	#[must_use]
	pub fn indices(&self) -> &[usize] {
		&self.indices
	}
	/// Zero-based segment pointers.
	#[must_use]
	pub fn indptr(&self) -> &[usize] {
		&self.indptr
	}
	/// Stored segment, with matching indices and coefficients.
	#[must_use]
	pub fn major_segment(&self, major: usize) -> Option<(&[usize], &[Complex64])> {
		let start = *self.indptr.get(major)?;
		let end = *self.indptr.get(major.checked_add(1)?)?;
		Some((self.indices.get(start..end)?, self.data.get(start..end)?))
	}
	/// Iterate canonical coefficients in declared compressed order.
	pub fn entries(&self) -> impl Iterator<Item = (usize, usize, Complex64)> + '_ {
		self.indptr
			.windows(2)
			.enumerate()
			.flat_map(move |(major, _)| {
				self.major_segment(major)
					.into_iter()
					.flat_map(move |(indices, values)| {
						indices.iter().copied().zip(values.iter().copied()).map(
							move |(minor, value)| match self.format {
								SparseFormat::Csr => (major, minor, value),
								SparseFormat::Csc => (minor, major, value),
							},
						)
					})
			})
	}
	/// Iterate a row as (column, value). Opposite-orientation access scans stored entries.
	pub fn row(&self, row: usize) -> impl Iterator<Item = (usize, Complex64)> + '_ {
		(self.format == SparseFormat::Csr)
			.then(|| self.major_segment(row))
			.flatten()
			.into_iter()
			.flat_map(|(indices, values)| indices.iter().copied().zip(values.iter().copied()))
			.chain(
				(self.format == SparseFormat::Csc)
					.then(|| self.entries())
					.into_iter()
					.flatten()
					.filter_map(move |(r, c, v)| (r == row).then_some((c, v))),
			)
	}
	/// Iterate a column as (row, value). Opposite-orientation access scans stored entries.
	pub fn column(&self, col: usize) -> impl Iterator<Item = (usize, Complex64)> + '_ {
		(self.format == SparseFormat::Csc)
			.then(|| self.major_segment(col))
			.flatten()
			.into_iter()
			.flat_map(|(indices, values)| indices.iter().copied().zip(values.iter().copied()))
			.chain(
				(self.format == SparseFormat::Csr)
					.then(|| self.entries())
					.into_iter()
					.flatten()
					.filter_map(move |(r, c, v)| (c == col).then_some((r, v))),
			)
	}
	/// Exact retained coefficient/index/pointer capacities, excluding inline metadata and allocator bookkeeping.
	/// # Errors
	/// Rejects byte-count overflow.
	pub fn retained_bytes(&self) -> Result<usize> {
		bytes(
			self.data.capacity(),
			self.indices.capacity(),
			self.indptr.capacity(),
		)
	}
	fn admit(&self, scratch_bytes: usize, work: usize, limits: SparseLimits) -> Result<()> {
		dimensions(self.rows, self.cols, limits)?;
		check_limit("sparse entries", self.nnz(), limits.max_entries)?;
		admit_bytes(
			self.retained_bytes()?
				.checked_add(scratch_bytes)
				.ok_or(Error::Overflow)?,
			limits,
		)?;
		check_limit("sparse work", work, limits.max_work)
	}
	/// Construct from zero-based (row, column, value) triplets.
	///
	/// # Errors
	/// Rejects malformed coordinates, nonfinite coefficients, overflow, allocation and budgets.
	pub fn from_triplets(
		rows: usize,
		cols: usize,
		format: SparseFormat,
		entries: Vec<(usize, usize, Complex64)>,
		limits: SparseLimits,
	) -> Result<Self> {
		dimensions(rows, cols, limits)?;
		check_limit("sparse entries", entries.len(), limits.max_entries)?;
		let major = match format {
			SparseFormat::Csr => rows,
			SparseFormat::Csc => cols,
		};
		let pointers = major.checked_add(1).ok_or(Error::Overflow)?;
		let retained = entries
			.capacity()
			.checked_mul(size_of::<(usize, usize, Complex64)>())
			.ok_or(Error::Overflow)?;
		// Triplets coexist with compressed buffers and canonicalization scratch/output.
		let compressed = bytes(entries.len(), entries.len(), pointers)?;
		let canonical_scratch = entries
			.len()
			.checked_mul(size_of::<(usize, usize, Complex64)>())
			.ok_or(Error::Overflow)?;
		let peak = compressed
			.checked_mul(2)
			.and_then(|n| n.checked_add(retained))
			.and_then(|n| n.checked_add(canonical_scratch))
			.ok_or(Error::Overflow)?;
		admit_bytes(peak, limits)?;
		let work = sorting_work(entries.len(), pointers)?
			.checked_mul(2)
			.ok_or(Error::Overflow)?;
		check_limit("sparse work", work, limits.max_work)?;
		for (index, &(row, col, value)) in entries.iter().enumerate() {
			if row >= rows || col >= cols {
				return Err(Error::Length("sparse triplet coordinate"));
			}
			if !value.re.is_finite() || !value.im.is_finite() {
				return Err(Error::NonFinite { index });
			}
		}
		let mut data = reserve(entries.len())?;
		data.resize(entries.len(), Complex64::new(0.0, 0.0));
		let mut indices = reserve(entries.len())?;
		indices.resize(entries.len(), 0);
		let mut indptr = reserve(pointers)?;
		indptr.resize(pointers, 0_usize);
		for &(r, c, _) in &entries {
			let m = match format {
				SparseFormat::Csr => r,
				SparseFormat::Csc => c,
			};
			let count = indptr
				.get_mut(m.checked_add(1).ok_or(Error::Overflow)?)
				.ok_or(Error::Length("sparse triplet major"))?;
			*count = count.checked_add(1).ok_or(Error::Overflow)?;
		}
		let mut total = 0_usize;
		for pointer in &mut indptr {
			total = total.checked_add(*pointer).ok_or(Error::Overflow)?;
			*pointer = total;
		}
		let mut cursors = reserve(pointers)?;
		cursors.extend_from_slice(&indptr);
		for (r, c, value) in entries {
			let (major_index, minor) = match format {
				SparseFormat::Csr => (r, c),
				SparseFormat::Csc => (c, r),
			};
			let cursor = cursors
				.get_mut(major_index)
				.ok_or(Error::Length("sparse triplet cursor"))?;
			*data
				.get_mut(*cursor)
				.ok_or(Error::Length("sparse triplet scatter"))? = value;
			*indices
				.get_mut(*cursor)
				.ok_or(Error::Length("sparse triplet scatter"))? = minor;
			*cursor = cursor.checked_add(1).ok_or(Error::Overflow)?;
		}
		drop(cursors);
		Self::builder(rows, cols)
			.format(format)
			.entries(data, indices, indptr)
			.build(limits)
	}
	/// Return the conjugate transpose by swapping compressed orientation.
	///
	/// # Errors
	/// Rejects dimensions, storage or work beyond the supplied limits and allocation failures.
	pub fn adjoint(&self, limits: SparseLimits) -> Result<Self> {
		let output_bytes = bytes(self.nnz(), self.nnz(), self.indptr.len())?;
		self.admit(
			output_bytes,
			self.nnz()
				.checked_add(self.indptr.len())
				.ok_or(Error::Overflow)?,
			limits,
		)?;
		let mut data = reserve(self.nnz())?;
		let mut indices = reserve(self.nnz())?;
		let mut indptr = reserve(self.indptr.len())?;
		data.extend(self.data.iter().map(Complex64::conj));
		indices.extend_from_slice(&self.indices);
		indptr.extend_from_slice(&self.indptr);
		Ok(Self {
			rows: self.cols,
			cols: self.rows,
			format: match self.format {
				SparseFormat::Csr => SparseFormat::Csc,
				SparseFormat::Csc => SparseFormat::Csr,
			},
			data,
			indices,
			indptr,
		})
	}
	/// Multiply by a finite dense vector using canonical entry order.
	///
	/// # Errors
	/// Rejects shape, budgets, allocation failure, nonfinite input or arithmetic overflow.
	#[allow(
		clippy::arithmetic_side_effects,
		reason = "Complex arithmetic is checked for finite results at every accumulation"
	)]
	pub fn matvec(&self, input: &[Complex64], limits: SparseLimits) -> Result<Vec<Complex64>> {
		if input.len() != self.cols {
			return Err(Error::Length("sparse matvec input"));
		}
		let work = self
			.nnz()
			.checked_mul(8)
			.and_then(|n| n.checked_add(self.rows))
			.and_then(|n| n.checked_add(self.cols))
			.ok_or(Error::Overflow)?;
		self.admit(
			self.rows
				.checked_mul(size_of::<Complex64>())
				.ok_or(Error::Overflow)?,
			work,
			limits,
		)?;
		finite(input)?;
		let mut output = zeros(self.rows)?;
		for (row, col, value) in self.entries() {
			let x = *input
				.get(col)
				.ok_or(Error::Length("sparse matvec column"))?;
			let y = output
				.get_mut(row)
				.ok_or(Error::Length("sparse matvec row"))?;
			*y += value * x;
			if !y.re.is_finite() || !y.im.is_finite() {
				return Err(Error::NonFinite { index: row });
			}
		}
		Ok(output)
	}
	/// Compute outward-rounded norm bounds using interval arithmetic.
	///
	/// # Errors
	/// Rejects storage/work budgets, allocation failure and unrepresentable finite bounds.
	pub fn norms(&self, limits: SparseLimits) -> Result<SparseNorms> {
		let lengths = self.rows.checked_add(self.cols).ok_or(Error::Overflow)?;
		let scratch = lengths
			.checked_mul(size_of::<f64>())
			.ok_or(Error::Overflow)?;
		let work = self
			.nnz()
			.checked_mul(64)
			.and_then(|n| n.checked_add(lengths))
			.ok_or(Error::Overflow)?;
		self.admit(scratch, work, limits)?;
		let mut rows = reserve(self.rows)?;
		rows.resize(self.rows, 0.0_f64);
		let mut cols = reserve(self.cols)?;
		cols.resize(self.cols, 0.0_f64);
		let mut max_abs = 0.0_f64;
		for (row, col, value) in self.entries() {
			let scale = value.re.abs().max(value.im.abs());
			let scale_interval = Interval::point(scale)?;
			let re = Interval::point(value.re)?.checked_div(scale_interval)?;
			let im = Interval::point(value.im)?.checked_div(scale_interval)?;
			let magnitude = re
				.square()?
				.checked_add(im.square()?)?
				.sqrt()?
				.checked_mul(scale_interval)?
				.upper();
			max_abs = max_abs.max(magnitude);
			let r = rows.get_mut(row).ok_or(Error::Length("sparse norm row"))?;
			*r = Interval::point(*r)?
				.checked_add(Interval::point(magnitude)?)?
				.upper();
			let c = cols
				.get_mut(col)
				.ok_or(Error::Length("sparse norm column"))?;
			*c = Interval::point(*c)?
				.checked_add(Interval::point(magnitude)?)?
				.upper();
		}
		let max_row_sum = rows.into_iter().fold(0.0_f64, f64::max);
		let max_col_sum = cols.into_iter().fold(0.0_f64, f64::max);
		// sqrt each factor before multiplying to avoid overflow in row_sum*col_sum.
		let spectral_upper_bound = Interval::point(max_row_sum)?
			.sqrt()?
			.checked_mul(Interval::point(max_col_sum)?.sqrt()?)?
			.upper();
		Ok(SparseNorms {
			max_abs,
			max_row_sum,
			max_col_sum,
			spectral_upper_bound,
		})
	}
}
impl<S> SparseMatrixBuilder<S> {
	/// Choose row or column compressed storage.
	#[must_use]
	pub const fn format(mut self, format: SparseFormat) -> Self {
		self.format = format;
		self
	}
	/// Interpret supplied pointers and minor indices as one based.
	#[must_use]
	pub const fn one_based(mut self) -> Self {
		self.index_base = 1;
		self
	}
}
impl SparseMatrixBuilder {
	/// Supply compressed storage buffers, transferring ownership.
	#[must_use]
	pub const fn entries(
		self,
		data: Vec<Complex64>,
		indices: Vec<usize>,
		indptr: Vec<usize>,
	) -> SparseMatrixBuilder<SuppliedEntries> {
		SparseMatrixBuilder {
			rows: self.rows,
			cols: self.cols,
			format: self.format,
			index_base: self.index_base,
			state: SuppliedEntries {
				data,
				indices,
				indptr,
			},
		}
	}
}
impl SparseMatrixBuilder<SuppliedEntries> {
	fn validate(&mut self, limits: SparseLimits) -> Result<(usize, usize)> {
		dimensions(self.rows, self.cols, limits)?;
		let entries = self.state.data.len();
		check_limit("sparse entries", entries, limits.max_entries)?;
		if self.state.indices.len() != entries {
			return Err(Error::Length("sparse data/index lengths"));
		}
		let (major, minor) = match self.format {
			SparseFormat::Csr => (self.rows, self.cols),
			SparseFormat::Csc => (self.cols, self.rows),
		};
		let pointers = major.checked_add(1).ok_or(Error::Overflow)?;
		if self.state.indptr.len() != pointers {
			return Err(Error::Length("sparse pointer length"));
		}
		let retained = bytes(
			self.state.data.capacity(),
			self.state.indices.capacity(),
			self.state.indptr.capacity(),
		)?;
		admit_bytes(retained, limits)?;
		check_limit(
			"sparse work",
			sorting_work(entries, pointers)?,
			limits.max_work,
		)?;
		finite(&self.state.data)?;
		for index in self.state.indices.iter_mut().chain(&mut self.state.indptr) {
			*index = index
				.checked_sub(self.index_base)
				.ok_or(Error::Length("sparse index base"))?;
		}
		if self.state.indptr.first() != Some(&0)
			|| self.state.indptr.last() != Some(&entries)
			|| self
				.state
				.indptr
				.windows(2)
				.any(|pair| matches!(pair,[a,b] if a>b))
			|| self.state.indices.iter().any(|&i| i >= minor)
		{
			return Err(Error::Length("sparse index/pointer range"));
		}
		let longest = self
			.state
			.indptr
			.windows(2)
			.filter_map(|p| match p {
				[a, b] => b.checked_sub(*a),
				_ => None,
			})
			.max()
			.unwrap_or(0);
		Ok((retained, longest))
	}
	/// Validate and canonicalize compressed storage.
	///
	/// # Errors
	/// Rejects malformed shapes, indices/pointers, nonfinite duplicate sums,
	/// checked accounting overflow, excess peak storage/work or allocation failure.
	#[allow(
		clippy::arithmetic_side_effects,
		reason = "Duplicate complex sums are checked for finiteness after every addition"
	)]
	pub fn build(mut self, limits: SparseLimits) -> Result<SparseMatrix> {
		let (retained, longest) = self.validate(limits)?;
		let entries = self.state.data.len();
		let pointers = self.state.indptr.len();
		let scratch_bytes = longest
			.checked_mul(size_of::<(usize, usize, Complex64)>())
			.ok_or(Error::Overflow)?;
		let output_bytes = bytes(entries, entries, pointers)?;
		admit_bytes(
			retained
				.checked_add(output_bytes)
				.and_then(|n| n.checked_add(scratch_bytes))
				.ok_or(Error::Overflow)?,
			limits,
		)?;
		let mut data = reserve(entries)?;
		let mut indices = reserve(entries)?;
		let mut indptr = reserve(pointers)?;
		let mut scratch = reserve(longest)?;
		for pair in self.state.indptr.windows(2) {
			let [start, end] = pair else {
				return Err(Error::Length("sparse pointer pair"));
			};
			scratch.clear();
			scratch.extend(
				self.state
					.indices
					.get(*start..*end)
					.ok_or(Error::Length("sparse segment"))?
					.iter()
					.copied()
					.zip(
						self.state
							.data
							.get(*start..*end)
							.ok_or(Error::Length("sparse segment"))?
							.iter()
							.copied(),
					)
					.enumerate()
					.map(|(ordinal, (index, value))| (index, ordinal, value)),
			);
			scratch.sort_unstable_by_key(|&(index, ordinal, _)| (index, ordinal));
			indptr.push(data.len());
			let mut iter = scratch.iter().peekable();
			while let Some(&(index, _, mut sum)) = iter.next() {
				while iter.peek().is_some_and(|&&(next, _, _)| next == index) {
					if let Some(&(_, _, value)) = iter.next() {
						sum += value;
						if !sum.re.is_finite() || !sum.im.is_finite() {
							return Err(Error::NonFinite { index: data.len() });
						}
					}
				}
				if sum != Complex64::new(0.0, 0.0) {
					indices.push(index);
					data.push(sum);
				}
			}
		}
		indptr.push(data.len());
		Ok(SparseMatrix {
			rows: self.rows,
			cols: self.cols,
			format: self.format,
			data,
			indices,
			indptr,
		})
	}
}
