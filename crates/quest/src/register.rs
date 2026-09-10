use crate::{Complex64, Environment, Error, Outcome, Probability, QubitCount, Result};
use crate::{
    environment::Reservation,
    error::BackendResult,
    values::{bytes_for, reserve_vec},
};
use cxx::UniquePtr;
use faer::traits::Conjugate;
use faer::{Mat, MatRef};
use std::{marker::PhantomData, pin::Pin};

mod sealed {
    pub trait Kind {
        const DENSITY: bool;
    }
}
pub trait RegisterKind: sealed::Kind {}
#[derive(Debug)]
pub struct StateVector;
#[derive(Debug)]
pub struct DensityMatrix;
impl sealed::Kind for StateVector {
    const DENSITY: bool = false;
}
impl sealed::Kind for DensityMatrix {
    const DENSITY: bool = true;
}
impl RegisterKind for StateVector {}
impl RegisterKind for DensityMatrix {}

/// A native register tied to its active environment. Cloning is explicit and deep.
pub struct Register<'env, K: RegisterKind> {
    // Declaration order ensures native destruction precedes releasing accounting.
    pub(crate) native: UniquePtr<quest_sys::Qureg>,
    reservation: Reservation<'env>,
    count: QubitCount,
    kind: PhantomData<K>,
}
impl<'env, K: RegisterKind> Register<'env, K> {
    pub(crate) fn allocate(environment: &'env Environment, count: QubitCount) -> Result<Self> {
        let entries = if K::DENSITY {
            count
                .dimension()
                .checked_mul(count.dimension())
                .ok_or(Error::Overflow)?
        } else {
            count.dimension()
        };
        // Include native state and workspace copies, on host and (when enabled) device.
        let reservation = environment.reserve(bytes_for(
            entries,
            if environment.capabilities().gpu { 8 } else { 4 },
        )?)?;
        let native = if K::DENSITY {
            quest_sys::create_density_qureg(count.native())
        } else {
            quest_sys::create_qureg(count.native())
        }
        .context("allocating register")?;
        Ok(Self {
            native,
            reservation,
            count,
            kind: PhantomData,
        })
    }
    pub fn num_qubits(&self) -> QubitCount {
        self.count
    }
    pub fn dimension(&self) -> usize {
        self.count.dimension()
    }
    pub fn environment(&self) -> &'env Environment {
        self.reservation.environment
    }
    pub(crate) fn is_density(&self) -> bool {
        K::DENSITY
    }
    pub(crate) fn pin(&mut self) -> Pin<&mut quest_sys::Qureg> {
        self.native.pin_mut()
    }
    pub(crate) fn check_qubit(&self, qubit: usize) -> Result<i32> {
        if qubit >= self.count.get() {
            Err(Error::Index {
                index: qubit,
                bound: self.count.get(),
            })
        } else {
            Ok(qubit as i32)
        }
    }
    pub fn init_zero(&mut self) -> Result<()> {
        quest_sys::init_zero_state(self.pin()).context("initializing zero state")
    }
    pub fn init_plus(&mut self) -> Result<()> {
        quest_sys::init_plus_state(self.pin()).context("initializing plus state")
    }
    pub fn init_pure(&mut self, amplitudes: &[Complex64]) -> Result<()> {
        if amplitudes.len() != self.dimension() {
            return Err(Error::Value("pure-state amplitude count must be 2^qubits"));
        }
        let _scratch = self
            .environment()
            .reserve(bytes_for(amplitudes.len(), 3)?)?;
        let buffer = pack(amplitudes.iter().copied(), amplitudes.len())?;
        quest_sys::init_arbitrary_pure_state(self.pin(), &buffer).context("initializing pure state")
    }
    pub fn h(&mut self, qubit: usize) -> Result<()> {
        let q = self.check_qubit(qubit)?;
        quest_sys::apply_hadamard(self.pin(), q).context("applying H")
    }
    pub fn x(&mut self, qubit: usize) -> Result<()> {
        let q = self.check_qubit(qubit)?;
        quest_sys::apply_pauli_x(self.pin(), q).context("applying X")
    }
    pub fn y(&mut self, qubit: usize) -> Result<()> {
        let q = self.check_qubit(qubit)?;
        quest_sys::apply_pauli_y(self.pin(), q).context("applying Y")
    }
    pub fn z(&mut self, qubit: usize) -> Result<()> {
        let q = self.check_qubit(qubit)?;
        quest_sys::apply_pauli_z(self.pin(), q).context("applying Z")
    }
    pub fn cx(&mut self, control: usize, target: usize) -> Result<()> {
        let c = self.check_qubit(control)?;
        let t = self.check_qubit(target)?;
        if c == t {
            return Err(Error::Value("control and target must be distinct"));
        }
        quest_sys::apply_controlled_pauli_x(self.pin(), c, t).context("applying CX")
    }
    pub fn measure(&mut self, qubit: usize) -> Result<Outcome> {
        let q = self.check_qubit(qubit)?;
        match quest_sys::apply_qubit_measurement(self.pin(), q).context("measuring qubit")? {
            0 => Ok(Outcome::Zero),
            1 => Ok(Outcome::One),
            _ => Err(Error::Value("backend returned an invalid outcome")),
        }
    }
    pub fn probability(&self, qubit: usize, outcome: Outcome) -> Result<Probability> {
        let q = self.check_qubit(qubit)?;
        Probability::new(
            quest_sys::calc_prob_of_qubit_outcome(&self.native, q, i32::from(outcome.as_bool()))
                .context("calculating outcome probability")?,
        )
    }
    pub fn total_probability(&self) -> Result<f64> {
        quest_sys::calc_total_prob(&self.native).context("calculating total probability")
    }
    pub fn try_clone(&self) -> Result<Self> {
        let entries = if K::DENSITY {
            self.dimension()
                .checked_mul(self.dimension())
                .ok_or(Error::Overflow)?
        } else {
            self.dimension()
        };
        let reservation = self.environment().reserve(bytes_for(
            entries,
            if self.environment().capabilities().gpu {
                8
            } else {
                4
            },
        )?)?;
        let native = quest_sys::create_clone_qureg(&self.native).context("cloning register")?;
        Ok(Self {
            native,
            reservation,
            count: self.count,
            kind: PhantomData,
        })
    }
}
impl<'env> Register<'env, StateVector> {
    pub fn amplitude(&self, index: usize) -> Result<Complex64> {
        if index >= self.dimension() {
            return Err(Error::Index {
                index,
                bound: self.dimension(),
            });
        }
        let value =
            quest_sys::get_qureg_amp(&self.native, index as i64).context("reading amplitude")?;
        Ok(Complex64::new(value.re, value.im))
    }
    pub fn amplitudes(&self, start: usize, count: usize) -> Result<Vec<Complex64>> {
        if start.checked_add(count).ok_or(Error::Overflow)? > self.dimension() {
            return Err(Error::Index {
                index: start,
                bound: self.dimension(),
            });
        }
        let _scratch = self.environment().reserve(bytes_for(count, 3)?)?;
        let native = quest_sys::get_qureg_amps(&self.native, start as i64, count as i64)
            .context("reading amplitudes")?;
        let mut out = reserve_vec(count)?;
        out.extend(native.into_iter().map(|v| Complex64::new(v.re, v.im)));
        Ok(out)
    }
    pub fn snapshot(&self) -> Result<Mat<Complex64>> {
        let _scratch = self.environment().reserve(bytes_for(
            self.dimension().checked_add(4).ok_or(Error::Overflow)?,
            3,
        )?)?;
        let mut out = matrix(self.dimension(), 1)?;
        for offset in (0..self.dimension()).step_by(4096) {
            let count = (self.dimension() - offset).min(4096);
            let native = quest_sys::get_qureg_amps(&self.native, offset as i64, count as i64)
                .context("exporting state snapshot")?;
            for (i, value) in native.into_iter().enumerate() {
                out[(offset + i, 0)] = Complex64::new(value.re, value.im);
            }
        }
        Ok(out)
    }
    pub fn to_density(&self) -> Result<Register<'env, DensityMatrix>> {
        let mut density = self.environment().density_matrix(self.count)?;
        let values = self.amplitudes(0, self.dimension())?;
        density.init_pure(&values)?;
        Ok(density)
    }
    #[cfg(feature = "ndarray")]
    pub fn to_ndarray(&self) -> Result<ndarray::Array1<Complex64>> {
        Ok(ndarray::Array1::from_vec(
            self.amplitudes(0, self.dimension())?,
        ))
    }
}
impl Register<'_, DensityMatrix> {
    pub fn entry(&self, row: usize, col: usize) -> Result<Complex64> {
        if row >= self.dimension() || col >= self.dimension() {
            return Err(Error::Index {
                index: row.max(col),
                bound: self.dimension(),
            });
        }
        let value = quest_sys::get_density_qureg_amp(&self.native, row as i64, col as i64)
            .context("reading density entry")?;
        Ok(Complex64::new(value.re, value.im))
    }
    /// Write a logical rectangular matrix view without changing its orientation.
    /// This is a raw state edit; positive semidefiniteness is the caller's model choice.
    pub fn write_block<T: Conjugate<Canonical = Complex64>>(
        &mut self,
        row: usize,
        col: usize,
        view: MatRef<'_, T>,
    ) -> Result<()> {
        if row.checked_add(view.nrows()).ok_or(Error::Overflow)? > self.dimension()
            || col.checked_add(view.ncols()).ok_or(Error::Overflow)? > self.dimension()
        {
            return Err(Error::Value("density block outside register"));
        }
        let count = view
            .nrows()
            .checked_mul(view.ncols())
            .ok_or(Error::Overflow)?;
        let _scratch = self.environment().reserve(bytes_for(count, 4)?)?;
        let mut values = reserve_vec(count)?;
        for r in 0..view.nrows() {
            for c in 0..view.ncols() {
                let v = logical(view, r, c);
                if !v.re.is_finite() || !v.im.is_finite() {
                    return Err(Error::Value("matrix entries must be finite"));
                }
                values.push(quest_sys::QuestComplex { re: v.re, im: v.im });
            }
        }
        quest_sys::set_density_qureg_amps(
            self.pin(),
            row as i64,
            col as i64,
            &values,
            view.nrows() as i64,
            view.ncols() as i64,
        )
        .context("writing density block")
    }
    pub fn snapshot(&self) -> Result<Mat<Complex64>> {
        let entries = self
            .dimension()
            .checked_add(4)
            .and_then(|n| n.checked_mul(self.dimension()))
            .ok_or(Error::Overflow)?;
        let _scratch = self.environment().reserve(bytes_for(entries, 3)?)?;
        let mut out = matrix(self.dimension(), self.dimension())?;
        // Bounded column blocks avoid the native rectangular getter's transpose scratch growing with the full density matrix.
        for c in 0..self.dimension() {
            for r in (0..self.dimension()).step_by(4096) {
                let rows = (self.dimension() - r).min(4096);
                let buffer = quest_sys::get_density_qureg_amps(
                    &self.native,
                    r as i64,
                    c as i64,
                    rows as i64,
                    1,
                )
                .context("exporting density snapshot")?;
                for (i, v) in buffer.into_iter().enumerate() {
                    out[(r + i, c)] = Complex64::new(v.re, v.im);
                }
            }
        }
        Ok(out)
    }
    pub fn dephase(&mut self, qubit: usize, probability: Probability) -> Result<()> {
        let q = self.check_qubit(qubit)?;
        quest_sys::mix_dephasing(self.pin(), q, probability.get()).context("applying dephasing")
    }
}

pub(crate) fn logical<T: Conjugate<Canonical = Complex64>>(
    view: MatRef<'_, T>,
    row: usize,
    col: usize,
) -> Complex64 {
    let value = view.canonical()[(row, col)];
    if T::IS_CANONICAL { value } else { value.conj() }
}
pub(crate) fn matrix(rows: usize, cols: usize) -> Result<Mat<Complex64>> {
    let mut out = Mat::new();
    out.try_reserve(rows, cols).map_err(|_| Error::Allocation)?;
    out.resize_with(rows, cols, |_, _| Complex64::new(0., 0.));
    Ok(out)
}
pub(crate) fn pack(
    values: impl Iterator<Item = Complex64>,
    count: usize,
) -> Result<Vec<quest_sys::QuestComplex>> {
    let mut buffer = reserve_vec(count)?;
    for v in values {
        if !v.re.is_finite() || !v.im.is_finite() {
            return Err(Error::Value("amplitudes must be finite"));
        }
        buffer.push(quest_sys::QuestComplex { re: v.re, im: v.im });
    }
    Ok(buffer)
}
