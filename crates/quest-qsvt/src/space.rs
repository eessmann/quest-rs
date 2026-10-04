use crate::{Complex64, Error, NumericalPolicy, Result, matrix};
use faer::{Mat, MatRef, mat::AsMatRef, traits::Conjugate};
use std::{marker::PhantomData, ops::Range, sync::Arc};

/// Marker for the logical row/output space of a projected encoding.
#[derive(Debug, Clone, Copy)]
pub struct Left;
/// Marker for the logical column/input space of a projected encoding.
#[derive(Debug, Clone, Copy)]
pub struct Right;
/// How the projector was supplied; exact coordinate membership is never inferred
/// by dropping small entries from a dense isometry.
#[derive(Debug, Clone)]
pub enum ProjectorKind {
	/// Exact computational coordinates in the caller's logical basis order.
	Coordinates(Arc<Vec<usize>>),
	/// Fixed physical bits and a contiguous range of packed free-bit values.
	/// Free bits are packed in ascending physical bit order.
	Compact {
		fixed_mask: usize,
		fixed_value: usize,
		logical_range: Range<usize>,
	},
	/// A supplied numerical isometry; coherent projector formed as `VV†`.
	Isometry,
	/// A separately supplied dense coherent projector payload.
	Dense(Arc<Mat<Complex64>>),
}
/// Immutable embedding of an ordered logical basis into an oracle's Hilbert space.
///
/// The side marker distinguishes encoding rows from columns; it does not force
/// equal logical dimensions. Coordinates remain compact until an explicit
/// [`Self::isometry_snapshot`] request.
#[derive(Debug, Clone)]
pub struct LogicalSpace<Side> {
	isometry: Option<Arc<Mat<Complex64>>>,
	dimension: usize,
	logical_dimension: usize,
	kind: ProjectorKind,
	residual: f64,
	_side: PhantomData<Side>,
}
impl<Side> LogicalSpace<Side> {
	/// Fix physical bits to the supplied values, with every remaining bit free.
	/// Logical basis order is increasing packed free-bit value: the lowest free
	/// physical bit is logical bit zero. No coordinate array is allocated.
	/// # Errors
	/// Rejects invalid dimensions/masks/values and retained-storage budgets.
	pub fn bit_constraints(
		dimension: usize,
		fixed_mask: usize,
		fixed_value: usize,
		policy: NumericalPolicy,
	) -> Result<Self> {
		let free_dimension = Self::free_dimension(dimension, fixed_mask, fixed_value)?;
		Self::constrained_range(
			dimension,
			fixed_mask,
			fixed_value,
			0..free_dimension,
			policy,
		)
	}
	/// Select a contiguous computational-coordinate range in increasing order.
	/// # Errors
	/// Rejects empty/out-of-bounds ranges, invalid dimensions or storage budgets.
	pub fn logical_range(
		dimension: usize,
		range: Range<usize>,
		policy: NumericalPolicy,
	) -> Result<Self> {
		Self::constrained_range(dimension, 0, 0, range, policy)
	}
	/// Select a contiguous range of packed free-bit values under fixed bits.
	/// For example, fixing bit 1 to one in an eight-dimensional register embeds
	/// packed values 0,1,2,3 as physical coordinates 2,3,6,7 respectively.
	/// # Errors
	/// Rejects invalid fixed bits, empty/out-of-bounds ranges or storage budgets.
	pub fn constrained_range(
		dimension: usize,
		fixed_mask: usize,
		fixed_value: usize,
		logical_range: Range<usize>,
		policy: NumericalPolicy,
	) -> Result<Self> {
		let free_dimension = Self::free_dimension(dimension, fixed_mask, fixed_value)?;
		if logical_range.start >= logical_range.end || logical_range.end > free_dimension {
			return Err(Error::Space("invalid compact logical range"));
		}
		if size_of::<[usize; 4]>() > policy.max_bytes {
			return Err(Error::Budget("compact logical space storage"));
		}
		Ok(Self {
			isometry: None,
			dimension,
			logical_dimension: logical_range.end.saturating_sub(logical_range.start),
			kind: ProjectorKind::Compact {
				fixed_mask,
				fixed_value,
				logical_range,
			},
			residual: 0.0,
			_side: PhantomData,
		})
	}
	const fn free_dimension(
		dimension: usize,
		fixed_mask: usize,
		fixed_value: usize,
	) -> Result<usize> {
		if dimension == 0
			|| !dimension.is_power_of_two()
			|| fixed_mask >= dimension
			|| fixed_value & !fixed_mask != 0
		{
			return Err(Error::Space("invalid compact bit constraints"));
		}
		Ok(dimension >> fixed_mask.count_ones())
	}
	/// Whether the ordered embedding consists of exact computational coordinates.
	#[must_use]
	pub const fn is_coordinate_space(&self) -> bool {
		matches!(
			self.kind,
			ProjectorKind::Coordinates(_) | ProjectorKind::Compact { .. }
		)
	}
	/// Physical coordinate of an ordered logical basis vector; dense embeddings
	/// and out-of-range logical indices return `None`.
	#[must_use]
	pub fn coordinate_at(&self, logical: usize) -> Option<usize> {
		if logical >= self.logical_dimension {
			return None;
		}
		match &self.kind {
			ProjectorKind::Coordinates(indices) => indices.get(logical).copied(),
			ProjectorKind::Compact {
				fixed_mask,
				fixed_value,
				logical_range,
			} => Some(
				*fixed_value
					| deposit(
						logical_range.start.checked_add(logical)?,
						self.dimension.saturating_sub(1) & !fixed_mask,
					),
			),
			_ => None,
		}
	}
	/// Exact coordinate membership, or `None` for numerical embeddings.
	#[must_use]
	pub fn contains_coordinate(&self, physical: usize) -> Option<bool> {
		match &self.kind {
			ProjectorKind::Coordinates(indices) => Some(indices.contains(&physical)),
			ProjectorKind::Compact {
				fixed_mask,
				fixed_value,
				logical_range,
			} => Some(
				physical < self.dimension
					&& physical & fixed_mask == *fixed_value
					&& logical_range.contains(&extract(
						physical,
						self.dimension.saturating_sub(1) & !fixed_mask,
					)),
			),
			_ => None,
		}
	}
	/// Disjoint bit cubes `(fixed_mask, fixed_value)` covering this projector.
	/// Compact ranges require at most twice the physical width; complete fixed-bit
	/// spaces require one cube. Explicit coordinates retain their supplied order.
	/// # Errors
	/// Rejects descriptor allocation failure.
	pub fn coordinate_cubes(&self) -> Result<Option<Vec<(usize, usize)>>> {
		let mut cubes = Vec::new();
		match &self.kind {
			ProjectorKind::Coordinates(indices) => {
				cubes
					.try_reserve_exact(indices.len())
					.map_err(|_| Error::Budget("coordinate cubes"))?;
				cubes.extend(
					indices
						.iter()
						.map(|&index| (self.dimension.saturating_sub(1), index)),
				);
			}
			ProjectorKind::Compact {
				fixed_mask,
				fixed_value,
				logical_range,
			} => {
				if logical_range.start == 0
					&& logical_range.end == self.dimension >> fixed_mask.count_ones()
				{
					cubes
						.try_reserve_exact(1)
						.map_err(|_| Error::Budget("compact coordinate cube"))?;
					cubes.push((*fixed_mask, *fixed_value));
					return Ok(Some(cubes));
				}
				cubes
					.try_reserve_exact(
						usize::try_from(self.dimension.ilog2())
							.map_err(|_| Error::Budget("compact width"))?
							.saturating_mul(2)
							.saturating_add(1),
					)
					.map_err(|_| Error::Budget("compact coordinate cubes"))?;
				let free_mask = self.dimension.saturating_sub(1) & !fixed_mask;
				let mut start = logical_range.start;
				while start < logical_range.end {
					let remaining = logical_range.end.saturating_sub(start);
					let exponent = start.trailing_zeros().min(remaining.ilog2());
					let size = 1usize << exponent;
					let varying = deposit(size.saturating_sub(1), free_mask);
					cubes.push((
						self.dimension.saturating_sub(1) & !varying,
						*fixed_value | deposit(start, free_mask),
					));
					start = start
						.checked_add(size)
						.ok_or(Error::Budget("compact range decomposition"))?;
				}
			}
			_ => return Ok(None),
		}
		Ok(Some(cubes))
	}
	/// Exact computational coordinates in caller order.
	///
	/// # Errors
	/// Rejects empty, repeated or out-of-range coordinates and allocation limits.
	pub fn coordinates(
		dimension: usize,
		coordinates: &[usize],
		policy: NumericalPolicy,
	) -> Result<Self> {
		if dimension == 0
			|| !dimension.is_power_of_two()
			|| coordinates.is_empty()
			|| coordinates.len() > dimension
		{
			return Err(Error::Space(
				"nonempty power-of-two physical space required",
			));
		}
		let bytes = coordinates
			.len()
			.checked_mul(size_of::<usize>())
			.and_then(|n| n.checked_mul(2))
			.ok_or(Error::Budget("coordinate storage"))?;
		if bytes > policy.max_bytes {
			return Err(Error::Budget("coordinate storage"));
		}
		let mut values = Vec::new();
		values
			.try_reserve_exact(coordinates.len())
			.map_err(|_| Error::Budget("coordinate allocation"))?;
		values.extend_from_slice(coordinates);
		let mut sorted = Vec::new();
		sorted
			.try_reserve_exact(coordinates.len())
			.map_err(|_| Error::Budget("coordinate validation allocation"))?;
		sorted.extend_from_slice(coordinates);
		sorted.sort_unstable();
		if sorted.iter().any(|&index| index >= dimension)
			|| sorted.windows(2).any(|pair| pair.first() == pair.last())
		{
			return Err(Error::Space("duplicate or invalid coordinate"));
		}
		Ok(Self {
			isometry: None,
			dimension,
			logical_dimension: coordinates.len(),
			kind: ProjectorKind::Coordinates(Arc::new(values)),
			residual: 0.0,
			_side: PhantomData,
		})
	}
	/// Copy logical values of a possibly strided/conjugated isometry and check
	/// V†V against identity at the fixed 1e-12 construction tolerance.
	///
	/// # Errors
	/// Rejects invalid dimensions, nonfinite values, failed admission or budgets.
	pub fn from_isometry<T: Conjugate<Canonical = Complex64>>(
		basis: impl AsMatRef<T = T, Rows = usize, Cols = usize>,
		policy: NumericalPolicy,
	) -> Result<Self> {
		let basis = basis.as_mat_ref();
		if basis.nrows() == 0
			|| !basis.nrows().is_power_of_two()
			|| basis.ncols() == 0
			|| basis.ncols() > basis.nrows()
		{
			return Err(Error::Space("isometry dimensions"));
		}
		policy.check(basis.nrows(), basis.nrows(), 3)?;
		let basis = matrix::snapshot(basis, policy)?;
		let gram = matrix::multiply(basis.adjoint(), basis.as_ref(), policy)?;
		let identity = matrix::allocate(gram.nrows(), gram.ncols(), policy, |row, col| {
			Complex64::new(if row == col { 1.0 } else { 0.0 }, 0.0)
		})?;
		let residual = matrix::difference(gram.as_ref(), identity.as_ref());
		if !residual.is_finite() || residual > 1e-12 {
			return Err(Error::Residual {
				operation: "isometry",
				residual,
				tolerance: 1e-12,
			});
		}
		Ok(Self {
			dimension: basis.nrows(),
			logical_dimension: basis.ncols(),
			isometry: Some(Arc::new(basis)),
			kind: ProjectorKind::Isometry,
			residual,
			_side: PhantomData,
		})
	}
	/// Supply a dense projector together with its ordered logical isometry.
	/// Both objects are retained; the matching residual is numerical evidence.
	///
	/// # Errors
	/// Rejects failed isometry admission, dimensions, nonfinite values or P≠VV†.
	pub fn from_dense_projector<
		T: Conjugate<Canonical = Complex64>,
		U: Conjugate<Canonical = Complex64>,
	>(
		projector: impl AsMatRef<T = T, Rows = usize, Cols = usize>,
		basis: impl AsMatRef<T = U, Rows = usize, Cols = usize>,
		policy: NumericalPolicy,
	) -> Result<Self> {
		let mut space = Self::from_isometry(basis, policy)?;
		let projector = projector.as_mat_ref();
		if projector.nrows() != space.physical_dimension()
			|| projector.ncols() != space.physical_dimension()
		{
			return Err(Error::Space("projector dimensions"));
		}
		policy.check(projector.nrows(), projector.ncols(), 4)?;
		let projector = matrix::snapshot(projector, policy)?;
		let expected = space.projector_matrix(policy)?;
		let residual = matrix::difference(projector.as_ref(), expected.as_ref());
		if !residual.is_finite() || residual > 1e-12 {
			return Err(Error::Residual {
				operation: "projector/isometry agreement",
				residual,
				tolerance: 1e-12,
			});
		}
		space.residual = space.residual.max(residual);
		space.kind = ProjectorKind::Dense(Arc::new(projector));
		Ok(space)
	}
	#[must_use]
	pub const fn physical_dimension(&self) -> usize {
		self.dimension
	}
	#[must_use]
	pub const fn logical_dimension(&self) -> usize {
		self.logical_dimension
	}
	/// Borrow an explicitly stored dense isometry. Coordinates remain compact.
	#[must_use]
	pub fn dense_isometry(&self) -> Option<MatRef<'_, Complex64>> {
		self.isometry.as_ref().map(|basis| basis.as_ref().as_ref())
	}
	/// Construct an owned ordered isometry only for cold numerical work.
	/// # Errors
	/// Rejects construction allocation limits.
	pub fn isometry_snapshot(&self, policy: NumericalPolicy) -> Result<Mat<Complex64>> {
		if let Some(basis) = self.dense_isometry() {
			return matrix::snapshot(basis, policy);
		}
		if !self.is_coordinate_space() {
			return Err(Error::Space("missing isometry storage"));
		}
		matrix::allocate(
			self.dimension,
			self.logical_dimension,
			policy,
			|row, col| Complex64::new(f64::from(self.coordinate_at(col) == Some(row)), 0.0),
		)
	}
	/// Retained payload bytes; coordinate storage does not scale with dimension.
	/// # Errors
	/// Rejects overflow in matrix storage accounting.
	pub fn storage_bytes(&self) -> Result<usize> {
		let basis = self.isometry.as_ref().map_or(Ok(0usize), |basis| {
			usize::try_from(basis.col_stride())
				.ok()
				.and_then(|stride| stride.checked_mul(basis.ncols()))
				.and_then(|entries| entries.checked_mul(size_of::<Complex64>()))
				.ok_or(Error::Budget("isometry bytes"))
		})?;
		let projector = match &self.kind {
			ProjectorKind::Coordinates(indices) => indices
				.len()
				.checked_mul(size_of::<usize>())
				.ok_or(Error::Budget("coordinate bytes"))?,
			ProjectorKind::Dense(matrix) => usize::try_from(matrix.col_stride())
				.ok()
				.and_then(|stride| stride.checked_mul(matrix.ncols()))
				.and_then(|entries| entries.checked_mul(size_of::<Complex64>()))
				.ok_or(Error::Budget("projector bytes"))?,
			ProjectorKind::Isometry => 0,
			ProjectorKind::Compact { .. } => size_of::<[usize; 4]>(),
		};
		basis
			.checked_add(projector)
			.ok_or(Error::Budget("logical space bytes"))
	}
	#[must_use]
	pub const fn kind(&self) -> &ProjectorKind {
		&self.kind
	}
	#[must_use]
	pub const fn construction_residual(&self) -> f64 {
		self.residual
	}
	/// Construct an independent owned projector snapshot.
	///
	/// Materialize the coherent projector representation.
	///
	/// For [`ProjectorKind::Dense`], returns the supplied `P`. For other kinds,
	/// forms canonical `VV†`. Projection/conditioning stages always use their
	/// ordered isometry, so numerical `P`/`VV†` agreement is not treated as exact.
	///
	/// # Errors
	/// Rejects allocation limits or invalid internal matrix dimensions.
	pub fn projector_matrix(&self, policy: NumericalPolicy) -> Result<Mat<Complex64>> {
		match &self.kind {
			ProjectorKind::Dense(projector) => {
				matrix::snapshot(projector.as_ref().as_ref(), policy)
			}
			ProjectorKind::Coordinates(_) | ProjectorKind::Compact { .. } => matrix::allocate(
				self.physical_dimension(),
				self.physical_dimension(),
				policy,
				|row, col| {
					Complex64::new(
						if row == col && self.contains_coordinate(row) == Some(true) {
							1.0
						} else {
							0.0
						},
						0.0,
					)
				},
			),
			ProjectorKind::Isometry => {
				let basis = self
					.dense_isometry()
					.ok_or(Error::Space("missing dense isometry"))?;
				matrix::multiply(basis, basis.adjoint(), policy)
			}
		}
	}
}

// Scatter/gather packed logical bits without allocating per-coordinate storage.
const fn deposit(mut packed: usize, mut mask: usize) -> usize {
	let mut value = 0;
	while mask != 0 {
		let bit = 1usize << mask.trailing_zeros();
		if packed & 1 != 0 {
			value |= bit;
		}
		packed >>= 1;
		mask &= mask.wrapping_sub(1);
	}
	value
}
const fn extract(value: usize, mut mask: usize) -> usize {
	let mut packed = 0;
	let mut local = 0u32;
	while mask != 0 {
		let bit = 1usize << mask.trailing_zeros();
		if value & bit != 0 {
			packed |= 1usize << local;
		}
		local = local.saturating_add(1);
		mask &= mask.wrapping_sub(1);
	}
	packed
}
