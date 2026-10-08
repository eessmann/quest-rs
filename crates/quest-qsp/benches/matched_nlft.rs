//! `SoftwareX`'s external controller owns correctness and statistical inference.
//! This adapter records every actual public operation through Criterion custom timing.
use criterion::Criterion;
use hdf5_metno::{File, types::VarLenUnicode};
use quest_numerics::{
	Accounted, ExecutionPolicy, FftBackend, OperationLimits, OperationResources, ResourceError,
	ResourceReport,
};
use quest_qsp::{Complex64, ForwardNlftWorkspace, InverseNlftWorkspace};
use serde_json::{Value, json};
use std::{
	error::Error,
	io::{BufRead, BufReader, Write},
	os::unix::net::UnixStream,
	path::Path,
	str::FromStr,
	time::{Duration, Instant},
};
#[derive(Debug)]
struct ReportedFailure;
impl std::fmt::Display for ReportedFailure {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.write_str("failure already reported to controller")
	}
}
impl Error for ReportedFailure {}
fn resource_report(report: ResourceReport) -> Value {
	let rejection = report.last_rejection.map(|error| match error {
		ResourceError::Overflow => json!({"kind":"overflow"}),
		ResourceError::Allocation => json!({"kind":"allocation"}),
		ResourceError::Limit {
			resource,
			requested,
			limit,
		} => json!({"kind":"limit","resource":resource,"requested":requested,"limit":limit}),
	});
	json!({"live_bytes":report.live_bytes,"peak_bytes":report.peak_bytes,
        "live_buffer_bytes":report.live_buffer_bytes,"live_planner_bytes_estimate":report.live_planner_bytes_estimate,
        "work_units":report.work_units,"reservations":report.reservations,
        "requested_peak_bytes":report.requested_peak_bytes,"requested_work_units":report.requested_work_units,
        "last_rejection":rejection,"limits":{"max_coefficients":report.limits.shapes.max_coefficients,
            "max_fft_len":report.limits.shapes.max_fft_len,"max_completion_grid":report.limits.shapes.max_completion_grid,
            "max_peak_bytes":report.limits.resources.max_peak_bytes,"max_work_units":report.limits.resources.max_work_units}})
}
type Result<T> = std::result::Result<T, Box<dyn Error>>;
struct Channel(BufReader<UnixStream>);
impl Channel {
	fn send(&mut self, event: &Value) -> Result<()> {
		serde_json::to_writer(self.0.get_mut(), event)?;
		self.0.get_mut().write_all(b"\n")?;
		self.0.get_mut().flush()?;
		let mut reply = String::new();
		self.0.read_line(&mut reply)?;
		if serde_json::from_str::<Value>(&reply)?
			.get("accepted")
			.and_then(Value::as_bool)
			!= Some(true)
		{
			return Err("controller rejected event or actual result".into());
		}
		Ok(())
	}
}
fn text<'a>(value: &'a Value, name: &str) -> Result<&'a str> {
	value
		.get(name)
		.and_then(Value::as_str)
		.ok_or_else(|| format!("missing string {name}").into())
}
fn number(value: &Value, name: &str) -> Result<usize> {
	Ok(usize::try_from(
		value
			.get(name)
			.and_then(Value::as_u64)
			.ok_or_else(|| format!("missing integer {name}"))?,
	)?)
}
fn read_array(file: &File, name: &str, count: usize, offset: i64) -> Result<Vec<Complex64>> {
	let dataset = file.dataset(name)?;
	if dataset.shape() != [count, 2] || !dataset.dtype()?.is::<f64>() {
		return Err("dataset shape/type mismatch".into());
	}
	if file.attr(&format!("{name}_offset"))?.read_scalar::<i64>()? != offset {
		return Err("invalid Laurent support".into());
	}
	let raw = dataset.read_raw::<f64>()?;
	if raw.iter().any(|v| !v.is_finite()) {
		return Err("nonfinite coefficient".into());
	}
	Ok(raw
		.as_chunks::<2>()
		.0
		.iter()
		.map(|&[real, imaginary]| Complex64::new(real, imaginary))
		.collect())
}
fn write_array(file: &File, name: &str, values: &[Complex64], offset: i64) -> Result<()> {
	let pairs: Vec<_> = values.iter().flat_map(|v| [v.re, v.im]).collect();
	file.new_dataset::<f64>()
		.shape([values.len(), 2])
		.create(name)?
		.write_raw(&pairs)?;
	file.new_attr::<i64>()
		.create(format!("{name}_offset").as_str())?
		.write_scalar(&offset)?;
	Ok(())
}
fn output_file(path: &Path) -> Result<File> {
	let file = File::create(path)?;
	for (name, value) in [
		("schema_version", "1.0"),
		("operation_version", "nlft-pair-v1"),
	] {
		file.new_attr::<VarLenUnicode>()
			.create(name)?
			.write_scalar(&VarLenUnicode::from_str(value)?)?;
	}
	Ok(file)
}
struct PhysicalPair {
	a: Vec<Complex64>,
	b: Vec<Complex64>,
}
enum Output {
	Pair(Accounted<PhysicalPair>),
	Reflections(Accounted<Vec<Complex64>>),
}
struct Adapter {
	operation: String,
	degree: i64,
	gamma: Vec<Complex64>,
	a: Vec<Complex64>,
	b: Vec<Complex64>,
	output_path: std::path::PathBuf,
	channel: Channel,
	warmup_index: usize,
	measurement_index: usize,
}
impl Adapter {
	fn call(&mut self, warmup: bool) -> Result<Duration> {
		let phase = if warmup { "warmup" } else { "measurement" };
		let index = if warmup {
			self.warmup_index
		} else {
			self.measurement_index
		};
		self.channel
			.send(&json!({"event":"executing", "phase":phase, "index":index}))?;
		// Operation-owned limits, allocations, plans, scratch and their cleanup are timed.
		// The owned result remains alive until external validation has acknowledged it.
		let start = Instant::now();
		let resources = OperationResources::from_limits(OperationLimits::million_degree());
		let result = if self.operation == "forward" {
			resources.charge_work(self.gamma.len())?;
			ForwardNlftWorkspace::new(
				FftBackend::Simd,
				resources.clone(),
				ExecutionPolicy::Sequential,
			)
			.forward(std::hint::black_box(&self.gamma))
			.map(|result| {
				Output::Pair(result.map(|mut pair| {
					pair.conjugate_complement.reverse();
					for value in &mut pair.conjugate_complement {
						*value = value.conj();
					}
					PhysicalPair {
						a: pair.conjugate_complement,
						b: pair.target,
					}
				}))
			})
		} else {
			InverseNlftWorkspace::new(
				FftBackend::Simd,
				resources.clone(),
				ExecutionPolicy::Sequential,
			)
			.inverse_physical(std::hint::black_box(&self.a), std::hint::black_box(&self.b))
			.map(Output::Reflections)
		};
		let elapsed = start.elapsed();
		let report = resources.report();
		let result = match result {
			Ok(result) => result,
			Err(error) => {
				self.channel.send(&json!({"event":"error","status":classify_error(&error),"detail":error.to_string(),
                    "duration_ns":u64::try_from(elapsed.as_nanos())?,"resource_report":resource_report(report)}))?;
				return Err(Box::new(ReportedFailure));
			}
		};
		{
			let file = output_file(&self.output_path)?;
			match &result {
				Output::Pair(pair) => {
					write_array(
						&file,
						"a",
						&pair.a,
						self.degree.checked_neg().ok_or("degree offset overflow")?,
					)?;
					write_array(&file, "b", &pair.b, 0)?;
				}
				Output::Reflections(gamma) => write_array(&file, "gamma", gamma, 0)?,
			}
		}
		self.channel
			.send(&json!({"event":"output", "phase":phase,"index":index,
            "duration_ns":u64::try_from(elapsed.as_nanos())?, "output_path":self.output_path,
            "kind":if self.operation=="forward" {"pair"} else {"reflections"},
            "resource_report":resource_report(report)}))?;
		let next = index.checked_add(1).ok_or("sample index overflow")?;
		if warmup {
			self.warmup_index = next;
		} else {
			self.measurement_index = next;
		}
		drop(result);
		Ok(elapsed)
	}
}
fn run(request: &Value, channel: Channel) -> Result<()> {
	if text(request, "schema_version")? != "1.0"
		|| text(request, "operation_version")? != "nlft-pair-v1"
		|| text(request, "implementation")? != "rust"
	{
		return Err("request identity mismatch".into());
	}
	if text(request, "backend")? != "rustfft-simd-sequential"
		|| request
			.get("resources")
			.and_then(|value| value.get("rust_profile"))
			.and_then(Value::as_str)
			!= Some("million-degree-inverse-forward-v1")
		|| request
			.get("contract")
			.and_then(Value::as_str)
			.unwrap_or("fresh-workspace")
			!= "fresh-workspace"
	{
		return Err("unsupported effective backend/resource/workspace contract".into());
	}
	let operation = text(request, "operation")?;
	if operation != "forward" && operation != "inverse" {
		return Err("unsupported operation".into());
	}
	let degree = number(request, "degree")?;
	let count = degree.checked_add(1).ok_or("coefficient count overflow")?;
	let offset = i64::try_from(degree)?;
	let file = File::open(text(request, "fixture_path")?)?;
	let mut adapter = Adapter {
		operation: operation.to_owned(),
		degree: offset,
		gamma: vec![],
		a: vec![],
		b: vec![],
		output_path: text(request, "output_path")?.into(),
		channel,
		warmup_index: 0,
		measurement_index: 0,
	};
	if operation == "forward" {
		adapter.gamma = read_array(&file, "gamma", count, 0)?;
	} else {
		adapter.a = read_array(
			&file,
			"a",
			count,
			offset.checked_neg().ok_or("degree offset overflow")?,
		)?;
		adapter.b = read_array(&file, "b", count, 0)?;
	}
	drop(file);
	let warmup = number(request, "warmup_calls")?;
	let samples = number(request, "sample_calls")?;
	if warmup == 0 || samples == 0 {
		return Err("positive warmup and sample counts required".into());
	}
	for _ in 1..warmup {
		adapter.call(true)?;
	}
	// The documented public Criterion duration floor yields its single mandatory
	// calibration call. Extra calibrated measurement calls are retained individually.
	let mut first_batch = true;
	let mut failure: Option<Box<dyn Error>> = None;
	let mut criterion = Criterion::default()
		.sample_size(samples.max(10))
		.warm_up_time(Duration::from_nanos(1))
		.measurement_time(Duration::from_millis(1))
		.without_plots()
		.configure_from_args();
	criterion.bench_function("matched_nlft", |bench| {
		bench.iter_custom(|iterations| {
			let warmup = first_batch;
			first_batch = false;
			let mut total = Duration::ZERO;
			for _ in 0..iterations {
				if failure.is_some() {
					break;
				}
				match adapter.call(warmup) {
					Ok(elapsed) => {
						if let Some(next) = total.checked_add(elapsed) {
							total = next;
						} else {
							failure = Some("duration overflow".into());
						}
					}
					Err(error) => failure = Some(error),
				}
			}
			total
		});
	});
	if let Some(error) = failure {
		return Err(error);
	}
	adapter.channel.send(&json!({"event":"finished"}))?;
	Ok(())
}
fn classify_error(error: &(dyn Error + 'static)) -> &'static str {
	if let Some(error) = error.downcast_ref::<quest_qsp::Error>() {
		return match error {
			quest_qsp::Error::Target(_) | quest_qsp::Error::Policy(_) => "invalid_input",
			quest_qsp::Error::Budget(_)
			| quest_qsp::Error::Numerics(
				quest_numerics::Error::Budget { .. }
				| quest_numerics::Error::Allocation
				| quest_numerics::Error::Resource(_),
			) => "admission_failure",
			_ => "numerical_failure",
		};
	}
	if error.downcast_ref::<std::io::Error>().is_some()
		|| error.downcast_ref::<hdf5_metno::Error>().is_some()
	{
		"infrastructure_failure"
	} else {
		"invalid_input"
	}
}
fn main() -> Result<()> {
	if std::env::args().any(|arg| arg == "--list") {
		if !std::env::args().any(|arg| arg == "--ignored") {
			println!("matched_nlft: benchmark");
		}
		return Ok(());
	}
	if std::env::args().any(|arg| {
		[
			"--warm-up-time",
			"--measurement-time",
			"--sample-size",
			"--quick",
			"--profile-time",
		]
		.iter()
		.any(|flag| arg.starts_with(flag))
	}) {
		return Err("campaign sampling cannot be overridden by Criterion CLI flags".into());
	}
	let request: Value =
		serde_json::from_slice(&std::fs::read(std::env::var("QUEST_NLFT_REQUEST")?)?)?;
	let stream = UnixStream::connect(text(&request, "socket_path")?)?;
	let mut channel = Channel(BufReader::new(stream.try_clone()?));
	channel.send(&json!({"event":"hello", "implementation":"rust", "backend":"rustfft-simd-sequential", "operation":text(&request,"operation")?,
        "input_fingerprint":text(&request,"input_fingerprint")?, "pid":std::process::id(),
        "versions":{"quest-qsp":env!("CARGO_PKG_VERSION"),"fft":"RustFFT SIMD","criterion":"0.8.2","workspace":"fresh"}}))?;
	if let Err(error) = run(&request, channel) {
		if error.downcast_ref::<ReportedFailure>().is_some() {
			return Err(error);
		}
		let mut channel = Channel(BufReader::new(stream));
		let detail = error.to_string();
		let status = classify_error(error.as_ref());
		let _ = channel.send(&json!({"event":"error","status":status,"detail":detail}));
		return Err(error);
	}
	Ok(())
}
