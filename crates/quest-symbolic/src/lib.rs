#![forbid(unsafe_code)]
#![expect(
    clippy::missing_errors_doc,
    clippy::must_use_candidate,
    reason = "Quest-symbolic is an internal algebra boundary; circuit constructors document public admission"
)]
//! Quest-owned exact angle obligations around the audited affine engine.
//!
//! The project-owned affine engine canonicalizes algebra. Source nodes remain shared and immutable so
//! cancellation cannot discard a parameter or a finite-conversion obligation.

use dashu_base::BitTest;
mod affine;
mod source;

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use affine::{Affine, Context, Limits, Owner as CoreOwner, Symbol as CoreSymbol};
use dashu_int::IBig;
use source::{Kind, Linear, MAX_INPUT_BINDINGS, Source, rational_bytes};

pub use affine::ExactError;
pub use dashu_ratio::RBig;

/// Quest's parameter namespace, independent of the algebra engine's type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Owner(u64);
impl Owner {
    pub const fn new(id: u64) -> Self {
        Self(id)
    }
    pub const fn id(self) -> u64 {
        self.0
    }
    const fn core(self) -> CoreOwner {
        CoreOwner::new(self.0)
    }
}
/// Quest's owned parameter identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Symbol {
    owner: Owner,
    index: u64,
}
impl Symbol {
    pub const fn new(owner: Owner, index: u64) -> Self {
        Self { owner, index }
    }
    pub const fn owner(self) -> Owner {
        self.owner
    }
    pub const fn index(self) -> u64 {
        self.index
    }
    const fn core(self) -> CoreSymbol {
        CoreSymbol::new(self.owner.core(), self.index)
    }
    const fn from_core(value: CoreSymbol) -> Self {
        Self::new(Owner::new(value.owner().id()), value.index())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    Exact(ExactError),
    SourceLimit,
    ForeignOwner,
    MissingBinding,
    DuplicateBinding,
    IncorrectAlgebra,
    ZeroDenominator,
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}
impl From<ExactError> for Error {
    fn from(value: ExactError) -> Self {
        Self::Exact(value)
    }
}
pub type Result<T> = std::result::Result<T, Error>;

/// A checked canonical affine expression with retained source obligations.
#[derive(Debug, Clone)]
pub struct Expr {
    context: Context,
    affine: Affine,
    source: Arc<Source>,
    summary: Arc<Linear>,
}
impl PartialEq for Expr {
    fn eq(&self, other: &Self) -> bool {
        self.affine == other.affine && Source::equivalent(&self.source, &other.source)
    }
}
impl Expr {
    /// Number of logical source obligations traversed by checked equivalence.
    #[must_use]
    pub fn source_node_count(&self) -> usize {
        self.source.logical_nodes()
    }
    /// Logical optimizer allowance for one source replay, substitution, and
    /// up to six interval-refinement attempts per original node. The fixed
    /// multiplier bounds ledger admission; it is not a CPU-instruction count.
    /// Cached pi-bound setup is shared across all angles.
    pub fn binding_work_estimate(&self) -> Result<u64> {
        let nodes = u64::try_from(self.source.logical_nodes()).map_err(|_| Error::SourceLimit)?;
        let terms = u64::try_from(self.affine.terms().count()).map_err(|_| Error::SourceLimit)?;
        nodes
            .checked_mul(16_384)
            .and_then(|work| {
                terms
                    .checked_mul(256)
                    .and_then(|terms| work.checked_add(terms))
            })
            .and_then(|work| work.checked_add(1))
            .ok_or(Error::SourceLimit)
    }
    /// Conservative retained bytes, counting shared source uses separately.
    pub fn retained_bytes(&self) -> Result<usize> {
        let mut bytes = std::mem::size_of::<Self>()
            .checked_add(std::mem::size_of::<Affine>())
            .and_then(|x| x.checked_add(const { 4 * std::mem::size_of::<usize>() }))
            .and_then(|x| {
                x.checked_add(
                    const { std::mem::size_of::<Linear>() + 2 * std::mem::size_of::<usize>() },
                )
            })
            .and_then(|x| x.checked_add(self.source.retained_bytes().ok()?))
            .ok_or(Error::SourceLimit)?;
        let add = |bytes: &mut usize, coefficient: &RBig| -> Result<()> {
            *bytes = bytes
                .checked_add(rational_bytes(coefficient)?)
                .ok_or(Error::SourceLimit)?;
            Ok(())
        };
        add(&mut bytes, self.affine.constant())?;
        add(&mut bytes, self.affine.pi_coefficient())?;
        for (_, coefficient) in self.affine.terms() {
            bytes = bytes
                .checked_add(const { 2 * std::mem::size_of::<(CoreSymbol, RBig)>() })
                .ok_or(Error::SourceLimit)?;
            add(&mut bytes, coefficient)?;
        }
        add(&mut bytes, &self.summary.radians)?;
        add(&mut bytes, &self.summary.pi)?;
        for coefficient in self.summary.terms.values() {
            bytes =
                bytes
                    .checked_add(
                        const {
                            std::mem::size_of::<(Symbol, RBig)>() + 5 * std::mem::size_of::<usize>()
                        },
                    )
                    .ok_or(Error::SourceLimit)?;
            add(&mut bytes, coefficient)?;
        }
        Ok(bytes)
    }
    /// Compare source obligations for a rewrite proof within the admitted source-node budget.
    pub fn equivalent_checked(&self, other: &Self) -> Result<bool> {
        if self.affine != other.affine {
            return Ok(false);
        }
        Source::equivalent_checked(&self.source, &other.source)
    }
    /// An exact rational-radian source leaf.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "Constructors take ownership of admitted coefficients"
    )]
    pub fn radians(value: RBig) -> Result<Self> {
        let value = admit_rational(&value)?;
        let context = Context::new(Owner::new(0).core());
        let affine = context.assemble(value.clone(), RBig::ZERO, vec![])?;
        Ok(Self {
            context,
            affine,
            source: Source::leaf(Kind::Radians(admit_rational(&value)?)),
            summary: Arc::new(Linear {
                radians: value,
                ..Linear::default()
            }),
        })
    }
    /// An exact rational multiple of mathematical pi source leaf.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "Constructors take ownership of admitted coefficients"
    )]
    pub fn pi(value: RBig) -> Result<Self> {
        let value = admit_rational(&value)?;
        let context = Context::new(Owner::new(0).core());
        let affine = context.assemble(RBig::ZERO, value.clone(), vec![])?;
        Ok(Self {
            context,
            affine,
            source: Source::leaf(Kind::Pi(admit_rational(&value)?)),
            summary: Arc::new(Linear {
                pi: value,
                ..Linear::default()
            }),
        })
    }
    /// Two exact coefficients retained as independent source leaves.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "Constructors take ownership of admitted coefficients"
    )]
    pub fn affine(radians: RBig, pi: RBig) -> Result<Self> {
        let radians = admit_rational(&radians)?;
        let pi = admit_rational(&pi)?;
        let context = Context::new(Owner::new(0).core());
        let affine = context.assemble(radians.clone(), pi.clone(), vec![])?;
        Ok(Self {
            context,
            affine,
            source: Source::leaf(Kind::Affine(
                admit_rational(&radians)?,
                admit_rational(&pi)?,
            )),
            summary: Arc::new(Linear {
                radians,
                pi,
                ..Linear::default()
            }),
        })
    }
    /// A symbol with an explicit program owner.
    pub fn parameter(symbol: Symbol) -> Result<Self> {
        let context = Context::new(symbol.owner().core());
        let affine = context.symbol(symbol.core())?;
        Ok(Self {
            context,
            affine,
            source: Source::leaf(Kind::Parameter(symbol)),
            summary: Arc::new(Linear {
                terms: BTreeMap::from([(symbol, RBig::ONE)]),
                ..Linear::default()
            }),
        })
    }
    pub const fn owner(&self) -> Owner {
        Owner::new(self.affine.owner().id())
    }
    pub fn constant(&self) -> &RBig {
        self.affine.constant()
    }
    pub fn pi_coefficient(&self) -> &RBig {
        self.affine.pi_coefficient()
    }
    /// Independently maintained exact source summary for a constant expression.
    /// This reads the replay summary, without invoking the checked affine
    /// normalizer; every constructor and combine operation checks it against
    /// the canonicalizer result before publishing the expression.
    /// A constant summary does not erase cancelled symbols' ownership, finite
    /// binding requirements, or the original source conversion obligations.
    #[must_use]
    pub fn independent_constant_summary(&self) -> Option<(&RBig, &RBig)> {
        self.summary
            .terms
            .is_empty()
            .then_some((&self.summary.radians, &self.summary.pi))
    }
    pub fn terms(&self) -> impl Iterator<Item = (Symbol, &RBig)> {
        self.affine
            .terms()
            .map(|(symbol, value)| (Symbol::from_core(*symbol), value))
    }
    pub fn is_zero(&self) -> bool {
        self.affine.is_zero()
    }
    pub fn has_source_parameters(&self) -> bool {
        self.source.has_parameters()
    }
    pub fn neg(&self) -> Result<Self> {
        let affine = self.affine.neg()?;
        let source = Source::negative(&self.source)?;
        let result = Self {
            context: self.context.clone(),
            affine,
            source,
            summary: Arc::new((*self.summary).clone().scale(&RBig::NEG_ONE)?),
        };
        result.verify_inductive()?;
        Ok(result)
    }
    /// Add after checked rehoming; equal owner IDs alone do not share a context.
    pub fn add(&self, other: &Self) -> Result<Self> {
        let (context, left, right) =
            if self.affine.terms().next().is_none() && other.affine.terms().next().is_some() {
                (
                    other.context.clone(),
                    rehome(&self.affine, &other.context)?,
                    other.affine.clone(),
                )
            } else {
                (
                    self.context.clone(),
                    self.affine.clone(),
                    rehome(&other.affine, &self.context)?,
                )
            };
        let affine = left.add(&right)?;
        let source = Source::sum(&self.source, &other.source)?;
        let result = Self {
            context,
            affine,
            source,
            summary: Arc::new((*self.summary).clone().add((*other.summary).clone())?),
        };
        result.verify_inductive()?;
        Ok(result)
    }
    pub fn scale_ratio(&self, numerator: IBig, denominator: IBig) -> Result<Self> {
        if denominator.is_zero() {
            return Err(Error::ZeroDenominator);
        }
        if u64::try_from(numerator.bit_len())
            .unwrap_or(u64::MAX)
            .max(u64::try_from(denominator.bit_len()).unwrap_or(u64::MAX))
            > Limits::default().max_coefficient_bits
        {
            return Err(Error::Exact(ExactError::CoefficientLimit));
        }
        let affine = self
            .affine
            .scale_ratio(numerator.clone(), denominator.clone())?;
        let factor = RBig::from_parts_signed(numerator, denominator);
        let source = Source::scale(&self.source, factor.clone())?;
        let result = Self {
            context: self.context.clone(),
            affine,
            source,
            summary: Arc::new((*self.summary).clone().scale(&factor)?),
        };
        result.verify_inductive()?;
        Ok(result)
    }
    /// Simultaneous substitution into a new program owner.
    pub fn substitute_into(&self, owner: Owner, replacements: &[(Symbol, Self)]) -> Result<Self> {
        if replacements.len() > MAX_INPUT_BINDINGS {
            return Err(Error::SourceLimit);
        }
        let needed = self.source.parameters();
        let mut by_symbol = BTreeMap::new();
        for (symbol, value) in replacements {
            if !needed.contains(symbol) {
                continue;
            }
            if by_symbol.insert(*symbol, value).is_some() {
                return Err(Error::DuplicateBinding);
            }
            value.validate_owner(owner)?;
        }
        if needed.iter().any(|symbol| !by_symbol.contains_key(symbol)) {
            return Err(Error::MissingBinding);
        }
        let context = Context::new(owner.core());
        let mapping = by_symbol
            .iter()
            .filter(|(symbol, _)| self.affine.terms().any(|(term, _)| *term == symbol.core()))
            .map(|(symbol, value)| Ok((symbol.core(), rehome(&value.affine, &context)?)))
            .collect::<Result<Vec<_>>>()?;
        let affine = self.affine.substitute_into(&context, &mapping)?;
        let sources = by_symbol
            .into_iter()
            .map(|(symbol, value)| (symbol, Arc::clone(&value.source)))
            .collect();
        let source = Source::substitute(&self.source, &sources)?;
        let summary = source.replay(&BTreeMap::new())?;
        let result = Self {
            context,
            affine,
            source,
            summary: Arc::new(summary),
        };
        result.verify_inductive()?;
        Ok(result)
    }
    /// Parameters remain visible after canonical cancellation.
    pub fn parameters(&self) -> impl Iterator<Item = Symbol> {
        self.source.parameters().into_iter()
    }
    pub fn validate_owner(&self, owner: Owner) -> Result<()> {
        if self.parameters().any(|symbol| symbol.owner() != owner) {
            Err(Error::ForeignOwner)
        } else {
            Ok(())
        }
    }
    /// Check every original pi conversion, including cancelled leaves.
    pub fn check_pi_leaves<E>(
        &self,
        check: impl FnMut(&RBig) -> std::result::Result<(), E>,
    ) -> std::result::Result<(), E> {
        self.source.pi_leaves(check)
    }
    /// Unmodified parameter source leaf, optionally negated.
    pub fn signed_parameter_leaf(&self) -> Option<(Symbol, bool)> {
        match &self.source.kind {
            Kind::Parameter(symbol) => Some((*symbol, false)),
            Kind::Negative(child) => match &child.kind {
                Kind::Parameter(symbol) => Some((*symbol, true)),
                _ => None,
            },
            _ => None,
        }
    }
    /// Unmodified pi source leaf retaining its original conversion obligation.
    pub fn pi_leaf(&self) -> Option<&RBig> {
        if let Kind::Pi(value) = &self.source.kind {
            Some(value)
        } else {
            None
        }
    }
    /// A pi source leaf or its direct negation retains `RationalPi` identity.
    pub fn pi_identity(&self) -> Option<RBig> {
        match &self.source.kind {
            Kind::Pi(value) => Some(value.clone()),
            Kind::Negative(child) => match &child.kind {
                Kind::Pi(value) => Some(std::ops::Neg::neg(value)),
                _ => None,
            },
            _ => None,
        }
    }
    /// Independently reconstruct all coefficients from source nodes.
    pub fn verify(&self) -> Result<()> {
        let replay = self.source.replay(&BTreeMap::new())?;
        self.compare_summary(&replay)?;
        self.verify_inductive()
    }
    fn compare_summary(&self, replay: &Linear) -> Result<()> {
        if replay.radians != self.summary.radians
            || replay.pi != self.summary.pi
            || replay.terms != self.summary.terms
        {
            return Err(Error::IncorrectAlgebra);
        }
        Ok(())
    }
    fn verify_inductive(&self) -> Result<()> {
        let replay = &self.summary;
        if replay.radians != *self.affine.constant()
            || replay.pi != *self.affine.pi_coefficient()
            || replay.terms.len() != self.affine.terms().count()
            || self
                .affine
                .terms()
                .any(|(symbol, value)| replay.terms.get(&Symbol::from_core(*symbol)) != Some(value))
        {
            return Err(Error::IncorrectAlgebra);
        }
        Ok(())
    }
    /// Bind exact rational-radian values and independently check the canonicalizer result.
    pub fn bind(&self, bindings: &[(Symbol, RBig)]) -> Result<(RBig, RBig)> {
        self.bind_checked(bindings, |_, _| Ok(()))
    }
    /// Bind while checking the finite-conversion obligation of every original source node.
    pub fn bind_checked<E: From<Error>>(
        &self,
        bindings: &[(Symbol, RBig)],
        check: impl FnMut(&RBig, &RBig) -> std::result::Result<(), E>,
    ) -> std::result::Result<(RBig, RBig), E> {
        if bindings.len() > MAX_INPUT_BINDINGS {
            return Err(Error::SourceLimit.into());
        }
        let needed = self.source.parameters();
        let mut values = BTreeMap::new();
        let mut estimated_bytes = 0u64;
        for (symbol, value) in bindings {
            if !needed.contains(symbol) {
                continue;
            }
            let bits = u64::try_from(value.numerator().bit_len())
                .unwrap_or(u64::MAX)
                .checked_add(u64::try_from(value.denominator().bit_len()).unwrap_or(u64::MAX))
                .ok_or(Error::SourceLimit)?;
            if u64::try_from(value.numerator().bit_len())
                .unwrap_or(u64::MAX)
                .max(u64::try_from(value.denominator().bit_len()).unwrap_or(u64::MAX))
                > Limits::default().max_coefficient_bits
            {
                return Err(Error::Exact(ExactError::CoefficientLimit).into());
            }
            estimated_bytes = estimated_bytes
                .checked_add(bits.div_ceil(8))
                .and_then(|x| x.checked_add(64))
                .ok_or(Error::SourceLimit)?;
            if estimated_bytes > Limits::default().max_bytes {
                return Err(Error::Exact(ExactError::StorageLimit).into());
            }
            if values.insert(*symbol, admit_rational(value)?).is_some() {
                return Err(Error::DuplicateBinding.into());
            }
        }
        if needed.iter().any(|symbol| !values.contains_key(symbol)) {
            return Err(Error::MissingBinding.into());
        }
        let context = Context::new(Owner::new(0).core());
        let mapping = values
            .iter()
            .filter(|(symbol, _)| self.affine.terms().any(|(term, _)| *term == symbol.core()))
            .map(|(symbol, value)| {
                Ok((
                    symbol.core(),
                    context.assemble(value.clone(), RBig::ZERO, vec![])?,
                ))
            })
            .collect::<Result<Vec<_>>>()?;
        let bound = self
            .affine
            .substitute_into(&context, &mapping)
            .map_err(Error::from)?;
        let replay = self.source.replay_checked(&values, check)?;
        if !replay.terms.is_empty()
            || replay.radians != *bound.constant()
            || replay.pi != *bound.pi_coefficient()
        {
            return Err(Error::IncorrectAlgebra.into());
        }
        Ok((bound.constant().clone(), bound.pi_coefficient().clone()))
    }
}

