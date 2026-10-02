use crate::{Error, Left, LogicalSpace, NumericalPolicy, OracleFragment, Result, Right};
use std::sync::Arc;

/// Positive physical normalization; never silently replaced by its square.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Normalization(f64);
impl Normalization {
    /// # Errors
    /// Rejects zero, negative and nonfinite normalizations.
    pub const fn new(value: f64) -> Result<Self> {
        if !value.is_finite() || value <= 0.0 {
            Err(Error::Encoding("normalization must be positive and finite"))
        } else {
            Ok(Self(value))
        }
    }
    #[must_use]
    pub const fn get(self) -> f64 {
        self.0
    }
}
/// Builder marker for a required input that has not yet been supplied.
#[derive(Debug)]
pub struct Missing;
/// Builder marker owning one supplied input.
#[derive(Debug)]
pub struct Present<T>(T);

/// Configure the oracle, two independent logical spaces and normalization in
/// any order. Only a fully supplied builder can publish an encoding.
///
/// ```compile_fail
/// quest_qsvt::EncodingBuilder::new().normalization(1.0).unwrap().build();
/// ```
/// The right interface cannot be substituted for a left interface:
/// ```compile_fail
/// let right=quest_qsvt::LogicalSpace::<quest_qsvt::Right>::coordinates(
///     2,&[0],quest_qsvt::NumericalPolicy::default()).unwrap();
/// quest_qsvt::EncodingBuilder::new().left(right);
/// ```
#[derive(Debug)]
pub struct EncodingBuilder<O = Missing, L = Missing, R = Missing, A = Missing, U = CheckUnitarity> {
    oracle: O,
    left: L,
    right: R,
    normalization: A,
    unitarity: U,
    policy: NumericalPolicy,
}
impl Default for EncodingBuilder {
    fn default() -> Self {
        Self::new()
    }
}
impl EncodingBuilder {
    /// Start a builder with fixed whole-oracle numerical admission by default.
    #[must_use]
    pub fn new() -> Self {
        Self {
            oracle: Missing,
            left: Missing,
            right: Missing,
            normalization: Missing,
            unitarity: CheckUnitarity,
            policy: NumericalPolicy::default(),
        }
    }
}
impl<L, R, A, U> EncodingBuilder<Missing, L, R, A, U> {
    /// Supply the coherent oracle, retaining its immutable shared circuit body.
    #[must_use]
    pub fn oracle(
        self,
        oracle: OracleFragment,
    ) -> EncodingBuilder<Present<OracleFragment>, L, R, A, U> {
        EncodingBuilder {
            oracle: Present(oracle),
            left: self.left,
            right: self.right,
            normalization: self.normalization,
            policy: self.policy,
            unitarity: self.unitarity,
        }
    }
}
impl<O, R, A, U> EncodingBuilder<O, Missing, R, A, U> {
    /// Supply the ordered logical row/output embedding.
    #[must_use]
    pub fn left(
        self,
        left: LogicalSpace<Left>,
    ) -> EncodingBuilder<O, Present<LogicalSpace<Left>>, R, A, U> {
        EncodingBuilder {
            oracle: self.oracle,
            left: Present(left),
            right: self.right,
            normalization: self.normalization,
            policy: self.policy,
            unitarity: self.unitarity,
        }
    }
}
impl<O, L, A, U> EncodingBuilder<O, L, Missing, A, U> {
    /// Supply the ordered logical column/input embedding.
    #[must_use]
    pub fn right(
        self,
        right: LogicalSpace<Right>,
    ) -> EncodingBuilder<O, L, Present<LogicalSpace<Right>>, A, U> {
        EncodingBuilder {
            oracle: self.oracle,
            left: self.left,
            right: Present(right),
            normalization: self.normalization,
            policy: self.policy,
            unitarity: self.unitarity,
        }
    }
}
impl<O, L, R, U> EncodingBuilder<O, L, R, Missing, U> {
    /// Supply the physical scale in `A = alpha L† U R`.
    /// # Errors
    /// Rejects a nonpositive or nonfinite normalization.
    pub fn normalization(
        self,
        value: f64,
    ) -> Result<EncodingBuilder<O, L, R, Present<Normalization>, U>> {
        Ok(EncodingBuilder {
            oracle: self.oracle,
            left: self.left,
            right: self.right,
            normalization: Present(Normalization::new(value)?),
            policy: self.policy,
            unitarity: self.unitarity,
        })
    }
}
impl<O, L, R, A, U> EncodingBuilder<O, L, R, A, U> {
    /// Set retained-storage and cold-reference budgets without changing tolerances.
    #[must_use]
    pub const fn policy(mut self, policy: NumericalPolicy) -> Self {
        self.policy = policy;
        self
    }
}
/// Default construction state: measure the whole numerical oracle.
#[derive(Debug)]
pub struct CheckUnitarity;
/// Caller-supplied mathematical premise. No exact inverse, cancellation, or
/// theorem error certificate follows from this declaration.
#[derive(Debug, Clone)]
pub struct ExplicitUnitaryPremise(Arc<str>);
impl ExplicitUnitaryPremise {
    /// # Errors
    /// Rejects an empty description of the external mathematical premise.
    pub fn new(description: impl Into<String>) -> Result<Self> {
        let description = description.into();
        if description.trim().is_empty() {
            return Err(Error::Encoding("unitarity premise requires a description"));
        }
        Ok(Self(Arc::from(description)))
    }
    #[must_use]
    pub fn description(&self) -> &str {
        &self.0
    }
}
/// Builder state selecting an explicitly recorded whole-oracle premise.
#[derive(Debug)]
pub struct AssumeUnitarity(ExplicitUnitaryPremise);
/// Provenance for whole-oracle admission, separate from individual gate checks.
#[derive(Debug, Clone)]
pub enum OracleUnitarityEvidence {
    /// A finite whole-oracle residual admitted at the library's fixed tolerance.
    Measured { residual: f64 },
    /// An external caller premise, not a numerical measurement.
    Assumed(ExplicitUnitaryPremise),
}
impl<O, L, R, A> EncodingBuilder<O, L, R, A, CheckUnitarity> {
    /// Replace dense whole-oracle measurement with a named external premise.
    ///
    /// Dimension and retained-storage checks remain mandatory. This declaration
    /// grants no exact inverse/cancellation capability or theorem certificate.
    #[must_use]
    pub fn unitarity_assumption(
        self,
        premise: ExplicitUnitaryPremise,
    ) -> EncodingBuilder<O, L, R, A, AssumeUnitarity> {
        EncodingBuilder {
            oracle: self.oracle,
            left: self.left,
            right: self.right,
            normalization: self.normalization,
            policy: self.policy,
            unitarity: AssumeUnitarity(premise),
        }
    }
}
type CompleteEncoding<U> = EncodingBuilder<
    Present<OracleFragment>,
    Present<LogicalSpace<Left>>,
    Present<LogicalSpace<Right>>,
    Present<Normalization>,
    U,
