use std::env;

mod generate;

fn main() -> anyhow::Result<()> {
    run().map_err(anyhow::Error::from_boxed)
}

fn run() -> Result<(), generate::DynError> {
    let mut args = env::args().skip(1);
    match args.next().as_deref() {
        Some("generate-quest-bindings") => generate::run(args.any(|arg| arg == "--check")),
        Some("configure-native") => {
            let output = args.next().ok_or(
                "usage: cargo run -p xtask -- configure-native OUTPUT.json [--target TARGET]",
            )?;
            let target = match args.next().as_deref() {
                Some("--target") => Some(
                    args.next()
                        .ok_or("--target requires a Rust target triple")?,
                ),
                Some(argument) => {
                    return Err(format!("unexpected configure-native argument: {argument}").into());
                }
                None => None,
            };
            if args.next().is_some() {
                return Err("unexpected trailing configure-native arguments".into());
            }
            let configuration = quest_build::configure_native(&output, target.as_deref())?;
            eprintln!(
                "Recorded QuEST {} for {} in {output}; set QUEST_NATIVE_CONFIG to this file in all consumer builds.",
                configuration.version, configuration.target
            );
            Ok(())
        }
        Some(command) => Err(format!("unknown xtask command: {command}").into()),
        None => {
            eprintln!(
                "usage: cargo run -p xtask -- generate-quest-bindings [--check]\n       cargo run -p xtask -- configure-native OUTPUT.json [--target TARGET]"
            );
            Ok(())
        }
    }
}
