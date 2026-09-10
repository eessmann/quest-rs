use crate::{BoundGate, Error, Result};
use faer::{Accum, Mat, MatRef, Par, mat::AsMatRef, traits::Conjugate};
use num_complex::Complex64;
use std::sync::Arc;

#[derive(Debug, Clone, Copy)]
pub struct MatrixPolicy {
    pub max_bytes: usize,
}
impl Default for MatrixPolicy {
    fn default() -> Self {
        Self {
            max_bytes: 64 * 1024 * 1024,
        }
    }
}
impl MatrixPolicy {
    pub(crate) fn check(self, dim: usize, copies: usize) -> Result<usize> {
        // faer 0.24 aligns POD columns to 64 bytes (four Complex64 values).
        let rows = dim
            .checked_next_multiple_of(4)
            .ok_or(Error::Budget("matrix column padding"))?;
        let bytes = rows
            .checked_mul(dim)
            .and_then(|n| n.checked_mul(size_of::<Complex64>()))
            .filter(|n| isize::try_from(*n).is_ok())
            .ok_or(Error::Budget("matrix dimension overflow"))?;
        if bytes
            .checked_mul(copies)
            .ok_or(Error::Budget("matrix scratch overflow"))?
            > self.max_bytes
        {
            return Err(Error::Budget("matrix bytes"));
        }
        Ok(bytes)
    }
}

fn allocate(
    dim: usize,
    policy: MatrixPolicy,
    value: impl FnMut(usize, usize) -> Complex64,
) -> Result<Mat<Complex64>> {
    policy.check(dim, 1)?;
    let mut matrix = Mat::new();
    matrix
        .try_reserve(dim, dim)
        .map_err(|_| Error::Budget("matrix allocation failed"))?;
    // The reservation above makes resize initialization allocation-free.
    matrix.resize_with(dim, dim, value);
    Ok(matrix)
}

// Column matvec kernels avoid hidden GEMM packing allocations. faer 0.24's
// sequential col-major/row-major matvec paths use no dynamic scratch. Inputs
// here are admitted owned column-major matrices, or their adjoint views.
fn multiply_into<T: Conjugate<Canonical = Complex64>>(
    out: &mut Mat<Complex64>,
    lhs: impl AsMatRef<T = T, Rows = usize, Cols = usize>,
    rhs: MatRef<'_, Complex64>,
    accum: Accum,
) {
    let lhs = lhs.as_mat_ref();
    for col in 0..rhs.ncols() {
        faer::linalg::matmul::matmul(
            out.as_mut().submatrix_mut(0, col, lhs.nrows(), 1),
            accum,
            lhs,
            rhs.submatrix(0, col, rhs.nrows(), 1),
            Complex64::new(1.0, 0.0),
            Par::Seq,
        );
    }
}

