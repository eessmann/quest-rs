use super::{Error, Result};
use crate::{Complex64, Register, StateVector};
use crate::{
	environment::{Reservation, RuntimeResources},
	error::BackendResult,
	values::{bytes_for, reserve_vec},
};
use cxx::UniquePtr;
use quest_qsvt::{NumericalPolicy, Projection, ProjectionSpace, ProjectorKind};

enum NativeProjector {
	Identity,
	Cubes { masks: Vec<u64>, values: Vec<u64> },
	Diagonal(UniquePtr<quest_sys::DiagMatr>),
	Dense(UniquePtr<quest_sys::CompMatr>),
}
pub(super) struct PreparedProjection<'env> {
	native: NativeProjector,
	// Drop native storage before releasing its accounting reservation.
	_reservation: Reservation<'env>,
	targets: Vec<i32>,
	controls: Vec<i32>,
	outcomes: Vec<i32>,
}
enum ProjectionData {
	Identity,
	Cubes { masks: Vec<u64>, values: Vec<u64> },
	Diagonal(Vec<quest_sys::QuestComplex>),
	Dense(faer::Mat<Complex64>),
}
pub(super) struct AdmittedProjection<'env> {
	data: ProjectionData,
	reservation: Reservation<'env>,
	targets: Vec<i32>,
	controls: Vec<i32>,
	outcomes: Vec<i32>,
}
impl<'env> AdmittedProjection<'env> {
	pub(super) fn native_dispatches(&self) -> usize {
		usize::from(!self.controls.is_empty())
			.saturating_add(usize::from(!matches!(self.data, ProjectionData::Identity)))
	}

