use crate::{Complex64, Error, IoPolicy, Result, finite};
use faer::Mat;
use std::ops::Add;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SparseFormat {
    #[default]
    Csr,
    Csc,
}
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
#[derive(Debug, Clone)]
pub struct SparseMatrix {
    rows: usize,
    cols: usize,
    format: SparseFormat,
    data: Vec<Complex64>,
    indices: Vec<usize>,
    indptr: Vec<usize>,
}
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
    /// Densify in declared storage order, summing duplicates deterministically.
    ///
    /// # Errors
    /// Rejects excessive dense storage or a nonfinite accumulated coefficient.
    pub fn densify(&self, policy: IoPolicy) -> Result<Mat<Complex64>> {
        let count = self
            .rows
            .checked_next_multiple_of(4)
            .and_then(|r| r.checked_mul(self.cols))
            .ok_or(Error::Budget("dense shape"))?;
        policy.check(count, 1)?;
        let mut matrix = Mat::new();
        matrix
            .try_reserve(self.rows, self.cols)
            .map_err(|_| Error::Budget("dense allocation"))?;
        matrix.resize_with(self.rows, self.cols, |_, _| Complex64::new(0.0, 0.0));
        for (major, range) in self.indptr.windows(2).enumerate() {
            let [start, end] = range else {
                return Err(Error::Format("sparse pointer pair"));
            };
            for entry in *start..*end {
                let minor = *self
                    .indices
                    .get(entry)
                    .ok_or(Error::Format("sparse index"))?;
                let value = *self.data.get(entry).ok_or(Error::Format("sparse value"))?;
                let (row, col) = match self.format {
                    SparseFormat::Csr => (major, minor),
                    SparseFormat::Csc => (minor, major),
                };
                matrix[(row, col)] = finite(matrix[(row, col)].add(value))?;
            }
        }
        Ok(matrix)
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
    pub fn build(mut self, policy: IoPolicy) -> Result<SparseMatrix> {
        if self.rows == 0
            || self.cols == 0
            || self.rows > policy.max_dimension
            || self.cols > policy.max_dimension
        {
            return Err(Error::Format("sparse dimensions"));
        }
        let entries = self.state.data.len();
        policy.check_sparse_storage(entries, self.state.indptr.len())?;
        policy.check_sparse_retained(
            self.state.data.capacity(),
            self.state.indices.capacity(),
            self.state.indptr.capacity(),
        )?;
        if self.state.indices.len() != entries {
            return Err(Error::Format("sparse data/index lengths"));
        }
        for &value in &self.state.data {
            finite(value)?;
        }
        let (major, minor) = match self.format {
            SparseFormat::Csr => (self.rows, self.cols),
            SparseFormat::Csc => (self.cols, self.rows),
        };
        if self.state.indptr.len()
            != major
                .checked_add(1)
                .ok_or(Error::Budget("sparse pointer size"))?
        {
            return Err(Error::Format("sparse pointer length"));
        }
        for index in self.state.indices.iter_mut().chain(&mut self.state.indptr) {
            *index = index
                .checked_sub(self.index_base)
                .ok_or(Error::Format("sparse index base"))?;
        }
        if self.state.indptr.first() != Some(&0)
            || self.state.indptr.last() != Some(&entries)
            || self
                .state
                .indptr
                .windows(2)
                .any(|pair| matches!(pair,[a,b] if a>b))
            || self.state.indices.iter().any(|&index| index >= minor)
        {
            return Err(Error::Format("sparse index or pointer range"));
        }
        Ok(SparseMatrix {
            rows: self.rows,
            cols: self.cols,
            format: self.format,
            data: self.state.data,
            indices: self.state.indices,
            indptr: self.state.indptr,
        })
    }
}
