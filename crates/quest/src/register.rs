use crate::{Complex64, Error, Outcome, Probability, QubitCount, Result};
use crate::{
	environment::{EnvironmentView, Reservation, RuntimeResources},
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

/// Immutable deployment of one successfully allocated native register.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[expect(
	clippy::struct_excessive_bools,
	reason = "Native deployment exposes four independent hardware/mode flags"
)]
pub struct RegisterDeployment {
	density: bool,
	gpu: bool,
	distributed: bool,
	multithreaded: bool,
	width: usize,
	rank: usize,
	nodes: usize,
	local_amplitudes: usize,
}
impl RegisterDeployment {
	fn from_native(
		raw: quest_sys::QuestRegisterDeployment,
		density: bool,
		width: usize,
	) -> Result<Self> {
		let flag = |value| match value {
			0 => Ok(false),
			1 => Ok(true),
			_ => Err(Error::Value("invalid native register deployment")),
		};
		let actual_density = flag(raw.is_density_matrix)?;
		let gpu = flag(raw.is_gpu_accelerated)?;
		let distributed = flag(raw.is_distributed)?;
		let multithreaded = flag(raw.is_multithreaded)?;
		let actual_width = usize::try_from(raw.num_qubits)
			.map_err(|_| Error::Value("invalid native register width"))?;
		let rank =
			usize::try_from(raw.rank).map_err(|_| Error::Value("invalid native register rank"))?;
		let nodes = usize::try_from(raw.num_nodes)
			.map_err(|_| Error::Value("invalid native node count"))?;
		let local_amplitudes = usize::try_from(raw.num_amps_per_node)
			.map_err(|_| Error::Value("invalid native local amplitude count"))?;
		if actual_density != density
			|| actual_width != width
			|| nodes == 0
			|| rank >= nodes
			|| local_amplitudes == 0
			|| (!distributed && (nodes != 1 || rank != 0))
		{
			return Err(Error::Value("inconsistent native register deployment"));
		}
		Ok(Self {
			density,
			gpu,
			distributed,
			multithreaded,
			width,
			rank,
			nodes,
			local_amplitudes,
		})
	}
	#[must_use]
	pub const fn is_density_matrix(self) -> bool {
		self.density
	}
	#[must_use]
	pub const fn is_gpu_accelerated(self) -> bool {
		self.gpu
	}
	#[must_use]
	pub const fn is_distributed(self) -> bool {
		self.distributed
	}
	#[must_use]
	pub const fn is_multithreaded(self) -> bool {
		self.multithreaded
	}
	#[must_use]
	pub const fn width(self) -> usize {
		self.width
	}
	#[must_use]
	pub const fn rank(self) -> usize {
		self.rank
	}
	#[must_use]
	pub const fn nodes(self) -> usize {
		self.nodes
	}
	#[must_use]
	pub const fn local_amplitudes(self) -> usize {
		self.local_amplitudes
	}
	/// Copy this actual native deployment into the compiler's validated target.
	/// # Errors
	/// Rejects a local amplitude count that cannot fit the compiler DTO.
	pub fn compiler_snapshot(self) -> Result<quest_compile::DeploymentSnapshot> {
		let kind = if self.density {
			quest_compile::DeploymentKind::DensityMatrix
		} else {
			quest_compile::DeploymentKind::StateVector
		};
		Ok(quest_compile::DeploymentSnapshot::new(
			kind,
			self.width,
			self.gpu,
			self.distributed,
			self.multithreaded,
			self.rank,
			self.nodes,
			u64::try_from(self.local_amplitudes).map_err(|_| Error::Overflow)?,
		)?)
	}
}

