#![allow(
	clippy::indexing_slicing,
	clippy::arithmetic_side_effects,
	clippy::float_cmp,
	clippy::panic,
	clippy::panic_in_result_fn,
	reason = "Small analytic fixtures deliberately use direct indices, exact expectations and failing assertions"
)]
use quest_polynomial::{
	Complex64, Interval, Laurent, Limits, Monomial, NormDomain, NormOptions, NormOutcome,
	NormStatus, Polynomial,
};
const fn c(x: f64) -> Complex64 {
	Complex64::new(x, 0.0)
}
#[test]
fn symmetric_laurent_conversion_matches_identity_and_rejects_asymmetry()
-> Result<(), Box<dyn std::error::Error>> {
	// z^2+z^-2 = 2 T_2((z+z^-1)/2).
	let p = Polynomial::new(
		Laurent::new(-2),
		vec![c(1.0), c(0.0), c(3.0), c(0.0), c(1.0)],
		Limits::default(),
	)?;
	let out = p.to_chebyshev_symmetric()?;
	assert_eq!(out.polynomial().coefficients(), &[c(3.0), c(0.0), c(2.0)]);
	assert!(out.coefficient_error_bound() >= 0.0 && out.coefficient_error_bound() < 1e-14);
	assert!((out.polynomial().evaluate_real(0.25)? - 1.25).abs() < 1e-14);
	assert!(
		Polynomial::new(
			Laurent::new(-1),
			vec![c(1.0), c(0.0), c(2.0)],
			Limits::default()
		)?
		.to_chebyshev_symmetric()
		.is_err()
	);
	// Stored zeros outside the effective support do not break symmetry.
	assert!(
		Polynomial::new(
			Laurent::new(-3),
			vec![c(0.0), c(1.0), c(0.0), c(3.0), c(0.0), c(1.0)],
			Limits::default()
		)?
		.to_chebyshev_symmetric()
		.is_ok()
	);
	Ok(())
}
#[test]
fn certified_interval_norm_contains_analytic_interior_maximum()
-> Result<(), Box<dyn std::error::Error>> {
	let p = Polynomial::new(Monomial, vec![c(1.0), c(0.0), c(-1.0)], Limits::default())?;
	let NormOutcome::Bounded(r) = p.certify_norm(
		NormDomain::RealInterval(Interval::new(-1.0, 1.0)?),
		NormOptions::default(),
	)?
	else {
		panic!("bounded polynomial")
	};
	assert!(r.bounds().contains(1.0));
	assert_eq!(r.status(), NormStatus::Converged);
	assert!(r.bounds().upper() - r.bounds().lower() < 1e-6);
	Ok(())
}
#[test]
fn complex_interval_and_disc_norms_enclose_analytic_values()
-> Result<(), Box<dyn std::error::Error>> {
	let p = Polynomial::new(
		Monomial,
		vec![Complex64::new(0.0, 1.0), c(1.0)],
		Limits::default(),
	)?;
	let NormOutcome::Bounded(r) = p.certify_norm(
		NormDomain::RealInterval(Interval::new(-1.0, 1.0)?),
		NormOptions::default(),
	)?
	else {
		panic!()
	};
	assert!(r.bounds().contains(2.0_f64.sqrt()));
	let NormOutcome::Bounded(r) = p.certify_norm(
		NormDomain::Disc {
			center: c(0.0),
			radius: 1.0,
		},
		NormOptions {
			absolute_tolerance: 1e-4,
			..NormOptions::default()
		},
	)?
	else {
		panic!()
	};
	assert!(r.bounds().contains(2.0));
	assert_eq!(r.status(), NormStatus::Converged);
	let NormOutcome::Bounded(r) = p.certify_norm(
		NormDomain::Disc {
			center: c(0.0),
			radius: 0.0,
		},
		NormOptions::default(),
	)?
	else {
		panic!()
	};
	assert!(r.bounds().contains(1.0));
	Ok(())
}
#[test]
fn laurent_poles_use_effective_support_and_off_origin_disc_is_bounded()
-> Result<(), Box<dyn std::error::Error>> {
	let p = Polynomial::new(Laurent::new(-1), vec![c(1.0)], Limits::default())?;
	assert!(matches!(
		p.certify_norm(
			NormDomain::RealInterval(Interval::new(-1.0, 1.0)?),
			NormOptions::default()
		)?,
		NormOutcome::UnboundedPole
	));
	assert!(matches!(
		p.certify_norm(
			NormDomain::Disc {
				center: c(0.0),
				radius: 1.0
			},
			NormOptions::default()
		)?,
		NormOutcome::UnboundedPole
	));
	let NormOutcome::Bounded(r) = p.certify_norm(
		NormDomain::Disc {
			center: c(2.0),
			radius: 1.0,
		},
		NormOptions {
			absolute_tolerance: 1e-4,
			..NormOptions::default()
		},
	)?
	else {
		panic!()
	};
	assert!(r.bounds().contains(1.0));
	let q = Polynomial::new(
		Laurent::new(-2),
		vec![c(0.0), c(0.0), c(3.0)],
		Limits::default(),
	)?;
	let NormOutcome::Bounded(r) = q.certify_norm(
		NormDomain::Disc {
			center: c(0.0),
			radius: 1.0,
		},
		NormOptions::default(),
	)?
	else {
		panic!()
	};
	assert!(r.bounds().contains(3.0));
	Ok(())
}
#[test]
fn budget_and_invalid_domain_cannot_masquerade_as_convergence()
-> Result<(), Box<dyn std::error::Error>> {
	let p = Polynomial::new(Monomial, vec![c(1.0), c(0.0), c(-1.0)], Limits::default())?;
	let NormOutcome::Bounded(r) = p.certify_norm(
		NormDomain::RealInterval(Interval::new(-1.0, 1.0)?),
		NormOptions {
			max_cells: 1,
			..NormOptions::default()
		},
	)?
	else {
		panic!()
	};
	assert_eq!(r.status(), NormStatus::Budget);
	assert!(r.bounds().contains(1.0));
	assert!(
		p.certify_norm(
			NormDomain::Disc {
				center: c(0.0),
				radius: -1.0
			},
			NormOptions::default()
		)
		.is_err()
	);
	assert!(
		p.certify_norm(
			NormDomain::RealInterval(Interval::new(-1.0, 1.0)?),
			NormOptions {
				absolute_tolerance: f64::NAN,
				..NormOptions::default()
			}
		)
		.is_err()
	);
	Ok(())
}
#[test]
fn representable_large_constant_norm_does_not_overflow_squaring()
-> Result<(), Box<dyn std::error::Error>> {
	let p = Polynomial::new(Monomial, vec![c(f64::MAX)], Limits::default())?;
	let NormOutcome::Bounded(r) = p.certify_norm(
		NormDomain::RealInterval(Interval::point(0.0)?),
		NormOptions::default(),
	)?
	else {
		panic!()
	};
	assert!(r.bounds().contains(f64::MAX));
	assert_eq!(r.status(), NormStatus::Converged);
	Ok(())
}
#[test]
fn zero_empty_point_and_pole_on_disc_boundary_have_distinct_results()
-> Result<(), Box<dyn std::error::Error>> {
	for coefficients in [vec![], vec![c(0.0), c(0.0)]] {
		let p = Polynomial::new(Laurent::new(-10), coefficients, Limits::default())?;
		let NormOutcome::Bounded(r) = p.certify_norm(
			NormDomain::Disc {
				center: c(0.0),
				radius: 1.0,
			},
			NormOptions::default(),
		)?
		else {
			panic!()
		};
		assert!(r.bounds().contains(0.0));
		assert_eq!(r.status(), NormStatus::Converged);
	}
	let p = Polynomial::new(Laurent::new(-1), vec![c(1.0)], Limits::default())?;
	assert!(matches!(
		p.certify_norm(
			NormDomain::Disc {
				center: c(1.0),
				radius: 1.0
			},
			NormOptions::default()
		)?,
		NormOutcome::UnboundedPole
	));
	let NormOutcome::Bounded(r) = p.certify_norm(
		NormDomain::RealInterval(Interval::point(2.0)?),
		NormOptions::default(),
	)?
	else {
		panic!()
	};
	assert!(r.bounds().contains(0.5));
	assert_eq!(r.status(), NormStatus::Converged);
	Ok(())
}
