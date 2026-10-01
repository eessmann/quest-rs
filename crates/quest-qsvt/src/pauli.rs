#![allow(
    clippy::float_cmp,
    reason = "Only exact complex zeros are tested for omitted arithmetic"
)]
//! Complete dense Pauli decomposition, independent of the native simulator.
use crate::{Complex64, Error, NumericalPolicy, Result};
use faer::{Mat, MatRef};
use std::ops::{Add, Div};
/// Explicit exponential-work admission. No threshold removes coefficients.
#[derive(Debug, Clone, Copy)]
pub struct PauliLimits {
    pub max_coefficients: usize,
    pub max_bytes: usize,
    pub max_work: usize,
}
impl Default for PauliLimits {
    fn default() -> Self {
        Self {
            max_coefficients: 1_048_576,
            max_bytes: 67_108_864,
            max_work: 1_073_741_824,
        }
    }
}
/// Binary64 coefficients of `A=sum_p c_p P_p`, `c_p=Tr(A P_p)/2^n`.
///
/// Base-4 digits are I,X,Y,Z, with qubit zero least significant. Labels display
/// the highest qubit first, so index 13 is ZX. These are rounded numerical
/// coefficients, not interval certificates of exact trace values.
#[derive(Debug, Clone)]
pub struct PauliDecomposition {
    num_qubits: u32,
    coefficients: Vec<Complex64>,
}
impl PauliDecomposition {
    #[must_use]
    pub const fn num_qubits(&self) -> u32 {
        self.num_qubits
    }
    #[must_use]
    pub fn coefficients(&self) -> &[Complex64] {
        &self.coefficients
    }
    /// Return a tensor label, or None for an out-of-range coefficient index.
    #[must_use]
    pub fn label(&self, index: usize) -> Option<String> {
        if index >= self.coefficients.len() {
            return None;
        }
        let mut label = String::with_capacity(usize::try_from(self.num_qubits).ok()?);
        for q in (0..self.num_qubits).rev() {
            label.push(*['I', 'X', 'Y', 'Z'].get((index >> (q.saturating_mul(2))) & 3)?);
        }
        Some(label)
    }
    /// Reconstruct the full dense matrix under the same coefficient ordering.
    /// # Errors
    /// Rejects work/storage admission or nonfinite accumulated arithmetic.
    pub fn reconstruct(&self, limits: PauliLimits) -> Result<Mat<Complex64>> {
        let dimension = 1usize
            .checked_shl(self.num_qubits)
            .ok_or(Error::Budget("Pauli dimension"))?;
        admit(dimension, limits)?;
        let mut output = crate::matrix::allocate(
            dimension,
            dimension,
            NumericalPolicy {
                max_bytes: limits.max_bytes,
            },
            |_, _| Complex64::new(0.0, 0.0),
        )?;
        for (index, coefficient) in self.coefficients.iter().enumerate() {
            if *coefficient == Complex64::new(0.0, 0.0) {
                continue;
            }
            let (flip, sign, phase) = masks(index, self.num_qubits);
            for row in 0..dimension {
                let col = row ^ flip;
                let factor = (phase.wrapping_add(if (row & sign).count_ones() & 1 == 0 {
                    0
                } else {
                    2
                })) & 3;
                output[(row, col)] = checked(output[(row, col)].add(rotate(*coefficient, factor)))?;
            }
        }
        Ok(output)
    }
}
fn admit(dimension: usize, limits: PauliLimits) -> Result<usize> {
    if dimension == 0 || !dimension.is_power_of_two() {
        return Err(Error::Space(
            "Pauli dimension must be a nonzero power of two",
        ));
    }
    let count = dimension
        .checked_mul(dimension)
        .ok_or(Error::Budget("Pauli coefficient count overflow"))?;
    let bytes = count
        .checked_mul(size_of::<Complex64>())
        .and_then(|v| v.checked_mul(2))
        .ok_or(Error::Budget("Pauli storage overflow"))?;
    // Each term needs O(n) setup and O(2^n) accumulation; reconstruction shares
    // this conservative work model. Matrix/coefficients coexist in byte admission.
    let work = count
        .checked_mul(
            dimension
                .checked_add(
                    usize::try_from(dimension.trailing_zeros())
                        .map_err(|_| Error::Budget("Pauli qubit count"))?,
                )
                .ok_or(Error::Budget("Pauli work overflow"))?,
        )
        .and_then(|v| v.checked_mul(8))
        .ok_or(Error::Budget("Pauli work overflow"))?;
    if count > limits.max_coefficients
        || bytes > limits.max_bytes
        || work > limits.max_work
        || isize::try_from(bytes).is_err()
    {
        return Err(Error::Budget("Pauli decomposition limits"));
    }
    Ok(count)
}
const fn checked(value: Complex64) -> Result<Complex64> {
    if value.re.is_finite() && value.im.is_finite() {
        Ok(value)
    } else {
        Err(Error::NonFinite)
    }
}
// factor P[state,state^flip] = i^phase * (-1)^popcount(state & sign).
fn masks(index: usize, qubits: u32) -> (usize, usize, u32) {
    let mut flip = 0;
    let mut sign = 0;
    let mut phase = 0u32;
    for q in 0..qubits {
        let label = (index >> (q.saturating_mul(2))) & 3;
        if label == 1 || label == 2 {
            flip |= 1usize << q;
        }
        if label == 2 || label == 3 {
            sign |= 1usize << q;
        }
        if label == 2 {
            phase = phase.wrapping_add(3) & 3;
        }
    }
    (flip, sign, phase)
}
fn rotate(z: Complex64, phase: u32) -> Complex64 {
    match phase & 3 {
        0 => z,
        1 => Complex64::new(-z.im, z.re),
        2 => Complex64::new(-z.re, -z.im),
        _ => Complex64::new(z.im, -z.re),
    }
}
/// Decompose any finite square 2^n matrix, including non-Hermitian matrices and
/// the scalar (zero-qubit) case. All 4^n coefficients are retained.
///
/// Phase masks give O(1) work per computational basis state after O(n) setup
/// per Pauli string. Execution is sequential. Unscaled accumulation preserves
/// subnormal sums; if a partial trace overflows, a bounded second pass normalizes
/// each summand to compute a representable average.
/// # Errors
/// Rejects invalid dimensions, nonfinite entries/results or excessive resources.
pub fn decompose_pauli(
    matrix: MatRef<'_, Complex64>,
    limits: PauliLimits,
) -> Result<PauliDecomposition> {
    if matrix.nrows() != matrix.ncols() {
        return Err(Error::Space("Pauli decomposition requires a square matrix"));
    }
    let dimension = matrix.nrows();
    let count = admit(dimension, limits)?;
    let divisor = f64::from(
        u32::try_from(dimension).map_err(|_| Error::Budget("Pauli normalization dimension"))?,
    );
    for col in 0..dimension {
        for row in 0..dimension {
            checked(matrix[(row, col)])?;
        }
    }
    let qubits = dimension.trailing_zeros();
    let mut coefficients = Vec::new();
    coefficients
        .try_reserve_exact(count)
        .map_err(|_| Error::Budget("Pauli coefficient allocation"))?;
    for index in 0..count {
        let (flip, sign, phase) = masks(index, qubits);
        let accumulate = |normalization: f64| -> Result<Complex64> {
            let mut coefficient = Complex64::new(0.0, 0.0);
            for state in 0..dimension {
                let factor = (phase.wrapping_add(if (state & sign).count_ones() & 1 == 0 {
                    0
                } else {
                    2
                })) & 3;
                coefficient = checked(coefficient.add(rotate(
                    matrix[(state ^ flip, state)].div(normalization),
                    factor,
                )))?;
            }
            Ok(coefficient)
        };
        // Preserve subnormal traces when unscaled accumulation is finite. On
        // overflow, restart with normalized summands; this still computes the
        // same trace average without admitting a nonfinite intermediate result.
        let coefficient = match accumulate(1.0) {
            Ok(sum) => checked(sum.div(divisor))?,
            Err(Error::NonFinite) => accumulate(divisor)?,
            Err(error) => return Err(error),
        };
        coefficients.push(coefficient);
    }
    Ok(PauliDecomposition {
        num_qubits: qubits,
        coefficients,
    })
}
