#![allow(
	clippy::arithmetic_side_effects,
	clippy::panic_in_result_fn,
	clippy::indexing_slicing,
	reason = "Independent sampled analytic signals"
)]
use quest_cfd::observations::{ForceSample, SheddingPolicy, analyze_shedding};
#[test]
fn periodic_lift_has_measured_frequency_but_constant_or_unresolved_lift_does_not()
-> Result<(), Box<dyn std::error::Error>> {
	let samples: Vec<_> = (0..=600)
		.map(|i| {
			let time = f64::from(i) * 0.01;
			ForceSample {
				time,
				drag: 2.,
				lift: (2. * std::f64::consts::TAU * time).sin(),
				pressure_difference: 1.5,
			}
		})
		.collect();
	let report = analyze_shedding(&samples, 0.1, 1., SheddingPolicy::default())?;
	assert!((report.mean_drag - 2.).abs() < 1e-12);
	assert!((report.lift_rms - std::f64::consts::FRAC_1_SQRT_2).abs() < 1e-12);
	let measured = report.frequency.expect("resolved periodic signal");
	assert!((measured.strouhal - 0.2).abs() < 1e-12);
	assert!(measured.completed_periods >= 10);
	let constant: Vec<_> = samples
		.iter()
		.map(|s| ForceSample { lift: 0., ..*s })
		.collect();
	assert!(
		analyze_shedding(&constant, 0.1, 1., SheddingPolicy::default())?
			.frequency
			.is_none()
	);
	assert!(
		analyze_shedding(&samples[..40], 0.1, 1., SheddingPolicy::default())?
			.frequency
			.is_none()
	);
	assert!(
		analyze_shedding(
			&samples,
			0.1,
			1.,
			SheddingPolicy {
				max_samples: 10,
				..SheddingPolicy::default()
			}
		)
		.is_err()
	);
	let mut invalid = samples;
	invalid[2].time = invalid[1].time;
	assert!(analyze_shedding(&invalid, 0.1, 1., SheddingPolicy::default()).is_err());
	Ok(())
}

#[test]
fn time_resolution_period_consistency_and_nonfinite_evidence_reject()
-> Result<(), Box<dyn std::error::Error>> {
	let samples: Vec<_> = (0..=100)
		.map(|i| {
			let time = f64::from(i) * 0.2;
			ForceSample {
				time,
				drag: 1. + time,
				lift: (std::f64::consts::TAU * time).sin(),
				pressure_difference: 2.,
			}
		})
		.collect();
	let report = analyze_shedding(&samples, 1., 1., SheddingPolicy::default())?;
	assert!((report.mean_drag - 11.).abs() < 1e-12);
	assert_eq!(
		report.frequency_unavailable_reason,
		Some("crossings are insufficiently time resolved")
	);
	let varying: Vec<_> = (0..=1000)
		.map(|i| {
			let time = f64::from(i) * 0.01;
			let phase = if time <= 5. {
				time
			} else {
				5. + (time - 5.) / 2.
			};
			ForceSample {
				time,
				drag: 1.,
				lift: (std::f64::consts::TAU * phase).sin(),
				pressure_difference: 0.,
			}
		})
		.collect();
	let report = analyze_shedding(&varying, 1., 1., SheddingPolicy::default())?;
	assert_eq!(
		report.frequency_unavailable_reason,
		Some("crossing periods fail consistency policy")
	);
	let mut invalid = samples.clone();
	invalid[1].lift = f64::NAN;
	assert!(analyze_shedding(&invalid, 1., 1., SheddingPolicy::default()).is_err());
	assert!(
		analyze_shedding(
			&samples,
			1.,
			1.,
			SheddingPolicy {
				max_work: 0,
				..SheddingPolicy::default()
			}
		)
		.is_err()
	);
	assert!(analyze_shedding(&samples, 1., 0., SheddingPolicy::default()).is_err());
	Ok(())
}
