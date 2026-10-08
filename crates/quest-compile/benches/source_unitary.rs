//! Exact source identity matrices; numerical admission is not a certified proof.
use criterion::Criterion;
use num_complex::Complex64;
use quest_compile::{MatrixPolicy, NumericalOperator};
use std::{error::Error, hint::black_box, time::Duration};

const DIMENSIONS: [usize; 4] = [64, 128, 256, 512];

fn phase(value: &str) -> Result<(), Box<dyn Error>> {
	if let Some(path) = std::env::var_os("QUEST_BENCH_PHASE") {
		std::fs::write(path, value)?;
	}
	Ok(())
}

fn admit(source: &faer::Mat<Complex64>) -> Result<NumericalOperator, Box<dyn Error>> {
	Ok(
		NumericalOperator::from_view(source, MatrixPolicy::default())?.admit_unitary(
			0.0,
			MatrixPolicy::default(),
			usize::MAX,
		)?,
	)
}

#[expect(
	clippy::panic,
	reason = "A failed admission aborts the benchmark instead of timing an error path"
)]
fn require<T, E: std::fmt::Display>(result: Result<T, E>) -> T {
	match result {
		Ok(value) => value,
		Err(error) => panic!("unitary measurement failed: {error}"),
	}
}

fn main() -> Result<(), Box<dyn Error>> {
	if std::env::args().any(|arg| arg == "--list") {
		if !std::env::args().any(|arg| arg == "--ignored") {
			for dimension in DIMENSIONS {
				println!("dimension{dimension}: benchmark");
			}
		}
		return Ok(());
	}
	phase("preparation")?;
	let dimension = match std::env::var("QUEST_BENCH_DIMENSION") {
		Ok(value) => value.parse()?,
		Err(_) if !std::env::args().any(|arg| arg == "--bench") => 64,
		Err(error) => return Err(error.into()),
	};
	if !DIMENSIONS.contains(&dimension) {
		return Err("unsupported source unitary dimension".into());
	}
	let source = faer::Mat::from_fn(dimension, dimension, |row, column| {
		Complex64::new(f64::from(u8::from(row == column)), 0.0)
	});
	phase("preflight")?;
	let admitted = admit(&source)?;
	let evidence = admitted
		.unitary_evidence()
		.ok_or("unitary admission omitted evidence")?;
	if evidence.residual() != 0.0 || evidence.tolerance() != 0.0 {
		return Err("accuracy_failure: exact identity must have zero Gram residual".into());
	}
	println!(
		"QUEST_BENCH_PREFLIGHT requested=0 achieved={}",
		evidence.residual()
	);
	if std::env::var_os("QUEST_BENCH_PREFLIGHT_ONLY").is_some() {
		return Ok(());
	}
	let mut criterion = Criterion::default()
		.sample_size(10)
		.warm_up_time(Duration::from_millis(100))
		.measurement_time(Duration::from_millis(200))
		.configure_from_args();
	phase("warmup")?;
	criterion.bench_function(&format!("dimension{dimension}"), |bench| {
		bench.iter(|| black_box(require(admit(black_box(&source)))));
	});
	criterion.final_summary();
	drop(criterion);
	phase("complete")?;
	Ok(())
}
