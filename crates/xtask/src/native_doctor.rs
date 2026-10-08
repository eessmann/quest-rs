//! Diagnostic receipt. Passing a probe is narrower than a downstream build.
use crate::generate::DynError;
use serde::Serialize;
use serde_json::{Value, json};
use std::{
	env,
	path::{Path, PathBuf},
};

#[derive(Serialize)]
struct DiagnosticStage {
	stage: &'static str,
	status: &'static str,
	details: Value,
}

#[derive(Serialize)]
struct Receipt {
	schema_version: u32,
	stages: Vec<DiagnosticStage>,
}

fn build_receipt() -> Result<(Receipt, Redactor), DynError> {
	let workspace = crate::generate::find_workspace_root()?;
	let work = crate::tooling::native_work_directory(&workspace, "xtask-native-doctor-")?;
	let mut receipt = Receipt {
		schema_version: 1,
		stages: Vec::new(),
	};
	let mut redactor = Redactor::from_environment(&workspace, work.path());
	let context = quest_build::NativeBuildContext::for_tooling(work.path(), None);
	match &context {
		Ok(context) => {
			receipt.passed(
				"native_context",
				json!({"host": context.host(), "target": context.target(), "profile": context.profile()}),
			);
			match context.discover() {
				Ok(package) => {
					redactor.add(package.prefix(), "$QUEST_PREFIX");
					receipt.passed("native_discovery", json!({
						"version": package.version(),
						"prefix": package.prefix(),
						"library": package.library(),
						"compiler": {"invocation": package.compiler_invocation(), "arguments": package.compiler_arguments(), "identity": package.compiler(), "id": package.compiler_id(), "version": package.compiler_version()},
						"capabilities": {"mpi": package.mpi_enabled(), "subcommunicators": package.subcommunicators_enabled(), "openmp": package.openmp_enabled(), "gpu": package.gpu_enabled(), "cuquantum": package.cuquantum_enabled()},
						"headers": {"include_dirs": package.headers().include_dirs, "system_include_dirs": package.headers().system_include_dirs, "implicit_include_dirs": package.headers().implicit_include_dirs, "definitions": package.headers().definitions, "frontend_flags": package.headers().frontend_flags, "sysroot": package.headers().sysroot},
						"runtime_library_dirs": package.runtime_library_dirs(),
						"evidence": "CMake imported target and compiled ABI admission; consumer runtime not checked"
					}));
					match crate::generate::clang::parser_diagnostics(&package) {
						Ok(details) => receipt.passed("binding_parser", details),
						Err(error) => receipt.failed("binding_parser", error.to_string()),
					}
				}
				Err(error) => {
					receipt.failed("native_discovery", error.to_string());
					receipt.skipped("binding_parser", "native discovery failed");
				}
			}
		}
		Err(error) => {
			receipt.failed("native_context", error.to_string());
			receipt.skipped("native_discovery", "native context is invalid");
			receipt.skipped("binding_parser", "native context is invalid");
		}
	}
	for (stage, kind) in [("mpi_selection", "mpi"), ("serial_hdf5_selection", "hdf5")] {
		match probe(kind, context.as_ref().ok()) {
			Ok(details) => receipt.passed(stage, details),
			Err(error) => receipt.failed(stage, error.to_string()),
		}
	}
	receipt.passed("selectors", json!({
		"compiler": selections(&["CC", "CXX", "CFLAGS", "CXXFLAGS", "CMAKE_TOOLCHAIN_FILE", "CMAKE_GENERATOR", "CPATH", "CPLUS_INCLUDE_PATH", "LIBRARY_PATH"]),
		"mpi": selections(&["MPICC", "MPI_PKG_CONFIG", "CRAY_MPICH_DIR"]),
		"hdf5": selections(&["HDF5_DIR", "HDF5_VERSION", "PKG_CONFIG_PATH", "PKG_CONFIG_LIBDIR"]),
		"parser": selections(&["LIBCLANG_PATH", "CLANG", "BINDGEN_EXTRA_CLANG_ARGS"]),
		"loader_environment": env::vars_os().filter_map(|(name, _)| name.to_str().filter(|name| name.starts_with("LD_") || name.starts_with("DYLD_")).map(str::to_owned)).collect::<Vec<_>>(),
		"mpi_parser_requirement": "Ordinary rsmpi builds use bindgen and require libclang; binding regeneration also requires a matching clang driver."
	}));
	Ok((receipt, redactor))
}

