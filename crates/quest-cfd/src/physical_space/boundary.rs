//! Bounded complete polynomial-time lifting in original broken physical coordinates.
#![allow(
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	clippy::suboptimal_flops,
	reason = "Admission bounds all complete physical arrays and finite quadrature arithmetic"
)]
use super::{PhysicalSpace, Point, basis::facet_nodes, dot, mass_apply, reserved};
use crate::CfdError;

const fn invalid() -> CfdError {
	CfdError::InvalidInput("polynomial physical boundary shape/constraint/resource failure")
}
fn add(a: usize, b: usize) -> Result<usize, CfdError> {
	a.checked_add(b).ok_or_else(invalid)
}
fn mul(a: usize, b: usize) -> Result<usize, CfdError> {
	a.checked_mul(b).ok_or_else(invalid)
}
fn payload<T>(v: &Vec<T>) -> Result<usize, CfdError> {
	mul(v.capacity(), size_of::<T>())
}
fn matrix(v: &Vec<Vec<f64>>) -> Result<usize, CfdError> {
	v.iter().try_fold(payload(v)?, |n, r| add(n, payload(r)?))
}
fn finite(v: &[f64]) -> bool {
	v.iter().all(|x| x.is_finite())
}

/// Original exterior face index, outward normal and complete nodal trace ordering.
#[derive(Clone, Debug)]
pub struct BoundaryFacetLayout {
	pub face: usize,
	pub normal: Point,
	pub nodes: Vec<Point>,
}
/// One coefficient of t^k, with every broken velocity and full exterior trace mode.
///
/// Lifting/body force use cell, component, scalar-node order. Prescribed traces use
/// Dirichlet faces in `dirichlet_facets()` order, vertex nodes then edge midpoints.
/// Body force is physical acceleration, integrated against test functions with M.
#[derive(Clone, Debug)]
pub struct BoundaryTimeCoefficient {
	pub lifting: Vec<f64>,
	pub prescribed: Vec<Vec<Point>>,
	pub body_force: Vec<f64>,
}
impl BoundaryTimeCoefficient {
	/// Bounded reference helper; allocations are bounded by the admitted `PhysicalSpace`.
	/// # Errors
	/// Rejects failed allocation or invalid geometry.
	pub fn zero(space: &PhysicalSpace) -> Result<Self, CfdError> {
		let n = space.diagnostics.local_velocity_dimension;
		Ok(Self {
			lifting: vec![0.; n],
			body_force: vec![0.; n],
			prescribed: space
				.dirichlet_facets()?
				.iter()
				.map(|f| vec![[0.; 3]; f.nodes.len()])
				.collect(),
		})
	}
}
/// One time coefficient of prescribed `nu grad(u)n - p n` on every natural facet.
#[derive(Clone, Debug)]
pub struct NaturalTractionTimeCoefficient {
	pub values: Vec<Vec<Point>>,
}
impl NaturalTractionTimeCoefficient {
	/// Zero traction in the complete natural facet-node ordering.
	/// # Errors
	/// Rejects allocation or invalid geometry.
	pub fn zero(space: &PhysicalSpace) -> Result<Self, CfdError> {
		Ok(Self {
			values: space
				.natural_traction_facets()?
				.iter()
				.map(|f| vec![[0.; 3]; f.nodes.len()])
				.collect(),
		})
	}
}
/// Separate constructor, drift and pressure work ceilings; all are classical bounds.
#[derive(Clone, Copy, Debug)]
pub struct BoundaryLimits {
	pub max_time_degree: usize,
	pub max_bytes: usize,
	pub max_construction_work: usize,
	pub max_drift_work: usize,
	pub max_pressure_work: usize,
}
impl Default for BoundaryLimits {
	fn default() -> Self {
		Self {
			max_time_degree: 8,
			max_bytes: 256 * 1024 * 1024,
			max_construction_work: 1_000_000_000,
			max_drift_work: 1_000_000_000,
			max_pressure_work: usize::try_from(10_000_000_000_u64).unwrap_or(usize::MAX),
		}
	}
}
/// Managed source plus operation scratch; allocator/kernel metadata and RSS excluded.
#[derive(Clone, Copy, Debug, serde::Serialize)]
pub struct BoundaryResources {
	pub retained_bytes: usize,
	pub borrowed_space_bytes: usize,
	/// Mesh-declared other owners live during this source operation; absent when zero.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub external_retained_bytes: Option<usize>,
	pub peak_bytes: usize,
	pub construction_work: usize,
	pub drift_work: usize,
	pub pressure_work: usize,
}
/// Full joint extraction includes every direct physical residual query and exact
/// polynomial preparation. Output remains bounded numerical extraction evidence.
#[derive(Clone, Copy, Debug)]
pub struct BoundaryExtractionLimits {
	pub polynomial: mathcore::multivariate::PolynomialLimits,
	pub max_residual_evaluations: usize,
	pub max_work: usize,
	/// Includes borrowed physical source, joint snapshot and transformed output.
	pub max_bytes: usize,
}
impl Default for BoundaryExtractionLimits {
	fn default() -> Self {
		Self {
			polynomial: mathcore::multivariate::PolynomialLimits::default(),
			max_residual_evaluations: 10000,
			max_work: usize::try_from(10_000_000_000_u64).unwrap_or(usize::MAX),
			max_bytes: 512 * 1024 * 1024,
		}
	}
}
/// Conservative inviscid power and its independent central boundary expression.
#[derive(Clone, Copy, Debug)]
pub struct ConvectionPower {
	pub force_power: f64,
	pub boundary_power: f64,
}
/// Complete physical lifting u=Qa+ell(t), without projecting the supplied ell away.
///
/// Prescribed normal velocity is admitted only after coefficientwise full polynomial
/// continuity. Central boundary convection is polynomial and uses the full g; it
/// is neither an upwind inflow/outflow solver nor a claim of boundary stability.
/// Supports the original closed boxes and validated closed or mixed affine owners.
/// Mixed natural data are prescribed unsymmetrized viscous-pressure traction.
/// Distributed lifting uses a separate contract.
#[derive(Debug)]
pub struct PolynomialBoundary<'space> {
	space: &'space PhysicalSpace,
	coefficients: Vec<BoundaryTimeCoefficient>,
	traction: Vec<NaturalTractionTimeCoefficient>,
	trace_values: Vec<Vec<Vec<f64>>>,
	lifting_projection: Vec<Vec<f64>>,
	resources: BoundaryResources,
	limits: BoundaryLimits,
}
impl PhysicalSpace {
	/// Full scalar nodal positions in each cell, matching broken coefficient order.
	/// # Errors
	/// Rejects failed allocation or nonfinite geometry.
	pub fn velocity_nodes(&self) -> Result<Vec<Vec<Point>>, CfdError> {
		let mut cells = reserved(self.cells.len())?;
		for cell in &self.cells {
			let mut points = reserved(self.basis.nodes.len())?;
			for bary in &self.basis.nodes {
				let p = std::array::from_fn(|axis| {
					cell.vertices
						.iter()
						.zip(bary)
						.map(|(v, b)| v[axis] * b)
						.sum()
				});
				if !finite(&p) {
					return Err(invalid());
				}
				points.push(p);
			}
			cells.push(points);
		}
		Ok(cells)
	}
	/// Every exterior full P1/P2 trace node; periodic spaces have no exterior faces.
	/// # Errors
	/// Rejects failed allocation or nonfinite geometry.
	pub fn boundary_facets(&self) -> Result<Vec<BoundaryFacetLayout>, CfdError> {
		let mut out = reserved(self.faces.len())?;
		for (index, face) in self
			.faces
			.iter()
			.enumerate()
			.filter(|(_, f)| f.right.is_none())
		{
			let mut nodes = reserved(facet_nodes(self.dimension, self.order).len())?;
			for bary in facet_nodes(self.dimension, self.order) {
				let point = std::array::from_fn(|axis| {
					face.left_nodes
						.iter()
						.zip(&bary)
						.map(|(&i, b)| self.cells[face.left].vertices[i][axis] * b)
						.sum()
				});
				if !finite(&point) {
					return Err(invalid());
				}
				nodes.push(point);
			}
			out.push(BoundaryFacetLayout {
				face: index,
				normal: face.normal,
				nodes,
			});
		}
		Ok(out)
	}
	/// Complete prescribed velocity facet layouts in stable exterior order.
	/// # Errors
	/// Rejects allocation or invalid geometry.
	pub fn dirichlet_facets(&self) -> Result<Vec<BoundaryFacetLayout>, CfdError> {
		let mut f = self.boundary_facets()?;
		f.retain(|x| !self.faces[x.face].outflow);
		Ok(f)
	}
	/// Complete natural traction facet layouts in stable exterior order.
	/// # Errors
	/// Rejects allocation or invalid geometry.
	pub fn natural_traction_facets(&self) -> Result<Vec<BoundaryFacetLayout>, CfdError> {
		let mut f = self.boundary_facets()?;
		f.retain(|x| self.faces[x.face].outflow);
		Ok(f)
	}
	/// Conservative actual Vec/String capacities plus admitted `MathCore` payloads.
	/// # Errors
	/// Rejects accounting overflow. Allocator metadata is excluded.
	pub fn retained_bytes(&self) -> Result<usize, CfdError> {
		let mut bytes = add(
			add(size_of::<Self>(), self.basis.retained_bytes()?)?,
			payload(&self.face_layout)?,
		)?;
		for rows in [&self.constraints, &self.chart, &self.sip] {
			bytes = add(bytes, matrix(rows)?)?;
		}
		bytes = add(bytes, payload(&self.boundary_force)?)?;
		bytes = add(bytes, payload(&self.cells)?)?;
		for c in &self.cells {
			for n in [
				payload(&c.vertices)?,
				payload(&c.grid)?,
				payload(&c.gradients)?,
			] {
				bytes = add(bytes, n)?;
			}
		}
		bytes = add(bytes, payload(&self.faces)?)?;
		for f in &self.faces {
			for n in [
				payload(&f.left_nodes)?,
				payload(&f.right_nodes)?,
				f.label.capacity(),
			] {
				bytes = add(bytes, n)?;
			}
			if let Some(v) = &f.prescribed {
				bytes = add(bytes, payload(v)?)?;
			}
		}
		bytes = add(bytes, payload(&self.volume)?)?;
		for row in &self.volume {
			bytes = add(bytes, payload(row)?)?;
			for s in row {
				for n in [
					payload(&s.bary)?,
					payload(&s.values)?,
					payload(&s.gradients)?,
				] {
					bytes = add(bytes, n)?;
				}
			}
		}
		bytes = add(bytes, payload(&self.facets)?)?;
		for row in &self.facets {
			bytes = add(bytes, payload(row)?)?;
			for s in row {
				for n in [
					payload(&s.left_bary)?,
					payload(&s.left_values)?,
					payload(&s.right_values)?,
					payload(&s.left_gradients)?,
					payload(&s.right_gradients)?,
				] {
					bytes = add(bytes, n)?;
				}
			}
		}
		Ok(bytes)
	}
}
impl<'space> PolynomialBoundary<'space> {
	/// All coefficients are finite binary64 time powers, retained as provided.
	/// # Errors
	/// Rejects incompatible continuity, normal traces, shape and resource ceilings.
	pub fn new(
		space: &'space PhysicalSpace,
		coefficients: Vec<BoundaryTimeCoefficient>,
		limits: BoundaryLimits,
	) -> Result<Self, CfdError> {
		Self::with_natural_traction(space, coefficients, Vec::new(), limits)
	}
	/// Complete time data; an empty traction owner means canonical zero natural load.
	/// # Errors
	/// Rejects unsupported source, trace/mode shape, nonfinite data or resource ceilings.
	#[allow(
		clippy::too_many_lines,
		reason = "Keep full time-data admission, original constraints and natural load ownership in one constructor"
	)]
	pub fn with_natural_traction(
		space: &'space PhysicalSpace,
		coefficients: Vec<BoundaryTimeCoefficient>,
		traction: Vec<NaturalTractionTimeCoefficient>,
		limits: BoundaryLimits,
	) -> Result<Self, CfdError> {
		let n = space.diagnostics.local_velocity_dimension;
		let m = space.dimension();
		let k = coefficients.len();
		if k == 0
			|| k - 1 > limits.max_time_degree
			|| k > 65
			|| !space.has_supported_boundary()
			|| (!traction.is_empty() && traction.len() != k)
		{
			return Err(invalid());
		}
		let samples = add(
			space.volume.iter().map(Vec::len).sum(),
			space.facets.iter().map(Vec::len).sum(),
		)?;
		let construction_work = add(
			100_000_000,
			mul(
				k,
				add(
					mul(mul(n, space.constraint_count())?, 64)?,
					mul(mul(samples, space.local_velocity_per_cell())?, 512)?,
				)?,
			)?,
		)?;
		let drift_work = add(
			mul(mul(n, n)?, 64)?,
			mul(mul(samples, space.local_velocity_per_cell())?, 512)?,
		)?;
		let drift_work = add(drift_work, mul(mul(k, n)?, 128)?)?;
		let natural_count = space.faces.iter().filter(|f| f.outflow).count();
		let natural_work = add(
			mul(
				mul(mul(k, natural_count)?, space.local_velocity_per_cell())?,
				128,
			)?,
			mul(mul(samples, space.local_velocity_per_cell())?, 512)?,
		)?;
		let drift_work = if natural_count == 0 {
			drift_work
		} else {
			add(drift_work, natural_work)?
		};
		let construction_work = if natural_count == 0 {
			construction_work
		} else {
			add(construction_work, natural_work)?
		};
		let pressure_work = add(
			mul(
				mul(mul(space.constraint_count(), space.constraint_count())?, n)?,
				128,
			)?,
			mul(drift_work, 2)?,
		)?;
		if construction_work > limits.max_construction_work
			|| drift_work > limits.max_drift_work
			|| pressure_work > limits.max_pressure_work
		{
			return Err(invalid());
		}
		let facets = space
			.faces
			.iter()
			.filter(|f| f.right.is_none() && !f.outflow)
			.count();
		let nodes = facet_nodes(space.dimension, space.order);
		let width = nodes.len();
		let mut retained = add(size_of::<Self>(), payload(&coefficients)?)?;
		for c in &coefficients {
			if c.lifting.len() != n || c.body_force.len() != n || c.prescribed.len() != facets {
				return Err(invalid());
			}
			retained = add(
				retained,
				add(
					add(payload(&c.lifting)?, payload(&c.body_force)?)?,
					payload(&c.prescribed)?,
				)?,
			)?;
			for trace in &c.prescribed {
				if trace.len() != width {
					return Err(invalid());
				}
				retained = add(retained, payload(trace)?)?;
			}
		}
		retained = add(retained, payload(&traction)?)?;
		for mode in &traction {
			if mode.values.len() != natural_count {
				return Err(invalid());
			}
			retained = add(retained, payload(&mode.values)?)?;
			for row in &mode.values {
				if row.len() != width {
					return Err(invalid());
				}
				retained = add(retained, payload(row)?)?;
			}
		}
		let source = space.retained_bytes()?;
		let external = space
			.mesh_metadata
			.as_ref()
			.map_or(0, |m| m.limits.external_retained_bytes);
		let source_live = add(source, external)?;
		// Includes all planned basis sample words, projections, pressure row QR,
		// operation buffers and constructor geometry/basis scratch before allocation.
		let scratch = add(mul(mul(space.constraint_count(), n)?, 32)?, mul(n, 512)?)?;
		let tables = add(
			mul(mul(space.facets.iter().map(Vec::len).sum(), width)?, 32)?,
			mul(mul(k, m)?, 32)?,
		)?;
		let planned = add(
			add(add(source_live, retained)?, scratch)?,
			add(tables, 4 * 1024 * 1024)?,
		)?;
		if planned > limits.max_bytes {
			return Err(invalid());
		}
		if traction
			.iter()
			.flat_map(|c| &c.values)
			.flatten()
			.any(|p| !finite(p) || (space.dimension == 2 && p[2] != 0.))
		{
			return Err(invalid());
		}
		let mut trace_values = reserved(space.faces.len())?;
		let trace_basis = super::Basis::new(space.dimension - 1, space.order)?;
		if trace_basis.retained_bytes()? > 4 * 1024 * 1024 {
			return Err(invalid());
		}
		for (face, samples) in space.faces.iter().zip(&space.facets) {
			let mut row = reserved(samples.len())?;
			if face.right.is_none() {
				// Recover facet barycentrics from the fixed nodal physical basis:
				// volume vertex coordinates are the first d+1 basis nodes.
				for (bary, _) in super::basis::quadrature(space.dimension - 1) {
					row.push(trace_basis.values(&bary)?);
				}
			}
			trace_values.push(row);
		}
		let mut lifting_projection = reserved(k)?;
		for c in &coefficients {
			if !finite(&c.lifting)
				|| !finite(&c.body_force)
				|| c.prescribed
					.iter()
					.flatten()
					.any(|p| !finite(p) || (space.dimension == 2 && p[2] != 0.))
			{
				return Err(invalid());
			}
			let scale = c
				.lifting
				.iter()
				.map(|v| v.abs())
				.chain(c.prescribed.iter().flatten().flatten().map(|v| v.abs()))
				.fold(1., f64::max);
			for (fi, face) in space.faces.iter().enumerate() {
				let Some(start) = space.face_layout[fi].normal_start else {
					continue;
				};
				for node in 0..width {
					let expected = space.face_layout[fi]
						.dirichlet
						.map_or(0., |index| dot(&face.normal, &c.prescribed[index][node]));
					let actual = dot(&space.constraints[start + node], &c.lifting);
					if !actual.is_finite()
						|| !expected.is_finite()
						|| (actual - expected).abs() > 1e-9 * scale
					{
						return Err(invalid());
					}
				}
			}

			if space.constraints[space.normal_count..].iter().any(|r| {
				let value = dot(r, &c.lifting);
				!value.is_finite() || value.abs() > 1e-9 * scale
			}) {
				return Err(invalid());
			}
			let mass = mass_apply(&space.cells, space.dimension, &space.basis, &c.lifting);
			let projected = space
				.chart
				.iter()
				.map(|q| dot(q, &mass))
				.collect::<Vec<_>>();
			if !finite(&mass) || !finite(&projected) {
				return Err(invalid());
			}
			lifting_projection.push(projected);
		}
		retained = add(retained, payload(&trace_values)?)?;
		for row in &trace_values {
			retained = add(retained, payload(row)?)?;
			for v in row {
				retained = add(retained, payload(v)?)?;
			}
		}
		retained = add(retained, matrix(&lifting_projection)?)?;
		let peak = add(add(source_live, retained)?, add(scratch, 4 * 1024 * 1024)?)?;
		if peak > limits.max_bytes {
			return Err(invalid());
		}
		Ok(Self {
			space,
			coefficients,
			traction,
			trace_values,
			lifting_projection,
			resources: BoundaryResources {
				retained_bytes: retained,
				borrowed_space_bytes: source,
				external_retained_bytes: (external != 0).then_some(external),
				peak_bytes: planned.max(peak),
				construction_work,
				drift_work,
				pressure_work,
			},
			limits,
		})
	}
	#[must_use]
	pub const fn dimension(&self) -> usize {
		self.space.dimension()
	}
	#[must_use]
	pub const fn resources(&self) -> BoundaryResources {
		self.resources
	}
	/// Borrowed complete physical chart and geometry used by this boundary owner.
	#[must_use]
	pub const fn physical_space(&self) -> &PhysicalSpace {
		self.space
	}
	/// Mechanical fluid-on-boundary traction of the full lifted velocity at this time.
	/// The supplied pressure retains its gauge; SIP and convective flux are excluded.
	/// # Errors
	/// Rejects invalid time/state/pressure/side, resource limits and arithmetic overflow.
	pub fn boundary_force(
		&self,
		time: f64,
		state: &[f64],
		pressure: &[Vec<f64>],
		side: super::BoxBoundarySide,
	) -> Result<Point, CfdError> {
		self.boundary_force_with_limits(
			time,
			state,
			pressure,
			side,
			super::MechanicalTractionLimits::default(),
		)
	}
	/// Bounded traction with admission before lifting reconstruction.
	/// # Errors
	/// Rejects malformed inputs, exceeded budgets, absent exterior sides or overflow.
	pub fn boundary_force_with_limits(
		&self,
		time: f64,
		state: &[f64],
		pressure: &[Vec<f64>],
		side: super::BoxBoundarySide,
		limits: super::MechanicalTractionLimits,
	) -> Result<Point, CfdError> {
		self.admit(time, state)?;
		self.space.admit_traction(
			side,
			limits,
			state.len(),
			self.resources.drift_work,
			self.resources.peak_bytes,
		)?;
		self.space.validate_pressure(pressure)?;
		let u = self.coefficients_at(time, state)?;
		let extra = mul(u.capacity().checked_sub(u.len()).ok_or_else(invalid)?, 8)?;
		self.space.admit_traction(
			side,
			limits,
			state.len(),
			self.resources.drift_work,
			add(self.resources.peak_bytes, extra)?,
		)?;
		self.space.force_from_coefficients(&u, pressure, side)
	}

	/// Integrated kinetic energy of the full supplied lifting plus every chart coordinate.
	/// # Errors
	/// Rejects invalid query data or overflow under this owner's admitted query envelope.
	pub fn energy(&self, time: f64, state: &[f64]) -> Result<f64, CfdError> {
		self.space
			.energy_from_coefficients(&self.coefficients_at(time, state)?)
	}
	/// Integrated half squared vorticity of the full lifted field.
	/// # Errors
	/// Rejects invalid query data or overflow under this owner's admitted query envelope.
	pub fn enstrophy(&self, time: f64, state: &[f64]) -> Result<f64, CfdError> {
		self.space
			.enstrophy_from_coefficients(&self.coefficients_at(time, state)?)
	}
	pub(crate) fn admit_label_force(
		&self,
		state_len: usize,
		label: &str,
		limits: super::MechanicalTractionLimits,
	) -> Result<(), CfdError> {
		self.space.admit_traction_selected(
			super::pressure_observables::TractionSelector::Label(label),
			limits,
			state_len,
			self.resources.drift_work,
			self.resources.peak_bytes,
		)
	}
	/// Mechanical force on all exterior facets with an exact retained label.
	/// # Errors
	/// Rejects missing/invalid labels, shape, budgets, or nonfinite physical data.
	pub fn boundary_force_on_label_with_limits(
		&self,
		time: f64,
		state: &[f64],
		pressure: &[Vec<f64>],
		label: &str,
		limits: super::MechanicalTractionLimits,
	) -> Result<Point, CfdError> {
		self.admit(time, state)?;
		let selector = super::pressure_observables::TractionSelector::Label(label);
		self.space.admit_traction_selected(
			selector,
			limits,
			state.len(),
			self.resources.drift_work,
			self.resources.peak_bytes,
		)?;
		self.space.validate_pressure(pressure)?;
		let u = self.coefficients_at(time, state)?;
		let extra = mul(u.capacity().checked_sub(u.len()).ok_or_else(invalid)?, 8)?;
		self.space.admit_traction_selected(
			selector,
			limits,
			state.len(),
			self.resources.drift_work,
			add(self.resources.peak_bytes, extra)?,
		)?;
		self.space
			.force_from_coefficients_selected(&u, pressure, selector)
	}
	fn admit(&self, time: f64, state: &[f64]) -> Result<(), CfdError> {
		if !time.is_finite()
			|| state.len() != self.dimension()
			|| !finite(state)
			|| self.resources.peak_bytes > self.limits.max_bytes
		{
			return Err(invalid());
		}
		Ok(())
	}
	fn powers(&self, time: f64) -> Result<Vec<f64>, CfdError> {
		let mut p = reserved(self.coefficients.len())?;
		let mut word = 1_f64;
		for _ in &self.coefficients {
			if !word.is_finite() {
				return Err(invalid());
			}
			p.push(word);
			word *= time;
		}
		Ok(p)
	}
	/// Original full coefficients, including every component of the supplied lifting.
	/// # Errors
	/// Rejects invalid query or overflow.
	pub fn coefficients_at(&self, time: f64, state: &[f64]) -> Result<Vec<f64>, CfdError> {
		self.admit(time, state)?;
		self.coefficients_with_modes(state, &self.powers(time)?)
	}
	fn coefficients_with_modes(&self, state: &[f64], modes: &[f64]) -> Result<Vec<f64>, CfdError> {
		let mut u = self.space.coefficients(state)?;
		for (c, b) in self.coefficients.iter().zip(modes) {
			for (v, ell) in u.iter_mut().zip(&c.lifting) {
				*v += b * ell;
			}
		}
		if !finite(&u) {
			return Err(invalid());
		}
		Ok(u)
	}
	fn derivative(&self, time: f64) -> Result<Vec<f64>, CfdError> {
		let mut out = vec![0.; self.space.diagnostics.local_velocity_dimension];
		let mut power = 1.;
		for (k, c) in self.coefficients.iter().enumerate().skip(1) {
			let factor = f64::from(u32::try_from(k).map_err(|_| invalid())?) * power;
			for (x, v) in out.iter_mut().zip(&c.lifting) {
				*x += factor * v;
			}
			power *= time;
		}
		if !finite(&out) {
			return Err(invalid());
		}
		Ok(out)
	}
	fn trace(&self, exterior: usize, values: &[f64], modes: &[f64]) -> Result<Point, CfdError> {
		let mut g = [0.; 3];
		for (c, b) in self.coefficients.iter().zip(modes) {
			for (node, value) in c.prescribed[exterior].iter().zip(values) {
				for axis in 0..self.space.dimension {
					g[axis] += b * value * node[axis];
				}
			}
		}
		if !finite(&g) {
			return Err(invalid());
		}
		Ok(g)
	}
	fn traction_value(
		&self,
		index: usize,
		basis: &[f64],
		modes: &[f64],
	) -> Result<Point, CfdError> {
		let mut value = [0.; 3];
		for (mode, power) in self.traction.iter().zip(modes) {
			for (node, b) in mode.values[index].iter().zip(basis) {
				for axis in 0..self.space.dimension {
					value[axis] += power * b * node[axis];
				}
			}
		}
		if !finite(&value) {
			return Err(invalid());
		}
		Ok(value)
	}
	#[allow(
		clippy::too_many_lines,
		reason = "One common conservative/advective force preserves explicit interior, Dirichlet and natural boundary signs"
	)]
	fn force(
		&self,
		u: &[f64],
		modes: &[f64],
		advective: bool,
		only_convection: bool,
	) -> Result<Vec<f64>, CfdError> {
		let space = self.space;
		let scalar = space.basis.nodes.len();
		let local = space.local_velocity_per_cell();
		let mut out = if only_convection {
			vec![0.; u.len()]
		} else {
			space
				.sip
				.iter()
				.map(|r| -space.viscosity * dot(r, u))
				.collect()
		};
		for (cell, samples) in space.volume.iter().enumerate() {
			for s in samples {
				let velocity = space.value(u, cell, &s.values);
				let gradient = space.gradient(u, cell, &s.gradients);
				for component in 0..space.dimension {
					for node in 0..scalar {
						out[cell * local + component * scalar + node] += s.weight
							* if advective {
								-s.values[node] * dot(&velocity, &gradient[component])
							} else {
								velocity[component] * dot(&velocity, &s.gradients[node])
							};
					}
				}
			}
		}
		for (fi, (face, samples)) in space.faces.iter().zip(&space.facets).enumerate() {
			let layout = &space.face_layout[fi];
			let minvolume = face.right.map_or(space.cells[face.left].volume, |r| {
				space.cells[r].volume.min(space.cells[face.left].volume)
			});
			let penalty = if space.order == 1 { 40. } else { 90. } * face.measure
				/ (if space.dimension == 2 { 2. } else { 3. } * minvolume);
			for (si, s) in samples.iter().enumerate() {
				let left = space.value(u, face.left, &s.left_values);
				let right = if let Some(r) = face.right {
					space.value(u, r, &s.right_values)
				} else if let Some(index) = layout.dirichlet {
					self.trace(index, &self.trace_values[fi][si], modes)?
				} else {
					left
				};
				let tau = if only_convection {
					[0.; 3]
				} else if let Some(index) = layout.natural {
					self.traction_value(index, &self.trace_values[fi][si], modes)?
				} else {
					[0.; 3]
				};
				let un = dot(&left, &face.normal);
				for component in 0..space.dimension {
					let avg = f64::midpoint(left[component], right[component]);
					for node in 0..scalar {
						out[face.left * local + component * scalar + node] -= s.weight
							* un
							* (if advective {
								avg - left[component]
							} else {
								avg
							})
							* s.left_values[node];
						if let Some(r) = face.right {
							out[r * local + component * scalar + node] += s.weight
								* un
								* (if advective {
									avg - right[component]
								} else {
									avg
								})
								* s.right_values[node];
						} else if !only_convection && !face.outflow {
							out[face.left * local + component * scalar + node] += space.viscosity
								* s.weight
								* right[component]
								* (-dot(&s.left_gradients[node], &face.normal)
									+ penalty * s.left_values[node]);
						}
						if !only_convection && face.outflow {
							out[face.left * local + component * scalar + node] +=
								s.weight * tau[component] * s.left_values[node];
						}
					}
				}
			}
		}
		if !only_convection {
			let mut body = vec![0.; u.len()];
			for (c, b) in self.coefficients.iter().zip(modes) {
				for (v, f) in body.iter_mut().zip(&c.body_force) {
					*v += b * f;
				}
			}
			let load = mass_apply(&space.cells, space.dimension, &space.basis, &body);
			for (v, f) in out.iter_mut().zip(load) {
				*v += f;
			}
		}
		if !finite(&out) {
			return Err(invalid());
		}
		Ok(out)
	}
	/// Complete conservative DG/SIP drift, including full forcing and `-Q^T M ell_dot`.
	/// # Errors
	/// Rejects invalid state/time or overflow.
	pub fn drift(&self, time: f64, state: &[f64]) -> Result<Vec<f64>, CfdError> {
		self.admit(time, state)?;
		let modes = self.powers(time)?;
		let u = self.coefficients_with_modes(state, &modes)?;
		let mut f = self.force(&u, &modes, false, false)?;
		let md = mass_apply(
			&self.space.cells,
			self.space.dimension,
			&self.space.basis,
			&self.derivative(time)?,
		);
		for (v, d) in f.iter_mut().zip(md) {
			*v -= d;
		}
		let out = self
			.space
			.chart
			.iter()
			.map(|q| dot(q, &f))
			.collect::<Vec<_>>();
		if !finite(&out) {
			return Err(invalid());
		}
		Ok(out)
	}
	/// Maximum original divergence, interior normal and prescribed exterior normal defect.
	/// # Errors
	/// Rejects malformed query or overflow.
	pub fn continuity_residual(&self, time: f64, state: &[f64]) -> Result<f64, CfdError> {
		let u = self.coefficients_at(time, state)?;
		let modes = self.powers(time)?;
		let mut defect = 0_f64;
		for (fi, face) in self.space.faces.iter().enumerate() {
			let Some(start) = self.space.face_layout[fi].normal_start else {
				continue;
			};
			for node in 0..facet_nodes(self.space.dimension, self.space.order).len() {
				let mut g = [0.; 3];
				if let Some(index) = self.space.face_layout[fi].dirichlet {
					for (c, b) in self.coefficients.iter().zip(&modes) {
						for axis in 0..self.space.dimension {
							g[axis] += b * c.prescribed[index][node][axis];
						}
					}
				}
				let expected = dot(&face.normal, &g);
				let actual = dot(&self.space.constraints[start + node], &u);
				if !finite(&g) || !actual.is_finite() || !expected.is_finite() {
					return Err(invalid());
				}
				defect = defect.max((actual - expected).abs());
			}
		}

		for r in &self.space.constraints[self.space.normal_count..] {
			let actual = dot(r, &u);
			if !actual.is_finite() {
				return Err(invalid());
			}
			defect = defect.max(actual.abs());
		}
		if !defect.is_finite() {
			return Err(invalid());
		}
		Ok(defect)
	}
	/// Full broken momentum force for the physical boundary problem.
	/// # Errors
	/// Rejects malformed query or overflow.
	pub fn momentum_force(&self, time: f64, state: &[f64]) -> Result<Vec<f64>, CfdError> {
		let u = self.coefficients_at(time, state)?;
		self.force(&u, &self.powers(time)?, false, false)
	}
	/// Independent advective-volume identity and actual central boundary energy flux.
	/// # Errors
	/// Rejects malformed queries or overflow; no boundary-stability claim is implied.
	pub fn convection_power(&self, time: f64, state: &[f64]) -> Result<ConvectionPower, CfdError> {
		let u = self.coefficients_at(time, state)?;
		let modes = self.powers(time)?;
		let f = self.force(&u, &modes, false, true)?;
		let other = self.force(&u, &modes, true, true)?;
		if f.iter()
			.zip(&other)
			.any(|(a, b)| (a - b).abs() > 1e-8 * a.abs().max(1.))
		{
			return Err(CfdError::Assembly(
				"original conservative/advective boundary identity failed",
			));
		}
		let mut boundary_power = 0.;
		for (fi, (face, samples)) in self.space.faces.iter().zip(&self.space.facets).enumerate() {
			if face.right.is_none() {
				for (si, s) in samples.iter().enumerate() {
					let v = self.space.value(&u, face.left, &s.left_values);
					let g = if let Some(index) = self.space.face_layout[fi].dirichlet {
						self.trace(index, &self.trace_values[fi][si], &modes)?
					} else {
						v
					};
					boundary_power -= 0.5 * s.weight * dot(&v, &face.normal) * dot(&v, &g);
				}
			}
		}
		let force_power = dot(&u, &f);
		if !force_power.is_finite() || !boundary_power.is_finite() {
			return Err(invalid());
		}
		Ok(ConvectionPower {
			force_power,
			boundary_power,
		})
	}
	/// Original full momentum recovery, including physical lifting acceleration.
	/// # Errors
	/// Rejects invalid query, mixed-rank/gauge ambiguity and failed original momentum.
	pub fn reconstruct_pressure(
		&self,
		time: f64,
		state: &[f64],
	) -> Result<super::PhysicalPressureRecovery, CfdError> {
		if self.space.faces.iter().any(|f| f.outflow) {
			return Err(CfdError::InvalidInput(
				"natural traction pressure requires the general pressure report",
			));
		}
		self.reconstruct_pressure_general(time, state)?
			.into_closed()
	}

	/// Whole-live mesh/time-owner pressure envelope, available before numerical recovery.
	/// # Errors
	/// Rejects the same shape-independent resource ceilings as pressure recovery.
	pub fn pressure_query_resources(
		&self,
	) -> Result<Option<super::PhysicalMeshQueryResources>, CfdError> {
		self.space.admit_mesh_pressure_extra(
			self.resources.retained_bytes,
			mul(self.space.diagnostics.local_velocity_dimension, 256)?,
			mul(self.resources.drift_work, 4)?,
		)
	}
	/// Original pressure with explicit closed-gauge or prescribed-traction level.
	/// # Errors
	/// Rejects query limits, incompatible rank or failed original momentum.
	pub fn reconstruct_pressure_general(
		&self,
		time: f64,
		state: &[f64],
	) -> Result<super::GeneralPressureRecovery, CfdError> {
		let mesh_resources = self.pressure_query_resources()?;
		let u = self.coefficients_at(time, state)?;
		let f = self.momentum_force(time, state)?;
		let mut acceleration = self.space.coefficients(&self.drift(time, state)?)?;
		for (a, d) in acceleration.iter_mut().zip(self.derivative(time)?) {
			*a += d;
		}
		let mut recovery = self.space.recover_pressure_general(
			&u,
			&acceleration,
			&f,
			self.continuity_residual(time, state)?,
		)?;
		recovery.mesh_resources = mesh_resources;
		Ok(recovery)
	}
}

