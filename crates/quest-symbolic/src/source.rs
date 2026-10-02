//! Shared source graph and independent affine replay.

use dashu_base::BitTest;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use crate::affine::Limits;

use crate::{Error, RBig, Result, Symbol};

const MAX_NODES: usize = 16_384;
pub const MAX_INPUT_BINDINGS: usize = 1 << 20;

#[derive(Debug)]
pub enum Kind {
    Radians(RBig),
    Pi(RBig),
    Affine(RBig, RBig),
    Parameter(Symbol),
    Negative(Arc<Source>),
    Sum(Arc<Source>, Arc<Source>),
    Scale(Arc<Source>, RBig),
}
#[derive(Debug)]
pub struct Source {
    pub kind: Kind,
    nodes: usize,
    has_parameters: bool,
}
impl Source {
    pub const fn logical_nodes(&self) -> usize {
        self.nodes
    }
    pub const fn has_parameters(&self) -> bool {
        self.has_parameters
    }
    pub fn retained_bytes(&self) -> Result<usize> {
        let mut total = 0usize;
        let mut failure = None;
        self.walk(|node| {
            if failure.is_some() {
                return;
            }
            let extra = match &node.kind {
                Kind::Radians(x) | Kind::Pi(x) | Kind::Scale(_, x) => rational_bytes(x),
                Kind::Affine(r, p) => rational_bytes(r).and_then(|r| {
                    rational_bytes(p).and_then(|p| r.checked_add(p).ok_or(Error::SourceLimit))
                }),
                Kind::Parameter(_) | Kind::Negative(_) | Kind::Sum(_, _) => Ok(0),
            };
            match extra.and_then(|extra| {
                total
                    .checked_add(
                        const { std::mem::size_of::<Self>() + 2 * std::mem::size_of::<usize>() },
                    )
                    .and_then(|base| base.checked_add(extra))
                    .ok_or(Error::SourceLimit)
            }) {
                Ok(bytes) => total = bytes,
                Err(error) => failure = Some(error),
            }
        });
        failure.map_or(Ok(total), Err)
    }
    pub fn equivalent_checked(left: &Self, right: &Self) -> Result<bool> {
        let budget = left
            .nodes
            .checked_add(right.nodes)
            .ok_or(Error::SourceLimit)?;
        if budget > MAX_NODES.checked_mul(2).ok_or(Error::SourceLimit)? {
            return Err(Error::SourceLimit);
        }
        let mut stack = Vec::new();
        stack
            .try_reserve_exact(budget)
            .map_err(|_| Error::SourceLimit)?;
        stack.push((left, right));
        while let Some((a, b)) = stack.pop() {
            if std::ptr::eq(a, b) {
                continue;
            }
            match (&a.kind, &b.kind) {
                (Kind::Radians(x), Kind::Radians(y)) | (Kind::Pi(x), Kind::Pi(y)) if x == y => {}
                (Kind::Affine(ar, ap), Kind::Affine(br, bp)) if ar == br && ap == bp => {}
                (Kind::Parameter(x), Kind::Parameter(y)) if x == y => {}
                (Kind::Negative(x), Kind::Negative(y)) => stack.push((x, y)),
                (Kind::Sum(ax, ay), Kind::Sum(bx, by)) => {
                    stack.push((ax, bx));
                    stack.push((ay, by));
                }
                (Kind::Scale(x, factor_x), Kind::Scale(y, factor_y)) if factor_x == factor_y => {
                    stack.push((x, y));
                }
                _ => return Ok(false),
            }
        }
        Ok(true)
    }
    pub(crate) fn equivalent(left: &Self, right: &Self) -> bool {
        if std::ptr::eq(left, right) {
            return true;
        }
        match (&left.kind, &right.kind) {
            (Kind::Radians(x), Kind::Radians(y)) | (Kind::Pi(x), Kind::Pi(y)) => x == y,
            (Kind::Affine(ar, ap), Kind::Affine(br, bp)) => ar == br && ap == bp,
            (Kind::Parameter(x), Kind::Parameter(y)) => x == y,
            _ => false,
        }
    }
    pub(crate) fn leaf(kind: Kind) -> Arc<Self> {
        let has_parameters = matches!(kind, Kind::Parameter(_));
        Arc::new(Self {
            kind,
            nodes: 1,
            has_parameters,
        })
    }
    pub(crate) fn negative(child: &Arc<Self>) -> Result<Arc<Self>> {
        if let Kind::Negative(inner) = &child.kind {
            return Ok(Arc::clone(inner));
        }
        Self::unary(Kind::Negative(Arc::clone(child)), child)
    }
    pub(crate) fn scale(child: &Arc<Self>, factor: RBig) -> Result<Arc<Self>> {
        Self::unary(Kind::Scale(Arc::clone(child), factor), child)
    }
    fn unary(kind: Kind, child: &Self) -> Result<Arc<Self>> {
        let nodes = child.nodes.checked_add(1).ok_or(Error::SourceLimit)?;
        if nodes > MAX_NODES {
            return Err(Error::SourceLimit);
        }
        Ok(Arc::new(Self {
            kind,
            nodes,
            has_parameters: child.has_parameters,
        }))
    }
    pub(crate) fn sum(left: &Arc<Self>, right: &Arc<Self>) -> Result<Arc<Self>> {
        let nodes = left
            .nodes
            .checked_add(right.nodes)
            .and_then(|count| count.checked_add(1))
            .ok_or(Error::SourceLimit)?;
        if nodes > MAX_NODES {
            return Err(Error::SourceLimit);
        }
        Ok(Arc::new(Self {
            kind: Kind::Sum(Arc::clone(left), Arc::clone(right)),
            nodes,
            has_parameters: left.has_parameters || right.has_parameters,
        }))
    }
    pub(crate) fn parameters(&self) -> BTreeSet<Symbol> {
        let mut symbols = BTreeSet::new();
        self.walk(|node| {
            if let Kind::Parameter(symbol) = &node.kind {
                symbols.insert(*symbol);
            }
        });
        symbols
    }
    pub(crate) fn pi_leaves<E>(
        &self,
        mut check: impl FnMut(&RBig) -> std::result::Result<(), E>,
    ) -> std::result::Result<(), E> {
        let mut result = Ok(());
        self.walk(|node| {
            if let Kind::Pi(value) = &node.kind
                && result.is_ok()
            {
                result = check(value);
            }
        });
        result
    }
    fn walk(&self, mut visit: impl FnMut(&Self)) {
        let mut stack = vec![self];
        while let Some(node) = stack.pop() {
            visit(node);
            match &node.kind {
                Kind::Negative(child) | Kind::Scale(child, _) => stack.push(child),
                Kind::Sum(left, right) => {
                    stack.push(right);
                    stack.push(left);
                }
                Kind::Radians(_) | Kind::Pi(_) | Kind::Affine(_, _) | Kind::Parameter(_) => {}
            }
        }
    }
    pub(crate) fn substitute(
        source: &Arc<Self>,
        replacements: &BTreeMap<Symbol, Arc<Self>>,
    ) -> Result<Arc<Self>> {
        let mut traversal = vec![(Arc::clone(source), false)];
        let mut output = Vec::new();
        while let Some((node, ready)) = traversal.pop() {
            if !ready {
                traversal.push((Arc::clone(&node), true));
                match &node.kind {
                    Kind::Negative(child) | Kind::Scale(child, _) => {
                        traversal.push((Arc::clone(child), false));
                    }
                    Kind::Sum(left, right) => {
                        traversal.push((Arc::clone(right), false));
                        traversal.push((Arc::clone(left), false));
                    }
                    Kind::Radians(_) | Kind::Pi(_) | Kind::Affine(_, _) | Kind::Parameter(_) => {}
                }
                continue;
            }
            let transformed = match &node.kind {
                Kind::Radians(_) | Kind::Pi(_) | Kind::Affine(_, _) => node,
                Kind::Parameter(symbol) => {
                    Arc::clone(replacements.get(symbol).ok_or(Error::MissingBinding)?)
                }
                Kind::Negative(_) => Self::negative(&output.pop().ok_or(Error::IncorrectAlgebra)?)?,
                Kind::Scale(_, factor) => Self::scale(
                    &output.pop().ok_or(Error::IncorrectAlgebra)?,
                    factor.clone(),
                )?,
                Kind::Sum(_, _) => {
                    let right = output.pop().ok_or(Error::IncorrectAlgebra)?;
                    let left = output.pop().ok_or(Error::IncorrectAlgebra)?;
                    Self::sum(&left, &right)?
                }
            };
            output.push(transformed);
        }
        output.pop().ok_or(Error::IncorrectAlgebra)
    }
    pub(crate) fn replay(&self, bindings: &BTreeMap<Symbol, RBig>) -> Result<Linear> {
        self.replay_checked(bindings, |_, _| Ok(()))
    }
    pub(crate) fn replay_checked<E: From<Error>>(
        &self,
        bindings: &BTreeMap<Symbol, RBig>,
        mut check: impl FnMut(&RBig, &RBig) -> std::result::Result<(), E>,
    ) -> std::result::Result<Linear, E> {
        let mut traversal = vec![(self, false)];
        let mut values: Vec<Linear> = Vec::new();
        while let Some((node, ready)) = traversal.pop() {
            if !ready {
                traversal.push((node, true));
                match &node.kind {
                    Kind::Negative(child) | Kind::Scale(child, _) => {
                        traversal.push((child, false));
                    }
                    Kind::Sum(left, right) => {
                        traversal.push((right, false));
                        traversal.push((left, false));
                    }
                    Kind::Radians(_) | Kind::Pi(_) | Kind::Affine(_, _) | Kind::Parameter(_) => {}
                }
                continue;
            }
            let value = match &node.kind {
                Kind::Radians(value) => Linear {
                    radians: value.clone(),
                    ..Linear::default()
                },
                Kind::Pi(value) => Linear {
                    pi: value.clone(),
                    ..Linear::default()
                },
                Kind::Affine(radians, pi) => Linear {
                    radians: radians.clone(),
                    pi: pi.clone(),
                    ..Linear::default()
                },
                Kind::Parameter(symbol) => bindings.get(symbol).map_or_else(
                    || Linear {
                        terms: BTreeMap::from([(*symbol, RBig::ONE)]),
                        ..Linear::default()
                    },
                    |value| Linear {
                        radians: value.clone(),
                        ..Linear::default()
                    },
                ),
                Kind::Negative(_) => values
                    .pop()
                    .ok_or(Error::IncorrectAlgebra)?
                    .scale(&RBig::NEG_ONE)?,
                Kind::Scale(_, factor) => {
                    values.pop().ok_or(Error::IncorrectAlgebra)?.scale(factor)?
                }
                Kind::Sum(_, _) => {
                    let right = values.pop().ok_or(Error::IncorrectAlgebra)?;
                    values.pop().ok_or(Error::IncorrectAlgebra)?.add(right)?
                }
            };
            if value.terms.is_empty() {
                check(&value.radians, &value.pi)?;
            }
            values.push(value);
        }
        values.pop().ok_or_else(|| Error::IncorrectAlgebra.into())
    }
}
pub fn rational_bytes(value: &RBig) -> Result<usize> {
    let limb_bytes = |bits: u64| -> Result<usize> {
        usize::try_from(bits.div_ceil(64))
            .map_err(|_| Error::SourceLimit)?
            .checked_mul(8)
            .and_then(|bytes| bytes.checked_mul(2))
            .and_then(|bytes| bytes.checked_add(const { 3 * std::mem::size_of::<usize>() }))
            .ok_or(Error::SourceLimit)
    };
    limb_bytes(u64::try_from(value.numerator().bit_len()).unwrap_or(u64::MAX))?
        .checked_add(limb_bytes(
            u64::try_from(value.denominator().bit_len()).unwrap_or(u64::MAX),
        )?)
        .ok_or(Error::SourceLimit)
}

