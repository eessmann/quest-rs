use crate::{
    Complex64, EncodingBuilder, Error, Left, LogicalSpace, Missing, Normalization, NumericalPolicy,
    ProjectedEncoding, Result, Right, matrix,
};
use faer::{
    Mat, MatRef, Par,
    dyn_stack::{MemBuffer, MemStack},
    mat::AsMatRef,
    traits::Conjugate,
};
use quest_circuit::{NumericalOperator, OracleFragment, QuantumRegionBuilder};
use std::ops::{Add, Div, Mul, Neg, Sub};

#[derive(Debug)]
pub struct DenseNormalization(Normalization);
/// A cold dense block-dilation builder. Logical row and column dimensions are
/// retained independently; zero padding never changes their interpretation.
/// ```compile_fail
/// # fn example(matrix: faer::MatRef<'_, quest_qsvt::Complex64>) -> quest_qsvt::Result<()> {
/// quest_qsvt::DenseEncodingBuilder::new(matrix, quest_qsvt::NumericalPolicy::default())?.build();
/// # Ok(()) }
/// ```
#[derive(Debug)]
pub struct DenseEncodingBuilder<A = Missing> {
    matrix: Mat<Complex64>,
    normalization: A,
    policy: NumericalPolicy,
}
impl DenseEncodingBuilder {
    /// Snapshot logical values, including strided or conjugated views.
    /// # Errors
    /// Rejects empty/nonfinite matrices and construction allocation limits.
    pub fn new<T: Conjugate<Canonical = Complex64>>(
        matrix: impl AsMatRef<T = T, Rows = usize, Cols = usize>,
        policy: NumericalPolicy,
    ) -> Result<Self> {
        let matrix = matrix.as_mat_ref();
        if matrix.nrows() == 0 || matrix.ncols() == 0 {
            return Err(Error::Encoding("dense block must be nonempty"));
        }
        Ok(Self {
            matrix: matrix::snapshot(matrix, policy)?,
            normalization: Missing,
            policy,
        })
    }
    /// # Errors
    /// Rejects nonpositive and nonfinite physical normalization.
    pub fn normalization(self, alpha: f64) -> Result<DenseEncodingBuilder<DenseNormalization>> {
        Ok(DenseEncodingBuilder {
            matrix: self.matrix,
            normalization: DenseNormalization(Normalization::new(alpha)?),
            policy: self.policy,
        })
    }
}
impl DenseEncodingBuilder<DenseNormalization> {
    /// Construct the Julia dilation of `A/alpha`, with sequential faer kernels.
    /// All PSD roots and whole-oracle unitarity are numerically checked.
    /// # Errors
    /// Rejects insufficient normalization, failed eigensolvers, residuals and budgets.
    pub fn build(self) -> Result<ProjectedEncoding> {
        let size = self
            .matrix
            .nrows()
            .max(self.matrix.ncols())
            .checked_next_power_of_two()
            .ok_or(Error::Budget("dense padding"))?;
        let dimension = size
            .checked_mul(2)
            .ok_or(Error::Budget("dilation dimension"))?;
        self.policy.check(dimension, dimension, 12)?;
        let alpha = self.normalization.0.get();
        let block = matrix::allocate(size, size, self.policy, |row, col| {
            if row < self.matrix.nrows() && col < self.matrix.ncols() {
                self.matrix[(row, col)].div(alpha)
            } else {
                Complex64::new(0.0, 0.0)
            }
        })?;
        let left_gram = matrix::multiply(block.as_ref(), block.adjoint(), self.policy)?;
        let right_gram = matrix::multiply(block.adjoint(), block.as_ref(), self.policy)?;
        let left_root = complement_root(left_gram.as_ref(), self.policy)?;
        let right_root = complement_root(right_gram.as_ref(), self.policy)?;
        let dilation = matrix::allocate(dimension, dimension, self.policy, |row, col| {
            match (row < size, col < size) {
                (true, true) => block[(row, col)],
                (true, false) => left_root[(row, col.saturating_sub(size))],
                (false, true) => right_root[(row.saturating_sub(size), col)],
                (false, false) => block[(col.saturating_sub(size), row.saturating_sub(size))]
                    .conj()
                    .neg(),
            }
        })?;
        let width =
            usize::try_from(dimension.ilog2()).map_err(|_| Error::Budget("dilation width"))?;
        let mut body = QuantumRegionBuilder::new(width, 0)?;
        let targets = (0..width)
            .map(|index| body.qubit(index))
            .collect::<quest_circuit::Result<Vec<_>>>()?;
        body.numerical(
            NumericalOperator::from_view(dilation.as_ref(), self.policy.matrix_policy())?,
            &targets,
            &[],
        )?;
        let oracle = OracleFragment::builder(body.finish()?.bind(&[])?)
            .matrix_tolerance(1e-12)?
            .matrix_policy(self.policy.matrix_policy())
            .build()?;
        EncodingBuilder::new()
            .oracle(oracle)
            .left(LogicalSpace::<Left>::coordinates(
                dimension,
                &(0..self.matrix.nrows()).collect::<Vec<_>>(),
                self.policy,
            )?)
            .right(LogicalSpace::<Right>::coordinates(
                dimension,
                &(0..self.matrix.ncols()).collect::<Vec<_>>(),
                self.policy,
            )?)
            .normalization(alpha)?
            .policy(self.policy)
            .build()
    }
}

