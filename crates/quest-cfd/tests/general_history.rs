#![allow(
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	clippy::panic_in_result_fn,
	clippy::default_trait_access,
	clippy::suboptimal_flops,
	reason = "Small independent manufactured reference"
)]
use quest_cfd::{
	CfdError,
	history::{HistoryDynamics, HistorySystem},
};
use quest_numerics::Complex64;

struct Manufactured;
impl HistoryDynamics for Manufactured {
	fn dimension(&self) -> usize {
		2
	}
	fn max_generator_entries(&self) -> usize {
		3
	}
	fn retained_bytes(&self) -> Result<usize, CfdError> {
		Ok(size_of::<Self>())
	}
	fn visit_generator(
		&self,
		t: f64,
		visit: &mut dyn FnMut(usize, usize, Complex64) -> Result<(), CfdError>,
	) -> Result<(), CfdError> {
		visit(0, 0, (-t).into())?;
		visit(0, 1, 2.0.into())?;
		visit(1, 1, (-1.0).into())
	}
	fn source(&self, t: f64, output: &mut [Complex64]) -> Result<(), CfdError> {
		// z=(1+t,2-t), f=z'-G(t)z, a nonnormal time-dependent problem.
		output[0] = (1.0 + t * (1.0 + t) - 2.0 * (2.0 - t)).into();
		output[1] = (1.0 - t).into();
		Ok(())
	}
}

#[test]
fn manufactured_nonautonomous_history_samples_source_at_every_dg_node()
-> Result<(), Box<dyn std::error::Error>> {
	for order in [1, 2] {
		let history = HistorySystem::assemble_dynamics(
			&Manufactured,
			&[1.0.into(), 2.0.into()],
			0.1,
			2,
			order,
			Default::default(),
		)?;
		let expected: Vec<_> = history
			.times()
			.iter()
			.flat_map(|t| [Complex64::from(1.0 + t), Complex64::from(2.0 - t)])
			.collect();
		let applied = history.operator().matvec(&expected, Default::default())?;
		let error = applied
			.iter()
			.zip(history.rhs())
			.map(|(a, b)| (*a - *b).norm())
			.fold(0.0, f64::max);
		assert!(error < 1e-13, "DG{order} manufactured residual {error}");
		assert!(history.spectral_bounds(Default::default())?.lower() > 0.0);
	}
	Ok(())
}

#[test]
fn temporal_dg2_supplies_nonnormal_bounds_without_svd() -> Result<(), Box<dyn std::error::Error>> {
	use quest_numerics::{SparseFormat, SparseMatrix};
	let generator = SparseMatrix::from_triplets(
		2,
		2,
		SparseFormat::Csr,
		vec![
			(0, 0, (-1.0).into()),
			(0, 1, 8.0.into()),
			(1, 1, (-1.0).into()),
		],
		Default::default(),
	)?;
	let history = HistorySystem::assemble(
		&generator,
		&[1.0.into(), 0.0.into()],
		0.02,
		2,
		2,
		Default::default(),
	)?;
	let bounds = history.spectral_bounds(Default::default())?;
	assert!(bounds.lower() > 0.0 && bounds.upper() > bounds.lower());
	// Independent bounded inverse (12 rows), never used in production assembly.
	let n = history.operator().rows();
	assert!(n <= 12);
	let mut augmented = vec![vec![Complex64::default(); 2 * n]; n];
	for (row, col, value) in history.operator().entries() {
		augmented[row][col] = value;
	}
	for (row, values) in augmented.iter_mut().enumerate() {
		values[n + row] = 1.0.into();
	}
	for column in 0..n {
		let pivot = (column..n)
			.max_by(|a, b| {
				augmented[*a][column]
					.norm()
					.total_cmp(&augmented[*b][column].norm())
			})
			.ok_or("missing pivot")?;
		augmented.swap(column, pivot);
		let divisor = augmented[column][column];
		assert!(divisor.norm() > 1e-10);
		for value in &mut augmented[column] {
			*value /= divisor;
		}
		for row in 0..n {
			if row == column {
				continue;
			}
			let multiplier = augmented[row][column];
			for entry in 0..2 * n {
				let correction = multiplier * augmented[column][entry];
				augmented[row][entry] -= correction;
			}
		}
	}
	let inverse_frobenius = augmented
		.iter()
		.flat_map(|row| row[n..].iter())
		.map(Complex64::norm_sqr)
		.sum::<f64>()
		.sqrt();
	assert!(
		bounds.lower() * inverse_frobenius < 1.0,
		"bound must also satisfy this stronger small-instance Frobenius check"
	);
	Ok(())
}

struct InvalidRecipe {
	bad_source: bool,
}
impl HistoryDynamics for InvalidRecipe {
	fn dimension(&self) -> usize {
		1
	}
	fn max_generator_entries(&self) -> usize {
		0
	}
	fn retained_bytes(&self) -> Result<usize, CfdError> {
		Ok(size_of::<Self>())
	}
	fn visit_generator(
		&self,
		_: f64,
		visit: &mut dyn FnMut(usize, usize, Complex64) -> Result<(), CfdError>,
	) -> Result<(), CfdError> {
		visit(0, 0, 1.0.into())
	}
	fn source(&self, _: f64, output: &mut [Complex64]) -> Result<(), CfdError> {
		if self.bad_source {
			output[0] = f64::NAN.into();
		}
		Ok(())
	}
}

#[test]
fn history_rejects_nonfinite_sources_and_underdeclared_recipes() {
	for bad_source in [true, false] {
		assert!(
			HistorySystem::assemble_dynamics(
				&InvalidRecipe { bad_source },
				&[0.0.into()],
				0.1,
				2,
				2,
				Default::default()
			)
			.is_err()
		);
	}
}

#[test]
fn dg1_coercivity_preserves_stiff_dissipation() -> Result<(), Box<dyn std::error::Error>> {
	use quest_numerics::{SparseFormat, SparseMatrix};
	// Strict dissipative symmetric part; the upper-triangular block is nonnormal.
	let g = SparseMatrix::from_triplets(
		2,
		2,
		SparseFormat::Csr,
		vec![
			(0, 0, (-100.).into()),
			(1, 1, (-100.).into()),
			(0, 1, 20.0.into()),
		],
		Default::default(),
	)?;
	let history =
		HistorySystem::assemble(&g, &[1.0.into(), 0.0.into()], 0.1, 2, 1, Default::default())?;
	let bounds = history.spectral_bounds(Default::default())?;
	assert!(bounds.lower() > 0.1);
	Ok(())
}