/// Immutable, finite numerical data. Approximate admission never makes this an
/// exact symbolic gate, and therefore this type deliberately has no `inverse`.
#[derive(Debug, Clone)]
pub struct NumericalOperator {
    matrix: Arc<Mat<Complex64>>,
    bytes: usize,
    num_qubits: usize,
    diagonal: bool,
}
impl NumericalOperator {
    /// # Errors
    /// Rejects invalid matrix shape, nonfinite entries, or a matrix exceeding the allocation budget.
    pub fn from_view<T: Conjugate<Canonical = Complex64>>(
        view: impl AsMatRef<T = T, Rows = usize, Cols = usize>,
        policy: MatrixPolicy,
    ) -> Result<Self> {
        let view = view.as_mat_ref();
        let dim = view.nrows();
        if dim == 0 || dim != view.ncols() || !dim.is_power_of_two() {
            return Err(Error::MatrixShape);
        }
        policy.check(dim, 1)?;
        let canonical = view.canonical();
        let conjugated = faer::Conj::get::<T>() == faer::Conj::Yes;
        let logical = |r, c| {
            let x = canonical[(r, c)];
            if conjugated { x.conj() } else { x }
        };
        // Invalid payloads are rejected before allocation; logical access keeps
        // transpose, padding, negative strides and conjugation meaningful.
        for col in 0..dim {
            for row in 0..dim {
                let x = logical(row, col);
                if !x.re.is_finite() || !x.im.is_finite() {
                    return Err(Error::NonFinite);
                }
            }
        }
        let matrix = allocate(dim, policy, logical)?;
        Self::from_owned(matrix)
    }
    fn from_owned(matrix: Mat<Complex64>) -> Result<Self> {
        for col in 0..matrix.ncols() {
            for row in 0..matrix.nrows() {
                let value = matrix[(row, col)];
                if !value.re.is_finite() || !value.im.is_finite() {
                    return Err(Error::NonFinite);
                }
            }
        }
        let bytes = usize::try_from(matrix.col_stride())
            .ok()
            .and_then(|stride| stride.checked_mul(matrix.ncols()))
            .and_then(|entries| entries.checked_mul(size_of::<Complex64>()))
            .ok_or(Error::Budget("matrix storage"))?;
        let num_qubits = usize::try_from(matrix.nrows().ilog2())
            .map_err(|_| Error::Budget("matrix qubit count"))?;
        let diagonal = (0..matrix.nrows()).all(|r| {
            (0..matrix.ncols()).all(|c| r == c || matrix[(r, c)] == Complex64::new(0.0, 0.0))
        });
        Ok(Self {
            diagonal,
            matrix: Arc::new(matrix),
            bytes,
            num_qubits,
        })
    }
    #[must_use]
    pub fn view(&self) -> MatRef<'_, Complex64> {
        self.matrix.as_ref().as_ref()
    }
    #[must_use]
    pub fn dimension(&self) -> usize {
        self.matrix.nrows()
    }
    #[must_use]
    pub const fn num_qubits(&self) -> usize {
        self.num_qubits
    }
    #[must_use]
    pub const fn bytes(&self) -> usize {
        self.bytes
    }
    /// Exact structural zeros, independent of approximate unitary admission.
    #[must_use]
    pub const fn is_diagonal(&self) -> bool {
        self.diagonal
    }

    /// Embed logical targets and signed controls into a larger ordered interface.
    /// # Errors
    /// Rejects overlap, dimensions, integer overflow and allocation budgets.
    pub(crate) fn embedded(
        &self,
        positions: &[usize],
        controls: &[(usize, bool)],
        width: usize,
        policy: MatrixPolicy,
    ) -> Result<Self> {
        let dimension = 1usize
            .checked_shl(u32::try_from(width).map_err(|_| Error::MatrixDimension)?)
            .ok_or(Error::MatrixDimension)?;
        if positions.len() != self.num_qubits {
            return Err(Error::MatrixDimension);
        }
        let mut mask = 0usize;
        for position in positions
            .iter()
            .copied()
            .chain(controls.iter().map(|(bit, _)| *bit))
        {
            if position >= width {
                return Err(Error::MatrixDimension);
            }
            let bit = 1usize
                .checked_shl(u32::try_from(position).map_err(|_| Error::MatrixDimension)?)
                .ok_or(Error::MatrixDimension)?;
            if mask & bit != 0 {
                return Err(Error::MatrixDimension);
            }
            mask |= bit;
        }
        let target_mask = positions.iter().try_fold(0usize, |mask, bit| {
            Ok(mask
                | 1usize
                    .checked_shl(u32::try_from(*bit).map_err(|_| Error::MatrixDimension)?)
                    .ok_or(Error::MatrixDimension)?)
        })?;
        // Width and positions were checked before allocation; closure arithmetic cannot overflow.
        let project = |basis: usize| {
            positions
                .iter()
                .enumerate()
                .fold(0usize, |local, (bit, position)| {
                    local | (((basis >> position) & 1) << bit)
                })
        };
        Self::from_owned(allocate(dimension, policy, |row, col| {
            if controls
                .iter()
                .all(|(bit, positive)| ((col >> bit) & 1) == usize::from(*positive))
            {
                if row & !target_mask == col & !target_mask {
                    self.view()[(project(row), project(col))]
                } else {
                    Complex64::new(0.0, 0.0)
                }
            } else {
                Complex64::new(f64::from(row == col), 0.0)
            }
        })?)
    }
    /// # Errors
    /// Returns an error if allocating the conjugate transpose exceeds the matrix budget.
    pub fn conjugate_transpose(&self, policy: MatrixPolicy) -> Result<Self> {
        Self::from_view(self.view().adjoint(), policy)
    }
    /// Returns self * rhs, using an explicitly sequential faer kernel.
    /// # Errors
    /// Rejects unequal dimensions, insufficient matrix budget, or nonfinite product entries.
    pub fn product(&self, rhs: &Self, policy: MatrixPolicy) -> Result<Self> {
        if self.dimension() != rhs.dimension() {
            return Err(Error::MatrixDimension);
        }
        policy.check(self.dimension(), 3)?;
        let mut out = allocate(self.dimension(), policy, |_, _| Complex64::new(0.0, 0.0))?;
        multiply_into(&mut out, self.view(), rhs.view(), Accum::Replace);
        Self::from_owned(out)
    }
    /// faer's norm reduction is sequential and does not consult global Par.
    #[must_use]
    pub fn frobenius_norm(&self) -> f64 {
        self.view().norm_l2()
    }
    #[expect(
        clippy::arithmetic_side_effects,
        reason = "Complex floating point subtraction cannot panic; residual finiteness is checked"
    )]
    /// # Errors
    /// Rejects insufficient matrix budget or a nonfinite residual.
    pub fn unitarity_residual(&self, policy: MatrixPolicy) -> Result<f64> {
        policy.check(self.dimension(), 2)?;
        let mut gram = allocate(self.dimension(), policy, |_, _| Complex64::new(0.0, 0.0))?;
        multiply_into(
            &mut gram,
            self.view().adjoint(),
            self.view(),
            Accum::Replace,
        );
        for i in 0..self.dimension() {
            gram[(i, i)] -= Complex64::new(1.0, 0.0);
        }
        let residual = gram.norm_l2();
        if !residual.is_finite() {
            return Err(Error::NonFinite);
        }
        Ok(residual)
    }
    /// # Errors
    /// Rejects invalid tolerance, excess residual, or insufficient matrix budget.
    pub fn check_unitary(&self, tolerance: f64, policy: MatrixPolicy) -> Result<UnitaryAdmission> {
        if !tolerance.is_finite() || tolerance < 0.0 {
            return Err(Error::NonFinite);
        }
        let residual = self.unitarity_residual(policy)?;
        if residual > tolerance {
            return Err(Error::Unitarity {
                residual,
                tolerance,
            });
        }
        Ok(UnitaryAdmission {
            residual,
            tolerance,
        })
    }
}