pub fn run(json_output: bool) -> Result<(), DynError> {
	let (receipt, redactor) = match build_receipt() {
		Ok(result) => result,
		Err(error) => {
			let mut receipt = Receipt {
				schema_version: 1,
				stages: Vec::new(),
			};
			receipt.failed("tooling_context", error.to_string());
			(
				receipt,
				Redactor::from_environment(Path::new("."), Path::new(".")),
			)
		}
	};
	let failed = receipt.stages.iter().any(|stage| stage.status == "failed");
	let mut value = serde_json::to_value(receipt)?;
	redactor.redact(&mut value);
	if json_output {
		println!("{}", serde_json::to_string_pretty(&value)?);
	} else {
		for stage in value
			.get("stages")
			.and_then(Value::as_array)
			.ok_or("invalid receipt")?
		{
			println!(
				"{}: {}\n{}",
				stage
					.get("stage")
					.and_then(Value::as_str)
					.unwrap_or_default(),
				stage
					.get("status")
					.and_then(Value::as_str)
					.unwrap_or_default(),
				serde_json::to_string_pretty(stage.get("details").ok_or("missing stage details")?)?
			);
		}
	}
	if failed {
		Err("native-doctor found failed stages; inspect the receipt".into())
	} else {
		Ok(())
	}
}

const PROBE_MARKER: &str = "quest-native-doctor-result:";

// Some dependency probes emit Cargo directives even outside a build script.
// A child process keeps those diagnostics out of the public JSON document.
fn probe(kind: &str, context: Option<&quest_build::NativeBuildContext>) -> Result<Value, DynError> {
	let mut command = std::process::Command::new(env::current_exe()?);
	if let Some(context) = context {
		context.apply_environment(&mut command);
	}
	let output = command.args(["__native-doctor-probe", kind]).output()?;
	if !output.status.success() {
		return Err(format!(
			"{kind} probe failed: {}",
			String::from_utf8_lossy(&output.stderr)
		)
		.into());
	}
	parse_probe_output(&output.stdout)
}

fn parse_probe_output(output: &[u8]) -> Result<Value, DynError> {
	let text = std::str::from_utf8(output)?;
	let mut results = text
		.lines()
		.filter_map(|line| line.strip_prefix(PROBE_MARKER));
	let value: Value = serde_json::from_str(results.next().ok_or("probe result missing")?)?;
	if results.next().is_some() {
		return Err("ambiguous doctor probe results".into());
	}
	if let Some(error) = value.get("error").and_then(Value::as_str) {
		return Err(error.to_owned().into());
	}
	value
		.get("ok")
		.cloned()
		.ok_or_else(|| "invalid doctor probe result".into())
}

pub fn probe_worker(kind: &str) -> Result<(), DynError> {
	let result = match kind {
		"mpi" => quest_build::probe_rsmpi().map(|selection| {
			let source = match selection.source() {
				quest_build::MpiSource::PkgConfig(package) => json!({"kind": "pkg_config", "package": package}),
				quest_build::MpiSource::CrayPkgConfig(package) => json!({"kind": "cray_pkg_config", "package": package}),
				quest_build::MpiSource::CompilerWrapper(wrapper) => json!({"kind": "compiler_wrapper", "wrapper": wrapper}),
				quest_build::MpiSource::FallbackPkgConfig => json!({"kind": "fallback_pkg_config"}),
			};
			json!({"source": source, "wrapper": selection.wrapper(), "include_dirs": selection.include_dirs(), "library_dirs": selection.library_dirs(), "libraries": selection.libraries(), "version": selection.version(), "evidence": "rsmpi discovery recipe; Cargo MPI builds additionally verify the loaded library and ABI against QuEST"})
		}),
		"hdf5" => quest_build::discover_serial_hdf5().map(|selection| json!({
			"source": selection.source(), "header": selection.header(), "include_dirs": selection.include_dirs(),
			"library_dirs": selection.library_dirs(), "version": selection.version(),
			"evidence": "HDF5 discovery and serial header admission; hdf5-metno-sys additionally verifies header/library runtime version agreement during normal builds"
		})),
		_ => return Err(format!("unknown native doctor probe: {kind}").into()),
	};
	let value = match result {
		Ok(value) => json!({"ok": value}),
		Err(error) => json!({"error": error.to_string()}),
	};
	println!("{PROBE_MARKER}{}", serde_json::to_string(&value)?);
	Ok(())
}

