use super::{Point, dot, reserved};
use crate::CfdError;
use mathcore::{
	RBig,
	exact::{Owner, Symbol},
	multivariate::{PolynomialKernel, PolynomialLimits, SparsePolynomial},
};
use quest_numerics::arithmetic::F64Backend;

#[derive(Clone, Debug)]
pub(super) struct Basis {
	pub nodes: Vec<Vec<f64>>,
	pub exact: Vec<SparsePolynomial>,
	kernels: Vec<PolynomialKernel<f64>>,
	derivatives: Vec<Vec<PolynomialKernel<f64>>>,
	pub mass: Vec<Vec<f64>>,
	pub cholesky: Vec<Vec<f64>>,
}
impl Basis {
	pub(super) fn retained_bytes(&self) -> Result<usize, CfdError> {
		let mut bytes = size_of::<Self>();
		let mut charge = |n: usize| -> Result<(), CfdError> {
			bytes = bytes
				.checked_add(n)
				.ok_or(CfdError::InvalidInput("physical basis capacity overflow"))?;
			Ok(())
		};
		for rows in [&self.nodes, &self.mass, &self.cholesky] {
			charge(
				rows.capacity()
					.checked_mul(size_of::<Vec<f64>>())
					.ok_or(CfdError::InvalidInput("physical basis capacity overflow"))?,
			)?;
			for row in rows {
				charge(
					row.capacity()
						.checked_mul(size_of::<f64>())
						.ok_or(CfdError::InvalidInput("physical basis capacity overflow"))?,
				)?;
			}
		}
		charge(
			self.exact
				.capacity()
				.checked_mul(size_of::<SparsePolynomial>())
				.ok_or(CfdError::InvalidInput("physical basis capacity overflow"))?,
		)?;
		for p in &self.exact {
			charge(p.retained_bytes()?)?;
		}
		charge(
			self.kernels
				.capacity()
				.checked_mul(size_of::<PolynomialKernel<f64>>())
				.ok_or(CfdError::InvalidInput("physical basis capacity overflow"))?,
		)?;
		for p in &self.kernels {
			charge(p.retained_bytes()?)?;
		}
		charge(
			self.derivatives
				.capacity()
				.checked_mul(size_of::<Vec<PolynomialKernel<f64>>>())
				.ok_or(CfdError::InvalidInput("physical basis capacity overflow"))?,
		)?;
		for row in &self.derivatives {
			charge(
				row.capacity()
					.checked_mul(size_of::<PolynomialKernel<f64>>())
					.ok_or(CfdError::InvalidInput("physical basis capacity overflow"))?,
			)?;
			for p in row {
				charge(p.retained_bytes()?)?;
			}
		}
		Ok(bytes)
	}
	pub fn new(dimension: usize, order: usize) -> Result<Self, CfdError> {
		let count = dimension + 1;
		let limits = PolynomialLimits {
			max_variables: 4,
			max_terms: 32,
			max_degree: 4,
			max_bytes: 1_048_576,
			max_work: 1_048_576,
			..PolynomialLimits::default()
		};
		let symbols = (0..count)
			.map(|i| {
				Ok(Symbol::new(
					Owner::new(0x4346_4442_444d_5032),
					u64::try_from(i).map_err(|_| CfdError::InvalidInput("basis symbol index"))?,
				))
			})
			.collect::<Result<Vec<_>, CfdError>>()?;
		let coordinates = (0..count)
			.map(|i| SparsePolynomial::variable(symbols.clone(), i, limits))
			.collect::<Result<Vec<_>, _>>()?;
		let two = SparsePolynomial::constant(symbols.clone(), RBig::from(2), limits)?;
		let four = SparsePolynomial::constant(symbols.clone(), RBig::from(4), limits)?;
		let mut exact = reserved(10)?;
		let mut nodes = reserved(10)?;
		for (i, coordinate) in coordinates.iter().enumerate() {
			exact.push(if order == 1 {
				coordinate.clone()
			} else {
				coordinate
					.multiply(coordinate)?
					.multiply(&two)?
					.subtract(coordinate)?
			});
			let mut node = vec![0.; count];
			node[i] = 1.;
			nodes.push(node);
		}
		if order == 2 {
			for i in 0..count {
				for j in i + 1..count {
					exact.push(coordinates[i].multiply(&coordinates[j])?.multiply(&four)?);
					let mut node = vec![0.; count];
					node[i] = 0.5;
					node[j] = 0.5;
					nodes.push(node);
				}
			}
		}
		let kernels = exact
			.iter()
			.map(|p| p.lower(&mut F64Backend))
			.collect::<Result<Vec<_>, _>>()?;
		let derivatives = exact
			.iter()
			.map(|p| {
				symbols
					.iter()
					.map(|&s| Ok(p.differentiate(s)?.lower(&mut F64Backend)?))
					.collect::<Result<Vec<_>, CfdError>>()
			})
			.collect::<Result<Vec<_>, _>>()?;
		let size = nodes.len();
		let mut basis = Self {
			nodes,
			exact,
			kernels,
			derivatives,
			mass: vec![vec![0.; size]; size],
			cholesky: vec![vec![0.; size]; size],
		};
		for (bary, weight) in quadrature(dimension) {
			let values = basis.values(&bary)?;
			for i in 0..size {
				for j in 0..size {
					basis.mass[i][j] += weight * values[i] * values[j];
				}
			}
		}
		for i in 0..size {
			for j in 0..=i {
				let value = basis.mass[i][j]
					- (0..j)
						.map(|k| basis.cholesky[i][k] * basis.cholesky[j][k])
						.sum::<f64>();
				if i == j {
					if !value.is_finite() || value <= 1e-12 {
						return Err(CfdError::Assembly("nonpositive complete local mass"));
					}
					basis.cholesky[i][j] = value.sqrt();
				} else {
					basis.cholesky[i][j] = value / basis.cholesky[j][j];
				}
			}
		}
		Ok(basis)
	}
	pub fn values(&self, bary: &[f64]) -> Result<Vec<f64>, CfdError> {
		self.kernels
			.iter()
			.map(|k| Ok(k.evaluate(&mut F64Backend, bary)?))
			.collect()
	}
	pub fn gradients(
		&self,
		bary: &[f64],
		bary_gradients: &[Point],
	) -> Result<Vec<Point>, CfdError> {
		self.derivatives
			.iter()
			.map(|row| {
				let derivatives = row
					.iter()
					.map(|k| k.evaluate(&mut F64Backend, bary))
					.collect::<Result<Vec<_>, _>>()?;
				Ok(std::array::from_fn(|axis| {
					derivatives
						.iter()
						.zip(bary_gradients)
						.map(|(d, g)| d * g[axis])
						.sum()
				}))
			})
			.collect()
	}
	pub fn whiten_row(&self, row: &[f64], volume: f64) -> Vec<f64> {
		let mut output = vec![0.; row.len()];
		for i in 0..row.len() {
			output[i] = (row[i] / volume.sqrt()
				- (0..i).map(|j| self.cholesky[i][j] * output[j]).sum::<f64>())
				/ self.cholesky[i][i];
		}
		output
	}
	pub fn unwhiten(&self, white: &[f64], volume: f64) -> Vec<f64> {
		let mut output = vec![0.; white.len()];
		for i in (0..white.len()).rev() {
			output[i] = (white[i] / volume.sqrt()
				- (i + 1..white.len())
					.map(|j| self.cholesky[j][i] * output[j])
					.sum::<f64>())
				/ self.cholesky[i][i];
		}
		output
	}
	pub fn apply_mass(&self, values: &[f64], volume: f64) -> Vec<f64> {
		self.mass.iter().map(|r| volume * dot(r, values)).collect()
	}
}
pub(super) fn quadrature(dimension: usize) -> Vec<(Vec<f64>, f64)> {
	let gauss = [
		(0.069_431_844_202_973_71, 0.173_927_422_568_726_93),
		(0.330_009_478_207_571_87, 0.326_072_577_431_273_07),
		(0.669_990_521_792_428_1, 0.326_072_577_431_273_07),
		(0.930_568_155_797_026_2, 0.173_927_422_568_726_93),
	];
	duffy_rule(dimension, &gauss)
}
pub(super) fn high_quadrature(dimension: usize) -> Vec<(Vec<f64>, f64)> {
	let gauss = [
		(0.019_855_071_751_231_884, 0.050_614_268_145_188_13),
		(0.101_666_761_293_186_64, 0.111_190_517_226_687_24),
		(0.237_233_795_041_835_5, 0.156_853_322_938_943_65),
		(0.408_282_678_752_175_1, 0.181_341_891_689_181),
		(0.591_717_321_247_825, 0.181_341_891_689_181),
		(0.762_766_204_958_164_5, 0.156_853_322_938_943_65),
		(0.898_333_238_706_813_4, 0.111_190_517_226_687_24),
		(0.980_144_928_248_768_1, 0.050_614_268_145_188_13),
	];
	duffy_rule(dimension, &gauss)
}
fn duffy_rule(dimension: usize, gauss: &[(f64, f64)]) -> Vec<(Vec<f64>, f64)> {
	let mut output = Vec::new();
	for &(r, wr) in gauss {
		if dimension == 1 {
			output.push((vec![1. - r, r], wr));
			continue;
		}
		for &(s, ws) in gauss {
			if dimension == 2 {
				output.push((vec![1. - r, r * (1. - s), r * s], 2. * r * wr * ws));
			} else {
				for &(t, wt) in gauss {
					output.push((
						vec![1. - r, r * (1. - s), r * s * (1. - t), r * s * t],
						6. * r * r * s * wr * ws * wt,
					));
				}
			}
		}
	}
	output
}
pub(super) fn facet_nodes(dimension: usize, order: usize) -> Vec<Vec<f64>> {
	let mut nodes = Vec::new();
	for i in 0..dimension {
		let mut n = vec![0.; dimension];
		n[i] = 1.;
		nodes.push(n);
	}
	if order == 2 {
		for i in 0..dimension {
			for j in i + 1..dimension {
				let mut n = vec![0.; dimension];
				n[i] = 0.5;
				n[j] = 0.5;
				nodes.push(n);
			}
		}
	}
	nodes
}

