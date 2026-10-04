fn main() -> Result<(), quest_build::BuildError> {
	if std::env::var_os("CARGO_FEATURE_QUANTUM").is_some() {
		quest_build::discover_from_env()?.emit_runtime_paths()?;
	}
	Ok(())
}
