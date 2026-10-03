fn main() -> Result<(), quest_build::BuildError> {
	println!("cargo::rustc-check-cfg=cfg(quest_native_mpi)");
	if std::env::var_os("CARGO_FEATURE_NATIVE").is_some() {
		let native = quest_build::discover_from_env()?;
		native.emit_native_capability_cfg();
		native.emit_runtime_paths()?;
	}
	if std::env::var_os("CARGO_FEATURE_HDF5").is_some() {
		quest_build::emit_serial_hdf5_runtime_paths()?;
	}
	Ok(())
}