#[cfg(test)]
mod tests {
	#![allow(
		clippy::unwrap_used,
		reason = "Numerical basis fixtures have fixed valid dimensions"
	)]
	use super::*;
	#[test]
	fn exact_nodal_basis_partition_gradient_and_affine_piola() {
		for dimension in [2, 3] {
			let basis = Basis::new(dimension, 2).unwrap();
			for (i, node) in basis.nodes.iter().enumerate() {
				for (j, value) in basis.values(node).unwrap().iter().enumerate() {
					assert!((*value - f64::from(i == j)).abs() < 1e-14);
				}
			}
			let vertices = if dimension == 2 {
				vec![[0., 0., 0.], [2., 0., 0.], [1., 3., 0.]]
			} else {
				vec![[0., 0., 0.], [2., 0., 0.], [1., 3., 0.], [0., 1., 4.]]
			};
			let cell =
				crate::simplex::physical_cell(vertices, vec![[0; 3]; dimension + 1], dimension)
					.unwrap();
			let determinant = if dimension == 2 { 6. } else { 24. };
			let reference = |b: &[f64]| {
				[
					b[1] * b[1] + b[2] * b[2],
					b[1] * b[2],
					if dimension == 3 { b[3] * b[3] } else { 0. },
				]
			};
			let piola = |v: Point| {
				[
					(2. * v[0] + v[1]) / determinant,
					(3. * v[1] + v[2]) / determinant,
					4. * v[2] / determinant,
				]
			};
			let values = basis
				.nodes
				.iter()
				.map(|b| piola(reference(b)))
				.collect::<Vec<_>>();
			for (bary, _) in quadrature(dimension) {
				let shape = basis.values(&bary).unwrap();
				let gradients = basis.gradients(&bary, &cell.gradients).unwrap();
				assert!((shape.iter().sum::<f64>() - 1.).abs() < 1e-13);
				for axis in 0..dimension {
					assert!(gradients.iter().map(|g| g[axis]).sum::<f64>().abs() < 1e-13);
				}
				let reconstructed = std::array::from_fn::<_, 3, _>(|axis| {
					values
						.iter()
						.zip(&shape)
						.map(|(v, s)| v[axis] * s)
						.sum::<f64>()
				});
				let expected = piola(reference(&bary));
				for axis in 0..dimension {
					assert!((reconstructed[axis] - expected[axis]).abs() < 1e-13);
				}
				let divergence = values
					.iter()
					.zip(&gradients)
					.map(|(v, g)| dot(v, g))
					.sum::<f64>();
				let expected_divergence =
					(3. * bary[1] + if dimension == 3 { 2. * bary[3] } else { 0. }) / determinant;
				assert!((divergence - expected_divergence).abs() < 1e-13);
			}
			// The opposite-to-vertex-1 face has reversed/sheared normal orientation.
			let scaled_normal = if dimension == 2 {
				[-3., 1., 0.]
			} else {
				[-12., 4., -1.]
			};
			for (face, _) in quadrature(dimension - 1) {
				let mut bary = vec![0.; dimension + 1];
				bary[0] = face[0];
				bary[2..=dimension].copy_from_slice(&face[1..dimension]);
				let reference_value = reference(&bary);
				let physical = piola(reference_value);
				assert!((dot(&physical, &scaled_normal) + reference_value[0]).abs() < 1e-13);
			}
		}
	}
}