#[derive(Debug, Clone, Copy)]
pub struct UnitaryAdmission {
    pub residual: f64,
    pub tolerance: f64,
}

#[expect(
    clippy::redundant_pub_crate,
    reason = "Keep this helper out of the crate wildcard public re-export"
)]
#[expect(
    clippy::arithmetic_side_effects,
    reason = "Complex subtraction cannot panic; residual finiteness is checked"
)]
pub(crate) fn check_channel(
    kraus: &[NumericalOperator],
    tolerance: f64,
    policy: MatrixPolicy,
) -> Result<()> {
    if !tolerance.is_finite() || tolerance < 0.0 {
        return Err(Error::NonFinite);
    }
    let dim = kraus.first().ok_or(Error::MatrixShape)?.dimension();
    if kraus.iter().any(|x| x.dimension() != dim) {
        return Err(Error::MatrixDimension);
    }
    policy.check(
        dim,
        kraus
            .len()
            .checked_add(1)
            .ok_or(Error::Budget("channel operators"))?,
    )?;
    let mut sum = allocate(dim, policy, |_, _| Complex64::new(0.0, 0.0))?;
    for k in kraus {
        multiply_into(&mut sum, k.view().adjoint(), k.view(), Accum::Add);
    }
    for i in 0..dim {
        sum[(i, i)] -= Complex64::new(1.0, 0.0);
    }
    let residual = sum.norm_l2();
    if !residual.is_finite() {
        return Err(Error::NonFinite);
    }
    if residual > tolerance {
        return Err(Error::ChannelCompleteness {
            residual,
            tolerance,
        });
    }
    Ok(())
}