impl crate::polynomial::PolynomialOde {
	/// Extract complete known polynomial-time BDM1/P0 or BDM2/P1 dynamics.
	///
	/// Joint quadratic polarization uses all coordinates plus every supplied time
	/// power as independent data modes. `MathCore` coalesces their exact recorded
	/// dyadic coefficients after replacing mode k by t^k and differentiating ell.
	/// Time is external; no physical coordinate is removed. This is a numerical
	/// snapshot of the finite assembly, not an exact roundoff certificate.
	/// # Errors
	/// Rejects joint dimension above64, aggregate query/byte/work limits, polynomial
	/// limits or failed independent physical-time comparisons.
	#[allow(
		clippy::too_many_lines,
		reason = "Joint extraction admission, exact time substitution and numerical evidence form one reviewable operation"
	)]
	pub fn from_polynomial_boundary(
		problem: &PolynomialBoundary<'_>,
		limits: BoundaryExtractionLimits,
	) -> Result<crate::polynomial::PolynomialSnapshot, CfdError> {
		use mathcore::{
			RBig,
			arithmetic::ExactConstant,
			exact::{Owner, Symbol},
			multivariate::{SparsePolynomial, rational_constant},
		};
		let n = problem.dimension();
		let modes = problem.coefficients.len();
		let joint = add(n, modes)?;
		if n == 0
			|| joint > 64
			|| joint > limits.polynomial.max_variables
			|| add(n, 1)? > limits.polynomial.max_variables
			|| mul(modes - 1, 2)?.checked_add(2).is_none_or(|d| {
				u32::try_from(d).map_or(true, |degree| degree > limits.polynomial.max_degree)
			}) {
			return Err(invalid());
		}
		let queries = add(add(mul(2, mul(joint, joint)?)?, 8)?, 21)?;
		let per_row = add(add(1, joint)?, mul(joint, add(joint, 1)?)? / 2)?;
		let terms = mul(joint, per_row)?;
		let work = add(
			mul(queries, problem.resources.drift_work)?,
			mul(mul(terms, add(joint, 8)?)?, 4096)?,
		)?;
		let bytes = add(
			problem.resources.peak_bytes,
			mul(limits.polynomial.max_bytes, 4)?,
		)?;
		if queries > limits.max_residual_evaluations
			|| work > limits.max_work
			|| bytes > limits.max_bytes
			|| terms > limits.polynomial.max_terms
		{
			return Err(invalid());
		}
		let mut snapshot = crate::polynomial::snapshot_quadratic(
			joint,
			|point| {
				let state = &point[..n];
				let modes = &point[n..];
				let u = problem.coefficients_with_modes(state, modes)?;
				let f = problem.force(&u, modes, false, false)?;
				let mut out = problem
					.space
					.chart
					.iter()
					.map(|q| dot(q, &f))
					.collect::<Vec<_>>();
				out.resize(joint, 0.);
				Ok(out)
			},
			limits.polynomial,
		)?;
		let symbols = (0..=n)
			.map(|i| {
				Ok(Symbol::new(
					Owner::new(0x4346_4442_4354_494d),
					u64::try_from(i).map_err(|_| invalid())?,
				))
			})
			.collect::<Result<Vec<_>, CfdError>>()?;
		let mut equations = reserved(n)?;
		for (row, p) in snapshot.dynamics.components().iter().take(n).enumerate() {
			let mut output = reserved(add(p.terms().count(), modes)?)?;
			for (powers, coefficient) in p.terms() {
				let mut exponents = powers[..n].to_vec();
				let mut time = 0u32;
				for (k, &exponent) in powers[n..].iter().enumerate() {
					time = time
						.checked_add(
							exponent
								.checked_mul(u32::try_from(k).map_err(|_| invalid())?)
								.ok_or_else(invalid)?,
						)
						.ok_or_else(invalid)?;
				}
				exponents.push(time);
				output.push((exponents, coefficient.clone()));
			}
			for (k, projection) in problem.lifting_projection.iter().enumerate().skip(1) {
				let mut powers = vec![0; n + 1];
				powers[n] = u32::try_from(k - 1).map_err(|_| invalid())?;
				let coefficient = -rational_constant(
					&ExactConstant::Binary64(projection[row]),
					limits.polynomial,
				)? * RBig::from(u32::try_from(k).map_err(|_| invalid())?);
				output.push((powers, coefficient));
			}
			equations.push(SparsePolynomial::from_terms(
				symbols.clone(),
				output,
				limits.polynomial,
			)?);
		}
		snapshot.dynamics = Self::from_polynomials(equations, n, limits.polynomial)?;
		// Independent off-node physical times and complete nonzero states test the
		// transformed residual rather than merely reproducing extraction samples.
		for probe in 1..=7u32 {
			let time = f64::from(probe) * 0.23 - 0.7;
			let state = (0..n)
				.map(|i| {
					Ok((f64::from(u32::try_from(i + 1).map_err(|_| invalid())?)
						* f64::from(probe)
						* 0.173)
						.sin()
						* 0.03)
				})
				.collect::<Result<Vec<_>, CfdError>>()?;
			let direct = problem.drift(time, &state)?;
			let reconstructed = snapshot.dynamics.drift(time, &state)?;
			for (a, b) in direct.iter().zip(&reconstructed) {
				let error = (a - b).abs();
				snapshot.evidence.independent_probe_max_error =
					snapshot.evidence.independent_probe_max_error.max(error);
				snapshot.evidence.independent_probe_scaled_error = snapshot
					.evidence
					.independent_probe_scaled_error
					.max(error / a.abs().max(1.));
			}
			snapshot.evidence.residual_evaluations =
				add(snapshot.evidence.residual_evaluations, 1)?;
		}
		if snapshot.evidence.independent_probe_scaled_error > 2e-9 {
			return Err(CfdError::Assembly(
				"complete polynomial-time boundary snapshot failed independent probes",
			));
		}
		snapshot.evidence.physical_dimension = n;
		snapshot.evidence.status="numerical joint quadratic snapshot of complete BDM1/P0 or BDM2/P1 with general admitted polynomial-time full lifting, prescribed trace, body force and lifting derivative; external time is not a physical coordinate; exact recorded dyadics and probe evidence do not certify original assembly roundoff".into();
		Ok(snapshot)
	}
}
