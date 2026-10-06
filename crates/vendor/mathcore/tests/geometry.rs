#![allow(
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	reason = "Independent bounded i128 Cramer oracle uses fixed dimensions<=3 and coordinates in -3..3"
)]
use googletest::prelude::*;
use mathcore::geometry::{DyadicScale, GeometryError, GeometryLimits, PairRelation};

fn limits() -> GeometryLimits {
	GeometryLimits::default()
}

#[gtest]
fn exact_contacts_crossings_and_hanging_triangle_are_distinguished() -> Result<()> {
	let a = [[0., 0., 0.], [1., 0., 0.], [0., 1., 0.]];
	let b = [[1., 0., 0.], [1., 1., 0.], [0., 1., 0.]];
	let h = [[0.5, 0., 0.], [1., 0., 0.], [0.5, -1., 0.]];
	let points: Vec<_> = a.into_iter().chain(b).chain(h).collect();
	let s = DyadicScale::admit(2, &points, limits())?;
	verify_eq!(
		s.validate_pair(&a, &[0, 1, 2], &b, &[1, 3, 2], limits())?,
		PairRelation::Shared { vertices: 2 }
	)?;
	verify_that!(
		s.validate_pair(&a, &[0, 1, 2], &h, &[4, 1, 5], limits()),
		err(anything())
	)?;
	let c = [[0., -0.25, 0.], [1., 0.75, 0.], [1., -0.25, 0.]];
	let s = DyadicScale::admit(2, &a.into_iter().chain(c).collect::<Vec<_>>(), limits())?;
	verify_that!(
		s.validate_pair(&a, &[0, 1, 2], &c, &[3, 4, 5], limits()),
		err(anything())
	)?;
	Ok(())
}

#[gtest]
fn tetrahedron_face_edge_vertex_and_hanging_contact_are_exact() -> Result<()> {
	let a = [[0., 0., 0.], [1., 0., 0.], [0., 1., 0.], [0., 0., 1.]];
	let face = [[0., 0., 0.], [1., 0., 0.], [0., 1., 0.], [0., 0., -1.]];
	let edge = [[0., 0., 0.], [1., 0., 0.], [0., -1., 0.], [0., 0., -1.]];
	let vertex = [[0., 0., 0.], [-1., 0., 0.], [0., -1., 0.], [0., 0., -1.]];
	let hanging = [[0.5, 0., 0.], [1., 0., 0.], [0.5, -1., 0.], [0.5, 0., -1.]];
	let points: Vec<_> = a
		.into_iter()
		.chain(face)
		.chain(edge)
		.chain(vertex)
		.chain(hanging)
		.collect();
	let s = DyadicScale::admit(3, &points, limits())?;
	for (b, ids, n) in [
		(face, [0, 1, 2, 4], 3),
		(edge, [0, 1, 5, 4], 2),
		(vertex, [0, 6, 5, 4], 1),
	] {
		verify_eq!(
			s.validate_pair(&a, &[0, 1, 2, 3], &b, &ids, limits())?,
			PairRelation::Shared { vertices: n }
		)?;
	}
	verify_that!(
		s.validate_pair(&a, &[0, 1, 2, 3], &hanging, &[7, 1, 8, 9], limits()),
		err(anything())
	)?;
	Ok(())
}

#[gtest]
fn shortcut_does_not_conceal_invalid_cells_ids_or_scale() -> Result<()> {
	let a = [[0., 0., 0.], [1., 0., 0.], [0., 1., 0.]];
	let b = [[4., 0., 0.], [5., 0., 0.], [4., 1., 0.]];
	let s = DyadicScale::admit(2, &a.into_iter().chain(b).collect::<Vec<_>>(), limits())?;
	verify_eq!(
		s.validate_pair(&a, &[0, 1, 2], &b, &[3, 4, 5], limits())?,
		PairRelation::Disjoint
	)?;
	let bad = [[4., 0., 0.], [5., 0., 0.], [6., 0., 0.]];
	verify_that!(
		s.validate_pair(&a, &[0, 1, 2], &bad, &[3, 4, 5], limits()),
		err(anything())
	)?;
	verify_that!(
		s.validate_pair(&a, &[0, 1, 2], &b, &[0, 4, 5], limits()),
		err(anything())
	)?;
	verify_that!(
		s.validate_pair(&a, &[0, 1, 2], &a, &[0, 1, 2], limits()),
		err(anything())
	)?;
	verify_that!(
		s.validate_simplex(&a, &[0, 0, 2], limits()),
		err(anything())
	)?;
	let off_scale = [[0.5, 0., 0.], [1., 0., 0.], [0., 1., 0.]];
	verify_that!(
		s.validate_simplex(&off_scale, &[0, 1, 2], limits()),
		err(anything())
	)?;
	verify_that!(
		s.validate_pair(&a, &[0, 1, 2], &a, &[3, 4, 5], limits()),
		err(anything())
	)?;
	Ok(())
}

