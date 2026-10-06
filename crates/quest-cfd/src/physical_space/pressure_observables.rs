use super::{CfdError, PhysicalSpace, Point, dot, reserved};
use crate::simplex::PressureProbeLimits;

/// Exterior box side. Periodic boxes have no exterior sides.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub enum BoxBoundarySide {
	XMin,
	XMax,
	YMin,
	YMax,
	ZMin,
	ZMax,
}
impl BoxBoundarySide {
	#[must_use]
	pub const fn axis(self) -> usize {
		match self {
			Self::XMin | Self::XMax => 0,
			Self::YMin | Self::YMax => 1,
			Self::ZMin | Self::ZMax => 2,
		}
	}
	#[must_use]
	pub const fn upper(self) -> bool {
		matches!(self, Self::XMax | Self::YMax | Self::ZMax)
	}
	/// Canonical box-only label; simplex boundary labels retain their existing meaning.
	#[must_use]
	pub const fn label(self) -> &'static str {
		match self {
			Self::XMin => "x-min",
			Self::XMax => "x-max",
			Self::YMin => "y-min",
			Self::YMax => "y-max",
			Self::ZMin => "z-min",
			Self::ZMax => "z-max",
		}
	}
	/// # Errors
	/// Rejects any label other than the six explicit box-side names.
	pub fn from_label(label: &str) -> Result<Self, CfdError> {
		match label {
			"x-min" => Ok(Self::XMin),
			"x-max" => Ok(Self::XMax),
			"y-min" => Ok(Self::YMin),
			"y-max" => Ok(Self::YMax),
			"z-min" => Ok(Self::ZMin),
			"z-max" => Ok(Self::ZMax),
			_ => Err(invalid()),
		}
	}
}

/// Additional bounded mechanical-traction query costs, excluding the prepared mesh.
#[derive(Clone, Copy, Debug)]
pub struct MechanicalTractionLimits {
	pub max_work: usize,
	pub max_bytes: usize,
}
impl Default for MechanicalTractionLimits {
	fn default() -> Self {
		Self {
			max_work: 100_000_000,
			max_bytes: 8 * 1024 * 1024,
		}
	}
}
const fn invalid() -> CfdError {
	CfdError::InvalidInput("physical pressure/traction query admission")
}
fn add(a: usize, b: usize) -> Result<usize, CfdError> {
	a.checked_add(b).ok_or_else(invalid)
}
fn mul(a: usize, b: usize) -> Result<usize, CfdError> {
	a.checked_mul(b).ok_or_else(invalid)
}

#[derive(Clone, Copy)]
pub(super) enum TractionSelector<'a> {
	BoxSide(BoxBoundarySide),
	Label(&'a str),
}
impl TractionSelector<'_> {
	fn valid(self, space: &PhysicalSpace) -> bool {
		match self {
			Self::BoxSide(side) => {
				space.geometry_kind() == super::PhysicalGeometryKind::Box
					&& side.axis() < space.dimension
			}
			Self::Label(label) => !label.is_empty() && label.len() <= 64,
		}
	}
	fn matches(self, face: &super::Face) -> bool {
		face.right.is_none()
			&& match self {
				Self::BoxSide(side) => {
					face.normal[side.axis()] * if side.upper() { 1. } else { -1. } > 0.9
				}
				Self::Label(label) => face.label == label,
			}
	}
}

