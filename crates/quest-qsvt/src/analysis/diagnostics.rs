use super::{Error, Result};
use crate::{Complex64, NumericalPolicy, Route, ValidatedTransform, matrix};
use faer::{
	Mat, MatRef, Par,
	dyn_stack::{MemBuffer, MemStack},
};
use std::ops::{Add, Div, Mul, Sub};
/// Singular-polynomial reference semantics, including the even right nullspace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReferenceParity {
	Odd,
	Even,
}
/// Owned finite reference values; their construction is numerical, never a theorem premise.
pub struct Reference {
	matrix: Mat<Complex64>,
}
impl Reference {
	/// # Errors
	/// Rejects empty/nonfinite data and allocation budgets.
	pub fn dense(matrix: MatRef<'_, Complex64>, policy: NumericalPolicy) -> Result<Self> {
		dimensions(matrix)?;
		Ok(Self {
			matrix: matrix::snapshot(matrix, policy)?,
		})
	}
	/// Build U p(Sigma) V† for odd parity or V p(Sigma) V† for even parity.
	/// The even result includes p(0) on the complete right nullspace. Parity is
	/// supplied explicitly; this routine does not certify a callback polynomial.
	/// # Errors
	/// Rejects invalid shapes/values, nonzero p(0) for odd parity, SVD failure or budgets.
	pub fn svd_polynomial(
		source: MatRef<'_, Complex64>,
		parity: ReferenceParity,
		polynomial: impl Fn(f64) -> Complex64,
		policy: NumericalPolicy,
	) -> Result<Self> {
		dimensions(source)?;
		let zero = finite(polynomial(0.0))?;
		if parity == ReferenceParity::Odd && zero != Complex64::new(0.0, 0.0) {
			return Err(Error::Input("odd reference requires p(0)=0"));
		}
		let decomposition = Svd::new(source, true, policy)?;
		let left = decomposition
			.left
			.as_ref()
			.ok_or(Error::Input("missing left singular vectors"))?;
		let right = decomposition
			.right
			.as_ref()
			.ok_or(Error::Input("missing right singular vectors"))?;
		let size = source.nrows().min(source.ncols());
		let output_rows = if parity == ReferenceParity::Odd {
			source.nrows()
		} else {
			source.ncols()
		};
		let mut weights = matrix::allocate(source.ncols(), 1, policy, |_, _| zero)?;
		for i in 0..size {
			weights[(i, 0)] = finite(polynomial(decomposition.values[(i, 0)].re))?;
		}
		let output = matrix::allocate(output_rows, source.ncols(), policy, |row, col| {
			let count = if parity == ReferenceParity::Odd {
				size
			} else {
				source.ncols()
			};
			(0..count).fold(Complex64::new(0.0, 0.0), |sum, i| {
				let first = if parity == ReferenceParity::Odd {
					left[(row, i)]
				} else {
					right[(row, i)]
				};
				sum.add(first.mul(weights[(i, 0)]).mul(right[(col, i)].conj()))
			})
		})?;
		dimensions(output.as_ref())?;
		Ok(Self { matrix: output })
	}
	#[must_use]
	pub fn matrix(&self) -> MatRef<'_, Complex64> {
		self.matrix.as_ref()
	}
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticMethod {
	DenseSpectralNorm,
	SeededPowerIteration,
}
/// Reproducible observed full-phase discrepancy, never an analytical upper bound.
#[derive(Debug, Clone, Copy)]
pub struct EmpiricalEvidence {
	observed: f64,
	method: DiagnosticMethod,
	seed: Option<u64>,
	probes: usize,
	iterations: usize,
	route: Route,
}
impl EmpiricalEvidence {
	#[must_use]
	pub const fn observed_error(self) -> f64 {
		self.observed
	}
	#[must_use]
	pub const fn method(self) -> DiagnosticMethod {
		self.method
	}
	#[must_use]
	pub const fn seed(self) -> Option<u64> {
		self.seed
	}
	#[must_use]
	pub const fn probes(self) -> usize {
		self.probes
	}
	#[must_use]
	pub const fn iterations(self) -> usize {
		self.iterations
	}
	#[must_use]
	pub const fn route(self) -> Route {
		self.route
	}
	#[must_use]
	pub const fn is_certified_upper_bound(self) -> bool {
		false
	}
}
pub struct MissingReference;
pub struct SuppliedReference(Reference);
pub struct DiagnosticBuilder<'t, R = MissingReference> {
	transform: &'t ValidatedTransform,
	reference: R,
	policy: NumericalPolicy,
	method: Method,
}
enum Method {
	Dense,
	Sampled {
		probes: usize,
		iterations: usize,
		seed: u64,
	},
}
impl ValidatedTransform {
	/// Start a cold, bounded comparison with a caller-supplied logical reference.
	/// Dense numerical SVD is the default; sampled mode remains a dense model
	/// calculation and provides no certified operator-norm upper bound.
	#[must_use]
	pub fn diagnostics(&self) -> DiagnosticBuilder<'_> {
		DiagnosticBuilder {
			transform: self,
			reference: MissingReference,
			policy: NumericalPolicy::default(),
			method: Method::Dense,
		}
	}
}
impl<'t> DiagnosticBuilder<'t> {
	#[must_use]
	pub const fn reference(self, reference: Reference) -> DiagnosticBuilder<'t, SuppliedReference> {
		DiagnosticBuilder {
			transform: self.transform,
			reference: SuppliedReference(reference),
			policy: self.policy,
			method: self.method,
		}
	}
}
impl<R> DiagnosticBuilder<'_, R> {
	#[must_use]
	pub const fn policy(mut self, policy: NumericalPolicy) -> Self {
		self.policy = policy;
		self
	}
	/// Select deterministic complex Rademacher probes and D†D power iterations.
	/// These diagnostics still materialize bounded dense operators; they are not
	/// a matrix-free distributed algorithm or a certified norm upper bound.
	/// # Errors
	/// Requires 1..=4096 probes and at most 1024 iterations.
	pub fn sampled(mut self, probes: usize, iterations: usize, seed: u64) -> Result<Self> {
		if probes == 0 || probes > 4096 || iterations > 1024 {
			return Err(Error::Input("sampled diagnostic work limits"));
		}
		self.method = Method::Sampled {
			probes,
			iterations,
			seed,
		};
		Ok(self)
	}
}
impl DiagnosticBuilder<'_, SuppliedReference> {
	/// Compare full complex values without phase alignment or renormalization.
	/// # Errors
	/// Rejects shapes, finite-data failures, materialization/SVD errors and budgets.
	pub fn run(self) -> Result<EmpiricalEvidence> {
		let physical = 1usize
			.checked_shl(
				u32::try_from(self.transform.operands().num_qubits())
					.map_err(|_| Error::Budget("physical width"))?,
			)
			.ok_or(Error::Budget("physical dimension"))?;
		// Include dense circuit, embeddings, references, discrepancy and solver
		// scratch conservatively before the transform's cold materialization.
		let storage = self
			.policy
			.check(physical, physical, 24)?
			.checked_mul(24)
			.ok_or(Error::Budget("dense diagnostic storage"))?;
		let mut remaining_operations = self
			.policy
			.max_bytes
			.checked_div(size_of::<quest_compile::Operation>().max(1))
			.ok_or(Error::Budget("diagnostic work"))?;
		let mut metadata = 0usize;
		for program in [Some(self.transform.main()), self.transform.continuation()]
			.into_iter()
			.flatten()
		{
			for instruction in program.instructions() {
				metadata = metadata.max(decomposition_storage(
					instruction.operation(),
					0,
					0,
					&mut remaining_operations,
				)?);
			}
		}
		if metadata
			.checked_add(storage)
			.is_none_or(|bytes| bytes > self.policy.max_bytes)
		{
			return Err(Error::Budget("oracle decomposition metadata"));
		}
		let expected = self.reference.0.matrix();
		if expected.nrows() != self.transform.output().logical_dimension()
			|| expected.ncols() != self.transform.input().logical_dimension()
		{
			return Err(Error::Input(
				"reference dimensions differ from extracted transform",
			));
		}
		let actual = self
			.transform
			.materialize_block_with_policy(NumericalPolicy {
				max_bytes: self.policy.max_bytes.saturating_sub(metadata),
			})?;
		if actual.nrows() != expected.nrows() || actual.ncols() != expected.ncols() {
			return Err(Error::Input(
				"reference dimensions differ from extracted transform",
			));
		}
		let difference = matrix::allocate(actual.nrows(), actual.ncols(), self.policy, |r, c| {
			actual[(r, c)].sub(expected[(r, c)])
		})?;
		dimensions(difference.as_ref())?;
		let (observed, method, seed, probes, iterations) = match self.method {
			Method::Dense => {
				let svd = Svd::new(difference.as_ref(), false, self.policy)?;
				let norm =
					(0..svd.values.nrows()).fold(0.0_f64, |n, i| n.max(svd.values[(i, 0)].re));
				(norm, DiagnosticMethod::DenseSpectralNorm, None, 0, 0)
			}
			Method::Sampled {
				probes,
				iterations,
				seed,
			} => (
				sample(difference.as_ref(), probes, iterations, seed, self.policy)?,
				DiagnosticMethod::SeededPowerIteration,
				Some(seed),
				probes,
				iterations,
			),
		};
		if !observed.is_finite() || observed < 0.0 {
			return Err(Error::Input("nonfinite diagnostic result"));
		}
		Ok(EmpiricalEvidence {
			observed,
			method,
			seed,
			probes,
			iterations,
			route: self.transform.route(),
		})
	}
}
struct Svd {
	values: Mat<Complex64>,
	left: Option<Mat<Complex64>>,
	right: Option<Mat<Complex64>>,
}
impl Svd {
	fn new(source: MatRef<'_, Complex64>, vectors: bool, policy: NumericalPolicy) -> Result<Self> {
		use faer::linalg::svd::{ComputeSvdVectors, svd, svd_scratch};
		let dimension = source.nrows().max(source.ncols());
		let matrix_bytes = policy.check(dimension, dimension, 16)?;
		let compute = if vectors {
			ComputeSvdVectors::Full
		} else {
			ComputeSvdVectors::No
		};
		let req = svd_scratch::<Complex64>(
			source.nrows(),
			source.ncols(),
			compute,
			compute,
			Par::Seq,
			faer::Spec::default(),
		);
		if matrix_bytes
			.checked_mul(16)
			.and_then(|n| n.checked_add(req.size_bytes()))
			.is_none_or(|n| n > policy.max_bytes)
		{
			return Err(Error::Budget("SVD scratch"));
		}
		let mut values =
			matrix::allocate(source.nrows().min(source.ncols()), 1, policy, |_, _| {
				Complex64::new(0.0, 0.0)
			})?;
		let mut left = if vectors {
			Some(matrix::allocate(
				source.nrows(),
				source.nrows(),
				policy,
				|_, _| Complex64::new(0.0, 0.0),
			)?)
		} else {
			None
		};
		let mut right = if vectors {
			Some(matrix::allocate(
				source.ncols(),
				source.ncols(),
				policy,
				|_, _| Complex64::new(0.0, 0.0),
			)?)
		} else {
			None
		};
		let mut scratch =
			MemBuffer::try_new(req).map_err(|_| Error::Budget("SVD scratch allocation"))?;
		svd(
			source,
			values.as_mut().col_mut(0).as_diagonal_mut(),
			left.as_mut().map(Mat::as_mut),
			right.as_mut().map(Mat::as_mut),
			Par::Seq,
			MemStack::new(&mut scratch),
			faer::Spec::default(),
		)
		.map_err(|_| Error::Svd)?;
		dimensions(values.as_ref())?;
		if (0..values.nrows()).any(|i| values[(i, 0)].re < 0.0) {
			return Err(Error::Input("negative computed singular value"));
		}
		Ok(Self {
			values,
			left,
			right,
		})
	}
}
fn dimensions(matrix: MatRef<'_, Complex64>) -> Result<()> {
	if matrix.nrows() == 0 || matrix.ncols() == 0 {
		return Err(Error::Input("nonempty reference required"));
	}
	for col in 0..matrix.ncols() {
		for row in 0..matrix.nrows() {
			finite(matrix[(row, col)])?;
		}
	}
	Ok(())
}
const fn finite(value: Complex64) -> Result<Complex64> {
	if !value.re.is_finite() || !value.im.is_finite() {
		Err(Error::Input("finite reference required"))
	} else {
		Ok(value)
	}
}
fn norm(matrix: MatRef<'_, Complex64>) -> f64 {
	(0..matrix.nrows()).fold(0.0_f64, |n, i| {
		n.hypot(matrix[(i, 0)].re).hypot(matrix[(i, 0)].im)
	})
}
fn sample(
	difference: MatRef<'_, Complex64>,
	probes: usize,
	iterations: usize,
	mut seed: u64,
	policy: NumericalPolicy,
) -> Result<f64> {
	let mut vector = matrix::allocate(difference.ncols(), 1, policy, |_, _| {
		Complex64::new(0.0, 0.0)
	})?;
	let mut observed = 0.0_f64;
	for _ in 0..probes {
		for row in 0..vector.nrows() {
			// Fixed SplitMix64 stream; sign-only probes avoid platform-dependent
			// random-distribution/transcendental implementations.
			seed = seed.wrapping_add(0x9e37_79b9_7f4a_7c15);
			let mut bits = seed;
			bits = (bits ^ (bits >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
			bits = (bits ^ (bits >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
			bits ^= bits >> 31;
			vector[(row, 0)] = Complex64::new(
				if bits & 1 == 0 { -1.0 } else { 1.0 },
				if bits & 2 == 0 { -1.0 } else { 1.0 },
			);
		}
		for iteration in 0..=iterations {
			let scale = norm(vector.as_ref());
			if scale == 0.0 {
				break;
			}
			if !scale.is_finite() {
				return Err(Error::Input("nonfinite sampled norm"));
			}
			for row in 0..vector.nrows() {
				vector[(row, 0)] = vector[(row, 0)].div(scale);
			}
			let image = matrix::multiply(difference, vector.as_ref(), policy)?;
			let value = norm(image.as_ref());
			if !value.is_finite() {
				return Err(Error::Input("nonfinite sampled error"));
			}
			observed = observed.max(value);
			if iteration != iterations {
				vector = matrix::multiply(difference.adjoint(), image.as_ref(), policy)?;
			}
		}
	}
	Ok(observed)
}

// Oracle decomposition retains a mapped operation vector while recursively
// interpreting children. Admit that live stack separately from dense matrices.
fn decomposition_storage(
	operation: &quest_compile::Operation,
	inherited: usize,
	depth: usize,
	remaining: &mut usize,
) -> Result<usize> {
	*remaining = remaining
		.checked_sub(1)
		.ok_or(Error::Budget("diagnostic operation work"))?;
	if depth > 64 {
		return Err(Error::Budget("diagnostic oracle nesting"));
	}
	let quest_compile::Operation::Oracle {
		fragment, controls, ..
	} = operation
	else {
		return Ok(0);
	};
	let inherited = inherited
		.checked_add(controls.len())
		.ok_or(Error::Budget("oracle controls"))?;
	let mut own = fragment
		.operations()
		.len()
		.checked_mul(size_of::<quest_compile::Operation>())
		.ok_or(Error::Budget("oracle operation storage"))?;
	let mut child_peak = 0usize;
	for child in fragment.operations() {
		use quest_compile::Operation;
		let (targets, controls, matrix) = match child {
			Operation::Gate {
				targets, controls, ..
			}
			| Operation::Oracle {
				targets, controls, ..
			} => (targets.len(), controls.len(), 0),
			Operation::Numerical {
				targets,
				controls,
				matrix,
			} => (
				targets.len(),
				controls.len(),
				matrix
					.dimension()
					.checked_next_multiple_of(4)
					.and_then(|rows| rows.checked_mul(matrix.dimension()))
					.and_then(|entries| entries.checked_mul(size_of::<Complex64>()))
					.ok_or(Error::Budget("oracle adjoint matrix"))?,
			),
			Operation::GlobalPhase { controls, .. } => (0, controls.len(), 0),
			Operation::Barrier { qubits } => (qubits.len(), 0, 0),
			_ => return Err(Error::Input("diagnostics require coherent operations")),
		};
		let operands = targets
			.checked_add(controls)
			.and_then(|n| n.checked_add(inherited))
			.and_then(|n| n.checked_mul(32))
			.ok_or(Error::Budget("oracle mapped operands"))?;
		own = own
			.checked_add(operands)
			.and_then(|n| n.checked_add(matrix))
			.ok_or(Error::Budget("oracle decomposition storage"))?;
		child_peak = child_peak.max(decomposition_storage(
			child,
			inherited,
			depth.saturating_add(1),
			remaining,
		)?);
	}
	own.checked_add(child_peak)
		.ok_or(Error::Budget("oracle decomposition stack"))
}
