//! Exact fixtures come from the campaign. Discovery is metadata-only.
use criterion::Criterion;
use quest_polynomial::{Laurent, Limits, Polynomial};
use quest_qsp::{
	Complex64, FrozenCandidate, Policy, SynthesisBuilder, UnitCircleResponse,
	benchmark_support::{self, ScatteringPair},
};
use std::{
	error::Error,
	hint::black_box,
	ops::{Add, Mul, Sub},
	time::Duration,
};

const SCOPES: [&str; 5] = [
	"inverse_only",
	"forward_only",
	"completion_inverse",
	"validated_roundtrip",
	"full_pipeline",
];

fn phase(name: &str) -> Result<(), Box<dyn Error>> {
	if let Some(path) = std::env::var_os("QUEST_BENCH_PHASE") {
		std::fs::write(path, name)?;
	}
	Ok(())
}

fn coefficients(value: &serde_json::Value, name: &str) -> Result<Vec<Complex64>, Box<dyn Error>> {
	value
		.get(format!("{name}_bits"))
		.and_then(serde_json::Value::as_array)
		.ok_or("missing coefficient array")?
		.iter()
		.map(|pair| {
			Ok(Complex64::new(
				f64::from_bits(
					pair.get(0)
						.and_then(serde_json::Value::as_u64)
						.ok_or("missing real bits")?,
				),
				f64::from_bits(
					pair.get(1)
						.and_then(serde_json::Value::as_u64)
						.ok_or("missing imaginary bits")?,
				),
			))
		})
		.collect()
}

fn full_pipeline(
	target: &Polynomial<Laurent>,
	policy: Policy,
) -> quest_qsp::Result<FrozenCandidate<UnitCircleResponse>> {
	SynthesisBuilder::new()
		.unit_circle_response(target)?
		.policy(policy)
		.admit()?
		.complete()?
		.synthesize()
}

fn error(actual: &ScatteringPair, expected: &ScatteringPair) -> Result<f64, Box<dyn Error>> {
	if actual.target.len() != expected.target.len()
		|| actual.conjugate_complement.len() != expected.conjugate_complement.len()
	{
		return Err("roundtrip support mismatch".into());
	}
	let mut maximum = 0.0_f64;
	for (a, b) in actual
		.target
		.iter()
		.chain(&actual.conjugate_complement)
		.zip(expected.target.iter().chain(&expected.conjugate_complement))
	{
		let difference = (*a).sub(*b).norm();
		if !difference.is_finite() {
			return Err("nonfinite roundtrip error".into());
		}
		maximum = maximum.max(difference);
	}
	Ok(maximum)
}

fn validate_full_pipeline(
	target: &Polynomial<Laurent>,
	coefficients: &[Complex64],
	policy: Policy,
	tolerance: f64,
) -> Result<(), Box<dyn Error>> {
	let selected = std::env::var("QUEST_BENCH_SCOPE").ok();
	if selected.as_deref() == Some("full_pipeline") {
		let candidate = full_pipeline(target, policy)?;
		for index in 0..32_u32 {
			let z = Complex64::from_polar(1.0, f64::from(index) * std::f64::consts::TAU / 32.0);
			let reference = coefficients
				.iter()
				.rev()
				.fold(Complex64::new(0.0, 0.0), |sum, coefficient| {
					sum.mul(z).add(coefficient)
				});
			let [[actual, _], _] = candidate.evaluate(z)?;
			let deviation = actual.sub(reference).norm();
			if !deviation.is_finite() || deviation > tolerance {
				return Err(format!("accuracy_failure: source response {deviation}").into());
			}
		}
	}
	Ok(())
}

fn verify_expected_reflections(
	fixture: &serde_json::Value,
	actual: &[Complex64],
	tolerance: f64,
) -> Result<(), Box<dyn Error>> {
	if fixture.get("expected_reflections_bits").is_some() {
		let expected = coefficients(fixture, "expected_reflections")?;
		if expected.len() != actual.len() {
			return Err("reflection support mismatch".into());
		}
		for (actual, expected) in actual.iter().zip(expected) {
			let deviation = actual.sub(expected).norm();
			if !deviation.is_finite() || deviation > tolerance {
				return Err(
					format!("accuracy_failure: source reflection deviation {deviation}").into(),
				);
			}
		}
	}
	Ok(())
}

fn load_fixture() -> Result<serde_json::Value, Box<dyn Error>> {
	let fixture = if let Some(path) = std::env::var_os("QUEST_BENCH_INPUT") {
		serde_json::from_slice(&std::fs::read(path)?)?
	} else if !std::env::args().any(|arg| arg == "--bench") {
		serde_json::json!({"canonical_coefficients_bits": [[0.2_f64.to_bits(), 0_u64]], "degree": 0})
	} else {
		return Err("QUEST_BENCH_INPUT is required for measurements".into());
	};
	Ok(fixture)
}