impl PhysicalSpace {
	/// Retained exact label for an exterior facet in the published facet layout.
	/// # Errors
	/// Rejects invalid indices and interior facets.
	pub fn boundary_label(&self, face: usize) -> Result<&str, CfdError> {
		self.faces
			.get(face)
			.filter(|f| f.right.is_none())
			.map(|f| f.label.as_str())
			.ok_or_else(invalid)
	}
	/// Mechanical force on all exterior facets bearing an exact source label.
	/// # Errors
	/// Rejects shape, label, resource limits and numerical overflow.
	pub fn boundary_force_on_label_with_limits(
		&self,
		state: &[f64],
		pressure: &[Vec<f64>],
		label: &str,
		limits: MechanicalTractionLimits,
	) -> Result<Point, CfdError> {
		let selector = TractionSelector::Label(label);
		self.admit_traction_selected(selector, limits, state.len(), 0, 0)?;
		self.validate_pressure(pressure)?;
		let u = self.coefficients(state)?;
		let extra = mul(u.capacity().checked_sub(u.len()).ok_or_else(invalid)?, 8)?;
		self.admit_traction_selected(selector, limits, state.len(), 0, extra)?;
		self.force_from_coefficients_selected(&u, pressure, selector)
	}
	fn pressure_bytes(&self) -> Result<usize, CfdError> {
		add(
			mul(self.cells.len(), size_of::<Vec<f64>>())?,
			mul(mul(self.cells.len(), self.pressure_modes_per_cell())?, 8)?,
		)
	}
	pub(super) fn validate_pressure(&self, pressure: &[Vec<f64>]) -> Result<(), CfdError> {
		if pressure.len() != self.cells.len()
			|| pressure.iter().any(|p| {
				p.len() != self.pressure_modes_per_cell() || p.iter().any(|v| !v.is_finite())
			}) {
			return Err(invalid());
		}
		Ok(())
	}
	/// P0 constants or P1 barycentric pressure, averaging all incident cell traces.
	/// This is a bounded physical probe; it does not alter the supplied pressure gauge.
	/// # Errors
	/// Rejects invalid pressure, nonfinite/outside points, budgets and allocation failure.
	pub fn sample_pressures(
		&self,
		pressure: &[Vec<f64>],
		points: &[Point],
	) -> Result<Vec<f64>, CfdError> {
		self.sample_pressures_with_limits(pressure, points, PressureProbeLimits::default())
	}
	/// Same trace convention as `sample_pressures`, with pre-scan work/storage admission.
	/// Caller buffer unused capacities and prepared source bytes remain caller-owned.
	/// # Errors
	/// Rejects limits, overflow, malformed/nonfinite data, allocation failure and outside points.
	pub fn sample_pressures_with_limits(
		&self,
		pressure: &[Vec<f64>],
		points: &[Point],
		limits: PressureProbeLimits,
	) -> Result<Vec<f64>, CfdError> {
		let work = add(
			mul(mul(points.len(), self.cells.len())?, 256)?,
			mul(self.cells.len(), self.pressure_modes_per_cell())?,
		)?;
		let bytes = add(add(self.pressure_bytes()?, mul(points.len(), 32)?)?, 256)?;
		if points.len() > limits.max_points || work > limits.max_work || bytes > limits.max_bytes {
			return Err(invalid());
		}
		self.validate_pressure(pressure)?;
		let mut result = reserved(points.len())?;
		if add(
			add(
				self.pressure_bytes()?,
				mul(points.len(), size_of::<Point>())?,
			)?,
			add(mul(result.capacity(), 8)?, 256)?,
		)? > limits.max_bytes
		{
			return Err(invalid());
		}
		for &point in points {
			if point.iter().any(|v| !v.is_finite())
				|| (self.dimension == 2 && point[2].abs() > 1e-10)
			{
				return Err(invalid());
			}
			let delta_point =
				|origin: Point| std::array::from_fn::<_, 3, _>(|i| point[i] - origin[i]);
			let mut sum = 0.;
			let mut count = 0_u32;
			for (cell, p) in self.cells.iter().zip(pressure) {
				let delta = delta_point(cell.vertices[0]);
				let bary: [f64; 4] = std::array::from_fn(|i| {
					if i <= self.dimension {
						dot(&cell.gradients[i], &delta) + f64::from(i == 0)
					} else {
						0.
					}
				});
				if bary[..=self.dimension]
					.iter()
					.all(|b| (-1e-10..=1. + 1e-10).contains(b))
				{
					let value = if self.order == 1 {
						p[0]
					} else {
						dot(p, &bary[..=self.dimension])
					};
					if !value.is_finite() {
						return Err(invalid());
					}
					sum += value;
					count = count.checked_add(1).ok_or_else(invalid)?;
				}
			}
			if count == 0 || !sum.is_finite() {
				return Err(invalid());
			}
			result.push(sum / f64::from(count));
		}
		Ok(result)
	}
	/// Mechanical fluid-on-boundary force integral `p n - nu grad(u) n`.
	/// Uses the interior physical trace. Excludes SIP penalties, convective flux,
	/// symmetric-gradient stress and drag/lift coefficient normalization.
	/// # Errors
	/// Rejects invalid state/pressure/side, budgets, absent exterior facets and overflow.
	pub fn boundary_force(
		&self,
		state: &[f64],
		pressure: &[Vec<f64>],
		side: BoxBoundarySide,
	) -> Result<Point, CfdError> {
		self.boundary_force_with_limits(state, pressure, side, MechanicalTractionLimits::default())
	}
	/// Mechanical traction with admission before coefficient reconstruction.
	/// # Errors
	/// Rejects unsupported sides, malformed inputs, work/storage limits or overflow.
	pub fn boundary_force_with_limits(
		&self,
		state: &[f64],
		pressure: &[Vec<f64>],
		side: BoxBoundarySide,
		limits: MechanicalTractionLimits,
	) -> Result<Point, CfdError> {
		self.admit_traction(side, limits, state.len(), 0, 0)?;
		self.validate_pressure(pressure)?;
		let coefficients = self.coefficients(state)?;
		let extra = mul(
			coefficients
				.capacity()
				.checked_sub(coefficients.len())
				.ok_or_else(invalid)?,
			8,
		)?;
		self.admit_traction(side, limits, state.len(), 0, extra)?;
		self.force_from_coefficients(&coefficients, pressure, side)
	}
	pub(super) fn admit_traction(
		&self,
		side: BoxBoundarySide,
		limits: MechanicalTractionLimits,
		state_len: usize,
		extra_work: usize,
		extra_bytes: usize,
	) -> Result<(), CfdError> {
		self.admit_traction_selected(
			TractionSelector::BoxSide(side),
			limits,
			state_len,
			extra_work,
			extra_bytes,
		)
	}
	pub(super) fn admit_traction_selected(
		&self,
		selector: TractionSelector<'_>,
		limits: MechanicalTractionLimits,
		state_len: usize,
		extra_work: usize,
		extra_bytes: usize,
	) -> Result<(), CfdError> {
		if !selector.valid(self)
			|| state_len != self.dimension()
			|| !self.faces.iter().any(|f| selector.matches(f))
		{
			return Err(invalid());
		}

		let n = self.diagnostics.local_velocity_dimension;
		let samples = self.facets.iter().map(Vec::len).try_fold(0usize, add)?;
		let work = add(
			add(
				mul(mul(n, self.dimension())?, 64)?,
				mul(mul(samples, self.local_velocity_per_cell())?, 256)?,
			)?,
			extra_work,
		)?;
		let work = if matches!(selector, TractionSelector::Label(_)) {
			add(
				work,
				self.faces
					.iter()
					.try_fold(0usize, |n, f| add(n, add(f.label.len(), 1)?))?,
			)?
		} else {
			work
		};
		let bytes = add(
			add(
				add(self.pressure_bytes()?, mul(add(state_len, n)?, 8)?)?,
				512,
			)?,
			extra_bytes,
		)?;
		if work > limits.max_work || bytes > limits.max_bytes {
			return Err(invalid());
		}
		Ok(())
	}
	pub(super) fn force_from_coefficients(
		&self,
		coefficients: &[f64],
		pressure: &[Vec<f64>],
		side: BoxBoundarySide,
	) -> Result<Point, CfdError> {
		self.force_from_coefficients_selected(
			coefficients,
			pressure,
			TractionSelector::BoxSide(side),
		)
	}
	pub(super) fn force_from_coefficients_selected(
		&self,
		coefficients: &[f64],
		pressure: &[Vec<f64>],
		selector: TractionSelector<'_>,
	) -> Result<Point, CfdError> {
		self.validate_pressure(pressure)?;
		if !selector.valid(self)
			|| coefficients.len() != self.diagnostics.local_velocity_dimension
			|| coefficients.iter().any(|v| !v.is_finite())
		{
			return Err(invalid());
		}

		let mut force = [0.; 3];
		for (face, samples) in self
			.faces
			.iter()
			.zip(&self.facets)
			.filter(|(f, _)| selector.matches(f))
		{
			for sample in samples {
				let p = if self.order == 1 {
					pressure[face.left][0]
				} else {
					dot(&pressure[face.left], &sample.left_bary)
				};
				let gradient = self.gradient(coefficients, face.left, &sample.left_gradients);
				for component in 0..self.dimension {
					force[component] += sample.weight
						* (p * face.normal[component]
							- self.viscosity * dot(&gradient[component], &face.normal));
				}
			}
		}
		if force.iter().any(|v| !v.is_finite()) {
			return Err(invalid());
		}
		Ok(force)
	}
}