#[derive(Debug, Default, Clone)]
pub struct Linear {
    pub radians: RBig,
    pub pi: RBig,
    pub terms: BTreeMap<Symbol, RBig>,
}
impl Linear {
    pub fn add(mut self, right: Self) -> Result<Self> {
        self.radians = checked_add(&self.radians, &right.radians)?;
        self.pi = checked_add(&self.pi, &right.pi)?;
        for (symbol, coefficient) in right.terms {
            let old = self.terms.remove(&symbol).unwrap_or(RBig::ZERO);
            let sum = checked_add(&old, &coefficient)?;
            if !sum.is_zero() {
                self.terms.insert(symbol, sum);
            }
        }
        if self.terms.len() > Limits::default().max_terms {
            return Err(Error::SourceLimit);
        }
        Ok(self)
    }
    pub fn scale(mut self, factor: &RBig) -> Result<Self> {
        self.radians = checked_mul(&self.radians, factor)?;
        self.pi = checked_mul(&self.pi, factor)?;
        for coefficient in self.terms.values_mut() {
            *coefficient = checked_mul(coefficient, factor)?;
        }
        self.terms.retain(|_, coefficient| !coefficient.is_zero());
        Ok(self)
    }
}
fn check_width(bits: u64) -> Result<()> {
    if bits > Limits::default().max_coefficient_bits {
        Err(Error::Exact(crate::ExactError::CoefficientLimit))
    } else {
        Ok(())
    }
}
fn checked_add(left: &RBig, right: &RBig) -> Result<RBig> {
    if left.is_zero() {
        return Ok(right.clone());
    }
    if right.is_zero() {
        return Ok(left.clone());
    }
    if left.numerator() == &std::ops::Neg::neg(right.numerator())
        && left.denominator() == right.denominator()
    {
        return Ok(RBig::ZERO);
    }
    let a = u64::try_from(left.numerator().bit_len())
        .unwrap_or(u64::MAX)
        .checked_add(u64::try_from(right.denominator().bit_len()).unwrap_or(u64::MAX))
        .ok_or(Error::SourceLimit)?;
    let b = u64::try_from(right.numerator().bit_len())
        .unwrap_or(u64::MAX)
        .checked_add(u64::try_from(left.denominator().bit_len()).unwrap_or(u64::MAX))
        .ok_or(Error::SourceLimit)?;
    check_width(a.max(b).checked_add(1).ok_or(Error::SourceLimit)?)?;
    check_width(
        u64::try_from(left.denominator().bit_len())
            .unwrap_or(u64::MAX)
            .checked_add(u64::try_from(right.denominator().bit_len()).unwrap_or(u64::MAX))
            .ok_or(Error::SourceLimit)?,
    )?;
    let value = std::ops::Add::add(left, right);
    check_width(
        u64::try_from(value.numerator().bit_len())
            .unwrap_or(u64::MAX)
            .max(u64::try_from(value.denominator().bit_len()).unwrap_or(u64::MAX)),
    )?;
    Ok(value)
}
fn checked_mul(left: &RBig, right: &RBig) -> Result<RBig> {
    if left.is_zero() || right.is_zero() {
        return Ok(RBig::ZERO);
    }
    if left == &RBig::ONE {
        return Ok(right.clone());
    }
    if right == &RBig::ONE {
        return Ok(left.clone());
    }
    if left == &RBig::NEG_ONE {
        return Ok(std::ops::Neg::neg(right.clone()));
    }
    if right == &RBig::NEG_ONE {
        return Ok(std::ops::Neg::neg(left.clone()));
    }
    check_width(
        u64::try_from(left.numerator().bit_len())
            .unwrap_or(u64::MAX)
            .checked_add(u64::try_from(right.numerator().bit_len()).unwrap_or(u64::MAX))
            .ok_or(Error::SourceLimit)?,
    )?;
    check_width(
        u64::try_from(left.denominator().bit_len())
            .unwrap_or(u64::MAX)
            .checked_add(u64::try_from(right.denominator().bit_len()).unwrap_or(u64::MAX))
            .ok_or(Error::SourceLimit)?,
    )?;
    let value = std::ops::Mul::mul(left, right);
    check_width(
        u64::try_from(value.numerator().bit_len())
            .unwrap_or(u64::MAX)
            .max(u64::try_from(value.denominator().bit_len()).unwrap_or(u64::MAX)),
    )?;
    Ok(value)
}
