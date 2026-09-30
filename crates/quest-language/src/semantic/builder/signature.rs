//! Compositional typed subroutine signatures. Nested pairs admit arbitrary arity.
use super::{
    Array, Builder, Classical, E, ErrorKind, Expr, PhantomData, ProgramId, S, SemanticError,
    SharedExpression, expression, syntax,
};
mod sealed {
    pub trait Sealed {}
}
/// A checked signature assembled from scalar values, array references and nested pairs.
pub trait Signature: sealed::Sealed + Clone {
    /// Typed handles passed to the definition closure.
    type Parameters;
    /// Typed values/references supplied by the caller.
    type Arguments;
    #[doc(hidden)]
    fn parameters(
        &self,
        builder: &mut Builder,
        declarations: &mut Vec<syntax::Parameter>,
    ) -> Result<Self::Parameters, SemanticError>;
    #[doc(hidden)]
    fn arguments(
        &self,
        builder: &Builder,
        arguments: Self::Arguments,
        output: &mut Vec<syntax::Expression>,
    ) -> Result<(), SemanticError>;
}
/// A classical value parameter; the type and width remain indexed by Rust.
#[derive(Debug, Clone)]
pub struct ValueParameter<T: Classical>(PhantomData<T>);
impl<T: Classical> Default for ValueParameter<T> {
    fn default() -> Self {
        Self::new()
    }
}
impl<T: Classical> ValueParameter<T> {
    #[must_use]
    pub const fn new() -> Self {
        Self(PhantomData)
    }
}
impl<T: Classical> sealed::Sealed for ValueParameter<T> {}
impl<T: Classical> Signature for ValueParameter<T> {
    type Parameters = Expr<T>;
    type Arguments = Expr<T>;
    fn parameters(
        &self,
        b: &mut Builder,
        out: &mut Vec<syntax::Parameter>,
    ) -> Result<Expr<T>, SemanticError> {
        let name = b.symbol("value")?;
        out.push(syntax::Parameter {
            name: name.clone(),
            ty: T::syntax_type()?,
            mutable: false,
        });
        Ok(b.wrap(SharedExpression::leaf(
            expression(E::Name(name)),
            b.expression_limits,
        )?))
    }
    fn arguments(
        &self,
        b: &Builder,
        a: Expr<T>,
        out: &mut Vec<syntax::Expression>,
    ) -> Result<(), SemanticError> {
        b.check(a.owner)?;
        out.push(a.expression.materialize(b.expression_limits)?);
        Ok(())
    }
}
/// A fixed-size array reference with explicit mutability and indexed element type.
#[derive(Debug, Clone)]
pub struct ArrayRef<T: Classical, const N: usize> {
    mutable: bool,
    marker: PhantomData<T>,
}
impl<T: Classical, const N: usize> ArrayRef<T, N> {
    #[must_use]
    pub const fn mutable() -> Self {
        Self {
            mutable: true,
            marker: PhantomData,
        }
    }
    #[must_use]
    pub const fn readonly() -> Self {
        Self {
            mutable: false,
            marker: PhantomData,
        }
    }
}
impl<T: Classical, const N: usize> sealed::Sealed for ArrayRef<T, N> {}
impl<T: Classical, const N: usize> Signature for ArrayRef<T, N> {
    type Parameters = Array<T>;
    type Arguments = Array<T>;
    fn parameters(
        &self,
        b: &mut Builder,
        out: &mut Vec<syntax::Parameter>,
    ) -> Result<Array<T>, SemanticError> {
        if N == 0 {
            return Err(SemanticError::new(ErrorKind::Type, "empty array reference"));
        }
        let name = b.symbol("reference")?;
        out.push(syntax::Parameter {
            name: name.clone(),
            ty: syntax::Type::Array {
                element: Box::new(T::syntax_type()?),
                dimensions: vec![expression(E::Number(N.to_string()))],
                reference: true,
            },
            mutable: self.mutable,
        });
        Ok(Array {
            owner: b.owner,
            name,
            marker: PhantomData,
        })
    }
    fn arguments(
        &self,
        b: &Builder,
        a: Array<T>,
        out: &mut Vec<syntax::Expression>,
    ) -> Result<(), SemanticError> {
        b.check(a.owner)?;
        out.push(expression(E::Name(a.name)));
        Ok(())
    }
}
impl sealed::Sealed for () {}
impl Signature for () {
    type Parameters = ();
    type Arguments = ();
    fn parameters(
        &self,
        _: &mut Builder,
        _: &mut Vec<syntax::Parameter>,
    ) -> Result<(), SemanticError> {
        Ok(())
    }
    fn arguments(
        &self,
        _: &Builder,
        (): (),
        _: &mut Vec<syntax::Expression>,
    ) -> Result<(), SemanticError> {
        Ok(())
    }
}
impl<A: Signature, B: Signature> sealed::Sealed for (A, B) {}
impl<A: Signature, B: Signature> Signature for (A, B) {
    type Parameters = (A::Parameters, B::Parameters);
    type Arguments = (A::Arguments, B::Arguments);
    fn parameters(
        &self,
        b: &mut Builder,
        out: &mut Vec<syntax::Parameter>,
    ) -> Result<Self::Parameters, SemanticError> {
        Ok((self.0.parameters(b, out)?, self.1.parameters(b, out)?))
    }
    fn arguments(
        &self,
        b: &Builder,
        (a, c): Self::Arguments,
        out: &mut Vec<syntax::Expression>,
    ) -> Result<(), SemanticError> {
        self.0.arguments(b, a, out)?;
        self.1.arguments(b, c, out)
    }
}
/// Builder-owned definition with a compositional parameter list and scalar return type.
#[derive(Debug, Clone)]
pub struct Subroutine<Sig: Signature, R: Classical> {
    owner: ProgramId,
    name: String,
    signature: Sig,
    marker: PhantomData<R>,
}
impl Builder {
    /// Define a subroutine with any nested combination of value/reference parameters.
    /// # Errors
    /// Rejects non-module definitions, invalid signature types and body failures; shared admission checks reference capabilities.
    pub fn define_subroutine<Sig: Signature, R: Classical>(
        &mut self,
        name: &str,
        signature: Sig,
        body: impl FnOnce(&mut Self, Sig::Parameters) -> Result<Expr<R>, SemanticError>,
    ) -> Result<Subroutine<Sig, R>, SemanticError> {
        if self.depth != 0 {
            return Err(SemanticError::new(
                ErrorKind::Type,
                "subroutine requires module scope",
            ));
        }
        let name = self.symbol(name)?;
        let mut parameters = Vec::new();
        let handles = signature.parameters(self, &mut parameters)?;
        let body = self.body(|b| {
            let result = body(b, handles)?;
            b.check(result.owner)?;
            b.push(S::Return(Some(
                result.expression.materialize(b.expression_limits)?,
            )));
            Ok(())
        })?;
        self.push(S::Subroutine {
            name: name.clone(),
            parameters,
            result: Some(R::syntax_type()?),
            body,
        });
        Ok(Subroutine {
            owner: self.owner,
            name,
            signature,
            marker: PhantomData,
        })
    }
    /// Call a compositional signature without copying referenced array storage.
    /// # Errors
    /// Rejects foreign handles; shared admission checks array shape, mutability and alias capabilities.
    pub fn call_subroutine<Sig: Signature, R: Classical>(
        &self,
        function: &Subroutine<Sig, R>,
        arguments: Sig::Arguments,
    ) -> Result<Expr<R>, SemanticError> {
        self.check(function.owner)?;
        let mut values = Vec::new();
        function.signature.arguments(self, arguments, &mut values)?;
        Ok(self.wrap(SharedExpression::leaf(
            expression(E::Call(function.name.clone(), values)),
            self.expression_limits,
        )?))
    }
}

