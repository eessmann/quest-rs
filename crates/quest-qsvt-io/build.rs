fn main() -> Result<(), quest_build::BuildError> {
	if std::env::var_os("CARGO_FEATURE_HDF5").is_some() {
		quest_build::emit_serial_hdf5_runtime_paths()?;
	}
	Ok(())
}
