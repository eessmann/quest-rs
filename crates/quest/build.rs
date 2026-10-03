fn main() -> Result<(), quest_build::BuildError> {
	println!("cargo::rustc-check-cfg=cfg(quest_native_mpi)");
	if std::env::var("DEP_QUEST_MPI_ENABLED").as_deref() == Ok("1")
		&& std::env::var("DEP_QUEST_SUBCOMMUNICATORS_ENABLED").as_deref() == Ok("1")
	{
		println!("cargo::rustc-cfg=quest_native_mpi");
	}
	quest_build::emit_final_target_runtime_paths()
}