fn admit_rational(value: &RBig) -> Result<RBig> {
    if u64::try_from(value.numerator().bit_len())
        .unwrap_or(u64::MAX)
        .max(u64::try_from(value.denominator().bit_len()).unwrap_or(u64::MAX))
        > Limits::default().max_coefficient_bits
    {
        return Err(Error::Exact(ExactError::CoefficientLimit));
    }
    Ok(value.clone())
}
fn rehome(value: &Affine, context: &Context) -> Result<Affine> {
    if value
        .terms()
        .any(|(symbol, _)| symbol.owner() != context.owner())
    {
        return Err(Error::ForeignOwner);
    }
    Ok(context.assemble(
        value.constant().clone(),
        value.pi_coefficient().clone(),
        value
            .terms()
            .map(|(symbol, coefficient)| (*symbol, coefficient.clone()))
            .collect(),
    )?)
}

#[cfg(test)]
mod tests {
    use super::{Error, Expr, Owner, RBig, Symbol};
    use dashu_int::IBig;
    use googletest::{Result, prelude::*};

    #[gtest]
    fn independent_replay_rejects_incorrect_canonicalizer_coefficients() -> Result<()> {
        let original = Expr::pi(RBig::ONE)?;
        let forged = Expr {
            context: original.context.clone(),
            affine: original.context.zero()?,
            source: original.source,
            summary: original.summary,
        };
        expect_true!(matches!(forged.verify(), Err(Error::IncorrectAlgebra)));
        Ok(())
    }