/// A native register tied to its active environment. Cloning is explicit and deep.
pub struct Register<'env, K: RegisterKind> {
	// Declaration order ensures native destruction precedes releasing accounting.
	pub(crate) native: UniquePtr<quest_sys::Qureg>,
	reservation: Reservation<'env>,
	count: QubitCount,
	deployment: RegisterDeployment,
	kind: PhantomData<K>,
}
impl<'env, K: RegisterKind> Register<'env, K> {
	pub(crate) fn allocate(environment: &'env RuntimeResources, count: QubitCount) -> Result<Self> {
		let reservation = Self::admit_allocation(environment, count)?;
		Self::allocate_admitted(reservation, count)
	}
	pub(crate) fn admit_allocation(
		environment: &'env RuntimeResources,
		count: QubitCount,
	) -> Result<Reservation<'env>> {
		let entries = if K::DENSITY {
			count
				.dimension()
				.checked_mul(count.dimension())
				.ok_or(Error::Overflow)?
		} else {
			count.dimension()
		};
		// Include native state and workspace copies, on host and (when enabled) device.
		environment.reserve(bytes_for(
			entries,
			if environment.capabilities().gpu { 8 } else { 4 },
		)?)
	}
	pub(crate) fn allocate_admitted(
		reservation: Reservation<'env>,
		count: QubitCount,
	) -> Result<Self> {
		let (distribution, gpu, threads) = reservation.environment.register_allocation_modes();
		let native = quest_sys::create_custom_qureg(
			count.native(),
			i32::from(K::DENSITY),
			distribution,
			gpu,
			threads,
		)
		.context("allocating register")?;
		let deployment = RegisterDeployment::from_native(
			quest_sys::get_qureg_deployment(&native).context("reading register deployment")?,
			K::DENSITY,
			count.get(),
		)?;
		Ok(Self {
			native,
			reservation,
			count,
			deployment,
			kind: PhantomData,
		})
	}
	#[must_use]
	pub const fn num_qubits(&self) -> QubitCount {
		self.count
	}
	#[must_use]
	pub const fn deployment(&self) -> RegisterDeployment {
		self.deployment
	}
	#[must_use]
	pub const fn dimension(&self) -> usize {
		self.count.dimension()
	}
	#[must_use]
	pub const fn environment(&self) -> EnvironmentView<'env> {
		EnvironmentView {
			resources: self.resources(),
		}
	}
	pub(crate) const fn resources(&self) -> &'env RuntimeResources {
		self.reservation.environment
	}
	#[expect(
		clippy::unused_self,
		reason = "The receiver infers the sealed register kind at execution call sites"
	)]
	pub(crate) const fn is_density(&self) -> bool {
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
			i32::try_from(qubit).map_err(|_| Error::Overflow)
		}
	}
	/// # Errors
	/// Propagates native initialization failure.
	pub fn init_zero(&mut self) -> Result<()> {
		quest_sys::init_zero_state(self.pin()).context("initializing zero state")
	}
	/// # Errors
	/// Propagates native initialization failure.
	pub fn init_plus(&mut self) -> Result<()> {
		quest_sys::init_plus_state(self.pin()).context("initializing plus state")
	}
	/// # Errors
	/// Rejects incorrect amplitude counts, nonfinite entries, resource limits, or native initialization failure.
	pub fn init_pure(&mut self, amplitudes: &[Complex64]) -> Result<()> {
		if amplitudes.len() != self.dimension() {
			return Err(Error::Value("pure-state amplitude count must be 2^qubits"));
		}
		let _scratch = self.resources().reserve(bytes_for(amplitudes.len(), 3)?)?;
		let buffer = pack(amplitudes.iter().copied(), amplitudes.len())?;
		quest_sys::init_arbitrary_pure_state(self.pin(), &buffer).context("initializing pure state")
	}
	/// # Errors
	/// Rejects an out-of-range qubit or native gate failure.
	pub fn h(&mut self, qubit: usize) -> Result<()> {
		let q = self.check_qubit(qubit)?;
		quest_sys::apply_hadamard(self.pin(), q).context("applying H")
	}
	/// # Errors
	/// Rejects an out-of-range qubit or native gate failure.
	pub fn x(&mut self, qubit: usize) -> Result<()> {
		let q = self.check_qubit(qubit)?;
		quest_sys::apply_pauli_x(self.pin(), q).context("applying X")
	}
	/// # Errors
	/// Rejects an out-of-range qubit or native gate failure.
	pub fn y(&mut self, qubit: usize) -> Result<()> {
		let q = self.check_qubit(qubit)?;
		quest_sys::apply_pauli_y(self.pin(), q).context("applying Y")
	}
	/// # Errors
	/// Rejects an out-of-range qubit or native gate failure.
	pub fn z(&mut self, qubit: usize) -> Result<()> {
		let q = self.check_qubit(qubit)?;
		quest_sys::apply_pauli_z(self.pin(), q).context("applying Z")
	}
	/// # Errors
	/// Rejects out-of-range or overlapping qubits and native gate failure.
	pub fn cx(&mut self, control: usize, target: usize) -> Result<()> {
		let c = self.check_qubit(control)?;
		let t = self.check_qubit(target)?;
		if c == t {
			return Err(Error::Value("control and target must be distinct"));
		}
		quest_sys::apply_controlled_pauli_x(self.pin(), c, t).context("applying CX")
	}
	/// # Errors
	/// Rejects out-of-range qubits, invalid native outcomes, or native measurement failure.
	pub fn measure(&mut self, qubit: usize) -> Result<Outcome> {
		let q = self.check_qubit(qubit)?;
		match quest_sys::apply_qubit_measurement(self.pin(), q).context("measuring qubit")? {
			0 => Ok(Outcome::Zero),
			1 => Ok(Outcome::One),
			_ => Err(Error::Value("backend returned an invalid outcome")),
		}
	}
	/// # Errors
	/// Rejects out-of-range qubits, invalid probabilities, or native calculation failure.
	pub fn probability(&self, qubit: usize, outcome: Outcome) -> Result<Probability> {
		let q = self.check_qubit(qubit)?;
		Probability::new(
			quest_sys::calc_prob_of_qubit_outcome(&self.native, q, i32::from(outcome.as_bool()))
				.context("calculating outcome probability")?,
		)
	}
	/// # Errors
	/// Propagates native probability calculation failure.
	pub fn total_probability(&self) -> Result<f64> {
		quest_sys::calc_total_prob(&self.native).context("calculating total probability")
	}
	/// # Errors
	/// Rejects allocation overflow, memory budget exhaustion, or native clone failure.
	pub fn try_clone(&self) -> Result<Self> {
		let reservation = Self::admit_allocation(self.resources(), self.count)?;
		let native = quest_sys::create_clone_qureg(&self.native).context("cloning register")?;
		let deployment = RegisterDeployment::from_native(
			quest_sys::get_qureg_deployment(&native)
				.context("reading cloned register deployment")?,
			K::DENSITY,
			self.count.get(),
		)?;
		Ok(Self {
			native,
			reservation,
			count: self.count,
			deployment,
			kind: PhantomData,
		})
	}
}
impl<'env> Register<'env, StateVector> {
	/// # Errors
	/// Rejects out-of-range indices or native read failure.
	pub fn amplitude(&self, index: usize) -> Result<Complex64> {
		if index >= self.dimension() {
			return Err(Error::Index {
				index,
				bound: self.dimension(),
			});
		}
		let value = quest_sys::get_qureg_amp(
			&self.native,
			i64::try_from(index).map_err(|_| Error::Overflow)?,
		)
		.context("reading amplitude")?;
		Ok(Complex64::new(value.re, value.im))
	}
	/// # Errors
	/// Rejects out-of-range intervals, allocation limits, or native read failure.
	pub fn amplitudes(&self, start: usize, count: usize) -> Result<Vec<Complex64>> {
		if start.checked_add(count).ok_or(Error::Overflow)? > self.dimension() {
			return Err(Error::Index {
				index: start,
				bound: self.dimension(),
			});
		}
		let _scratch = self.resources().reserve(bytes_for(count, 3)?)?;
		let native = quest_sys::get_qureg_amps(
			&self.native,
			i64::try_from(start).map_err(|_| Error::Overflow)?,
			i64::try_from(count).map_err(|_| Error::Overflow)?,
		)
		.context("reading amplitudes")?;
		let mut out = reserve_vec(count)?;
		out.extend(native.into_iter().map(|v| Complex64::new(v.re, v.im)));
		Ok(out)
	}
	/// # Errors
	/// Rejects allocation overflow, memory budget exhaustion, or native read failure.
	pub fn snapshot(&self) -> Result<Mat<Complex64>> {
		let _scratch = self.resources().reserve(bytes_for(
			self.dimension().checked_add(4).ok_or(Error::Overflow)?,
			3,
		)?)?;
		let mut out = matrix(self.dimension(), 1)?;
		for offset in (0..self.dimension()).step_by(4096) {
			let count = self
				.dimension()
				.checked_sub(offset)
				.ok_or(Error::Overflow)?
				.min(4096);
			let native = quest_sys::get_qureg_amps(
				&self.native,
				i64::try_from(offset).map_err(|_| Error::Overflow)?,
				i64::try_from(count).map_err(|_| Error::Overflow)?,
			)
			.context("exporting state snapshot")?;
			for (i, value) in native.into_iter().enumerate() {
				out[(offset.checked_add(i).ok_or(Error::Overflow)?, 0)] =
					Complex64::new(value.re, value.im);
			}
		}
		Ok(out)
	}
	/// # Errors
	/// Rejects resource limits or native state transfer failure.
	pub fn to_density(&self) -> Result<Register<'env, DensityMatrix>> {
		let mut density = self.resources().density_matrix(self.count)?;
		quest_sys::init_pure_state(density.pin(), &self.native)
			.context("initializing density from pure state")?;
		Ok(density)
	}
	#[cfg(feature = "ndarray")]
	/// # Errors
	/// Rejects resource limits or native state read failure.
	pub fn to_ndarray(&self) -> Result<ndarray::Array1<Complex64>> {
		Ok(ndarray::Array1::from_vec(
			self.amplitudes(0, self.dimension())?,
		))
	}
}
impl Register<'_, DensityMatrix> {
	/// # Errors
	/// Rejects out-of-range matrix coordinates or native read failure.
	pub fn entry(&self, row: usize, col: usize) -> Result<Complex64> {
		if row >= self.dimension() || col >= self.dimension() {
			return Err(Error::Index {
				index: row.max(col),
				bound: self.dimension(),
			});
		}
		let value = quest_sys::get_density_qureg_amp(
			&self.native,
			i64::try_from(row).map_err(|_| Error::Overflow)?,
			i64::try_from(col).map_err(|_| Error::Overflow)?,
		)
		.context("reading density entry")?;
		Ok(Complex64::new(value.re, value.im))
	}
	/// Write a logical rectangular matrix view without changing its orientation.
	/// This is a raw state edit; positive semidefiniteness is the caller's model choice.
	/// # Errors
	/// Rejects out-of-range blocks, nonfinite entries, resource limits, or native write failure.
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
		let _scratch = self.resources().reserve(bytes_for(count, 4)?)?;
		let values = pack_matrix(view)?;
		quest_sys::set_density_qureg_amps(
			self.pin(),
			i64::try_from(row).map_err(|_| Error::Overflow)?,
			i64::try_from(col).map_err(|_| Error::Overflow)?,
			&values,
			i64::try_from(view.nrows()).map_err(|_| Error::Overflow)?,
			i64::try_from(view.ncols()).map_err(|_| Error::Overflow)?,
		)
		.context("writing density block")
	}
	/// # Errors
	/// Rejects allocation overflow, memory budget exhaustion, or native read failure.
	pub fn snapshot(&self) -> Result<Mat<Complex64>> {
		let entries = self
			.dimension()
			.checked_add(4)
			.and_then(|n| n.checked_mul(self.dimension()))
			.ok_or(Error::Overflow)?;
		let _scratch = self.resources().reserve(bytes_for(entries, 3)?)?;
		let mut out = matrix(self.dimension(), self.dimension())?;
		// Bounded column blocks avoid the native rectangular getter's transpose scratch growing with the full density matrix.
		for c in 0..self.dimension() {
			for r in (0..self.dimension()).step_by(4096) {
				let rows = self
					.dimension()
					.checked_sub(r)
					.ok_or(Error::Overflow)?
					.min(4096);
				let buffer = quest_sys::get_density_qureg_amps(
					&self.native,
					i64::try_from(r).map_err(|_| Error::Overflow)?,
					i64::try_from(c).map_err(|_| Error::Overflow)?,
					i64::try_from(rows).map_err(|_| Error::Overflow)?,
					1,
				)
				.context("exporting density snapshot")?;
				for (i, v) in buffer.into_iter().enumerate() {
					out[(r.checked_add(i).ok_or(Error::Overflow)?, c)] = Complex64::new(v.re, v.im);
				}
			}
		}
		Ok(out)
	}
	/// # Errors
	/// Rejects an out-of-range qubit or native channel failure.
	pub fn dephase(&mut self, qubit: usize, probability: Probability) -> Result<()> {
		let q = self.check_qubit(qubit)?;
		quest_sys::mix_dephasing(self.pin(), q, probability.get()).context("applying dephasing")
	}
}

pub fn logical<T: Conjugate<Canonical = Complex64>>(
	view: MatRef<'_, T>,
	row: usize,
	col: usize,
) -> Complex64 {
	let value = view.canonical()[(row, col)];
	if T::IS_CANONICAL { value } else { value.conj() }
}
pub fn matrix(rows: usize, cols: usize) -> Result<Mat<Complex64>> {
	let mut out = Mat::new();
	out.try_reserve(rows, cols).map_err(|_| Error::Allocation)?;
	out.resize_with(rows, cols, |_, _| Complex64::new(0., 0.));
	Ok(out)
}
pub fn pack(
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

/// Pack the logical view, including conjugation, in the bridge's row-major order.
pub fn pack_matrix<T: Conjugate<Canonical = Complex64>>(
	view: MatRef<'_, T>,
) -> Result<Vec<quest_sys::QuestComplex>> {
	let count = view
		.nrows()
		.checked_mul(view.ncols())
		.ok_or(Error::Overflow)?;
	pack(
		(0..view.nrows()).flat_map(|row| (0..view.ncols()).map(move |col| logical(view, row, col))),
		count,
	)
}