fn main() -> Result<(), Box<dyn Error>> {
	if std::env::args().any(|arg| arg == "--list") {
		if std::env::args().any(|arg| arg == "--ignored") {
			return Ok(());
		}
		for name in SCOPES {
			println!("{name}: benchmark");
		}
		return Ok(());
	}
	phase("preparation")?;
	let fixture = load_fixture()?;
	let coefficients = coefficients(&fixture, "canonical_coefficients")?;
	let degree = fixture
		.get("degree")
		.and_then(serde_json::Value::as_u64)
		.ok_or("missing source degree")?;
	let tolerance = fixture
		.get("requested_tolerance_bits")
		.and_then(serde_json::Value::as_u64)
		.map_or(if degree <= 10_000 { 1e-12 } else { 1e-10 }, f64::from_bits);
	let policy = Policy {
		accuracy: quest_qsp::AccuracyPolicy {
			response_tolerance: tolerance,
			..(Policy::default()).accuracy
		},
		..Policy::default()
	};
	let completion_policy = Policy {
		accuracy: quest_qsp::AccuracyPolicy {
			response_tolerance: tolerance * 8.0,
			..(policy).accuracy
		},
		..policy
	};
	let target = Polynomial::new(Laurent::new(0), coefficients.clone(), Limits::default())?;
	let pair = if fixture.get("conjugate_complement_bits").is_some() {
		let pair = ScatteringPair {
			target: coefficients.clone(),
			conjugate_complement: self::coefficients(&fixture, "conjugate_complement")?,
		};
		let resources = quest_numerics::OperationResources::from_limits(policy.limits);
		let storage = [pair.target.capacity(), pair.conjugate_complement.capacity()]
			.into_iter()
			.map(|count| {
				count
					.checked_mul(size_of::<Complex64>())
					.map(|bytes| (bytes, 0))
					.ok_or("fixture capacity overflow")
			})
			.collect::<Result<Vec<_>, _>>()?;
		quest_numerics::Accounted::new_many(pair, resources.reserve_many(&storage)?)
	} else {
		benchmark_support::complete(&coefficients, completion_policy)?
	};
	phase("preflight")?;
	let recovered = benchmark_support::inverse(&pair, policy)?;
	verify_expected_reflections(&fixture, &recovered, tolerance)?;
	let forward = benchmark_support::forward(&recovered, policy)?;
	let achieved = error(&forward, &pair)?;
	if achieved > tolerance {
		return Err(format!("accuracy_failure: requested={tolerance} achieved={achieved}").into());
	}
	validate_full_pipeline(&target, &coefficients, policy, tolerance)?;
	println!("QUEST_BENCH_PREFLIGHT requested={tolerance} achieved={achieved}");
	if std::env::var_os("QUEST_BENCH_PREFLIGHT_ONLY").is_some() {
		return Ok(());
	}
	measurements(
		&target,
		&coefficients,
		&pair,
		&recovered,
		policy,
		completion_policy,
		tolerance,
	)
}

fn measurements(
	target: &Polynomial<Laurent>,
	coefficients: &[Complex64],
	pair: &ScatteringPair,
	recovered: &[Complex64],
	policy: Policy,
	completion_policy: Policy,
	tolerance: f64,
) -> Result<(), Box<dyn Error>> {
	let mut criterion = Criterion::default()
		.sample_size(10)
		.warm_up_time(Duration::from_millis(100))
		.measurement_time(Duration::from_millis(200))
		.configure_from_args();
	phase("warmup")?;
	criterion.bench_function("inverse_only", |bench| {
		bench.iter(|| black_box(require(benchmark_support::inverse(black_box(pair), policy))));
	});
	criterion.bench_function("forward_only", |bench| {
		bench.iter(|| {
			black_box(require(benchmark_support::forward(
				black_box(recovered),
				policy,
			)))
		});
	});
	criterion.bench_function("completion_inverse", |bench| {
		bench.iter(|| {
			let completed = require(benchmark_support::complete(
				black_box(coefficients),
				completion_policy,
			));
			black_box(require(benchmark_support::inverse(&completed, policy)))
		});
	});
	criterion.bench_function("validated_roundtrip", |bench| {
		bench.iter(|| {
			let completed = require(benchmark_support::complete(
				black_box(coefficients),
				completion_policy,
			));
			let recovered = require(benchmark_support::inverse(&completed, policy));
			let forward = require(benchmark_support::forward(&recovered, policy));
			let achieved = require_box(error(&forward, &completed));
			require(accept_error(achieved, tolerance));
			black_box(forward)
		});
	});
	criterion.bench_function("full_pipeline", |bench| {
		bench.iter(|| black_box(require(full_pipeline(black_box(target), policy))));
	});
	criterion.final_summary();
	drop(criterion);
	phase("complete")?;
	Ok(())
}

fn accept_error(achieved: f64, tolerance: f64) -> quest_qsp::Result<()> {
	if achieved > tolerance {
		return Err(quest_qsp::Error::NotEstablished {
			stage: "benchmark roundtrip",
			bound: achieved,
			tolerance,
		});
	}
	Ok(())
}
#[expect(
	clippy::panic,
	reason = "Failed measurements abort; error paths are never timed as success"
)]
fn require<T>(value: quest_qsp::Result<T>) -> T {
	match value {
		Ok(value) => value,
		Err(error) => panic!("measurement failed: {error}"),
	}
}
#[expect(
	clippy::panic,
	reason = "Invalid numerical evidence aborts measurement"
)]
fn require_box<T>(value: Result<T, Box<dyn Error>>) -> T {
	match value {
		Ok(value) => value,
		Err(error) => panic!("measurement failed: {error}"),
	}
}