    #[gtest]
    fn shared_doubling_keeps_equality_bounded_and_source_obligations() -> Result<()> {
        let symbol = Symbol::new(Owner::new(7), 3);
        let mut expression = Expr::parameter(symbol)?;
        for _ in 0..10 {
            expression = expression.add(&expression)?;
        }
        expect_eq!(expression.clone(), expression);
        expect_eq!(expression.parameters().collect::<Vec<_>>(), vec![symbol]);
        expression.verify()?;
        Ok(())
    }

    #[gtest]
    fn independent_shared_dag_comparison_is_bounded() -> Result<()> {
        let symbol = Symbol::new(Owner::new(7), 3);
        let mut left = Expr::parameter(symbol)?;
        let mut right = Expr::parameter(symbol)?;
        for _ in 0..10 {
            left = left.add(&left)?;
            right = right.add(&right)?;
        }
        expect_true!(left.equivalent_checked(&right)?);
        Ok(())
    }

    #[gtest]
    fn binding_ignores_unrelated_program_parameters() -> Result<()> {
        let a = Symbol::new(Owner::new(7), 1);
        let b = Symbol::new(Owner::new(7), 2);
        let expression = Expr::parameter(a)?;
        let value = RBig::from(3);
        expect_eq!(
            expression.bind(&[(a, value.clone()), (b, RBig::from(5))])?,
            (value, RBig::ZERO)
        );
        let constant = Expr::pi(RBig::ONE)?;
        expect_eq!(
            constant.bind(&[(a, RBig::from(2))])?,
            (RBig::ZERO, RBig::ONE)
        );
        Ok(())
    }

    #[gtest]
    fn independent_replay_admits_exact_cancellation_at_width_limit() -> Result<()> {
        let denominator = std::ops::Shl::shl(IBig::from(1), 16_000usize);
        let positive = Expr::radians(RBig::from_parts_signed(1.into(), denominator.clone()))?;
        let negative = Expr::radians(RBig::from_parts_signed((-1).into(), denominator))?;
        let zero = positive.add(&negative)?;
        expect_true!(zero.is_zero());
        zero.verify()?;
        Ok(())
    }
}