impl BoundGate {
    /// Numeric realization of the pinned ideal gate; target 0 is the local LSB.
    #[expect(
        clippy::arithmetic_side_effects,
        reason = "Complex floating point formulas cannot panic; admission checks finite results"
    )]
    /// # Errors
    /// Rejects insufficient matrix budget or nonfinite gate entries.
    pub fn matrix(&self, policy: MatrixPolicy) -> Result<NumericalOperator> {
        use BoundGate::{H, Id, Phase, Rx, Ry, Rz, S, Sdg, Swap, Sx, Sxdg, T, Tdg, U, X, Y, Z};
        let c = |r: f64, i: f64| Complex64::new(r, i);
        let exp = |a: f64| Complex64::from_polar(1.0, a);
        let hadamard = std::f64::consts::FRAC_1_SQRT_2;
        let values = match self {
            Id => [c(1.0, 0.0), c(0.0, 0.0), c(0.0, 0.0), c(1.0, 0.0)],
            X => [c(0.0, 0.0), c(1.0, 0.0), c(1.0, 0.0), c(0.0, 0.0)],
            Y => [c(0.0, 0.0), c(0.0, -1.0), c(0.0, 1.0), c(0.0, 0.0)],
            Z => [c(1.0, 0.0), c(0.0, 0.0), c(0.0, 0.0), c(-1.0, 0.0)],
            H => [
                c(hadamard, 0.0),
                c(hadamard, 0.0),
                c(hadamard, 0.0),
                c(-hadamard, 0.0),
            ],
            S => [
                c(1.0, 0.0),
                c(0.0, 0.0),
                c(0.0, 0.0),
                exp(std::f64::consts::FRAC_PI_2),
            ],
            Sdg => [
                c(1.0, 0.0),
                c(0.0, 0.0),
                c(0.0, 0.0),
                exp(-std::f64::consts::FRAC_PI_2),
            ],
            T => [
                c(1.0, 0.0),
                c(0.0, 0.0),
                c(0.0, 0.0),
                exp(std::f64::consts::FRAC_PI_4),
            ],
            Tdg => [
                c(1.0, 0.0),
                c(0.0, 0.0),
                c(0.0, 0.0),
                exp(-std::f64::consts::FRAC_PI_4),
            ],
            Phase(angle) => [c(1.0, 0.0), c(0.0, 0.0), c(0.0, 0.0), exp(*angle)],
            Sx => [c(0.5, 0.5), c(0.5, -0.5), c(0.5, -0.5), c(0.5, 0.5)],
            Sxdg => [c(0.5, -0.5), c(0.5, 0.5), c(0.5, 0.5), c(0.5, -0.5)],
            Rx(a) => {
                let (sine, cosine) = (a / 2.0).sin_cos();
                [c(cosine, 0.0), c(0.0, -sine), c(0.0, -sine), c(cosine, 0.0)]
            }
            Ry(a) => {
                let (sine, cosine) = (a / 2.0).sin_cos();
                [c(cosine, 0.0), c(-sine, 0.0), c(sine, 0.0), c(cosine, 0.0)]
            }
            Rz(a) => [exp(-a / 2.0), c(0.0, 0.0), c(0.0, 0.0), exp(a / 2.0)],
            U { theta, phi, lambda } => {
                let (sine, cosine) = (theta / 2.0).sin_cos();
                let phase = exp(theta / 2.0);
                [
                    phase * cosine,
                    -phase * exp(*lambda) * sine,
                    phase * exp(*phi) * sine,
                    // Multiplication keeps finite phases valid even when their
                    // sum would overflow before periodic reduction.
                    phase * exp(*phi) * exp(*lambda) * cosine,
                ]
            }
            Swap => {
                policy.check(4, 1)?;
                return NumericalOperator::from_owned(allocate(4, policy, |r, col| {
                    c(
                        if r == ((col & 1) << 1 | (col & 2) >> 1) {
                            1.0
                        } else {
                            0.0
                        },
                        0.0,
                    )
                })?);
            }
        };
        policy.check(2, 1)?;
        let [top_left, top_right, bottom_left, bottom_right] = values;
        NumericalOperator::from_owned(allocate(2, policy, |row, col| match (row, col) {
            (0, 0) => top_left,
            (0, _) => top_right,
            (_, 0) => bottom_left,
            _ => bottom_right,
        })?)
    }
}