	pub(super) fn new(
		environment: &'env RuntimeResources,
		projection: &Projection,
	) -> Result<Self> {
		let available = environment
			.memory_budget()
			.bytes()
			.saturating_sub(environment.allocated_bytes());
		let mut targets = projection
			.targets()
			.iter()
			.map(|&q| i32::try_from(q).map_err(|_| crate::Error::Overflow))
			.collect::<crate::Result<Vec<_>>>()?;
		let controls = projection
			.controls()
			.iter()
			.map(|q| i32::try_from(q.qubit).map_err(|_| crate::Error::Overflow))
			.collect::<crate::Result<Vec<_>>>()?;
		let outcomes: Vec<i32> = projection
			.controls()
			.iter()
			.map(|q| i32::from(q.value))
			.collect();
		let dimension = bit(targets.len())?;
		let overhead = targets
			.len()
			.checked_add(controls.len())
			.and_then(|n| n.checked_mul(64))
			.and_then(|n| n.checked_add(size_of::<PreparedProjection<'_>>()))
			.ok_or(crate::Error::Overflow)?;
		if projection.space().is_coordinate_space() {
			return Self::coordinate(
				environment,
				projection,
				dimension,
				overhead,
				targets,
				controls,
				outcomes,
			);
		}
		let native = dense(
			environment,
			projection,
			dimension,
			overhead,
			available,
			&mut targets,
			&controls,
		)?;
		Ok(Self {
			data: native.0,
			reservation: native.1,
			targets,
			controls,
			outcomes,
		})
	}
	fn coordinate(
		environment: &'env RuntimeResources,
		projection: &Projection,
		dimension: usize,
		overhead: usize,
		targets: Vec<i32>,
		mut controls: Vec<i32>,
		mut outcomes: Vec<i32>,
	) -> Result<Self> {
		// Compact ranges never enumerate logical indices, even during admission.
		let compact = match projection.space() {
			ProjectionSpace::Left(space) => {
				matches!(space.kind(), ProjectorKind::Compact { .. })
			}
			ProjectionSpace::Right(space) => {
				matches!(space.kind(), ProjectorKind::Compact { .. })
			}
			ProjectionSpace::Joint { left, right } => {
				matches!(left.kind(), ProjectorKind::Compact { .. })
					|| matches!(right.kind(), ProjectorKind::Compact { .. })
			}
		};
		let cubes = if compact {
			projection.space().coordinate_cubes()?
		} else {
			None
		};
		let single = cubes.as_ref().map_or_else(
			|| coordinate_cube(projection.space(), dimension),
			|cubes| (cubes.len() == 1).then(|| cubes.first().copied()).flatten(),
		);
		if let Some((mask, value)) = single {
			for (local, &target) in targets.iter().enumerate() {
				let bit = bit(local)?;
				if mask & bit != 0 {
					controls.push(target);
					outcomes.push(i32::from(value & bit != 0));
				}
			}
			return Ok(Self {
				data: ProjectionData::Identity,
				reservation: environment.reserve(overhead)?,
				targets,
				controls,
				outcomes,
			});
		}
		if let Some(cubes) = cubes {
			if environment.capabilities().gpu {
				return Err(crate::Error::Value(
					"compact range projection currently requires CPU execution",
				)
				.into());
			}
			let bytes = cubes
				.len()
				.checked_mul(size_of::<[u64; 2]>())
				.and_then(|n| n.checked_add(overhead))
				.ok_or(crate::Error::Overflow)?;
			let reservation = environment.reserve(bytes)?;
			let mut masks = reserve_vec(cubes.len())?;
			let mut values = reserve_vec(cubes.len())?;
			for (mask, value) in cubes {
				masks.push(u64::try_from(mask).map_err(|_| crate::Error::Overflow)?);
				values.push(u64::try_from(value).map_err(|_| crate::Error::Overflow)?);
			}
			return Ok(Self {
				data: ProjectionData::Cubes { masks, values },
				reservation,
				targets,
				controls,
				outcomes,
			});
		}
		// Preserve the existing GPU-capable explicit-coordinate diagonal route.
		let bytes = bytes_for(
			dimension,
			if environment.capabilities().gpu { 5 } else { 3 },
		)?
		.checked_add(overhead)
		.ok_or(crate::Error::Overflow)?;
		let reservation = environment.reserve(bytes)?;
		let mut values = reserve_vec(dimension)?;
		values.resize(dimension, quest_sys::QuestComplex { re: 0.0, im: 0.0 });
		for logical in 0..projection.logical_dimension() {
			let coordinate = projection
				.space()
				.coordinate_at(logical)
				.ok_or(crate::Error::Value("projection coordinate"))?;
			values
				.get_mut(coordinate)
				.ok_or(crate::Error::Value("projection coordinate"))?
				.re = 1.0;
		}
		Ok(Self {
			data: ProjectionData::Diagonal(values),
			reservation,
			targets,
			controls,
			outcomes,
		})
	}
	pub(super) fn materialize(self) -> Result<PreparedProjection<'env>> {
		let native = match self.data {
			ProjectionData::Identity => NativeProjector::Identity,
			ProjectionData::Cubes { masks, values } => NativeProjector::Cubes { masks, values },
			ProjectionData::Diagonal(values) => {
				let mut native = quest_sys::create_diag_matr(
					i32::try_from(self.targets.len()).map_err(|_| crate::Error::Overflow)?,
				)
				.context("allocating coordinate projection")?;
				quest_sys::set_diag_matr(native.pin_mut(), &values)
					.context("transferring coordinate projection")?;
				NativeProjector::Diagonal(native)
			}
			ProjectionData::Dense(matrix) => {
				NativeProjector::Dense(crate::execution::native_matrix(matrix.as_ref())?)
			}
		};
		Ok(PreparedProjection {
			native,
			_reservation: self.reservation,
			targets: self.targets,
			controls: self.controls,
			outcomes: self.outcomes,
		})
	}
}
fn dense<'env>(
	environment: &'env RuntimeResources,
	projection: &Projection,
	dimension: usize,
	overhead: usize,
	available: usize,
	targets: &mut Vec<i32>,
	controls: &[i32],
) -> Result<(ProjectionData, Reservation<'env>)> {
	let entries = dimension
		.max(2)
		.checked_mul(dimension.max(2))
		.ok_or(crate::Error::Overflow)?;
	let bytes = bytes_for(entries, if environment.capabilities().gpu { 8 } else { 6 })?
		.checked_add(overhead)
		.ok_or(crate::Error::Overflow)?;
	let reservation = environment.reserve(bytes)?;
	let basis = projection.space().isometry_snapshot(NumericalPolicy {
		max_bytes: available.min(bytes),
	})?;
	let mut matrix = crate::register::matrix(dimension, dimension)?;
	faer::linalg::matmul::matmul(
		matrix.as_mut(),
		faer::Accum::Replace,
		basis.as_ref(),
		basis.adjoint(),
		Complex64::new(1.0, 0.0),
		faer::Par::Seq,
	);
	let matrix = if dimension == 1 {
		targets.push(*controls.first().ok_or(crate::Error::Value(
			"scalar projection requires a physical qubit",
		))?);
		let scalar = matrix[(0, 0)];
		faer::Mat::from_fn(2, 2, |r, c| {
			if r == c {
				scalar
			} else {
				Complex64::new(0.0, 0.0)
			}
		})
	} else {
		matrix
	};
	Ok((ProjectionData::Dense(matrix), reservation))
}
impl<'env> PreparedProjection<'env> {
	#[expect(
		dead_code,
		reason = "Single-environment convenience boundary retained alongside aggregate admission"
	)]
	pub(super) fn new(
		environment: &'env RuntimeResources,
		projection: &Projection,
	) -> Result<Self> {
		AdmittedProjection::new(environment, projection)?.materialize()
	}
	pub(super) fn apply(&self, register: &mut Register<'_, StateVector>) -> crate::Result<()> {
		if !self.controls.is_empty() {
			quest_sys::apply_multi_qubit_projector(register.pin(), &self.controls, &self.outcomes)
				.context("projecting QSVT control sector")?;
		}
		match &self.native {
			NativeProjector::Identity => Ok(()),
			NativeProjector::Cubes { masks, values } => {
				quest_sys::project_qureg_basis_cubes(register.pin(), &self.targets, masks, values)
					.context("applying compact coordinate projection")
			}
			NativeProjector::Diagonal(matrix) => {
				quest_sys::leftapply_diag_matr(register.pin(), &self.targets, matrix)
					.context("applying coordinate projection")
			}
			NativeProjector::Dense(matrix) => {
				quest_sys::leftapply_comp_matr(register.pin(), &self.targets, matrix)
					.context("applying isometry projection")
			}
		}
	}
}
// Explicit coordinates may already describe a cube; inspect them lazily.
fn coordinate_cube(space: &ProjectionSpace, dimension: usize) -> Option<(usize, usize)> {
	let base = space.coordinate_at(0)?;
	let varying = (0..space.logical_dimension()).try_fold(0usize, |bits, logical| {
		Some(bits | (space.coordinate_at(logical)? ^ base))
	})?;
	(1usize.checked_shl(varying.count_ones())? == space.logical_dimension())
		.then_some(((dimension.saturating_sub(1)) & !varying, base & !varying))
}
fn bit(position: usize) -> Result<usize> {
	1usize
		.checked_shl(u32::try_from(position).map_err(|_| crate::Error::Overflow)?)
		.ok_or(Error::Runtime(crate::Error::Overflow))
}
