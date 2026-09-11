#![cfg(feature = "native")]
use googletest::prelude::*;
use num_complex::Complex64 as C;
use quest_qsvt_io::{
    IoPolicy,
    hdf5::{StoredBlockEncoding, read_state_vector, write_block_encoding, write_state_vector},
};
use std::{
    ops::{Div, Neg, Sub},
    path::Path,
};
static FIXTURES: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn command(args: &[&str]) -> googletest::Result<serde_json::Value> {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_quest-qsvt-cli"))
        .args(args)
        .output()?;
    expect_true!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout)?;
    // Native backends may print an initialization banner before the report.
    let start = text
        .find('{')
        .ok_or_else(|| std::io::Error::other("missing JSON report"))?;
    Ok(serde_json::from_str(
        text.get(start..)
            .ok_or_else(|| std::io::Error::other("report"))?,
    )?)
}
fn path(value: &Path) -> googletest::Result<&str> {
    Ok(value
        .to_str()
        .ok_or_else(|| std::io::Error::other("path"))?)
}
fn phases(path: &Path) -> googletest::Result<()> {
    std::fs::write(
        path,
        format!(
            r#"{{"angles":[{0},{0}],"convention":"pyqsp-wx-symmetric"}}"#,
            std::f64::consts::FRAC_PI_4
        ),
    )?;
    Ok(())
}
#[gtest]
fn native_embedded_and_overlap_preserve_complex_phase_and_mass() -> googletest::Result<()> {
    let _guard = FIXTURES
        .lock()
        .map_err(|_| std::io::Error::other("fixture lock"))?;
    let dir = tempfile::tempdir()?;
    let encoding_path = dir.path().join("encoding.h5");
    let qsp = dir.path().join("phases.json");
    let input = dir.path().join("input.h5");
    let reference = dir.path().join("reference.h5");
    let output = dir.path().join("output.h5");
    let trace = dir.path().join("trace.json");
    let z = C::new(0.3, 0.4);
    let u = faer::Mat::from_fn(2, 2, |r, c| {
        if r == c {
            if r == 0 { z } else { z.conj().neg() }
        } else {
            C::new(0.75_f64.sqrt(), 0.0)
        }
    });
    let basis = faer::Mat::from_fn(2, 1, |r, _| C::new(if r == 0 { 1.0 } else { 0.0 }, 0.0));
    let block = StoredBlockEncoding::builder(u, basis.clone(), basis)
        .metadata(1.0, [1, 1], [1, 1])
        .build(IoPolicy::default())?;
    write_block_encoding(&encoding_path, &block, IoPolicy::default())?;
    write_state_vector(&input, &[C::new(1.0, 0.0)], IoPolicy::default())?;
    write_state_vector(&reference, &[C::new(0.0, 1.0)], IoPolicy::default())?;
    phases(&qsp)?;
    let report = command(&[
        "embedded",
        "--encoding",
        path(&encoding_path)?,
        "--qsp",
        path(&qsp)?,
        "--route",
        "standard",
        "--input-state",
        path(&input)?,
        "--output-state",
        path(&output)?,
        "--trace",
        path(&trace)?,
    ])?;
    // Dense imported projectors: 9 circuit +4 projection +3 probability calls.
    expect_that!(
        report
            .pointer("/mass/native_dispatches/total")
            .and_then(serde_json::Value::as_u64),
        some(eq(16))
    );
    expect_that!(
        report
            .pointer("/mass/native_dispatches/scope")
            .and_then(serde_json::Value::as_str),
        some(eq("successful_run_per_rank"))
    );
    let amplitudes = read_state_vector(&output, IoPolicy::default())?;
    expect_that!(
        amplitudes
            .first()
            .ok_or_else(|| std::io::Error::other("amplitudes"))?
            .re,
        near(z.re, 1e-12)
    );
    expect_that!(
        amplitudes
            .first()
            .ok_or_else(|| std::io::Error::other("amplitudes"))?
            .im,
        near(z.im, 1e-12)
    );
    expect_that!(
        report
            .pointer("/mass/retained")
            .unwrap_or(&serde_json::Value::Null)
            .as_f64()
            .unwrap_or(-1.0),
        near(0.25, 1e-12)
    );
    expect_true!(trace.exists());
    let report = command(&[
        "overlap",
        "--encoding",
        path(&encoding_path)?,
        "--qsp",
        path(&qsp)?,
        "--route",
        "standard",
        "--input-state",
        path(&input)?,
        "--reference-state",
        path(&reference)?,
    ])?;
    // Controlled projections add4 projectors +6 clone/add calls; readout7.
    expect_that!(
        report
            .pointer("/mass/native_dispatches/total")
            .and_then(serde_json::Value::as_u64),
        some(eq(30))
    );
    expect_that!(
        report
            .pointer("/overlap/0")
            .unwrap_or(&serde_json::Value::Null)
            .as_f64()
            .unwrap_or(-1.0),
        near(0.4, 1e-12)
    );
    expect_that!(
        report
            .pointer("/overlap/1")
            .unwrap_or(&serde_json::Value::Null)
            .as_f64()
            .unwrap_or(-1.0),
        near(-0.3, 1e-12)
    );
    Ok(())
}
#[gtest]
fn complex_solve_recovers_physical_norm_and_separately_checks_residual() -> googletest::Result<()> {
    let _guard = FIXTURES
        .lock()
        .map_err(|_| std::io::Error::other("fixture lock"))?;
    let dir = tempfile::tempdir()?;
    let matrix = dir.path().join("matrix.h5");
    let rhs = dir.path().join("rhs.h5");
    let qsp = dir.path().join("phases.json");
    let output = dir.path().join("physical.h5");
    let normalized = dir.path().join("normalized.h5");
    let diagonal = [C::new(3.0, 4.0), C::new(-4.0, 3.0)];
    let b = [C::new(1.0, 2.0), C::new(3.0, -1.0)];
    {
        let file = hdf5_metno::File::create(&matrix)?;
        let group = file.create_group("matrix")?;
        group
            .new_attr::<u64>()
            .shape(2)
            .create("shape")?
            .write_raw(&[2, 2])?;
        group
            .new_dataset::<C>()
            .shape([2, 2])
            .create("dense")?
            .write_raw(&[diagonal[0], C::new(0.0, 0.0), C::new(0.0, 0.0), diagonal[1]])?;
    }
    write_state_vector(&rhs, &b, IoPolicy::default())?;
    phases(&qsp)?;
    let report = command(&[
        "solve",
        "--matrix",
        path(&matrix)?,
        "--rhs",
        path(&rhs)?,
        "--qsp",
        path(&qsp)?,
        "--route",
        "standard",
        "--reciprocal-scale",
        "1.0",
        "--output-state",
        path(&output)?,
        "--normalized-output-state",
        path(&normalized)?,
        "--residual-tolerance",
        "1e-10",
    ])?;
    let physical = read_state_vector(output, IoPolicy::default())?;
    for ((actual, b), diagonal) in physical.iter().zip(b).zip(diagonal) {
        expect_that!(actual.sub(b.div(diagonal)).norm(), near(0.0, 1e-12));
    }
    expect_that!(
        report
            .pointer("/relative_physical_residual")
            .unwrap_or(&serde_json::Value::Null)
            .as_f64()
            .unwrap_or(-1.0),
        near(0.0, 1e-12)
    );
    let normalized = read_state_vector(normalized, IoPolicy::default())?;
    expect_that!(
        normalized.iter().map(C::norm_sqr).sum::<f64>(),
        near(1.0, 1e-12)
    );
    expect_that!(
        physical.iter().map(C::norm_sqr).sum::<f64>(),
        near(0.6, 1e-12)
    );
    Ok(())
}

