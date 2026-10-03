fn main() -> Result<(), quest_build::BuildError> {
	quest_build::emit_serial_hdf5_runtime_paths()?;
	Ok(())
}