/// One scalar quantum-reference parameter.
#[derive(Debug, Clone, Copy)]
pub struct QubitParameter;
/// A fixed-size quantum register reference parameter.
#[derive(Debug, Clone, Copy)]
pub struct QubitArrayParameter<const N: usize>;
fn quantum_parameter(
    b: &mut Builder,
    out: &mut Vec<syntax::Parameter>,
    count: Option<usize>,
) -> Result<super::Qubit, SemanticError> {
    if count == Some(0) {
        return Err(SemanticError::new(
            ErrorKind::Type,
            "empty quantum reference",
        ));
    }
    let name = b.symbol("quantum")?;
    out.push(syntax::Parameter {
        name: name.clone(),
        ty: syntax::Type::Qubit(count.map(|n| Box::new(expression(E::Number(n.to_string()))))),
        mutable: true,
    });
    Ok(super::Qubit {
        owner: b.owner,
        expression: expression(E::Name(name)),
    })
}
fn quantum_argument(
    b: &Builder,
    q: super::Qubit,
    out: &mut Vec<syntax::Expression>,
) -> Result<(), SemanticError> {
    b.check(q.owner)?;
    out.push(q.expression);
    Ok(())
}
impl sealed::Sealed for QubitParameter {}
impl Signature for QubitParameter {
    type Parameters = super::Qubit;
    type Arguments = super::Qubit;
    fn parameters(
        &self,
        b: &mut Builder,
        out: &mut Vec<syntax::Parameter>,
    ) -> Result<Self::Parameters, SemanticError> {
        quantum_parameter(b, out, None)
    }
    fn arguments(
        &self,
        b: &Builder,
        q: Self::Arguments,
        out: &mut Vec<syntax::Expression>,
    ) -> Result<(), SemanticError> {
        quantum_argument(b, q, out)
    }
}
impl<const N: usize> sealed::Sealed for QubitArrayParameter<N> {}
impl<const N: usize> Signature for QubitArrayParameter<N> {
    type Parameters = super::Qubit;
    type Arguments = super::Qubit;
    fn parameters(
        &self,
        b: &mut Builder,
        out: &mut Vec<syntax::Parameter>,
    ) -> Result<Self::Parameters, SemanticError> {
        quantum_parameter(b, out, Some(N))
    }
    fn arguments(
        &self,
        b: &Builder,
        q: Self::Arguments,
        out: &mut Vec<syntax::Expression>,
    ) -> Result<(), SemanticError> {
        quantum_argument(b, q, out)
    }
}
/// Builder-owned effectful procedure with no return value.
#[derive(Debug, Clone)]
pub struct Procedure<Sig: Signature> {
    owner: ProgramId,
    name: String,
    signature: Sig,
}
impl Builder {
    /// Define a procedure over any compositional classical/quantum signature.
    /// # Errors
    /// Rejects nested definitions and body failures; shared admission checks effects and reference capabilities.
    pub fn define_procedure<Sig: Signature>(
        &mut self,
        name: &str,
        signature: Sig,
        body: impl FnOnce(&mut Self, Sig::Parameters) -> Result<(), SemanticError>,
    ) -> Result<Procedure<Sig>, SemanticError> {
        if self.depth != 0 {
            return Err(SemanticError::new(
                ErrorKind::Type,
                "procedure requires module scope",
            ));
        }
        let name = self.symbol(name)?;
        let mut parameters = Vec::new();
        let handles = signature.parameters(self, &mut parameters)?;
        let body = self.body(|b| body(b, handles))?;
        self.push(S::Subroutine {
            name: name.clone(),
            parameters,
            result: None,
            body,
        });
        Ok(Procedure {
            owner: self.owner,
            name,
            signature,
        })
    }
    /// Call a void procedure and retain its effects in source order.
    /// # Errors
    /// Rejects foreign identities; shared admission checks quantum arity, mutability and unitary contexts.
    pub fn call_procedure<Sig: Signature>(
        &mut self,
        function: &Procedure<Sig>,
        arguments: Sig::Arguments,
    ) -> Result<(), SemanticError> {
        self.check(function.owner)?;
        let mut values = Vec::new();
        function.signature.arguments(self, arguments, &mut values)?;
        self.push(S::Expression(expression(E::Call(
            function.name.clone(),
            values,
        ))));
        Ok(())
    }
}

