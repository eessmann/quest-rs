//! Typed arrays and references use the same checked storage and call semantics as QASM.
use super::{
    Builder, Classical, E, ErrorKind, Expr, Int, PhantomData, ProgramId, Qubit, S, SemanticError,
    SharedExpression, expression, syntax,
};
#[derive(Debug, Clone)]
pub struct Array<T: Classical> {
    pub(super) owner: ProgramId,
    pub(super) name: String,
    pub(super) marker: PhantomData<T>,
}
#[derive(Debug, Clone)]
pub struct Function<A: Classical, R: Classical> {
    owner: ProgramId,
    name: String,
    marker: PhantomData<(A, R)>,
}
impl Builder {
    /// Create initialized fixed-length storage with typed elements.
    /// # Errors
    /// Rejects empty arrays, foreign handles, and shared resource limits.
    pub fn array<T: Classical>(
        &mut self,
        name: &str,
        values: &[Expr<T>],
    ) -> Result<Array<T>, SemanticError> {
        if values.is_empty() {
            return Err(SemanticError::new(
                ErrorKind::Type,
                "array must be nonempty",
            ));
        }
        for value in values {
            self.check(value.owner)?;
        }
        SharedExpression::admit_expansion(
            values.iter().map(|v| &v.expression),
            1,
            1,
            None,
            self.expression_limits,
        )?;
        let name = self.symbol(name)?;
        self.push(S::Declare {
            name: name.clone(),
            ty: syntax::Type::Array {
                element: Box::new(T::syntax_type()?),
                dimensions: vec![expression(E::Number(values.len().to_string()))],
                reference: false,
            },
            initializer: Some(expression(E::Array(
                values
                    .iter()
                    .map(|v| v.expression.materialize(self.expression_limits))
                    .collect::<Result<_, _>>()?,
            ))),
            qualifier: syntax::Qualifier::Local,
        });
        Ok(Array {
            owner: self.owner,
            name,
            marker: PhantomData,
        })
    }
    /// Read a typed element; dynamic bounds failures remain runtime effects.
    /// # Errors
    /// Rejects foreign array/index handles.
    pub fn array_read<T: Classical, const W: u8>(
        &self,
        array: &Array<T>,
        index: &Expr<Int<W>>,
    ) -> Result<Expr<T>, SemanticError> {
        self.check(array.owner)?;
        self.check(index.owner)?;
        Ok(self.wrap(SharedExpression::leaf(
            expression(E::Index(
                Box::new(expression(E::Name(array.name.clone()))),
                Box::new(index.expression.materialize(self.expression_limits)?),
            )),
            self.expression_limits,
        )?))
    }
    /// Write a typed element; admission and execution enforce reference mutability and bounds.
    /// # Errors
    /// Rejects foreign storage or value handles.
    pub fn array_write<T: Classical, const W: u8>(
        &mut self,
        array: &Array<T>,
        index: &Expr<Int<W>>,
        value: &Expr<T>,
    ) -> Result<(), SemanticError> {
        self.check(array.owner)?;
        self.check(index.owner)?;
        self.check(value.owner)?;
        self.push(S::Assign {
            target: expression(E::Index(
                Box::new(expression(E::Name(array.name.clone()))),
                Box::new(index.expression.materialize(self.expression_limits)?),
            )),
            operator: None,
            value: value.expression.materialize(self.expression_limits)?,
        });
        Ok(())
    }
    /// Define a typed scalar subroutine with an explicit return value.
    /// # Errors
    /// Rejects local definitions, invalid types, and body failures.
    pub fn define_function<A: Classical, R: Classical>(
        &mut self,
        name: &str,
        body: impl FnOnce(&mut Self, Expr<A>) -> Result<Expr<R>, SemanticError>,
    ) -> Result<Function<A, R>, SemanticError> {
        if self.depth != 0 {
            return Err(SemanticError::new(
                ErrorKind::Type,
                "function requires module scope",
            ));
        }
        let name = self.symbol(name)?;
        let parameter = self.symbol("argument")?;
        let argument = self.wrap(SharedExpression::leaf(
            expression(E::Name(parameter.clone())),
            self.expression_limits,
        )?);
        let body = self.body(|b| {
            let result = body(b, argument)?;
            b.check(result.owner)?;
            b.push(S::Return(Some(
                result.expression.materialize(b.expression_limits)?,
            )));
            Ok(())
        })?;
        self.push(S::Subroutine {
            name: name.clone(),
            parameters: vec![syntax::Parameter {
                name: parameter,
                ty: A::syntax_type()?,
                mutable: false,
            }],
            result: Some(R::syntax_type()?),
            body,
        });
        Ok(Function {
            owner: self.owner,
            name,
            marker: PhantomData,
        })
    }
    /// Invoke a typed scalar function. Traps and effects remain explicit in SSA.
    /// # Errors
    /// Rejects foreign function or argument identities.
    pub fn call_function<A: Classical, R: Classical>(
        &self,
        function: &Function<A, R>,
        argument: &Expr<A>,
    ) -> Result<Expr<R>, SemanticError> {
        self.check(function.owner)?;
        self.check(argument.owner)?;
        Ok(self.wrap(SharedExpression::leaf(
            expression(E::Call(
                function.name.clone(),
                vec![argument.expression.materialize(self.expression_limits)?],
            )),
            self.expression_limits,
        )?))
    }
    /// Emit an irreversible immutable payload capture over ordered quantum operands.
    /// # Errors
    /// Rejects foreign handles; verification checks the supplied bank and arity.
    pub fn payload(&mut self, capture: usize, operands: &[Qubit]) -> Result<(), SemanticError> {
        for operand in operands {
            self.check(operand.owner)?;
        }
        self.push(S::Payload {
            capture,
            operands: operands.iter().map(|q| q.expression.clone()).collect(),
        });
        Ok(())
    }
}

