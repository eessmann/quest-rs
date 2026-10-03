use criterion::{BatchSize, Criterion};
use quest::{Complex64 as C, Environment, QubitCount};
use quest_qsp::{PhaseSequence, WxSymmetric};
use quest_qsvt::{DenseEncodingBuilder, NumericalPolicy, TransformBuilder, ValidatedTransform};
use std::{
	cell::RefCell,
	hint::black_box,
	time::{Duration, Instant},
};

#[expect(
	clippy::panic,
	reason = "A benchmark failure must stop measurement, never time an error path"
)]
fn checked<T, E: std::fmt::Display>(result: Result<T, E>) -> T {
	match result {
		Ok(value) => value,
		Err(error) => panic!("QSVT benchmark failed: {error}"),
	}
}
fn fixture() -> quest_qsvt::Result<ValidatedTransform> {
	let matrix = faer::Mat::from_fn(1, 1, |_, _| C::new(0.3, 0.4));
	let encoding = DenseEncodingBuilder::new(matrix.as_ref(), NumericalPolicy::default())?
		.normalization(1.0)?
		.build()?;
	let phases = PhaseSequence::<WxSymmetric>::builder(vec![0.1, 0.2, 0.2, 0.1]).build()?;
	TransformBuilder::new()
		.encoding(encoding)
		.standard(phases)
		.build()
}
fn report_reference(
	environment: &Environment,
	transform: &ValidatedTransform,
) -> Result<(), Box<dyn std::error::Error>> {
	let mut register =
		environment.state_vector(QubitCount::new(transform.operands().num_qubits())?)?;
	let mut prepared = environment.qsvt().transform(transform.clone()).prepare()?;
	register.init_zero()?;
	let result = prepared.run(&mut register)?;
	let mass = result.mass().retained();
	let snapshot = result.logical_snapshot()?;
	let amplitude = snapshot.as_ref().get(0, 0);
	let _ = result.release();
	let mut overlap = environment
		.qsvt()
		.transform(transform.clone())
		.overlap()
		.input(vec![C::new(1.0, 0.0)])
		.reference(vec![C::new(1.0, 0.0)])
		.prepare()?;
	let observed = overlap.run()?;
	eprintln!(
		"{{\"fixture\":\"complex_scalar_degree3\",\"amplitude\":[{},{}],\"mass\":{},\"overlap\":[{},{}],\"overlap_mass\":{}}}",
		amplitude.re,
		amplitude.im,
		mass,
		observed.overlap().re,
		observed.overlap().im,
		observed.retained_mass()
	);
	Ok(())
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
	let environment = Environment::builder().build()?;
	let transform = fixture()?;
	let register = RefCell::new(
		environment.state_vector(QubitCount::new(transform.operands().num_qubits())?)?,
	);
	let mut prepared = environment.qsvt().transform(transform.clone()).prepare()?;
	report_reference(&environment, &transform)?;
	let mut criterion = Criterion::default().configure_from_args();
	criterion.bench_function("qsvt/construction", |b| {
		b.iter(|| black_box(checked(fixture())));
	});
	criterion.bench_function("qsvt/lowering_projection_admission", |b| {
		b.iter_batched(
			|| transform.clone(),
			|t| black_box(checked(environment.qsvt().transform(t).admit())),
			BatchSize::PerIteration,
		);
	});
	criterion.bench_function("qsvt/native_preparation", |b| {
		b.iter_batched(
			|| checked(environment.qsvt().transform(transform.clone()).admit()),
			|a| black_box(checked(a.prepare())),
			BatchSize::PerIteration,
		);
	});
	criterion.bench_function("qsvt/repeated_execution_subnormalized", |b| {
		b.iter_batched(
			|| checked(register.borrow_mut().init_zero()),
			|()| {
				let mut register = register.borrow_mut();
				let result = checked(prepared.run(&mut register));
				let mass = result.mass();
				let _ = result.release();
				black_box(mass)
			},
			BatchSize::PerIteration,
		);
	});
	criterion.bench_function("qsvt/execution_and_conditioning", |b| {
		b.iter_batched(
			|| checked(register.borrow_mut().init_zero()),
			|()| {
				let mut register = register.borrow_mut();
				let conditioned = checked(checked(prepared.run(&mut register)).condition());
				let _ = conditioned.release();
			},
			BatchSize::PerIteration,
		);
	});
	criterion.bench_function("qsvt/conditioning_only", |b| {
		b.iter_custom(|iterations| {
			let mut elapsed = Duration::ZERO;
			for _ in 0..iterations {
				let mut register = register.borrow_mut();
				checked(register.init_zero());
				let result = checked(prepared.run(&mut register));
				let started = Instant::now();
				let conditioned = checked(result.condition());
				elapsed = elapsed.saturating_add(started.elapsed());
				let _ = conditioned.release();
			}
			elapsed
		});
	});
	criterion.bench_function("qsvt/constructed_through_execution", |b| {
		b.iter_batched(
			|| checked(register.borrow_mut().init_zero()),
			|()| {
				let mut prepared =
					checked(environment.qsvt().transform(checked(fixture())).prepare());
				let mut register = register.borrow_mut();
				let _ = checked(prepared.run(&mut register)).release();
			},
			BatchSize::PerIteration,
		);
	});
	criterion.final_summary();
	drop(criterion);
	Ok(())
}
