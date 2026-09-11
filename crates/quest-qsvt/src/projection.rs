use crate::{
    Complex64, Error, Left, LogicalSpace, NumericalPolicy, ProjectedEncoding, Result, Right, matrix,
};
use faer::Mat;
use std::collections::BTreeSet;

/// Explicit physical positions. Source target order is semantically significant.
#[derive(Debug, Clone)]
pub struct OperandLayout {
    qubits: usize,
    source: Vec<usize>,
    response: usize,
    auxiliary: Option<usize>,
    idle_high: usize,
}
impl OperandLayout {
    /// # Errors
    /// Rejects repeated, missing, and out-of-range physical positions.
    pub fn new(
        qubits: usize,
        source: Vec<usize>,
        response: usize,
        auxiliary: Option<usize>,
    ) -> Result<Self> {
        let mut unique = BTreeSet::new();
        for position in source.iter().copied().chain([response]).chain(auxiliary) {
            if position >= qubits || !unique.insert(position) {
                return Err(Error::Encoding("invalid transform operand layout"));
            }
        }
        if unique.len() != qubits {
            return Err(Error::Encoding("transform layout must cover the register"));
        }
        Ok(Self {
            qubits,
            source,
            response,
            auxiliary,
            idle_high: 0,
        })
    }
    /// # Errors
    /// Rejects overflowing physical widths.
    pub fn canonical(source_width: usize, auxiliary: bool) -> Result<Self> {
        let offset = if auxiliary { 2usize } else { 1usize };
        let qubits = source_width
            .checked_add(offset)
            .ok_or(Error::Budget("transform width"))?;
        Self::new(
            qubits,
            (offset..qubits).collect(),
            0,
            auxiliary.then_some(1),
        )
    }
    /// Append initialized-zero high qubits without changing any active operand.
    /// Every input, bridge and output projection fixes these qubits to zero.
    /// # Errors
    /// Rejects width overflow and widths that cannot index a native-independent state.
    pub fn with_idle_high_qubits(mut self, additional: usize) -> Result<Self> {
        let qubits = self
            .qubits
            .checked_add(additional)
            .ok_or(Error::Budget("idle register width"))?;
        bit(qubits)?;
        self.idle_high = self
            .idle_high
            .checked_add(additional)
            .ok_or(Error::Budget("idle register width"))?;
        self.qubits = qubits;
        Ok(self)
    }
    #[must_use]
    pub const fn num_idle_high_qubits(&self) -> usize {
        self.idle_high
    }
    fn idle_controls(&self) -> impl Iterator<Item = ProjectionControl> {
        (self.qubits.saturating_sub(self.idle_high)..self.qubits).map(|qubit| ProjectionControl {
            qubit,
            value: false,
        })
    }
    #[must_use]
    pub const fn num_qubits(&self) -> usize {
        self.qubits
    }
    #[must_use]
    pub fn source(&self) -> &[usize] {
        &self.source
    }
    #[must_use]
    pub const fn response(&self) -> usize {
        self.response
    }
    #[must_use]
    pub const fn auxiliary(&self) -> Option<usize> {
        self.auxiliary
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProjectionControl {
    pub qubit: usize,
    pub value: bool,
}

/// Source interfaces remain strongly typed even when a route selects a different
/// output side. Joint hermitianization orders left logical coordinates first.
#[derive(Debug, Clone)]
pub enum ProjectionSpace {
    Left(LogicalSpace<Left>),
    Right(LogicalSpace<Right>),
    Joint {
        left: LogicalSpace<Left>,
        right: LogicalSpace<Right>,
    },
}
impl ProjectionSpace {
    #[must_use]
    pub const fn logical_dimension(&self) -> usize {
        match self {
            Self::Left(v) => v.logical_dimension(),
            Self::Right(v) => v.logical_dimension(),
            Self::Joint { left, right } => left
                .logical_dimension()
                .saturating_add(right.logical_dimension()),
        }
    }
    /// # Errors
    /// Rejects allocation limits for a cold dense snapshot.
    pub fn isometry_snapshot(&self, policy: NumericalPolicy) -> Result<Mat<Complex64>> {
        match self {
            Self::Left(v) => v.isometry_snapshot(policy),
            Self::Right(v) => v.isometry_snapshot(policy),
            Self::Joint { left, right } => joint_snapshot(left, right, policy),
        }
    }
}

/// A separate projection stage; this is never an oracle or coherent circuit.
#[derive(Debug, Clone)]
pub struct Projection {
    space: ProjectionSpace,
    targets: Vec<usize>,
    controls: Vec<ProjectionControl>,
}
impl Projection {
    #[must_use]
    pub fn source(
        encoding: &ProjectedEncoding,
        layout: &OperandLayout,
        left: bool,
        auxiliary: Option<bool>,
    ) -> Self {
        let mut controls = vec![ProjectionControl {
            qubit: layout.response,
            value: false,
        }];
        if let Some((qubit, value)) = layout.auxiliary.zip(auxiliary) {
            controls.push(ProjectionControl { qubit, value });
        }
        controls.extend(layout.idle_controls());
        Self {
            space: if left {
                ProjectionSpace::Left(encoding.left().clone())
            } else {
                ProjectionSpace::Right(encoding.right().clone())
            },
            targets: layout.source.clone(),
            controls,
        }
    }
    /// # Errors
    /// Rejects missing auxiliary positions and allocation limits.
    pub fn joint(encoding: &ProjectedEncoding, layout: &OperandLayout) -> Result<Self> {
        encoding
            .left()
            .physical_dimension()
            .checked_mul(2)
            .ok_or(Error::Budget("joint isometry rows"))?;
        encoding
            .left()
            .logical_dimension()
            .checked_add(encoding.right().logical_dimension())
            .ok_or(Error::Budget("joint isometry columns"))?;
        let mut targets = layout.source.clone();
        targets.push(
            layout
                .auxiliary
                .ok_or(Error::Encoding("joint projection requires auxiliary qubit"))?,
        );
        let mut controls = vec![ProjectionControl {
            qubit: layout.response,
            value: false,
        }];
        controls.extend(layout.idle_controls());
        Ok(Self {
            space: ProjectionSpace::Joint {
                left: encoding.left().clone(),
                right: encoding.right().clone(),
            },
            targets,
            controls,
        })
    }
    #[must_use]
    pub const fn space(&self) -> &ProjectionSpace {
        &self.space
    }
    #[must_use]
    pub fn targets(&self) -> &[usize] {
        &self.targets
    }
    #[must_use]
    pub fn controls(&self) -> &[ProjectionControl] {
        &self.controls
    }
    #[must_use]
    pub const fn logical_dimension(&self) -> usize {
        self.space.logical_dimension()
    }
    /// Materialize the ordered embedding into the complete physical register.
    /// # Errors
    /// Rejects overflowing dimensions and construction allocation limits.
    pub fn materialize_isometry(
        &self,
        qubits: usize,
        policy: NumericalPolicy,
    ) -> Result<Mat<Complex64>> {
        let dimension = bit(qubits)?;
        let basis = self.space.isometry_snapshot(policy)?;
        let mut embedded = matrix::allocate(dimension, basis.ncols(), policy, |_, _| {
            Complex64::new(0.0, 0.0)
        })?;
        let mut base = 0usize;
        for control in &self.controls {
            if control.value {
                base |= bit(control.qubit)?;
            }
        }
        for local in 0..basis.nrows() {
            let mut row = base;
            for (index, &target) in self.targets.iter().enumerate() {
                if local & bit(index)? != 0 {
                    row |= bit(target)?;
                }
            }
            if row >= dimension {
                return Err(Error::Encoding("projection register width"));
            }
            for col in 0..basis.ncols() {
                embedded[(row, col)] = basis[(local, col)];
            }
        }
        Ok(embedded)
    }
}
fn bit(index: usize) -> Result<usize> {
    1usize
        .checked_shl(u32::try_from(index).map_err(|_| Error::Budget("bit width"))?)
        .ok_or(Error::Budget("bit width"))
}

fn joint_snapshot(
    left: &LogicalSpace<Left>,
    right: &LogicalSpace<Right>,
    policy: NumericalPolicy,
) -> Result<Mat<Complex64>> {
    let dimension = left.physical_dimension();
    let rows = dimension
        .checked_mul(2)
        .ok_or(Error::Budget("joint isometry rows"))?;
    let cols = left
        .logical_dimension()
        .checked_add(right.logical_dimension())
        .ok_or(Error::Budget("joint isometry columns"))?;
    policy.check(rows, cols, 2)?;
    let left = left.isometry_snapshot(policy)?;
    let right = right.isometry_snapshot(policy)?;
    matrix::allocate(rows, cols, policy, |row, col| {
        if row < dimension && col < left.ncols() {
            left[(row, col)]
        } else if row >= dimension && col >= left.ncols() {
            right[(
                row.saturating_sub(dimension),
                col.saturating_sub(left.ncols()),
            )]
        } else {
            Complex64::new(0.0, 0.0)
        }
    })
}