/// A fixed-shape, rank-indexed classical array reference parameter.
#[derive(Debug, Clone)]
pub struct RankedArrayRef<T: Classical, const R: usize> {
    shape: [usize; R],
    mutable: bool,
    marker: PhantomData<T>,
}
impl<T: Classical, const R: usize> RankedArrayRef<T, R> {
    #[must_use]
    pub const fn mutable(shape: [usize; R]) -> Self {
        Self {
            shape,
            mutable: true,
            marker: PhantomData,
        }
    }
    #[must_use]
    pub const fn readonly(shape: [usize; R]) -> Self {
        Self {
            shape,
            mutable: false,
            marker: PhantomData,
        }
    }
}
impl<T: Classical, const R: usize> sealed::Sealed for RankedArrayRef<T, R> {}
impl<T: Classical, const R: usize> Signature for RankedArrayRef<T, R> {
    type Parameters = super::RankedArray<T, R>;
    type Arguments = super::RankedArray<T, R>;
    fn parameters(
        &self,
        b: &mut Builder,
        out: &mut Vec<syntax::Parameter>,
    ) -> Result<Self::Parameters, SemanticError> {
        let name = b.symbol("ranked_reference")?;
        let ty = super::ranked::array_type::<T>(&self.shape, true)?;
        out.push(syntax::Parameter {
            name: name.clone(),
            ty,
            mutable: self.mutable,
        });
        Ok(super::RankedArray {
            owner: b.owner,
            expression: expression(E::Name(name)),
            shape: self.shape,
            marker: PhantomData,
        })
    }
    fn arguments(
        &self,
        b: &Builder,
        array: Self::Arguments,
        out: &mut Vec<syntax::Expression>,
    ) -> Result<(), SemanticError> {
        b.check(array.owner)?;
        if array.shape != self.shape {
            return Err(SemanticError::new(ErrorKind::Type, "reference array shape"));
        }
        out.push(array.expression);
        Ok(())
    }
}