#[gtest]
fn exact_displacements_do_not_round_anchor_differences() -> Result<()> {
	let a = [0.1, 0., 0.];
	let b = [1.1, 0., 0.];
	let c = [0., 0., 0.];
	let d = [1., 0., 0.];
	let s = DyadicScale::admit(2, &[a, b, c, d], limits())?;
	verify_eq!(s.matches_displacement(&a, &b, &a, &b, limits())?, true)?;
	verify_eq!(s.matches_displacement(&a, &b, &c, &d, limits())?, false)?;
	verify_eq!(
		s.matches_displacement(&c, &d, &[0., -0., 0.], &d, limits())?,
		true
	)?;
	Ok(())
}

#[gtest]
fn source_and_repeated_query_limits_reject_without_hidden_scale_credit() -> Result<()> {
	let a = [[0., 0., 0.], [1., 0., 0.], [0., 1., 0.]];
	let s = DyadicScale::admit(2, &a, limits())?;
	verify_eq!(s.pair_peak_bytes(), 65536)?;
	let small = GeometryLimits {
		max_work: s.pair_work() - 1,
		..limits()
	};
	verify_that!(s.validate_simplex(&a, &[0, 1, 2], small), err(anything()))?;
	verify_that!(s.validate_simplex(&a, &[0, 1, 2], small), err(anything()))?;
	for l in [
		GeometryLimits {
			max_bytes: 65535,
			..limits()
		},
		GeometryLimits {
			max_coefficient_bits: 1,
			..limits()
		},
	] {
		verify_that!(s.validate_simplex(&a, &[0, 1, 2], l), err(anything()))?;
	}
	let extreme = [[f64::from_bits(1), 0., 0.], [1., 0., 0.]];
	verify_that!(DyadicScale::admit(2, &extreme, limits()), err(anything()))?;
	verify_that!(
		DyadicScale::admit(
			2,
			&a,
			GeometryLimits {
				max_work: 1,
				..limits()
			}
		),
		err(anything())
	)?;
	verify_that!(
		DyadicScale::admit(2, &[[f64::NAN, 0., 0.]], limits()),
		err(anything())
	)?;
	Ok(())
}

#[gtest]
fn single_binary_bit_gap_overlap_and_hard_width_costs() -> Result<()> {
	let a = [[0., 0., 0.], [1., 0., 0.], [0., 1., 0.]];
	for (x, legal) in [
		(f64::from_bits(1.0_f64.to_bits() + 1), true),
		(f64::from_bits(1.0_f64.to_bits() - 1), false),
	] {
		let b = [[x, 0., 0.], [2., 0., 0.], [x, 1., 0.]];
		let s = DyadicScale::admit(2, &a.into_iter().chain(b).collect::<Vec<_>>(), limits())?;
		verify_eq!(
			s.validate_pair(&a, &[0, 1, 2], &b, &[3, 4, 5], limits())
				.is_ok(),
			legal
		)?;
	}
	for (value, bits, work) in [
		(2.0_f64.powi(63), 64, 1_196_032),
		(2.0_f64.powi(127), 128, 3_457_024),
	] {
		let l = GeometryLimits {
			max_coordinate_bits: 128,
			..limits()
		};
		let s = DyadicScale::admit(3, &[[value, 0., 0.], [1., 0., 0.]], l)?;
		verify_eq!(s.coordinate_bits(), bits)?;
		verify_eq!(s.pair_work(), work)?;
	}
	Ok(())
}

#[gtest]
fn shared_decoder_preserves_exact_import_and_existing_raw_width_guard() -> Result<()> {
	use dashu_int::IBig;
	use mathcore::{
		RBig,
		arithmetic::ExactConstant,
		multivariate::{PolynomialLimits, rational_constant},
	};
	let l = PolynomialLimits::default();
	for (value, expected) in [
		(0., RBig::ZERO),
		(-0., RBig::ZERO),
		(1., RBig::ONE),
		(-1.5, RBig::from_parts_signed(IBig::from(-3), IBig::from(2))),
		(
			f64::from_bits(1),
			RBig::from_parts_signed(IBig::ONE, IBig::ONE << 1074),
		),
		(f64::MAX, RBig::from(IBig::from((1_u64 << 53) - 1) << 971)),
	] {
		verify_eq!(
			rational_constant(&ExactConstant::Binary64(value), l)?,
			expected
		)?;
	}
	verify_that!(
		rational_constant(
			&ExactConstant::Binary64(1.),
			PolynomialLimits {
				max_coefficient_bits: 104,
				..l
			}
		),
		err(anything())
	)?;
	verify_eq!(
		rational_constant(
			&ExactConstant::Binary64(1.),
			PolynomialLimits {
				max_coefficient_bits: 105,
				..l
			}
		)?,
		RBig::ONE
	)?;
	verify_that!(
		rational_constant(&ExactConstant::Binary64(f64::INFINITY), l),
		err(anything())
	)?;
	Ok(())
}