>;
impl CompleteEncoding<CheckUnitarity> {
    /// Measure whole-oracle unitarity at fixed 1e-12 numerical tolerance.
    /// # Errors
    /// Rejects dimensions, numerical admission failures, and construction budgets.
    pub fn build(self) -> Result<ProjectedEncoding> {
        let (oracle, left, right, normalization, policy) = self.parts()?;
        let matrix = crate::materialize_oracle(&oracle, policy)?;
        let numerical =
            quest_compile::NumericalOperator::from_view(matrix.as_ref(), policy.matrix_policy())?;
        let residual = numerical.unitarity_residual(policy.matrix_policy())?;
        if !residual.is_finite() || residual > 1e-12 {
            return Err(Error::Residual {
                operation: "whole oracle unitarity",
                residual,
                tolerance: 1e-12,
            });
        }
        Ok(ProjectedEncoding {
            oracle,
            left,
            right,
            normalization,
            policy,
            unitarity: OracleUnitarityEvidence::Measured { residual },
        })
    }
}
impl CompleteEncoding<AssumeUnitarity> {
    /// Publish with the explicitly recorded external whole-oracle premise.
    /// # Errors
    /// Rejects inconsistent dimensions and retained storage budgets.
    pub fn build(self) -> Result<ProjectedEncoding> {
        let premise = self.unitarity.0.clone();
        let (oracle, left, right, normalization, policy) = self.parts()?;
        Ok(ProjectedEncoding {
            oracle,
            left,
            right,
            normalization,
            policy,
            unitarity: OracleUnitarityEvidence::Assumed(premise),
        })
    }
}
type EncodingParts = (
    OracleFragment,
    LogicalSpace<Left>,
    LogicalSpace<Right>,
    Normalization,
    NumericalPolicy,
);
impl<U> CompleteEncoding<U> {
    fn parts(self) -> Result<EncodingParts> {
        let dimension = 1usize
            .checked_shl(
                u32::try_from(self.oracle.0.num_qubits())
                    .map_err(|_| Error::Budget("oracle qubits"))?,
            )
            .ok_or(Error::Budget("oracle dimension"))?;
        if dimension != self.left.0.physical_dimension()
            || dimension != self.right.0.physical_dimension()
        {
            return Err(Error::Encoding(
                "logical physical dimensions do not match oracle",
            ));
        }
        let storage = self
            .left
            .0
            .storage_bytes()?
            .checked_add(self.right.0.storage_bytes()?)
            .and_then(|bytes| {
                bytes.checked_add(OracleFragment::shared_storage_bytes([&self.oracle.0]).ok()?)
            })
            .ok_or(Error::Budget("encoding storage"))?;
        if storage > self.policy.max_bytes {
            return Err(Error::Budget("encoding retained storage"));
        }
        Ok((
            self.oracle.0,
            self.left.0,
            self.right.0,
            self.normalization.0,
            self.policy,
        ))
    }
}