/// Builder-owned subroutine returning a fixed-rank array value.
#[derive(Debug, Clone)]
pub struct ArraySubroutine<Sig: Signature, T: Classical, const R: usize> {
    owner: ProgramId,
    name: String,
    signature: Sig,
    shape: [usize; R],
    marker: PhantomData<T>,
}
impl Builder {
    /// Define a subroutine returning an array by value.
    /// # Errors
    /// Rejects invalid scope, shapes, foreign handles and body failures.
    pub fn define_array_subroutine<Sig: Signature, T: Classical, const R: usize>(
        &mut self,
        name: &str,
        signature: Sig,
        shape: [usize; R],
        body: impl FnOnce(&mut Self, Sig::Parameters) -> Result<super::RankedArray<T, R>, SemanticError>,
    ) -> Result<ArraySubroutine<Sig, T, R>, SemanticError> {
        if self.depth != 0 {
            return Err(SemanticError::new(
                ErrorKind::Type,
                "subroutine requires module scope",
            ));
        }
        let ty = super::ranked::array_type::<T>(&shape, false)?;
        let name = self.symbol(name)?;
        let mut parameters = Vec::new();
        let handles = signature.parameters(self, &mut parameters)?;
        let body = self.body(|b| {
            let result = body(b, handles)?;
            b.check(result.owner)?;
            if result.shape != shape {
                return Err(SemanticError::new(ErrorKind::Type, "array return shape"));
            }
            b.push(S::Return(Some(result.expression)));
            Ok(())
        })?;
        self.push(S::Subroutine {
            name: name.clone(),
            parameters,
            result: Some(ty),
            body,
        });
        Ok(ArraySubroutine {
            owner: self.owner,
            name,
            signature,
            shape,
            marker: PhantomData,
        })
    }
    /// Call an array-valued subroutine with a compositional signature.
    /// # Errors
    /// Rejects foreign handles and argument shape mismatch; admission checks reference capabilities.
    pub fn call_array_subroutine<Sig: Signature, T: Classical, const R: usize>(
        &self,
        function: &ArraySubroutine<Sig, T, R>,
        arguments: Sig::Arguments,
    ) -> Result<super::RankedArray<T, R>, SemanticError> {
        self.check(function.owner)?;
        let mut values = Vec::new();
        function.signature.arguments(self, arguments, &mut values)?;
        Ok(super::RankedArray {
            owner: self.owner,
            expression: expression(E::Call(function.name.clone(), values)),
            shape: function.shape,
            marker: PhantomData,
        })
    }
}
