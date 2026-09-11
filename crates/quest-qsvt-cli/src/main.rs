#![forbid(unsafe_code)]
//! QSP/QSVT application entrypoint.
use clap::Parser;
fn main() -> color_eyre::Result<()> {
    color_eyre::install()?;
    let report = quest_qsvt_cli::Cli::parse().run()?;
    if report
        .get("emit_report")
        .and_then(serde_json::Value::as_bool)
        == Some(false)
    {
        return Ok(());
    }
    println!("{}", serde_json::to_string_pretty(&report)?);
    if report
        .get("failed")
        .and_then(serde_json::Value::as_u64)
        .is_some_and(|n| n != 0)
    {
        return Err(color_eyre::eyre::eyre!(
            "one or more requested numerical checks failed"
        ));
    }
    Ok(())
}
