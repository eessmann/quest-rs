//! Rank-indexed classical array places, values and external interfaces.
use super::{
    Builder, Classical, E, ErrorKind, Expr, Int, PhantomData, ProgramId, S, SemanticError,
    SharedExpression, expression, syntax,
};
/// A builder-owned array expression with fixed rank and declared shape.
/// Read/write/ref capabilities are checked by the shared semantic admission.
#[derive(Debug, Clone)]
pub struct RankedArray<T: Classical, const R: usize> {
    pub(super) owner: ProgramId,
    pub(super) expression: syntax::Expression,
    pub(super) shape: [usize; R],
    pub(super) marker: PhantomData<T>,
}
impl<T: Classical, const R: usize> RankedArray<T, R> {
    #[must_use]
    pub const fn shape(&self) -> &[usize; R] {
        &self.shape
    }
}
pub(super) fn array_type<T: Classical>(
    shape: &[usize],
    reference: bool,
) -> Result<syntax::Type, SemanticError> {
    if shape.is_empty() || shape.len() > 256 || shape.contains(&0) {
        return Err(SemanticError::new(
            ErrorKind::Type,
            "array requires nonempty dimensions and bounded rank",
        ));
    }
    Ok(syntax::Type::Array {
        element: Box::new(T::syntax_type()?),
        dimensions: shape
            .iter()
            .map(|n| expression(E::Number(n.to_string())))
            .collect(),
        reference,
    })
}
impl Builder {
    fn ranked_layout<T: Classical>(&self, shape: &[usize]) -> Result<syntax::Type, SemanticError> {
        let count = shape
            .iter()
            .try_fold(1usize, |n, d| n.checked_mul(*d))
            .ok_or_else(|| SemanticError::budget("array shape overflow"))?;
        if count > self.expression_limits.nodes
            || count
                .checked_mul(256)
                .is_none_or(|n| n > self.expression_limits.storage_bytes)
        {
            return Err(SemanticError::budget("array construction storage"));
        }
        array_type::<T>(shape, false)
    }
    fn ranked_declare<T: Classical, const R: usize>(
        &mut self,
        name: &str,
        shape: [usize; R],
        initializer: Option<syntax::Expression>,
        qualifier: syntax::Qualifier,
    ) -> Result<RankedArray<T, R>, SemanticError> {
        let ty = self.ranked_layout::<T>(&shape)?;
        let external = qualifier != syntax::Qualifier::Local;
        if external && self.depth != 0 {
            return Err(SemanticError::new(
                ErrorKind::Type,
                "array interfaces require module scope",
            ));
        }
        let internal = self.symbol(name)?;
        let name = if external { name.to_owned() } else { internal };
        self.push(S::Declare {
            name: name.clone(),
            ty,
            initializer,
            qualifier,
        });
        Ok(RankedArray {
            owner: self.owner,
            expression: expression(E::Name(name)),
            shape,
            marker: PhantomData,
        })
    }
    /// Declare an immutable external array input with fixed rank and shape.
    /// # Errors
    /// Rejects invalid shapes, scope, scalar widths and construction limits.
    pub fn input_array<T: Classical, const R: usize>(
        &mut self,
        name: &str,
        shape: [usize; R],
    ) -> Result<RankedArray<T, R>, SemanticError> {
        self.ranked_declare(name, shape, None, syntax::Qualifier::Input)
    }
    /// Declare an array output initialized from a same-rank array value or place.
    /// # Errors
    /// Rejects foreign expressions, invalid scope and construction limits.
    pub fn output_array<T: Classical, const R: usize>(
        &mut self,
        name: &str,
        initial: &RankedArray<T, R>,
    ) -> Result<RankedArray<T, R>, SemanticError> {
        self.check(initial.owner)?;
        self.ranked_declare(
            name,
            initial.shape,
            Some(initial.expression.clone()),
            syntax::Qualifier::Output,
        )
    }
    /// Copy an array value into new mutable local storage.
    /// # Errors
    /// Rejects foreign expressions and construction limits.
    pub fn copy_array<T: Classical, const R: usize>(
        &mut self,
        name: &str,
        initial: &RankedArray<T, R>,
    ) -> Result<RankedArray<T, R>, SemanticError> {
        self.check(initial.owner)?;
        self.ranked_declare(
            name,
            initial.shape,
            Some(initial.expression.clone()),
            syntax::Qualifier::Local,
        )
    }
    /// Create initialized rank-indexed storage from row-major scalar expressions.
    /// # Errors
    /// Rejects inconsistent shape/element count, foreign handles and construction limits.
    pub fn ranked_array<T: Classical, const R: usize>(
        &mut self,
        name: &str,
        shape: [usize; R],
        values: &[Expr<T>],
    ) -> Result<RankedArray<T, R>, SemanticError> {
        self.ranked_layout::<T>(&shape)?;
        let count = shape
            .iter()
            .try_fold(1usize, |n, d| n.checked_mul(*d))
            .ok_or_else(|| SemanticError::budget("array shape overflow"))?;
        if values.len() != count {
            return Err(SemanticError::new(
                ErrorKind::Type,
                "array initializer shape",
            ));
        }
        for value in values {
            self.check(value.owner)?;
        }
        let mut wrappers = 0usize;
        let mut level = 1usize;
        for dimension in shape {
            wrappers = wrappers
                .checked_add(level)
                .ok_or_else(|| SemanticError::budget("array wrapper count"))?;
            level = level
                .checked_mul(dimension)
                .ok_or_else(|| SemanticError::budget("array shape overflow"))?;
        }
        SharedExpression::admit_expansion(
            values.iter().map(|v| &v.expression),
            wrappers,
            R,
            None,
            self.expression_limits,
        )?;
        let mut cursor = values.iter();

        let initializer = initializer(&shape, &mut cursor, self.expression_limits)?;
        self.ranked_declare(name, shape, Some(initializer), syntax::Qualifier::Local)
    }
    fn ranked_place<T: Classical, const R: usize, const W: u8>(
        &self,
        array: &RankedArray<T, R>,
        indices: &[Expr<Int<W>>],
    ) -> Result<syntax::Expression, SemanticError> {
        self.check(array.owner)?;
        for index in indices {
            self.check(index.owner)?;
        }
        SharedExpression::admit_expansion(
            indices.iter().map(|v| &v.expression),
            indices.len(),
            indices.len(),
            Some(&array.expression),
            self.expression_limits,
        )?;
        let mut value = array.expression.clone();

        for index in indices {
            self.check(index.owner)?;
            value = expression(E::Index(
                Box::new(value),
                Box::new(index.expression.materialize(self.expression_limits)?),
            ));
        }
        Ok(value)
    }
    /// Read a scalar through exactly one index per array dimension.
    /// # Errors
    /// Rejects foreign handles; runtime checks dynamic index bounds.
    pub fn ranked_read<T: Classical, const R: usize, const W: u8>(
        &self,
        array: &RankedArray<T, R>,
        indices: &[Expr<Int<W>>; R],
    ) -> Result<Expr<T>, SemanticError> {
        Ok(self.wrap(SharedExpression::leaf(
            self.ranked_place(array, indices)?,
            self.expression_limits,
        )?))
    }
    /// Write a scalar through exactly one index per array dimension.
    /// # Errors
    /// Rejects foreign handles; admission checks mutability and execution checks dynamic bounds.
    pub fn ranked_write<T: Classical, const R: usize, const W: u8>(
        &mut self,
        array: &RankedArray<T, R>,
        indices: &[Expr<Int<W>>; R],
        value: &Expr<T>,
    ) -> Result<(), SemanticError> {
        self.check(value.owner)?;
        let target = self.ranked_place(array, indices)?;
        self.push(S::Assign {
            target,
            operator: None,
            value: value.expression.materialize(self.expression_limits)?,
        });
        Ok(())
    }
    /// Select a lower-rank reference using a prefix of indices; no storage is copied.
    /// # Errors
    /// Rejects foreign handles or a rank/indices mismatch; dynamic alias/bounds checks remain in SSA.
    pub fn ranked_slice<T: Classical, const R: usize, const D: usize, const W: u8>(
        &self,
        array: &RankedArray<T, R>,
        indices: &[Expr<Int<W>>],
    ) -> Result<RankedArray<T, D>, SemanticError> {
        if D == 0 || indices.len().checked_add(D) != Some(R) {
            return Err(SemanticError::new(ErrorKind::Type, "array slice rank"));
        }
        let shape = array
            .shape
            .get(indices.len()..)
            .and_then(|s| s.try_into().ok())
            .ok_or_else(|| SemanticError::new(ErrorKind::Type, "array slice shape"))?;
        Ok(RankedArray {
            owner: self.owner,
            expression: self.ranked_place(array, indices)?,
            shape,
            marker: PhantomData,
        })
    }
    /// Assign a whole same-rank array, retaining normal copy and reference semantics.
    /// # Errors
    /// Rejects foreign handles or differing shapes; admission checks destination mutability.
    pub fn assign_array<T: Classical, const R: usize>(
        &mut self,
        target: &RankedArray<T, R>,
        value: &RankedArray<T, R>,
    ) -> Result<(), SemanticError> {
        self.check(target.owner)?;
        self.check(value.owner)?;
        if target.shape != value.shape {
            return Err(SemanticError::new(
                ErrorKind::Type,
                "array assignment shape",
            ));
        }
        self.push(S::Assign {
            target: target.expression.clone(),
            operator: None,
            value: value.expression.clone(),
        });
        Ok(())
    }
}
fn initializer<'a, T: Classical + 'a>(
    shape: &[usize],
    values: &mut impl Iterator<Item = &'a Expr<T>>,
    limits: super::CompileLimits,
) -> Result<syntax::Expression, SemanticError> {
    let Some((count, tail)) = shape.split_first() else {
        return values
            .next()
            .ok_or_else(|| SemanticError::new(ErrorKind::Type, "array initializer exhausted"))?
            .expression
            .materialize(limits);
    };
    let elements = (0..*count)
        .map(|_| initializer(tail, values, limits))
        .collect::<Result<_, _>>()?;
    Ok(expression(E::Array(elements)))
}