#[gtest]
fn profile_child() -> Result<()> {
	if let Ok(mode) = std::env::var("MATHCORE_GEOMETRY_PROFILE_CHILD") {
		let l = GeometryLimits {
			max_coordinate_bits: 128,
			..limits()
		};
		let big = 2.0_f64.powi(127);
		let points = [[big, 0., 0.], [0., big, 0.], [0., 0., big], [1., 1., 1.]];
		if mode == "tiny_budget" {
			verify_eq!(
				DyadicScale::admit(
					3,
					&points,
					GeometryLimits {
						max_bytes: 1,
						max_work: 1,
						..l
					}
				)
				.err(),
				Some(GeometryError::Budget("source admission"))
			)?;
			return Ok(());
		}
		let result = DyadicScale::admit(3, &points, l)
			.and_then(|s| s.validate_simplex(&points, &[0, 1, 2, 3], l));
		verify_eq!(result.is_ok(), mode == "accepted")?;
	}
	Ok(())
}
#[gtest]
fn unsafe_mul_and_square_tuning_profiles_are_rejected_in_subprocesses() -> Result<()> {
	for name in ["DASHU_THRESHOLD_SIMPLE_MUL", "DASHU_THRESHOLD_SIMPLE_SQR"] {
		let status = std::process::Command::new(std::env::current_exe()?)
			.args(["--exact", "profile_child", "--nocapture"])
			.env_remove("DASHU_THRESHOLD_SIMPLE_MUL")
			.env_remove("DASHU_THRESHOLD_SIMPLE_SQR")
			.env(name, "0")
			.env("MATHCORE_GEOMETRY_PROFILE_CHILD", "1")
			.status()?;
		verify_eq!(status.success(), true)?;
	}
	let status = std::process::Command::new(std::env::current_exe()?)
		.args(["--exact", "profile_child", "--nocapture"])
		.env("DASHU_THRESHOLD_SIMPLE_MUL", "24")
		.env("DASHU_THRESHOLD_SIMPLE_SQR", "30")
		.env("DASHU_THRESHOLD_KARATSUBA_MUL", "0")
		.env("DASHU_THRESHOLD_NTT_MUL", "0")
		.env("DASHU_THRESHOLD_KARATSUBA_SQR", "0")
		.env("DASHU_THRESHOLD_NTT_SQR", "0")
		.env("MATHCORE_GEOMETRY_PROFILE_CHILD", "accepted")
		.status()?;
	verify_eq!(status.success(), true)?;
	Ok(())
}

#[gtest]
fn profile_string_domain_and_tiny_budget_precedence_are_explicit() -> Result<()> {
	for (name, min) in [
		("DASHU_THRESHOLD_SIMPLE_MUL", "24"),
		("DASHU_THRESHOLD_SIMPLE_SQR", "30"),
	] {
		for (value, mode) in [
			(format!("{}{min}", "0".repeat(31)), "rejected"),
			(format!("{}{min}", "0".repeat(30)), "accepted"),
			("2４".to_owned(), "rejected"),
			("invalid".to_owned(), "rejected"),
			("0".to_owned(), "tiny_budget"),
		] {
			let status = std::process::Command::new(std::env::current_exe()?)
				.args(["--exact", "profile_child", "--nocapture"])
				.env_remove("DASHU_THRESHOLD_SIMPLE_MUL")
				.env_remove("DASHU_THRESHOLD_SIMPLE_SQR")
				.env(name, value)
				.env("MATHCORE_GEOMETRY_PROFILE_CHILD", mode)
				.status()?;
			verify_eq!(status.success(), true)?;
		}
	}
	Ok(())
}

