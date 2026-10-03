use crate::{Complex64, Error, NumericalPolicy, Result, matrix};
use faer::{Mat, MatRef, mat::AsMatRef, traits::Conjugate};
use std::{marker::PhantomData, sync::Arc};

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
		match &self.kind {
			ProjectorKind::Coordinates(indices) => matrix::allocate(
				self.dimension,
				self.logical_dimension,
				policy,
				|row, col| {
					Complex64::new(
						if indices.get(col) == Some(&row) {
							1.0
						} else {
							0.0
						},
						0.0,
					)
				},
			),
			_ => Err(Error::Space("missing isometry storage")),
		}
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
			ProjectorKind::Coordinates(indices) => matrix::allocate(
				self.physical_dimension(),
				self.physical_dimension(),
				policy,
				|row, col| {
					Complex64::new(
						if row == col && indices.contains(&row) {
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
