use num_bigint::BigInt;

/// Exact input identity; finite binary64 angles are decoded as dyadic rationals.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum AngleTarget {
    DyadicRadians {
        bits: u64,
    },
    RationalPi {
        numerator: BigInt,
        denominator: BigInt,
    },
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Axis {
    X,
    Y,
    Z,
}
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Target {
    pub axis: Axis,
    pub angle: AngleTarget,
}
/// Sequence execution is left to right. `W` is the scalar exp(i pi/4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Gate {
    H,
    X,
    Y,
    Z,
    S,
    Sdg,
    T,
    Tdg,
    Cx,
    Cz,
    Swap,
    W,
}
impl Gate {
    #[must_use]
    pub const fn target_count(self) -> usize {
        match self {
            Self::W => 0,
            Self::Cx | Self::Cz | Self::Swap => 2,
            _ => 1,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Control {
    pub qubit: usize,
    pub positive: bool,
}
/// Ordered targets use the first target as local bit zero. `Cx` controls its first target.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Operation {
    pub gate: Gate,
    pub targets: Vec<usize>,
    pub controls: Vec<Control>,
}
/// Untrusted interchange representation; verification admits all bounds before matrix work.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Sequence {
    pub qubits: usize,
    pub operations: Vec<Operation>,
}
/// Caller resource bounds, in addition to the hard four-qubit and 4096-bit precision caps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Limits {
    pub qubits: usize,
    pub gates: usize,
    pub coefficient_bits: u64,
    pub bytes: usize,
    pub precision_bits: usize,
    pub taylor_terms: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            qubits: 4,
            gates: 1000,
            coefficient_bits: 16_384,
            bytes: 67_108_864,
            precision_bits: 256,
            taylor_terms: 2048,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("invalid exact-math input: {0}")]
    Invalid(String),
    #[error("{resource} budget exceeded: requested {requested}, limit {limit}")]
    Budget {
        resource: String,
        requested: u64,
        limit: u64,
    },
    #[error("resource arithmetic or allocation failed: {0}")]
    Resource(String),
    #[error("full matrices differ, including phase")]
    NotEquivalent,
    #[error("no eighth-root scalar phase makes the full matrices equal")]
    PhaseNotRecovered,
    #[error("no certificate at the permitted precision proves the requested tolerance")]
    NotCertified,
}
pub type Result<T> = std::result::Result<T, Error>;
pub type Rational = num_rational::Ratio<BigInt>;
pub fn budget(resource: &str, requested: u64, limit: u64) -> Error {
    Error::Budget {
        resource: resource.into(),
        requested,
        limit,
    }
}
pub fn size(value: usize) -> Result<u64> {
    u64::try_from(value).map_err(|_| Error::Resource("size does not fit u64".into()))
}
/// Account conservatively for bigint limbs, headers and arithmetic scratch.
pub fn allocation_bytes(bits: u64, count: usize) -> Result<u64> {
    let limbs = bits
        .checked_add(7)
        .map(|value| value / 8)
        .ok_or_else(|| Error::Resource("limb bytes".into()))?;
    let per = limbs
        .checked_mul(2)
        .and_then(|value| value.checked_add(64))
        .ok_or_else(|| Error::Resource("bigint bytes".into()))?;
    per.checked_mul(size(count)?)
        .ok_or_else(|| Error::Resource("allocation bytes".into()))
}
pub fn allocation(bits: u64, count: usize, limits: Limits) -> Result<()> {
    let requested = allocation_bytes(bits, count)?;
    let limit = size(limits.bytes)?;
    if requested > limit {
        return Err(budget("allocation bytes", requested, limit));
    }
    Ok(())
}
