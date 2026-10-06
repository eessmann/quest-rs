#![allow(
	clippy::arithmetic_side_effects,
	reason = "Pre-admitted degree<=81 and fixed phase file/token bounds constrain all scalar products"
)]
//! Compile once, then import the exact immutable Symmetric interchange stream.
use super::{
	policy::{self, Result},
	runtime,
};
use quest::{
	collective::CollectiveEnvironment, qsvt::matching_lcu::collective::PreparedMatchingLcu,
};
use quest_qsvt::{
	NumericalPolicy,
	reciprocal::{ReciprocalPolynomial, SpectralBounds, SpectralEvidence},
	replay_transform::TransformSchedule,
};
use quest_qsvt_io::{IoPolicy, QspInput, read_qsp_json, write_qsp_json};
use serde_json::{Value, json};
use std::{path::Path, time::Instant};
fn create(path: &Path, bytes: &[u8]) -> Result<()> {
	if bytes.len() > 131_072 {
		return Err("frozen file bound".into());
	}
	let mut file = std::fs::OpenOptions::new()
		.write(true)
		.create_new(true)
		.open(path)?;
	std::io::Write::write_all(&mut file, bytes)?;
	Ok(())
}
#[allow(
	clippy::too_many_lines,
	reason = "Fixed compilation keeps candidate capacity, immutable exports and successor ownership admission together"
)]
pub fn compile(
	env: &CollectiveEnvironment<'_, '_>,
	source: &PreparedMatchingLcu<'_, '_, '_>,
	directory: &Path,
	construction: &Value,
) -> Result<Value> {
	let comm = env.communicator();
	if comm.size()? != 1 {
		return Err("compilation requires exactly one rank".into());
	}
	let guard = env.reserve_external_bytes(policy::COMPILE_BYTES)?;
	let start = Instant::now();
	let result = runtime::local(comm, || {
		let descriptor = source.plan().descriptor();
		let alpha = descriptor.normalization;
		let (lo, hi) = policy::spectrum()?;
		let spectrum=SpectralBounds::new(lo,hi,SpectralEvidence::Analytic{description:"Nominal dyadic A=5/8 I+i/8 X; A†A=13/32 I; directed sqrt(26)/8 interval; implemented source/PREP error remains unknown".into()})?;
		let polynomial = ReciprocalPolynomial::geometric(
			&spectrum,
			alpha,
			policy::TOLERANCE,
			policy::MAX_DEGREE,
			NumericalPolicy {
				max_bytes: 32 * 1024 * 1024,
			},
		)?;
		let candidate = polynomial.synthesize(quest_qsp::Policy {
			algorithm: quest_qsp::SynthesisAlgorithm::InverseNlftDivideConquer,
			response_tolerance: 1e-11,
			contractivity_margin: 1e-12,
			max_completion_grid: 16_384,
			backend: quest_qsp::FftBackend::Scalar,
			limits: quest_numerics::Limits {
				max_len: 16_384,
				max_bytes: 32 * 1024 * 1024,
				max_work: 1_000_000_000,
			},
		})?;
		let diagnostics = [
			polynomial.scale(),
			polynomial.error_bound(),
			polynomial.coefficient_rounding_bound(),
			candidate.completion_residual(),
		];
		if diagnostics.iter().any(|x| !x.is_finite() || *x < 0.)
			|| candidate
				.reconstruction_residual()
				.is_some_and(|x| !x.is_finite() || x < 0.)
			|| polynomial.scale() == 0.
			|| polynomial.error_bound() > policy::TOLERANCE
			|| candidate.completion_grid() > 16_384
		{
			return Err("finite candidate diagnostics/caps".into());
		}
		let phases = candidate.phase_sequence();
		let coefficients: Vec<f64> = polynomial
			.target()
			.coefficients()
			.iter()
			.map(|c| c.re)
			.collect();
		if coefficients.is_empty()
			|| coefficients.len() > 82
			|| !coefficients.len().is_multiple_of(2)
			|| coefficients.capacity() * 8 > 4096
			|| candidate.phases().len() != coefficients.len()
			|| polynomial
				.target()
				.coefficients()
				.iter()
				.any(|c| c.im != 0.)
		{
			return Err("fixed odd degree/coefficient/phase capacity".into());
		}
		let degree = coefficients.len().checked_sub(1).ok_or("degree")?;
		let phase_text = write_qsp_json(&QspInput::Symmetric(phases))?;
		if phase_text.len() > 65_536 || phase_text.capacity() > 131_072 {
			return Err("phase serialization capacity".into());
		}
		let coeff_bytes: Vec<u8> = coefficients
			.iter()
			.flat_map(|c| c.to_bits().to_le_bytes())
			.collect();
		if coeff_bytes.capacity() > 4096 {
			return Err("coefficient hash capacity".into());
		}
		let p = policy::chebyshev(&coefficients, lo.midpoint(hi) / alpha)?;
		let metadata = json!({"schema":"quest-persisted-weighted-phase-freeze-v1","dimension":32,"degree":degree,"alpha":alpha,"scale":polynomial.scale(),"error_bound":polynomial.error_bound(),"coefficient_rounding_bound":polynomial.coefficient_rounding_bound(),"physical_rescaling":polynomial.physical_rescaling(1.)?,"spectrum_lower":lo,"spectrum_upper":hi,"spectral_evidence":"nominal analytic dyadic spectrum; no uniform implemented-source certificate","source_identity":descriptor.source_identity,"construction_identity":descriptor.construction_identity,"coefficients":coefficients,"coefficient_sha256":runtime::hash(&coeff_bytes),"phase_sha256":runtime::hash(phase_text.as_bytes()),"phase_format":"existing QspInput::Symmetric JSON","synthesis":{"algorithm":"InverseNlftDivideConquer","backend":"Scalar","max_completion_grid":16_384,"response_tolerance":1e-11,"contractivity_margin":1e-12,"limits":{"length":16_384,"bytes":33_554_432,"work":1_000_000_000},"completion_grid":candidate.completion_grid(),"completion_residual":candidate.completion_residual(),"reconstruction_residual":candidate.reconstruction_residual()},"finite_polynomial_success":p*p,"reciprocal_ideal_success":(alpha*polynomial.scale()/lo.midpoint(hi)).powi(2),"reciprocal_tolerance":1e-4,"max_degree":81,"constructor_retained_polynomial_bytes":polynomial.retained_bytes()?,"certificate":null,"construction":construction});
		let bytes = serde_json::to_vec(&metadata)?;
		runtime::metadata_scan(&bytes)?;
		if runtime::json_payload(&metadata)? > 524_288 {
			return Err("metadata owner capacity".into());
		}
		if bytes.capacity() > 32_768 {
			return Err("metadata serialization capacity".into());
		}
		create(&directory.join("phases.json"), phase_text.as_bytes())?;
		create(&directory.join("freeze.json"), &bytes)?;
		Ok(metadata)
	})?;
	let stage = runtime::finish(comm, start)?;
	let mut output = json!({"status":"completed","freeze":null,"compilation":stage,"compilation_envelope_bytes":policy::COMPILE_BYTES,"candidate_provenance":"binary64 diagnostics retained; imported execution is not an independent certificate"});
	output
		.as_object_mut()
		.ok_or("compile output object")?
		.insert("freeze".into(), result);
	drop(stage);
	let admission = runtime::local(comm, || {
		runtime::admit_json_overlap(
			&[&output, construction],
			&[1, 1],
			262_144 + 65_536,
			policy::READOUT_BYTES,
		)
	});
	if let Err(e) = admission {
		drop(output);
		let rejected = runtime::local(comm, || {
			Ok(
				json!({"status":"rejected","phase":"compilation receipt handoff","error":e.to_string(),"compiled_phase_files_exist":true}),
			)
		})?;
		drop(guard);
		return Ok(rejected);
	}
	drop(guard);
	Ok(output)
}
pub struct Imported {
	pub metadata: Value,
	pub schedule: TransformSchedule,
	pub rescaling: f64,
	pub finite_success: f64,
	pub ideal_success: f64,
}
fn f(v: &Value, key: &str) -> Result<f64> {
	let x = v.get(key).and_then(Value::as_f64).ok_or("freeze scalar")?;
	if !x.is_finite() {
		return Err("nonfinite freeze scalar".into());
	}
	Ok(x)
}
#[allow(
	clippy::too_many_lines,
	reason = "Frozen field/identity admission stays beside shared phase import"
)]
pub fn import(source: &PreparedMatchingLcu<'_, '_, '_>, directory: &Path) -> Result<Imported> {
	let metadata = runtime::canonical(&directory.join("freeze.json"))?;
	let expected = [
		"schema",
		"dimension",
		"degree",
		"alpha",
		"scale",
		"error_bound",
		"coefficient_rounding_bound",
		"physical_rescaling",
		"spectrum_lower",
		"spectrum_upper",
		"spectral_evidence",
		"source_identity",
		"construction_identity",
		"coefficients",
		"coefficient_sha256",
		"phase_sha256",
		"phase_format",
		"synthesis",
		"finite_polynomial_success",
		"reciprocal_ideal_success",
		"reciprocal_tolerance",
		"max_degree",
		"constructor_retained_polynomial_bytes",
		"certificate",
		"construction",
	];
	let fields = metadata.as_object().ok_or("freeze object")?;
	if fields.len() != expected.len() || expected.iter().any(|key| !fields.contains_key(*key)) {
		return Err("freeze known fields".into());
	}
	if metadata.get("phase_format").and_then(Value::as_str)
		!= Some("existing QspInput::Symmetric JSON")
	{
		return Err("freeze phase format".into());
	}

	if metadata.get("schema").and_then(Value::as_str)
		!= Some("quest-persisted-weighted-phase-freeze-v1")
		|| metadata.get("dimension").and_then(Value::as_u64) != Some(32)
		|| metadata.get("max_degree").and_then(Value::as_u64) != Some(81)
		|| f(&metadata, "reciprocal_tolerance")?.to_bits() != 1e-4_f64.to_bits()
	{
		return Err("fixed phase metadata contract".into());
	}
	let synthesis = metadata.get("synthesis").ok_or("candidate provenance")?;
	let grid = synthesis
		.get("completion_grid")
		.and_then(Value::as_u64)
		.ok_or("candidate grid")?;
	let polynomial_bytes = metadata
		.get("constructor_retained_polynomial_bytes")
		.and_then(Value::as_u64)
		.ok_or("polynomial owner provenance")?;
	if grid == 0 || grid > 16_384 || polynomial_bytes == 0 || polynomial_bytes > 33_554_432 {
		return Err("candidate diagnostic caps".into());
	}
	let descriptor = source.plan().descriptor();
	if f(&metadata, "alpha")?.to_bits() != descriptor.normalization.to_bits()
		|| metadata.get("source_identity").and_then(Value::as_u64)
			!= Some(descriptor.source_identity)
		|| metadata
			.get("construction_identity")
			.and_then(Value::as_u64)
			!= Some(descriptor.construction_identity)
	{
		return Err("phase A/U descriptor differs".into());
	}
	let phase_bytes = runtime::file(&directory.join("phases.json"), 65_536)?;
	if metadata.get("phase_sha256").and_then(Value::as_str)
		!= Some(runtime::hash(&phase_bytes).as_str())
	{
		return Err("phase byte identity".into());
	}
	let phase_text = std::str::from_utf8(&phase_bytes)?;
	let QspInput::Symmetric(sequence) = read_qsp_json(
		phase_text,
		IoPolicy {
			max_bytes: policy::LOAD_BYTES,
			max_coefficients: 82,
			max_dimension: 32,
		},
	)?
	else {
		return Err("frozen symmetric format".into());
	};
	let degree = usize::try_from(
		metadata
			.get("degree")
			.and_then(Value::as_u64)
			.ok_or("degree")?,
	)?;
	if degree == 0 || degree > 81 || degree % 2 == 0 || sequence.values().len() != degree + 1 {
		return Err("frozen degree bound".into());
	}
	let values = metadata
		.get("coefficients")
		.and_then(Value::as_array)
		.ok_or("coefficients")?;
	if values.len() != degree + 1 {
		return Err("coefficient count".into());
	}
	let coefficients: Vec<f64> = values
		.iter()
		.map(|c| {
			c.as_f64()
				.filter(|c| c.is_finite())
				.ok_or("finite coefficient")
		})
		.collect::<std::result::Result<_, _>>()?;
	let bytes: Vec<u8> = coefficients
		.iter()
		.flat_map(|c| c.to_bits().to_le_bytes())
		.collect();
	if metadata.get("coefficient_sha256").and_then(Value::as_str)
		!= Some(runtime::hash(&bytes).as_str())
	{
		return Err("coefficient byte identity".into());
	}
	let (lo, hi) = policy::spectrum()?;
	if f(&metadata, "spectrum_lower")?.to_bits() != lo.to_bits()
		|| f(&metadata, "spectrum_upper")?.to_bits() != hi.to_bits()
	{
		return Err("nominal spectrum identity".into());
	}
	let scale = f(&metadata, "scale")?;
	let alpha = descriptor.normalization;
	if scale <= 0.
		|| f(&metadata, "error_bound")? < 0.
		|| f(&metadata, "error_bound")? > 1e-4
		|| f(&metadata, "coefficient_rounding_bound")? < 0.
	{
		return Err("fixed reciprocal premise".into());
	}
	let rescaling = 1. / (alpha * scale);
	if f(&metadata, "physical_rescaling")?.to_bits() != rescaling.to_bits() {
		return Err("physical rescaling identity".into());
	}
	let response = policy::chebyshev(&coefficients, lo.midpoint(hi) / alpha)?;
	let ideal_success = (alpha * scale / (lo.midpoint(hi))).powi(2);
	if f(&metadata, "finite_polynomial_success")?.to_bits() != (response * response).to_bits()
		|| f(&metadata, "reciprocal_ideal_success")?.to_bits() != ideal_success.to_bits()
	{
		return Err("independent success target identity".into());
	}
	let synthesis = metadata.get("synthesis").ok_or("synthesis provenance")?;
	if synthesis.get("algorithm").and_then(Value::as_str) != Some("InverseNlftDivideConquer")
		|| synthesis.get("backend").and_then(Value::as_str) != Some("Scalar")
		|| synthesis.get("max_completion_grid").and_then(Value::as_u64) != Some(16_384)
		|| f(synthesis, "response_tolerance")?.to_bits() != 1e-11_f64.to_bits()
		|| f(synthesis, "contractivity_margin")?.to_bits() != 1e-12_f64.to_bits()
		|| metadata.get("certificate") != Some(&Value::Null)
	{
		return Err("frozen synthesis declarations".into());
	}
	if f(synthesis, "completion_residual")? < 0. {
		return Err("candidate diagnostics".into());
	}
	if synthesis.get("reconstruction_residual").is_none() {
		return Err("missing reconstruction provenance".into());
	}
	if synthesis.get("reconstruction_residual") != Some(&Value::Null)
		&& f(synthesis, "reconstruction_residual")? < 0.
	{
		return Err("reconstruction diagnostic".into());
	}
	let limits = synthesis.get("limits").ok_or("candidate limits")?;
	if limits.get("length").and_then(Value::as_u64) != Some(16_384)
		|| limits.get("bytes").and_then(Value::as_u64) != Some(33_554_432)
		|| limits.get("work").and_then(Value::as_u64) != Some(1_000_000_000)
	{
		return Err("candidate limits".into());
	}
	let schedule = TransformSchedule::from_phase_sequence(
		descriptor.clone(),
		sequence,
		NumericalPolicy {
			max_bytes: policy::LOAD_BYTES,
		},
	)?;
	Ok(Imported {
		metadata,
		schedule,
		rescaling,
		finite_success: response * response,
		ideal_success,
	})
}