#[gtest]
fn solve_rejects_rank_deficiency_before_publishing_output() -> googletest::Result<()> {
    let _guard = FIXTURES
        .lock()
        .map_err(|_| std::io::Error::other("fixture lock"))?;
    let dir = tempfile::tempdir()?;
    let matrix = dir.path().join("singular.h5");
    let rhs = dir.path().join("rhs.h5");
    let qsp = dir.path().join("phases.json");
    let output = dir.path().join("physical.h5");
    {
        let file = hdf5_metno::File::create(&matrix)?;
        let group = file.create_group("matrix")?;
        group
            .new_attr::<u64>()
            .shape(2)
            .create("shape")?
            .write_raw(&[2, 2])?;
        group
            .new_dataset::<f64>()
            .shape([2, 2])
            .create("dense")?
            .write_raw(&[1.0, 0.0, 0.0, 0.0])?;
    }
    write_state_vector(
        &rhs,
        &[C::new(1.0, 0.0), C::new(1.0, 0.0)],
        IoPolicy::default(),
    )?;
    phases(&qsp)?;
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_quest-qsvt-cli"))
        .args([
            "solve",
            "--matrix",
            path(&matrix)?,
            "--rhs",
            path(&rhs)?,
            "--qsp",
            path(&qsp)?,
            "--route",
            "standard",
            "--reciprocal-scale",
            "1.0",
            "--output-state",
            path(&output)?,
        ])
        .output()?;
    expect_false!(result.status.success());
    expect_true!(String::from_utf8_lossy(&result.stderr).contains("numerical full rank"));
    expect_false!(output.exists());
    Ok(())
}
