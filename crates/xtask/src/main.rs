use std::env;

mod generate;

fn main() -> Result<(), generate::DynError> {
    let mut args = env::args().skip(1);
    match args.next().as_deref() {
        Some("generate-quest-bindings") => generate::run(args.any(|arg| arg == "--check")),
        Some(command) => Err(format!("unknown xtask command: {command}").into()),
        None => {
            eprintln!("usage: cargo run -p xtask -- generate-quest-bindings [--check]");
            Ok(())
        }
    }
}