/// Projected unitary encoding `alpha V_left† U V_right`. Construction residuals
/// do not promote an approximately admitted oracle to exact symbolic semantics.
///
/// Left and right logical dimensions are independent. Clones share immutable
/// circuit and logical-space payloads; they do not initialize native resources.
#[derive(Debug, Clone)]
pub struct ProjectedEncoding {
    oracle: OracleFragment,
    left: LogicalSpace<Left>,
    right: LogicalSpace<Right>,
    normalization: Normalization,
    unitarity: OracleUnitarityEvidence,
    policy: NumericalPolicy,
}
impl ProjectedEncoding {
    /// Materialize the physical logical block, including normalization.
    /// # Errors
    /// Rejects cold construction budgets and numerical dimension mismatches.
    pub fn logical_matrix(&self) -> Result<faer::Mat<crate::Complex64>> {
        use std::ops::Mul;
        let oracle = crate::materialize_oracle(&self.oracle, self.policy)?;
        let left = self.left.isometry_snapshot(self.policy)?;
        let right = self.right.isometry_snapshot(self.policy)?;
        let applied = crate::matrix::multiply(oracle.as_ref(), right.as_ref(), self.policy)?;
        let mut block = crate::matrix::multiply(left.adjoint(), applied.as_ref(), self.policy)?;
        for col in 0..block.ncols() {
            for row in 0..block.nrows() {
                block[(row, col)] = block[(row, col)].mul(self.normalization.get());
            }
        }
        Ok(block)
    }
    #[must_use]
    pub const fn unitarity_evidence(&self) -> &OracleUnitarityEvidence {
        &self.unitarity
    }
    #[must_use]
    pub const fn oracle(&self) -> &OracleFragment {
        &self.oracle
    }
    #[must_use]
    pub const fn left(&self) -> &LogicalSpace<Left> {
        &self.left
    }
    #[must_use]
    pub const fn right(&self) -> &LogicalSpace<Right> {
        &self.right
    }
    #[must_use]
    pub const fn normalization(&self) -> Normalization {
        self.normalization
    }
    #[must_use]
    pub const fn policy(&self) -> NumericalPolicy {
        self.policy
    }
    #[must_use]
    pub fn num_qubits(&self) -> usize {
        self.oracle.num_qubits()
    }
}
