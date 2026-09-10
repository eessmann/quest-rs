use crate::{Error, Result};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QubitCount(u32);
impl QubitCount {
    pub fn new(value: usize) -> Result<Self> {
        let count = u32::try_from(value).map_err(|_| Error::QubitCount(value))?;
        if count == 0 || count >= usize::BITS - 1 || count >= 63 {
            return Err(Error::QubitCount(value));
        }
        Ok(Self(count))
    }
    pub fn get(self) -> usize {
        self.0 as usize
    }
    pub fn dimension(self) -> usize {
        1usize << self.0
    }
    pub(crate) fn native(self) -> i32 {
        self.0 as i32
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MemoryBudget(usize);
impl MemoryBudget {
    pub const fn new(bytes: usize) -> Self {
        Self(bytes)
    }
    pub const fn bytes(self) -> usize {
        self.0
    }
}
impl Default for MemoryBudget {
    fn default() -> Self {
        Self(512 * 1024 * 1024)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Probability(f64);
impl Probability {
    pub fn new(value: f64) -> Result<Self> {
        if !value.is_finite() || !(0.0..=1.0).contains(&value) {
            return Err(Error::Value("probability must be finite and in [0,1]"));
        }
        Ok(Self(value))
    }
    pub fn get(self) -> f64 {
        self.0
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Outcome {
    Zero,
    One,
}
impl Outcome {
    pub fn as_bool(self) -> bool {
        self == Self::One
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Shots(usize);
impl Shots {
    pub fn new(value: usize) -> Result<Self> {
        if value == 0 {
            Err(Error::Value("shot count must be positive"))
        } else {
            Ok(Self(value))
        }
    }
    pub fn get(self) -> usize {
        self.0
    }
}

pub(crate) fn bytes_for(elements: usize, copies: usize) -> Result<usize> {
    elements
        .checked_mul(std::mem::size_of::<crate::Complex64>())
        .and_then(|n| n.checked_mul(copies))
        .filter(|&n| n <= isize::MAX as usize)
        .ok_or(Error::Overflow)
}
pub(crate) fn reserve_vec<T>(len: usize) -> Result<Vec<T>> {
    let mut out = Vec::new();
    out.try_reserve_exact(len).map_err(|_| Error::Allocation)?;
    Ok(out)
}