/// A subroutine borrowing fixed-size typed array storage.
#[derive(Debug, Clone)]
pub struct ArrayFunction<T: Classical, R: Classical> {
    owner: ProgramId,
    name: String,
    marker: PhantomData<(T, R)>,
}
impl Builder {
    /// Define a subroutine with an explicit array reference and scalar return.
    /// The shared checker enforces the declared reference mutability and fixed shape.
    /// # Errors
    /// Rejects foreign handles, incompatible types or dimensions, and configured construction limits.
    pub fn define_array_function<T: Classical, R: Classical>(
        &mut self,
        name: &str,
        length: usize,
        mutable: bool,
        body: impl FnOnce(&mut Self, Array<T>) -> Result<Expr<R>, SemanticError>,
    ) -> Result<ArrayFunction<T, R>, SemanticError> {
        if self.depth != 0 || length == 0 {
            return Err(SemanticError::new(
                ErrorKind::Type,
                "array function requires module scope and a nonempty reference",
            ));
        }
        let name = self.symbol(name)?;
        let parameter = self.symbol("array")?;
        let reference = Array {
            owner: self.owner,
            name: parameter.clone(),
            marker: PhantomData,
        };
        let body = self.body(|b| {
            let result = body(b, reference)?;
            b.check(result.owner)?;
            b.push(S::Return(Some(
                result.expression.materialize(b.expression_limits)?,
            )));
            Ok(())
        })?;
        self.push(S::Subroutine {
            name: name.clone(),
            parameters: vec![syntax::Parameter {
                name: parameter,
                ty: syntax::Type::Array {
                    element: Box::new(T::syntax_type()?),
                    dimensions: vec![expression(E::Number(length.to_string()))],
                    reference: true,
                },
                mutable,
            }],
            result: Some(R::syntax_type()?),
            body,
        });
        Ok(ArrayFunction {
            owner: self.owner,
            name,
            marker: PhantomData,
        })
    }
    /// Call an array-reference subroutine without copying the caller's storage.
    /// # Errors
    /// Rejects foreign handles, incompatible types or dimensions, and configured construction limits.
    pub fn call_array_function<T: Classical, R: Classical>(
        &self,
        function: &ArrayFunction<T, R>,
        array: &Array<T>,
    ) -> Result<Expr<R>, SemanticError> {
        self.check(function.owner)?;
        self.check(array.owner)?;
        Ok(self.wrap(SharedExpression::leaf(
            expression(E::Call(
                function.name.clone(),
                vec![expression(E::Name(array.name.clone()))],
            )),
            self.expression_limits,
        )?))
    }
}
