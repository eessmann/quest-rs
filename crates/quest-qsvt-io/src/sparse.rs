use crate::{Complex64, Error, IoPolicy, Result};
use faer::Mat;

pub use quest_numerics::SparseFormat;
#[derive(Debug)]
pub struct MissingEntries;
#[derive(Debug)]
pub struct SuppliedEntries {
	data: Vec<Complex64>,
	indices: Vec<usize>,
	indptr: Vec<usize>,
}
#[derive(Debug)]
pub struct SparseMatrixBuilder<S = MissingEntries> {
	rows: usize,
	cols: usize,
	format: SparseFormat,
	index_base: usize,
	state: S,
}
/// Compatibility adapter around the reusable canonical numerical matrix.
#[derive(Debug, Clone)]
pub struct SparseMatrix(quest_numerics::SparseMatrix);
impl SparseMatrix {
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
	/// Borrow reusable canonical storage without densification.
	#[must_use]
	pub const fn as_numerics(&self) -> &quest_numerics::SparseMatrix {
		&self.0
	}
	/// Transfer the canonical matrix into numerical or encoding code.
	#[must_use]
	pub fn into_numerics(self) -> quest_numerics::SparseMatrix {
		self.0
	}
	/// Wrap an already validated canonical matrix.
	#[must_use]
	pub const fn from_numerics(matrix: quest_numerics::SparseMatrix) -> Self {
		Self(matrix)
	}
	/// Densify canonical entries under the existing I/O admission policy.
	///
	/// # Errors
	/// Rejects excessive dense storage or allocation failure.
	pub fn densify(&self, policy: IoPolicy) -> Result<Mat<Complex64>> {
		let count = self
			.0
			.rows()
			.checked_next_multiple_of(4)
			.and_then(|r| r.checked_mul(self.0.cols()))
			.ok_or(Error::Budget("dense shape"))?;
		policy.check(count, 1)?;
		let mut matrix = Mat::new();
		matrix
			.try_reserve(self.0.rows(), self.0.cols())
			.map_err(|_| Error::Budget("dense allocation"))?;
		matrix.resize_with(self.0.rows(), self.0.cols(), |_, _| {
			Complex64::new(0.0, 0.0)
		});
		for (row, col, value) in self.0.entries() {
			matrix[(row, col)] = value;
		}
		Ok(matrix)
	}
}
const fn numerical_error(error: &quest_numerics::Error) -> Error {
	match error {
		quest_numerics::Error::NonFinite { .. } => Error::NonFinite,
		quest_numerics::Error::Budget { .. }
		| quest_numerics::Error::Overflow
		| quest_numerics::Error::Allocation => Error::Budget("sparse numerics"),
		_ => Error::Format("invalid sparse matrix"),
	}
}
impl<S> SparseMatrixBuilder<S> {
	#[must_use]
	pub const fn format(mut self, format: SparseFormat) -> Self {
		self.format = format;
		self
	}
	#[must_use]
	pub const fn one_based(mut self) -> Self {
		self.index_base = 1;
		self
	}
}
impl SparseMatrixBuilder {
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
	/// Normalize the index base and validate every compressed segment.
	///
	/// # Errors
	/// Rejects invalid shapes, indices, pointers, nonfinite data and budgets.
	pub fn build(self, policy: IoPolicy) -> Result<SparseMatrix> {
		if self.rows == 0
			|| self.cols == 0
			|| self.rows > policy.max_dimension
			|| self.cols > policy.max_dimension
		{
			return Err(Error::Format("sparse dimensions"));
		}
		policy.check_sparse_storage(self.state.data.len(), self.state.indptr.len())?;
		policy.check_sparse_retained(
			self.state.data.capacity(),
			self.state.indices.capacity(),
			self.state.indptr.capacity(),
		)?;
		let builder =
			quest_numerics::SparseMatrix::builder(self.rows, self.cols).format(self.format);
		let builder = if self.index_base == 1 {
			builder.one_based()
		} else {
			builder
		};
		builder
			.entries(self.state.data, self.state.indices, self.state.indptr)
			.build(quest_numerics::SparseLimits {
				max_dimension: policy.max_dimension,
				max_entries: policy.max_coefficients,
				max_bytes: policy.max_bytes,
				max_work: usize::MAX,
			})
			.map(SparseMatrix)
			.map_err(|error| numerical_error(&error))
	}
}