// Independent homogeneous Cramer enumeration, not the production edge/facet method.
const fn det(m: &[[i128; 3]; 3], d: usize) -> i128 {
	if d == 2 {
		return m[0][0] * m[1][1] - m[0][1] * m[1][0];
	}
	m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
		- m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
		+ m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
}
fn orientation(p: &[[i128; 3]; 4], d: usize) -> i128 {
	det(
		&std::array::from_fn(|r| std::array::from_fn(|c| p[c + 1][r] - p[0][r])),
		d,
	)
}
fn planes(p: &[[i128; 3]; 4], d: usize) -> [[i128; 4]; 4] {
	let sign = orientation(p, d).signum();
	std::array::from_fn(|j| {
		let mut replaced = *p;
		replaced[j] = [0; 3];
		let b = orientation(&replaced, d) * sign;
		let mut row = [0; 4];
		row[3] = b;
		for k in 0..d {
			replaced[j] = [0; 3];
			replaced[j][k] = 1;
			row[k] = orientation(&replaced, d) * sign - b;
		}
		row
	})
}
fn cramer_legal(
	a: &[[i128; 3]; 4],
	ai: &[usize; 4],
	b: &[[i128; 3]; 4],
	bi: &[usize; 4],
	d: usize,
) -> bool {
	let ap = planes(a, d);
	let bp = planes(b, d);
	let all: Vec<_> = ap[..=d].iter().chain(&bp[..=d]).copied().collect();
	for i in 0..all.len() {
		for j in i + 1..all.len() {
			for k in if d == 3 {
				j + 1..all.len()
			} else {
				all.len()..all.len() + 1
			} {
				let rows = if d == 3 {
					[all[i], all[j], all[k]]
				} else {
					[all[i], all[j], [0; 4]]
				};
				let m = std::array::from_fn(|r| std::array::from_fn(|c| rows[r][c]));
				let denominator = det(&m, d);
				if denominator == 0 {
					continue;
				}
				let mut numerators = [0; 3];
				for axis in 0..d {
					let mut replaced = m;
					for r in 0..d {
						replaced[r][axis] = -rows[r][3];
					}
					numerators[axis] = det(&replaced, d);
				}
				let value = |p: &[i128; 4]| {
					(0..d).map(|axis| p[axis] * numerators[axis]).sum::<i128>() + p[3] * denominator
				};
				if all.iter().any(|p| value(p) * denominator.signum() < 0) {
					continue;
				}
				for vertex in 0..=d {
					if !bi[..=d].contains(&ai[vertex]) && value(&ap[vertex]) != 0 {
						return false;
					}
				}
			}
		}
	}
	true
}
#[gtest]
fn deterministic_general_pairs_match_independent_cramer_vertices() -> Result<()> {
	let mut random = 0x1234_5678_u64;
	let mut counts = [0_usize; 2];
	for d in [2, 3] {
		for _ in 0..250 {
			let mut a = [[0_i128; 3]; 4];
			let mut b = a;
			for p in a[..=d].iter_mut().chain(b[..=d].iter_mut()) {
				for value in &mut p[..d] {
					random = random
						.wrapping_mul(6_364_136_223_846_793_005)
						.wrapping_add(1);
					*value = i128::from(i32::try_from((random >> 32) % 7)? - 3);
				}
			}
			if orientation(&a, d) == 0 || orientation(&b, d) == 0 {
				continue;
			}
			let ai = [0, 1, 2, 3];
			let mut bi = [4, 5, 6, 7];
			for j in 0..=d {
				for i in 0..=d {
					if a[i] == b[j] {
						bi[j] = ai[i];
					}
				}
			}
			if bi[..=d].iter().all(|id| ai[..=d].contains(id)) {
				continue;
			}
			let floats = |p: &[i128; 3]| -> Result<[f64; 3]> {
				Ok([
					f64::from(i32::try_from(p[0])?),
					f64::from(i32::try_from(p[1])?),
					f64::from(i32::try_from(p[2])?),
				])
			};
			let af: Vec<_> = a[..=d].iter().map(floats).collect::<Result<_>>()?;
			let bf: Vec<_> = b[..=d].iter().map(floats).collect::<Result<_>>()?;
			let s = DyadicScale::admit(
				d,
				&af.iter().chain(&bf).copied().collect::<Vec<_>>(),
				limits(),
			)?;
			let legal = cramer_legal(&a, &ai, &b, &bi, d);
			counts[usize::from(legal)] += 1;
			verify_eq!(
				s.validate_pair(&af, &ai[..=d], &bf, &bi[..=d], limits())
					.is_ok(),
				cramer_legal(&a, &ai, &b, &bi, d)
			)?;
		}
	}
	verify_gt!(counts[0], 100)?;
	verify_gt!(counts[1], 0)?;
	println!(
		"independent Cramer comparisons: nonconforming={}, conforming={}",
		counts[0], counts[1]
	);
	Ok(())
}
