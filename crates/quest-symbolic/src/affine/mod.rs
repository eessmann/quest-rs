//! Bounded, immutable affine expressions over exact rationals and symbolic pi.
//!
//! A value has the canonical form `r + q*pi + sum(c_i*s_i)`. Symbols carry an
//! explicit owner. A context supplies the owner and shared finite budgets;
//! cloning the context shares its counters. Constants can be imported into
//! another context with [`Context::assemble`]. Symbolic values require an
//! explicit complete mapping with [`Affine::substitute_into`].

use crate::RBig;
use dashu_base::BitTest;
use dashu_int::IBig;
use std::fmt;
#[cfg(test)]
use std::fmt::Write as _;
use std::mem::size_of;
use std::ops::{Add, Mul, Neg};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

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
#[expect(
    clippy::struct_field_names,
    reason = "All fields are explicit upper resource bounds"
)]
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
#[cfg(test)]
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
    #[cfg(test)]
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
    #[cfg(test)]
    pub fn usage(&self) -> Usage {
        Usage {
            work: self.state.work.load(Ordering::Relaxed),
            retained_bytes: self.state.retained.load(Ordering::Relaxed),
            staged_bytes: self.state.staged.load(Ordering::Relaxed),
        }
    }
    #[cfg(test)]
    pub fn zero(&self) -> Result<Affine, ExactError> {
        self.assemble(RBig::ZERO, RBig::ZERO, Vec::new())
    }
    #[cfg(test)]
    pub fn one(&self) -> Result<Affine, ExactError> {
        self.assemble(RBig::ONE, RBig::ZERO, Vec::new())
    }
    #[cfg(test)]
    pub fn ratio(&self, numerator: IBig, denominator: IBig) -> Result<Affine, ExactError> {
        let coefficient = self.make_ratio(numerator, denominator)?;
        self.assemble(coefficient, RBig::ZERO, Vec::new())
    }
    #[cfg(test)]
    pub fn pi(&self) -> Result<Affine, ExactError> {
        self.assemble(RBig::ZERO, RBig::ONE, Vec::new())
    }
    pub fn symbol(&self, symbol: Symbol) -> Result<Affine, ExactError> {
        self.assemble(RBig::ZERO, RBig::ZERO, vec![(symbol, RBig::ONE)])
    }
    pub fn assemble(
        &self,
        constant: RBig,
        pi: RBig,
        mut terms: Vec<(Symbol, RBig)>,
    ) -> Result<Affine, ExactError> {
        if u64::try_from(terms.len()).map_err(|_| ExactError::WorkLimit)? > self.limits.max_work {
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
        self.charge_work(count_work(terms.len())?)?;
        self.charge_work(sort_work(terms.len())?)?;
        let _stage = self.stage(storage_bytes(&constant, &pi, &terms, terms.capacity())?)?;
        let constant = self.admit_coefficient(constant)?;
        let pi = self.admit_coefficient(pi)?;
        for (_, coefficient) in &mut terms {
            *coefficient = self.admit_coefficient(std::mem::take(coefficient))?;
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
    fn make_ratio(&self, numerator: IBig, denominator: IBig) -> Result<RBig, ExactError> {
        if denominator.is_zero() {
            return Err(ExactError::ZeroDenominator);
        }
        self.check_int(&numerator)?;
        self.check_int(&denominator)?;
        self.charge_work(bits_work(&[
            u64::try_from(numerator.bit_len()).unwrap_or(u64::MAX),
            u64::try_from(denominator.bit_len()).unwrap_or(u64::MAX),
        ])?)?;
        Ok(RBig::from_parts_signed(numerator, denominator))
    }
    fn check_int(&self, value: &IBig) -> Result<(), ExactError> {
        if u64::try_from(value.bit_len()).unwrap_or(u64::MAX) > self.limits.max_coefficient_bits {
            Err(ExactError::CoefficientLimit)
        } else {
            Ok(())
        }
    }
    fn check_coefficient(&self, value: &RBig) -> Result<(), ExactError> {
        self.check_int(value.numerator())?;
        if u64::try_from(value.denominator().bit_len()).unwrap_or(u64::MAX)
            > self.limits.max_coefficient_bits
        {
            Err(ExactError::CoefficientLimit)
        } else {
            Ok(())
        }
    }
    fn admit_coefficient(&self, value: RBig) -> Result<RBig, ExactError> {
        self.check_coefficient(&value)?;
        self.charge_work(bits_work(&[
            u64::try_from(value.numerator().bit_len()).unwrap_or(u64::MAX),
            u64::try_from(value.denominator().bit_len()).unwrap_or(u64::MAX),
        ])?)?;
        // RBig is already reduced and stores a positive denominator.
        Ok(value)
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
    fn checked_add(&self, a: &RBig, b: &RBig) -> Result<RBig, ExactError> {
        self.check_coefficient(a)?;
        self.check_coefficient(b)?;
        if a.is_zero() {
            return Ok(b.clone());
        }
        if b.is_zero() {
            return Ok(a.clone());
        }
        if a.numerator() == &b.numerator().neg() && a.denominator() == b.denominator() {
            return Ok(RBig::ZERO);
        }
        let max = self.limits.max_coefficient_bits;
        let left = u64::try_from(a.numerator().bit_len())
            .unwrap_or(u64::MAX)
            .saturating_add(u64::try_from(b.denominator().bit_len()).unwrap_or(u64::MAX));
        let right = u64::try_from(b.numerator().bit_len())
            .unwrap_or(u64::MAX)
            .saturating_add(u64::try_from(a.denominator().bit_len()).unwrap_or(u64::MAX));
        let denominator = u64::try_from(a.denominator().bit_len())
            .unwrap_or(u64::MAX)
            .saturating_add(u64::try_from(b.denominator().bit_len()).unwrap_or(u64::MAX));
        if left.max(right).saturating_add(1) > max || denominator > max {
            return Err(ExactError::CoefficientLimit);
        }
        self.charge_work(bits_work(&[left, right, denominator])?)?;
        let result = a.add(b);
        self.check_coefficient(&result)?;
        Ok(result)
    }
    fn checked_mul(&self, a: &RBig, b: &RBig) -> Result<RBig, ExactError> {
        self.check_coefficient(a)?;
        self.check_coefficient(b)?;
        if a.is_zero() || b.is_zero() {
            return Ok(RBig::ZERO);
        }
        if a.is_one() {
            return Ok(b.clone());
        }
        if b.is_one() {
            return Ok(a.clone());
        }
        if a == &RBig::ONE.neg() {
            return Ok(b.clone().neg());
        }
        if b == &RBig::ONE.neg() {
            return Ok(a.clone().neg());
        }
        let numerator = u64::try_from(a.numerator().bit_len())
            .unwrap_or(u64::MAX)
            .saturating_add(u64::try_from(b.numerator().bit_len()).unwrap_or(u64::MAX));
        let denominator = u64::try_from(a.denominator().bit_len())
            .unwrap_or(u64::MAX)
            .saturating_add(u64::try_from(b.denominator().bit_len()).unwrap_or(u64::MAX));
        if numerator > self.limits.max_coefficient_bits
            || denominator > self.limits.max_coefficient_bits
        {
            return Err(ExactError::CoefficientLimit);
        }
        self.charge_work(bits_work(&[numerator, denominator])?)?;
        let result = a.mul(b);
        self.check_coefficient(&result)?;
        Ok(result)
    }
    fn publish(
        &self,
        constant: RBig,
        pi: RBig,
        terms: Vec<(Symbol, RBig)>,
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

fn count_work(count: usize) -> Result<u64, ExactError> {
    u64::try_from(count)
        .ok()
        .and_then(|n| n.checked_add(1))
        .ok_or(ExactError::WorkLimit)
}
fn bits_work(bits: &[u64]) -> Result<u64, ExactError> {
    bits.iter()
        .try_fold(0u64, |sum, value| {
            sum.checked_add(*value).ok_or(ExactError::WorkLimit)
        })?
        .checked_div(64)
        .and_then(|n| n.checked_add(1))
        .ok_or(ExactError::WorkLimit)
}
fn coefficient_bytes(value: &RBig) -> Result<u64, ExactError> {
    u64::try_from(value.numerator().bit_len())
        .unwrap_or(u64::MAX)
        .div_ceil(8)
        .checked_add(
            u64::try_from(value.denominator().bit_len())
                .unwrap_or(u64::MAX)
                .div_ceil(8),
        )
        .ok_or(ExactError::StorageLimit)
}
fn storage_bytes(
    constant: &RBig,
    pi: &RBig,
    terms: &[(Symbol, RBig)],
    capacity: usize,
) -> Result<u64, ExactError> {
    let allocation = capacity
        .checked_mul(size_of::<(Symbol, RBig)>())
        .ok_or(ExactError::StorageLimit)?;
    let base = size_of::<Data>()
        .checked_add(allocation)
        .ok_or(ExactError::StorageLimit)?;
    let base = u64::try_from(base).map_err(|_| ExactError::StorageLimit)?;
    terms.iter().try_fold(
        base.checked_add(coefficient_bytes(constant)?)
            .and_then(|n| n.checked_add(coefficient_bytes(pi).ok()?))
            .ok_or(ExactError::StorageLimit)?,
        |sum, (_, c)| {
            sum.checked_add(coefficient_bytes(c)?)
                .ok_or(ExactError::StorageLimit)
        },
    )
}
fn sort_work(count: usize) -> Result<u64, ExactError> {
    if count < 2 {
        return Ok(0);
    }
    let count = u64::try_from(count).map_err(|_| ExactError::WorkLimit)?;
    let levels = u64::from(count.bit_width());
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
    constant: RBig,
    pi: RBig,
    terms: Vec<(Symbol, RBig)>,
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
#[cfg(test)]
pub struct Export {
    text: String,
    bytes: u64,
    state: Arc<State>,
}
#[cfg(test)]
impl Export {
    pub fn as_str(&self) -> &str {
        &self.text
    }
}
#[cfg(test)]
impl PartialEq for Export {
    fn eq(&self, other: &Self) -> bool {
        self.text == other.text
    }
}
#[cfg(test)]
impl Eq for Export {}
#[cfg(test)]
impl Drop for Export {
    fn drop(&mut self) {
        self.state.retained.fetch_sub(self.bytes, Ordering::AcqRel);
        self.state
            .total_bytes
            .fetch_sub(self.bytes, Ordering::AcqRel);
    }
}

#[cfg(test)]
fn decimal_digits_bound(bits: u64) -> Result<u64, ExactError> {
    bits.checked_mul(30_103)
        .and_then(|v| v.checked_add(99_999))
        .and_then(|v| (v / 100_000).checked_add(2))
        .ok_or(ExactError::StorageLimit)
}
#[cfg(test)]
fn rational_text_bound(value: &RBig) -> Result<u64, ExactError> {
    decimal_digits_bound(u64::try_from(value.numerator().bit_len()).unwrap_or(u64::MAX))?
        .checked_add(decimal_digits_bound(
            u64::try_from(value.denominator().bit_len()).unwrap_or(u64::MAX),
        )?)
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
    pub const fn owner(&self) -> Owner {
        self.context.owner
    }
    pub fn constant(&self) -> &RBig {
        &self.data.constant
    }
    pub fn pi_coefficient(&self) -> &RBig {
        &self.data.pi
    }
    pub fn terms(&self) -> impl Iterator<Item = (&Symbol, &RBig)> {
        self.data.terms.iter().map(|(s, c)| (s, c))
    }
    pub fn is_zero(&self) -> bool {
        self.data.constant.is_zero() && self.data.pi.is_zero() && self.data.terms.is_empty()
    }
    #[cfg(test)]
    pub fn is_one(&self) -> bool {
        self.data.constant.is_one() && self.data.pi.is_zero() && self.data.terms.is_empty()
    }
    pub fn neg(&self) -> Result<Self, ExactError> {
        self.scale_ratio((-1).into(), 1.into())
    }
    pub fn add(&self, other: &Self) -> Result<Self, ExactError> {
        self.combine(other, false)
    }
    #[cfg(test)]
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
        self.context.charge_work(count_work(count)?)?;
        let estimate = self.data.bytes.saturating_add(other.data.bytes);
        let _stage = self.context.stage(estimate)?;
        let negative = |v: &RBig| if subtract { v.clone().neg() } else { v.clone() };
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
                    i = i.saturating_add(1);
                    j = j.saturating_add(1);
                    (*sa, self.context.checked_add(ca, &negative(cb))?)
                }
                (Some((sa, ca)), Some((sb, _))) if sa < sb => {
                    i = i.saturating_add(1);
                    (*sa, ca.clone())
                }
                (_, Some((sb, cb))) => {
                    j = j.saturating_add(1);
                    (*sb, negative(cb))
                }
                (Some((sa, ca)), None) => {
                    i = i.saturating_add(1);
                    (*sa, ca.clone())
                }
                (None, None) => break,
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
    pub fn scale_ratio(&self, numerator: IBig, denominator: IBig) -> Result<Self, ExactError> {
        let factor = self.context.make_ratio(numerator, denominator)?;
        self.scale(&factor)
    }
    #[cfg(test)]
    pub fn divide_ratio(&self, numerator: IBig, denominator: IBig) -> Result<Self, ExactError> {
        if numerator.is_zero() || denominator.is_zero() {
            return Err(ExactError::ZeroDenominator);
        }
        let reciprocal = self.context.make_ratio(denominator, numerator)?;
        self.scale(&reciprocal)
    }
    fn scale(&self, factor: &RBig) -> Result<Self, ExactError> {
        self.context
            .charge_work(count_work(self.data.terms.len())?)?;
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
    pub fn substitute_into(
        &self,
        target: &Context,
        replacements: &[(Symbol, Self)],
    ) -> Result<Self, ExactError> {
        let count = u64::try_from(replacements.len()).map_err(|_| ExactError::WorkLimit)?;
        if count > target.limits.max_work {
            return Err(ExactError::WorkLimit);
        }
        let comparisons = u64::from(count.max(1).bit_width());
        let work = count
            .checked_add(u64::try_from(self.data.terms.len()).map_err(|_| ExactError::WorkLimit)?)
            .and_then(|v| v.checked_mul(comparisons))
            .ok_or(ExactError::WorkLimit)?;
        target.charge_work(work)?;
        let bytes = count
            .checked_mul(
                u64::try_from(size_of::<(Symbol, Self)>()).map_err(|_| ExactError::StorageLimit)?,
            )
            .ok_or(ExactError::StorageLimit)?;
        let _mapping_stage = target.stage(bytes)?;
        let mut mapping = replacements.to_vec();
        mapping.sort_unstable_by_key(|(s, _)| *s);
        for pair in mapping.windows(2) {
            if let [left, right] = pair
                && left.0 == right.0
            {
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
            target.assemble(self.data.constant.clone(), self.data.pi.clone(), Vec::new())?;
        for (symbol, coeff) in &self.data.terms {
            let replacement = match mapping.binary_search_by_key(symbol, |(s, _)| *s) {
                Ok(index) => mapping
                    .get(index)
                    .ok_or(ExactError::UnmappedSymbol)?
                    .1
                    .clone(),
                Err(_) if Arc::ptr_eq(&self.context.state, &target.state) => {
                    target.symbol(*symbol)?
                }
                Err(_) => return Err(ExactError::UnmappedSymbol),
            };
            output = output.add(&replacement.scale(coeff)?)?;
        }
        Ok(output)
    }
    #[cfg(test)]
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
            .and_then(|v| v.checked_add(u64::try_from(size_of::<Export>()).ok()?))
            .ok_or(ExactError::StorageLimit)?;
        let _stage = self.context.stage(stage_bytes)?;
        let mut output =
            String::with_capacity(usize::try_from(estimate).map_err(|_| ExactError::StorageLimit)?);
        write!(
            output,
            "owner={};r={}/{};pi={}/{};terms=[",
            self.owner().id(),
            self.constant().numerator(),
            self.constant().denominator(),
            self.pi_coefficient().numerator(),
            self.pi_coefficient().denominator()
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
                coefficient.numerator(),
                coefficient.denominator()
            )
            .map_err(|_| ExactError::StorageLimit)?;
        }
        output.push(']');
        let bytes = u64::try_from(output.capacity())
            .map_err(|_| ExactError::StorageLimit)?
            .checked_add(u64::try_from(size_of::<Export>()).map_err(|_| ExactError::StorageLimit)?)
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
            .map(|i| (Symbol::new(context.owner(), i), RBig::ONE))
            .collect();
        let expression = context.assemble(RBig::ZERO, RBig::ZERO, terms)?;
        for zero in [
            expression.sub(&expression)?,
            expression.scale_ratio(0.into(), 1.into())?,
        ] {
            expect_true!(zero.is_zero());
            let capacity_bytes = zero
                .data
                .terms
                .capacity()
                .checked_mul(size_of::<(Symbol, RBig)>())
                .or_fail()?;
            expect_true!(
                zero.data.bytes
                    >= u64::try_from(capacity_bytes.checked_add(size_of::<Data>()).or_fail()?)?
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
