use super::{Error, Result};
use crate::{Complex64, Register, StateVector};
use crate::{
    environment::{Reservation, RuntimeResources},
    error::BackendResult,
    values::{bytes_for, reserve_vec},
};
use cxx::UniquePtr;
use quest_qsvt::{LogicalSpace, NumericalPolicy, Projection, ProjectionSpace, ProjectorKind};

enum NativeProjector {
    Identity,
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
        let mut controls = projection
            .controls()
            .iter()
            .map(|q| i32::try_from(q.qubit).map_err(|_| crate::Error::Overflow))
            .collect::<crate::Result<Vec<_>>>()?;
        let mut outcomes: Vec<i32> = projection
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
        let _scratch = environment.reserve(
            projection
                .logical_dimension()
                .checked_mul(size_of::<usize>())
                .and_then(|n| n.checked_mul(3))
                .ok_or(crate::Error::Overflow)?,
        )?;
        let coordinates = coordinate_indices(projection.space())?;
        let is_cube = coordinates.as_ref().and_then(|indices| cube(indices));
        if let Some((base, varying)) = is_cube {
            for (local, &target) in targets.iter().enumerate() {
                let mask = bit(local)?;
                if varying & mask == 0 {
                    controls.push(target);
                    outcomes.push(i32::from(base & mask != 0));
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
        if let Some(indices) = coordinates {
            let bytes = bytes_for(
                dimension,
                if environment.capabilities().gpu { 5 } else { 3 },
            )?
            .checked_add(overhead)
            .ok_or(crate::Error::Overflow)?;
            let reservation = environment.reserve(bytes)?;
            let mut values = reserve_vec(dimension)?;
            values.resize(dimension, quest_sys::QuestComplex { re: 0.0, im: 0.0 });
            for index in indices {
                values
                    .get_mut(index)
                    .ok_or(crate::Error::Value("projection coordinate"))?
                    .re = 1.0;
            }
            return Ok(Self {
                data: ProjectionData::Diagonal(values),
                reservation,
                targets,
                controls,
                outcomes,
            });
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
    pub(super) fn materialize(self) -> Result<PreparedProjection<'env>> {
        let native = match self.data {
            ProjectionData::Identity => NativeProjector::Identity,
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
fn copy_coordinates<S>(space: &LogicalSpace<S>) -> crate::Result<Option<Vec<usize>>> {
    if let ProjectorKind::Coordinates(values) = space.kind() {
        let mut copied = reserve_vec(values.len())?;
        copied.extend_from_slice(values);
        Ok(Some(copied))
    } else {
        Ok(None)
    }
}
pub(super) fn coordinate_indices(space: &ProjectionSpace) -> Result<Option<Vec<usize>>> {
    match space {
        ProjectionSpace::Left(v) => Ok(copy_coordinates(v)?),
        ProjectionSpace::Right(v) => Ok(copy_coordinates(v)?),
        ProjectionSpace::Joint { left, right } => {
            if let (Some(mut a), Some(b)) = (copy_coordinates(left)?, copy_coordinates(right)?) {
                a.try_reserve_exact(b.len())
                    .map_err(|_| crate::Error::Allocation)?;
                for value in b {
                    a.push(
                        value
                            .checked_add(left.physical_dimension())
                            .ok_or(crate::Error::Overflow)?,
                    );
                }
                Ok(Some(a))
            } else {
                Ok(None)
            }
        }
    }
}
// A unique coordinate set of cardinality 2^varying_bits contains the entire
// subcube. Exact integer membership, never numerical sparsity inference.
fn cube(indices: &[usize]) -> Option<(usize, usize)> {
    let base = *indices.first()?;
    let varying = indices
        .iter()
        .fold(0usize, |bits, &index| bits | (index ^ base));
    (1usize.checked_shl(varying.count_ones())? == indices.len()).then_some((base, varying))
}
fn bit(position: usize) -> Result<usize> {
    1usize
        .checked_shl(u32::try_from(position).map_err(|_| crate::Error::Overflow)?)
        .ok_or(Error::Runtime(crate::Error::Overflow))
}
