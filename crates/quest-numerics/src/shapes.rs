//! Statically shaped, heap-owned matrices. Dimension mismatch is rejected at
//! construction; heap storage avoids embedding large Jacobians in solver frames.
use crate::arithmetic::{ArithmeticError, ArithmeticResult};
/// A zero dimension is rejected at monomorphization.
/// ```compile_fail
/// use quest_numerics::shapes::Matrix;
/// let _ = Matrix::<f64,0,2>::from_vec(Vec::new());
/// ```
#[derive(Clone, Debug)]
pub struct Matrix<T, const R: usize, const C: usize> {
    data: Vec<T>,
}
impl<T, const R: usize, const C: usize> Matrix<T, R, C> {
    /// # Errors
    /// Rejects shape mismatch, size overflow, or allocation failure.
    pub fn from_vec(data: Vec<T>) -> ArithmeticResult<Self> {
        const { assert!(R > 0 && C > 0, "zero matrix dimension") };
        if R.checked_mul(C) != Some(data.len()) {
            return Err(ArithmeticError::Domain("matrix shape"));
        }
        Ok(Self { data })
    }
    /// # Errors
    /// Rejects shape mismatch, size overflow, or allocation failure.
    pub fn from_rows(rows: [[T; C]; R]) -> ArithmeticResult<Self> {
        const { assert!(R > 0 && C > 0, "zero matrix dimension") };
        let n = R
            .checked_mul(C)
            .ok_or(ArithmeticError::Budget("matrix shape"))?;
        let mut data = Vec::new();
        data.try_reserve_exact(n)
            .map_err(|_| ArithmeticError::Budget("matrix allocation"))?;
        for row in rows {
            data.extend(row);
        }
        Self::from_vec(data)
    }
    /// # Errors
    /// Rejects an index outside the static dimensions.
    pub fn get(&self, row: usize, column: usize) -> ArithmeticResult<&T> {
        if row >= R || column >= C {
            return Err(ArithmeticError::Domain("matrix index"));
        }
        self.data
            .get(
                row.checked_mul(C)
                    .and_then(|v| v.checked_add(column))
                    .ok_or(ArithmeticError::Budget("matrix index"))?,
            )
            .ok_or(ArithmeticError::Domain("matrix shape"))
    }
    /// # Errors
    /// Rejects an index outside the static dimensions.
    pub fn get_mut(&mut self, row: usize, column: usize) -> ArithmeticResult<&mut T> {
        if row >= R || column >= C {
            return Err(ArithmeticError::Domain("matrix index"));
        }
        self.data
            .get_mut(
                row.checked_mul(C)
                    .and_then(|v| v.checked_add(column))
                    .ok_or(ArithmeticError::Budget("matrix index"))?,
            )
            .ok_or(ArithmeticError::Domain("matrix shape"))
    }
    /// Mutable entries preserve the admitted dimensions.
    pub fn as_mut_slice(&mut self) -> &mut [T] {
        &mut self.data
    }
    #[must_use]
    pub fn into_vec(self) -> Vec<T> {
        self.data
    }
    #[must_use]
    pub fn as_slice(&self) -> &[T] {
        &self.data
    }
    #[must_use]
    pub const fn rows(&self) -> usize {
        R
    }
    #[must_use]
    pub const fn columns(&self) -> usize {
        C
    }
}