fn selections(names: &[&str]) -> Value {
	Value::Object(
		names
			.iter()
			.map(|name| {
				(
					(*name).into(),
					env::var_os(name).map_or(Value::Null, |value| {
						Value::String(value.to_string_lossy().into_owned())
					}),
				)
			})
			.collect(),
	)
}

impl Receipt {
	fn skipped(&mut self, stage: &'static str, reason: &'static str) {
		self.stages.push(DiagnosticStage {
			stage,
			status: "skipped",
			details: json!({"reason": reason}),
		});
	}
	fn passed(&mut self, stage: &'static str, details: Value) {
		self.stages.push(DiagnosticStage {
			stage,
			status: "passed",
			details,
		});
	}
	fn failed(&mut self, stage: &'static str, error: String) {
		self.stages.push(DiagnosticStage {
			stage,
			status: "failed",
			details: json!({"error": Value::String(error)}),
		});
	}
}

#[derive(Default)]
struct Redactor {
	prefixes: Vec<(String, String)>,
}
impl Redactor {
	fn from_environment(workspace: &Path, work: &Path) -> Self {
		let mut result = Self::default();
		for (variable, label) in [
			("HOME", "$HOME"),
			("CARGO_TARGET_DIR", "$CARGO_TARGET_DIR"),
			("HDF5_DIR", "$HDF5_PREFIX"),
			("CRAY_MPICH_DIR", "$MPI_PREFIX"),
			("QUEST_ROOT", "$QUEST_PREFIX"),
		] {
			if let Some(value) = env::var_os(variable) {
				result.add(&PathBuf::from(value), label);
			}
		}
		result.add(workspace, "$WORKSPACE");
		result.add(work, "$PROBE_DIR");
		result
	}
	fn add(&mut self, path: &Path, label: &str) {
		for path in std::iter::once(path.to_owned()).chain(path.canonicalize().ok()) {
			if path.is_absolute() && path.parent().is_some() {
				self.prefixes
					.push((path.to_string_lossy().into_owned(), label.into()));
			}
		}
		self.prefixes
			.sort_by_key(|(prefix, _)| std::cmp::Reverse(prefix.len()));
	}
	fn redact(&self, value: &mut Value) {
		match value {
			Value::String(text) => {
				for (prefix, label) in &self.prefixes {
					let mut parts = text.split(prefix);
					let mut replaced = parts.next().unwrap_or_default().to_owned();
					for suffix in parts {
						// Preserve other user names which merely share a prefix.
						let boundary = suffix.chars().next().is_none_or(|next| {
							next == '/'
								|| next.is_whitespace()
								|| matches!(next, ':' | ';' | ',' | '\'' | '"' | ')' | ']')
						});
						replaced.push_str(if boundary { label } else { prefix });
						replaced.push_str(suffix);
					}
					*text = replaced;
				}
			}
			Value::Array(values) => values.iter_mut().for_each(|value| self.redact(value)),
			Value::Object(values) => values.values_mut().for_each(|value| self.redact(value)),
			_ => {}
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use googletest::prelude::*;
	#[gtest]
	fn probe_receipt_ignores_cargo_directives_and_preserves_failures() -> googletest::Result<()> {
		let value = parse_probe_output(b"cargo:rerun-if-env-changed=MPICC\nquest-native-doctor-result:{\"ok\":{\"libraries\":[\"mpi\"]}}\n").or_fail()?;
		expect_eq!(value["libraries"][0], "mpi");
		let error =
			parse_probe_output(b"quest-native-doctor-result:{\"error\":\"ABI mismatch\"}\n")
				.expect_err("failed probe must remain a failure");
		expect_that!(error.to_string(), contains_substring("ABI mismatch"));
		expect_true!(parse_probe_output(b"cargo:rerun-if-env-changed=MPICC\n").is_err());
		Ok(())
	}

	#[gtest]
	fn receipt_redacts_nested_personal_paths_and_diagnostics() {
		let mut redactor = Redactor::default();
		redactor.add(Path::new("/private/home/alice"), "$HOME");
		redactor.add(Path::new("/private/home/alice/project"), "$WORKSPACE");
		let mut value = json!({"args": ["-I/private/home/alice/project/include"], "error": "failed /private/home/alice/sdk/compiler", "similar": "/private/home/alice2/file"});
		redactor.redact(&mut value);
		expect_eq!(value["args"][0], "-I$WORKSPACE/include");
		expect_eq!(value["error"], "failed $HOME/sdk/compiler");
		expect_eq!(value["similar"], "/private/home/alice2/file");
	}
}
