//! Bounded, immutable affine expressions over exact rationals and symbolic pi.
//!
//! A value has the canonical form `r + q*pi + sum(c_i*s_i)`. Symbols carry an
//! explicit owner. A context supplies the owner and shared finite budgets;
//! cloning the context shares its counters. Constants can be imported into
//! another context with [`Context::from_parts`]. Symbolic values require an
//! explicit complete mapping with [`Affine::substitute_into`].

use num_bigint::BigInt;
use num_rational::Ratio;
use num_traits::{One, Zero};
use std::fmt;
use std::fmt::Write as _;
use std::mem::size_of;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

pub type Rational = Ratio<BigInt>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Owner(u64);
impl Owner {
    pub const fn new(id: u64) -> Self {
        Self(id)
    }
    pub const fn id(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
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
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    pub max_terms: usize,
    pub max_coefficient_bits: u64,
    pub max_bytes: u64,
    pub max_work: u64,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            max_terms: 4096,
            max_coefficient_bits: 16384,
            max_bytes: 64 * 1024 * 1024,
            max_work: 16 * 1024 * 1024,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Usage {
    pub work: u64,
    pub retained_bytes: u64,
    pub staged_bytes: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExactError {
    ZeroDenominator,
    ForeignSymbol,
    ForeignContext,
    UnmappedSymbol,
    DuplicateSubstitution,
    TermLimit,
    CoefficientLimit,
    StorageLimit,
    WorkLimit,
    InvalidLimits,
}
impl fmt::Display for ExactError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for ExactError {}

#[derive(Default, Debug)]
struct State {
    work: AtomicU64,
    total_bytes: AtomicU64,
    retained: AtomicU64,
    staged: AtomicU64,
}

#[derive(Clone, Debug)]
pub struct Context {
    owner: Owner,
    limits: Limits,
    state: Arc<State>,
}
impl Context {
    pub fn new(owner: Owner) -> Self {
        Self {
            owner,
            limits: Limits::default(),
            state: Arc::new(State::default()),
        }
    }
    pub fn with_limits(owner: Owner, limits: Limits) -> Result<Self, ExactError> {
        let hard = Limits::default();
        if limits.max_terms == 0
            || limits.max_coefficient_bits == 0
            || limits.max_bytes < 128
            || limits.max_work == 0
            || limits.max_terms > hard.max_terms
            || limits.max_coefficient_bits > hard.max_coefficient_bits
            || limits.max_bytes > hard.max_bytes
            || limits.max_work > hard.max_work
        {
            return Err(ExactError::InvalidLimits);
        }
        Ok(Self {
            owner,
            limits,
            state: Arc::new(State::default()),
        })
    }
    pub const fn owner(&self) -> Owner {
        self.owner
    }
    pub const fn limits(&self) -> Limits {
        self.limits
    }
    pub fn usage(&self) -> Usage {
        Usage {
            work: self.state.work.load(Ordering::Relaxed),
            retained_bytes: self.state.retained.load(Ordering::Relaxed),
            staged_bytes: self.state.staged.load(Ordering::Relaxed),
        }
    }
    pub fn zero(&self) -> Result<Affine, ExactError> {
        self.from_parts(Rational::zero(), Rational::zero(), Vec::new())
    }
    pub fn one(&self) -> Result<Affine, ExactError> {
        self.from_parts(Rational::one(), Rational::zero(), Vec::new())
    }
    pub fn ratio(&self, numerator: BigInt, denominator: BigInt) -> Result<Affine, ExactError> {
        let coefficient = self.make_ratio(numerator, denominator)?;
        self.from_parts(coefficient, Rational::zero(), Vec::new())
    }
    pub fn pi(&self) -> Result<Affine, ExactError> {
        self.from_parts(Rational::zero(), Rational::one(), Vec::new())
    }
    pub fn symbol(&self, symbol: Symbol) -> Result<Affine, ExactError> {
        self.from_parts(
            Rational::zero(),
            Rational::zero(),
            vec![(symbol, Rational::one())],
        )
    }
    pub fn from_parts(
        &self,
        constant: Rational,
        pi: Rational,
        mut terms: Vec<(Symbol, Rational)>,
    ) -> Result<Affine, ExactError> {
        if terms.len() as u64 > self.limits.max_work {
            return Err(ExactError::WorkLimit);
        }
        if terms.iter().any(|(s, _)| s.owner != self.owner) {
            return Err(ExactError::ForeignSymbol);
        }
        self.check_coefficient(&constant)?;
        self.check_coefficient(&pi)?;
        for (_, c) in &terms {
            self.check_coefficient(c)?;
        }
        self.charge_work(terms.len() as u64 + 1)?;
        self.charge_work(sort_work(terms.len())?)?;
        let _stage = self.stage(storage_bytes(&constant, &pi, &terms, terms.capacity())?)?;
        let constant = self.normalize(constant)?;
        let pi = self.normalize(pi)?;
        for (_, coefficient) in &mut terms {
            *coefficient = self.normalize(std::mem::replace(coefficient, Rational::zero()))?;
        }
        terms.sort_unstable_by_key(|(symbol, _)| *symbol);
        // Compact within the admitted input allocation. Building a second
        // vector would require another simultaneous storage reservation.
        terms.retain(|(_, coefficient)| !coefficient.is_zero());
        let mut failure = None;
        terms.dedup_by(|(symbol, coefficient), (previous, accumulated)| {
            if symbol != previous || failure.is_some() {
                return false;
            }
            match self.checked_add(accumulated, coefficient) {
                Ok(value) => {
                    *accumulated = value;
                    true
                }
                Err(error) => {
                    failure = Some(error);
                    false
                }
            }
        });
        if let Some(error) = failure {
            return Err(error);
        }
        terms.retain(|(_, coefficient)| !coefficient.is_zero());
        self.publish(constant, pi, terms)
    }
    fn make_ratio(&self, numerator: BigInt, denominator: BigInt) -> Result<Rational, ExactError> {
        if denominator.is_zero() {
            return Err(ExactError::ZeroDenominator);
        }
        self.check_int(&numerator)?;
        self.check_int(&denominator)?;
        self.charge_work((numerator.bits() + denominator.bits()) / 64 + 1)?;
        Ok(Rational::new(numerator, denominator))
    }
    fn check_int(&self, value: &BigInt) -> Result<(), ExactError> {
        if value.bits() > self.limits.max_coefficient_bits {
            Err(ExactError::CoefficientLimit)
        } else {
            Ok(())
        }
    }
    fn check_coefficient(&self, value: &Rational) -> Result<(), ExactError> {
        if value.denom().is_zero() {
            return Err(ExactError::ZeroDenominator);
        }
        self.check_int(value.numer())?;
        self.check_int(value.denom())
    }
    fn normalize(&self, value: Rational) -> Result<Rational, ExactError> {
        self.check_coefficient(&value)?;
        self.charge_work((value.numer().bits() + value.denom().bits()) / 64 + 1)?;
        let normalized = Rational::new(value.numer().clone(), value.denom().clone());
        self.check_coefficient(&normalized)?;
        Ok(normalized)
    }
    fn charge_work(&self, units: u64) -> Result<(), ExactError> {
        self.state
            .work
            .try_update(Ordering::AcqRel, Ordering::Relaxed, |old| {
                old.checked_add(units)
                    .filter(|new| *new <= self.limits.max_work)
            })
            .map_err(|_| ExactError::WorkLimit)?;
        Ok(())
    }
    fn checked_add(&self, a: &Rational, b: &Rational) -> Result<Rational, ExactError> {
        self.check_coefficient(a)?;
        self.check_coefficient(b)?;
        if a.is_zero() {
            return Ok(b.clone());
        }
        if b.is_zero() {
            return Ok(a.clone());
        }
        if a.numer() == &-b.numer() && a.denom() == b.denom() {
            return Ok(Rational::zero());
        }
        let max = self.limits.max_coefficient_bits;
        let left = a.numer().bits().saturating_add(b.denom().bits());
        let right = b.numer().bits().saturating_add(a.denom().bits());
        let denominator = a.denom().bits().saturating_add(b.denom().bits());
        if left.max(right).saturating_add(1) > max || denominator > max {
            return Err(ExactError::CoefficientLimit);
        }
        self.charge_work((left + right + denominator) / 64 + 1)?;
        let result = a + b;
        self.check_coefficient(&result)?;
        Ok(result)
    }
    fn checked_mul(&self, a: &Rational, b: &Rational) -> Result<Rational, ExactError> {
        self.check_coefficient(a)?;
        self.check_coefficient(b)?;
        if a.is_zero() || b.is_zero() {
            return Ok(Rational::zero());
        }
        if a.is_one() {
            return Ok(b.clone());
        }
        if b.is_one() {
            return Ok(a.clone());
        }
        if a == &-Rational::one() {
            return Ok(-b.clone());
        }
        if b == &-Rational::one() {
            return Ok(-a.clone());
        }
        let numerator = a.numer().bits().saturating_add(b.numer().bits());
        let denominator = a.denom().bits().saturating_add(b.denom().bits());
        if numerator > self.limits.max_coefficient_bits
            || denominator > self.limits.max_coefficient_bits
        {
            return Err(ExactError::CoefficientLimit);
        }
        self.charge_work((numerator + denominator) / 64 + 1)?;
        let result = a * b;
        self.check_coefficient(&result)?;
        Ok(result)
    }
    fn publish(
        &self,
        constant: Rational,
        pi: Rational,
        terms: Vec<(Symbol, Rational)>,
    ) -> Result<Affine, ExactError> {
        if terms.len() > self.limits.max_terms {
            return Err(ExactError::TermLimit);
        }
        self.check_coefficient(&constant)?;
        self.check_coefficient(&pi)?;
        for (_, coefficient) in &terms {
            self.check_coefficient(coefficient)?;
        }
        let bytes = storage_bytes(&constant, &pi, &terms, terms.capacity())?;
        self.reserve(&self.state.retained, bytes)?;
        Ok(Affine {
            context: self.clone(),
            data: Arc::new(Data {
                constant,
                pi,
                terms,
                bytes,
                state: self.state.clone(),
            }),
        })
    }
    fn reserve(&self, counter: &AtomicU64, bytes: u64) -> Result<(), ExactError> {
        loop {
            let old = self.state.total_bytes.load(Ordering::Acquire);
            let Some(new) = old
                .checked_add(bytes)
                .filter(|v| *v <= self.limits.max_bytes)
            else {
                return Err(ExactError::StorageLimit);
            };
            if self
                .state
                .total_bytes
                .compare_exchange_weak(old, new, Ordering::AcqRel, Ordering::Relaxed)
                .is_ok()
            {
                counter.fetch_add(bytes, Ordering::AcqRel);
                return Ok(());
            }
        }
    }
    fn stage(&self, bytes: u64) -> Result<Stage, ExactError> {
        self.reserve(&self.state.staged, bytes)?;
        Ok(Stage {
            bytes,
            state: self.state.clone(),
        })
    }
}

fn coefficient_bytes(value: &Rational) -> u64 {
    value.numer().bits().div_ceil(8) + value.denom().bits().div_ceil(8)
}
fn storage_bytes(
    constant: &Rational,
    pi: &Rational,
    terms: &[(Symbol, Rational)],
    capacity: usize,
) -> Result<u64, ExactError> {
    let allocation = capacity
        .checked_mul(size_of::<(Symbol, Rational)>())
        .ok_or(ExactError::StorageLimit)?;
    let base = size_of::<Data>()
        .checked_add(allocation)
        .ok_or(ExactError::StorageLimit)?;
    let base = u64::try_from(base).map_err(|_| ExactError::StorageLimit)?;
    terms.iter().try_fold(
        base + coefficient_bytes(constant) + coefficient_bytes(pi),
        |sum, (_, c)| {
            sum.checked_add(coefficient_bytes(c))
                .ok_or(ExactError::StorageLimit)
        },
    )
}
fn sort_work(count: usize) -> Result<u64, ExactError> {
    if count < 2 {
        return Ok(0);
    }
    let count = u64::try_from(count).map_err(|_| ExactError::WorkLimit)?;
    let levels = u64::from(u64::BITS - count.leading_zeros());
    count
        .checked_mul(levels)
        .and_then(|value| value.checked_mul(2))
        .ok_or(ExactError::WorkLimit)
}
struct Stage {
    bytes: u64,
    state: Arc<State>,
}
impl Drop for Stage {
    fn drop(&mut self) {
        self.state.staged.fetch_sub(self.bytes, Ordering::AcqRel);
        self.state
            .total_bytes
            .fetch_sub(self.bytes, Ordering::AcqRel);
    }
}
#[derive(Debug)]
struct Data {
    constant: Rational,
    pi: Rational,
    terms: Vec<(Symbol, Rational)>,
    bytes: u64,
    state: Arc<State>,
}
impl Drop for Data {
    fn drop(&mut self) {
        self.state.retained.fetch_sub(self.bytes, Ordering::AcqRel);
        self.state
            .total_bytes
            .fetch_sub(self.bytes, Ordering::AcqRel);
    }
}

/// A counted canonical text export. Its retained bytes are released on drop.
#[derive(Debug)]
pub struct Export {
    text: String,
    bytes: u64,
    state: Arc<State>,
}
impl Export {
    pub fn as_str(&self) -> &str {
        &self.text
    }
}
impl PartialEq for Export {
    fn eq(&self, other: &Self) -> bool {
        self.text == other.text
    }
}
impl Eq for Export {}
impl Drop for Export {
    fn drop(&mut self) {
        self.state.retained.fetch_sub(self.bytes, Ordering::AcqRel);
        self.state
            .total_bytes
            .fetch_sub(self.bytes, Ordering::AcqRel);
    }
}

fn decimal_digits_bound(bits: u64) -> Result<u64, ExactError> {
    bits.checked_mul(30_103)
        .and_then(|v| v.checked_add(99_999))
        .map(|v| v / 100_000 + 2)
        .ok_or(ExactError::StorageLimit)
}
fn rational_text_bound(value: &Rational) -> Result<u64, ExactError> {
    decimal_digits_bound(value.numer().bits())?
        .checked_add(decimal_digits_bound(value.denom().bits())?)
        .and_then(|v| v.checked_add(2))
        .ok_or(ExactError::StorageLimit)
}

#[derive(Clone, Debug)]
pub struct Affine {
    context: Context,
    data: Arc<Data>,
}
impl PartialEq for Affine {
    fn eq(&self, other: &Self) -> bool {
        self.data.constant == other.data.constant
            && self.data.pi == other.data.pi
            && self.data.terms == other.data.terms
    }
}
impl Eq for Affine {}
impl Affine {
    pub fn owner(&self) -> Owner {
        self.context.owner
    }
    pub fn constant(&self) -> &Rational {
        &self.data.constant
    }
    pub fn pi_coefficient(&self) -> &Rational {
        &self.data.pi
    }
    pub fn terms(&self) -> impl Iterator<Item = (&Symbol, &Rational)> {
        self.data.terms.iter().map(|(s, c)| (s, c))
    }
    pub fn is_zero(&self) -> bool {
        self.data.constant.is_zero() && self.data.pi.is_zero() && self.data.terms.is_empty()
    }
    pub fn is_one(&self) -> bool {
        self.data.constant.is_one() && self.data.pi.is_zero() && self.data.terms.is_empty()
    }
    pub fn neg(&self) -> Result<Self, ExactError> {
        self.scale_ratio((-1).into(), 1.into())
    }
    pub fn add(&self, other: &Self) -> Result<Self, ExactError> {
        self.combine(other, false)
    }
    pub fn sub(&self, other: &Self) -> Result<Self, ExactError> {
        self.combine(other, true)
    }
    fn combine(&self, other: &Self, subtract: bool) -> Result<Self, ExactError> {
        if !Arc::ptr_eq(&self.context.state, &other.context.state) && !other.data.terms.is_empty() {
            return Err(ExactError::ForeignContext);
        }
        let limit = self.context.limits.max_terms;
        let count = self.data.terms.len().saturating_add(other.data.terms.len());
        if count > limit.saturating_mul(2) {
            return Err(ExactError::TermLimit);
        }
        self.context.charge_work(count as u64 + 1)?;
        let estimate = self.data.bytes.saturating_add(other.data.bytes);
        let _stage = self.context.stage(estimate)?;
        let negative = |v: &Rational| if subtract { -v.clone() } else { v.clone() };
        let constant = self
            .context
            .checked_add(&self.data.constant, &negative(&other.data.constant))?;
        let pi = self
            .context
            .checked_add(&self.data.pi, &negative(&other.data.pi))?;
        let mut terms = Vec::new();
        terms
            .try_reserve_exact(count)
            .map_err(|_| ExactError::StorageLimit)?;
        let (mut i, mut j) = (0, 0);
        while i < self.data.terms.len() || j < other.data.terms.len() {
            let a = self.data.terms.get(i);
            let b = other.data.terms.get(j);
            let value = match (a, b) {
                (Some((sa, ca)), Some((sb, cb))) if sa == sb => {
                    i += 1;
                    j += 1;
                    (*sa, self.context.checked_add(ca, &negative(cb))?)
                }
                (Some((sa, ca)), Some((sb, _))) if sa < sb => {
                    i += 1;
                    (*sa, ca.clone())
                }
                (_, Some((sb, cb))) => {
                    j += 1;
                    (*sb, negative(cb))
                }
                (Some((sa, ca)), None) => {
                    i += 1;
                    (*sa, ca.clone())
                }
                (None, None) => unreachable!(),
            };
            if !value.1.is_zero() {
                terms.push(value);
            }
            if terms.len() > limit {
                return Err(ExactError::TermLimit);
            }
        }
        self.context.publish(constant, pi, terms)
    }
    pub fn scale_ratio(&self, numerator: BigInt, denominator: BigInt) -> Result<Self, ExactError> {
        let factor = self.context.make_ratio(numerator, denominator)?;
        self.scale(&factor)
    }
    pub fn divide_ratio(&self, numerator: BigInt, denominator: BigInt) -> Result<Self, ExactError> {
        if numerator.is_zero() || denominator.is_zero() {
            return Err(ExactError::ZeroDenominator);
        }
        let reciprocal = self.context.make_ratio(denominator, numerator)?;
        self.scale(&reciprocal)
    }
    fn scale(&self, factor: &Rational) -> Result<Self, ExactError> {
        self.context.charge_work(self.data.terms.len() as u64 + 1)?;
        let _stage = self.context.stage(self.data.bytes)?;
        let constant = self.context.checked_mul(&self.data.constant, factor)?;
        let pi = self.context.checked_mul(&self.data.pi, factor)?;
        let mut terms = Vec::new();
        terms
            .try_reserve_exact(self.data.terms.len())
            .map_err(|_| ExactError::StorageLimit)?;
        for (s, c) in &self.data.terms {
            let product = self.context.checked_mul(c, factor)?;
            if !product.is_zero() {
                terms.push((*s, product));
            }
        }
        self.context.publish(constant, pi, terms)
    }
    pub fn substitute(&self, replacements: &[(Symbol, Self)]) -> Result<Self, ExactError> {
        self.substitute_into(&self.context, replacements)
    }
    pub fn substitute_into(
        &self,
        target: &Context,
        replacements: &[(Symbol, Self)],
    ) -> Result<Self, ExactError> {
        let count = u64::try_from(replacements.len()).map_err(|_| ExactError::WorkLimit)?;
        if count > target.limits.max_work {
            return Err(ExactError::WorkLimit);
        }
        let comparisons = u64::from(u64::BITS - count.max(1).leading_zeros());
        let work = count
            .checked_add(u64::try_from(self.data.terms.len()).map_err(|_| ExactError::WorkLimit)?)
            .and_then(|v| v.checked_mul(comparisons))
            .ok_or(ExactError::WorkLimit)?;
        target.charge_work(work)?;
        let bytes = count
            .checked_mul(size_of::<(Symbol, Self)>() as u64)
            .ok_or(ExactError::StorageLimit)?;
        let _mapping_stage = target.stage(bytes)?;
        let mut mapping = replacements.to_vec();
        mapping.sort_unstable_by_key(|(s, _)| *s);
        for pair in mapping.windows(2) {
            if pair[0].0 == pair[1].0 {
                return Err(ExactError::DuplicateSubstitution);
            }
        }
        for (s, value) in &mapping {
            if s.owner != self.owner() {
                return Err(ExactError::ForeignSymbol);
            }
            if value.owner() != target.owner || !Arc::ptr_eq(&value.context.state, &target.state) {
                return Err(ExactError::ForeignContext);
            }
        }
        let mut output =
            target.from_parts(self.data.constant.clone(), self.data.pi.clone(), Vec::new())?;
        for (symbol, coeff) in &self.data.terms {
            let replacement = match mapping.binary_search_by_key(symbol, |(s, _)| *s) {
                Ok(index) => mapping[index].1.clone(),
                Err(_) if Arc::ptr_eq(&self.context.state, &target.state) => {
                    target.symbol(*symbol)?
                }
                Err(_) => return Err(ExactError::UnmappedSymbol),
            };
            output = output.add(&replacement.scale(coeff)?)?;
        }
        Ok(output)
    }
    pub fn export(&self) -> Result<Export, ExactError> {
        let mut estimate = 64u64
            .checked_add(rational_text_bound(self.constant())?)
            .and_then(|v| v.checked_add(rational_text_bound(self.pi_coefficient()).ok()?))
            .ok_or(ExactError::StorageLimit)?;
        for (_, coefficient) in &self.data.terms {
            estimate = estimate
                .checked_add(32)
                .and_then(|v| v.checked_add(rational_text_bound(coefficient).ok()?))
                .ok_or(ExactError::StorageLimit)?;
        }
        self.context.charge_work(estimate)?;
        let stage_bytes = estimate
            .checked_mul(2)
            .and_then(|v| v.checked_add(size_of::<Export>() as u64))
            .ok_or(ExactError::StorageLimit)?;
        let _stage = self.context.stage(stage_bytes)?;
        let mut output =
            String::with_capacity(usize::try_from(estimate).map_err(|_| ExactError::StorageLimit)?);
        write!(
            output,
            "owner={};r={}/{};pi={}/{};terms=[",
            self.owner().id(),
            self.constant().numer(),
            self.constant().denom(),
            self.pi_coefficient().numer(),
            self.pi_coefficient().denom()
        )
        .map_err(|_| ExactError::StorageLimit)?;
        for (index, (symbol, coefficient)) in self.data.terms.iter().enumerate() {
            if index != 0 {
                output.push(',');
            }
            write!(
                output,
                "{}={}/{}",
                symbol.index(),
                coefficient.numer(),
                coefficient.denom()
            )
            .map_err(|_| ExactError::StorageLimit)?;
        }
        output.push(']');
        let bytes = u64::try_from(output.capacity())
            .map_err(|_| ExactError::StorageLimit)?
            .checked_add(size_of::<Export>() as u64)
            .ok_or(ExactError::StorageLimit)?;
        if bytes > stage_bytes {
            return Err(ExactError::StorageLimit);
        }
        self.context.reserve(&self.context.state.retained, bytes)?;
        Ok(Export {
            text: output,
            bytes,
            state: self.context.state.clone(),
        })
    }
}

#[cfg(test)]
mod storage_tests {
    use super::*;
    use googletest::prelude::*;

    #[gtest]
    fn cancelled_storage_is_compact_or_fully_charged() -> googletest::Result<()> {
        let context = Context::new(Owner::new(80));
        let terms = (0..64)
            .map(|i| (Symbol::new(context.owner(), i), Rational::one()))
            .collect();
        let expression = context.from_parts(Rational::zero(), Rational::zero(), terms)?;
        for zero in [
            expression.sub(&expression)?,
            expression.scale_ratio(0.into(), 1.into())?,
        ] {
            expect_true!(zero.is_zero());
            let capacity_bytes = zero.data.terms.capacity() * size_of::<(Symbol, Rational)>();
            expect_true!(zero.data.bytes >= u64::try_from(capacity_bytes + size_of::<Data>())?);
        }
        Ok(())
    }
}