fn complement_root(gram: MatRef<'_, Complex64>, policy: NumericalPolicy) -> Result<Mat<Complex64>> {
    let size = gram.nrows();
    let complement = matrix::allocate(size, size, policy, |row, col| {
        Complex64::new(if row == col { 1.0 } else { 0.0 }, 0.0).sub(gram[(row, col)])
    })?;
    let mut eigenvalues = matrix::allocate(size, 1, policy, |_, _| Complex64::new(0.0, 0.0))?;
    let mut vectors = matrix::allocate(size, size, policy, |_, _| Complex64::new(0.0, 0.0))?;
    let requirement = faer::linalg::evd::self_adjoint_evd_scratch::<Complex64>(
        size,
        faer::linalg::evd::ComputeEigenvectors::Yes,
        Par::Seq,
        faer::Spec::default(),
    );
    let storage = policy.check(size, size, 12)?;
    if requirement
        .size_bytes()
        .checked_add(
            storage
                .checked_mul(12)
                .ok_or(Error::Budget("eigensolver storage"))?,
        )
        .is_none_or(|bytes| bytes > policy.max_bytes)
    {
        return Err(Error::Budget("eigensolver scratch"));
    }
    let mut scratch = MemBuffer::try_new(requirement)
        .map_err(|_| Error::Budget("eigensolver scratch allocation"))?;
    faer::linalg::evd::self_adjoint_evd(
        complement.as_ref(),
        eigenvalues.as_mut().col_mut(0).as_diagonal_mut(),
        Some(vectors.as_mut()),
        Par::Seq,
        MemStack::new(&mut scratch),
        faer::Spec::default(),
    )
    .map_err(|_| Error::Encoding("PSD eigensolver did not converge"))?;
    for index in 0..size {
        let value = eigenvalues[(index, 0)].re;
        if !value.is_finite() || value < -1e-12 {
            return Err(Error::Encoding("normalization does not give a contraction"));
        }
        eigenvalues[(index, 0)] = Complex64::new(value.max(0.0).sqrt(), 0.0);
    }
    matrix::allocate(size, size, policy, |row, col| {
        (0..size).fold(Complex64::new(0.0, 0.0), |total, index| {
            total.add(
                vectors[(row, index)]
                    .mul(eigenvalues[(index, 0)])
                    .mul(vectors[(col, index)].conj()),
            )
        })
    })
}
