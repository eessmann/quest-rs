#![cfg(all(feature = "zx", target_os = "linux"))]
use googletest::prelude::*;
use quest_math::{Control, Gate, Limits, Operation, Sequence};
use quest_optimizer_client::{Client, WorkerLimits};
fn operation(gate: Gate, targets: &[usize]) -> Operation {
	Operation {
		gate,
		targets: targets.to_vec(),
		controls: Vec::new(),
	}
}
fn worker() -> Result<Client> {
	Ok(Client::new(
		env!("CARGO_BIN_EXE_quest-optimizer-worker"),
		WorkerLimits::default(),
	)?)
}
#[gtest]
fn real_zx_worker_returns_phase_sensitive_certificate_for_original_interface() -> Result<()> {
	let original = Sequence {
		qubits: 3,
		operations: vec![
			operation(Gate::H, &[0]),
			operation(Gate::H, &[0]),
			operation(Gate::Swap, &[1, 2]),
			operation(Gate::W, &[]),
		],
	};
	let certificate = worker()?.optimize_zx(&original, 41, Limits::default())?;
	verify_eq!(certificate.target(), &original)?;
	verify_eq!(certificate.candidate().qubits, original.qubits)?;
	quest_math::verify_exact(certificate.candidate(), &original, Limits::default())?;
	Ok(())
}
#[gtest]
fn real_zx_worker_certifies_negative_control_and_declines_unsupported_control() -> Result<()> {
	let mut original = Sequence {
		qubits: 2,
		operations: vec![Operation {
			gate: Gate::X,
			targets: vec![1],
			controls: vec![Control {
				qubit: 0,
				positive: false,
			}],
		}],
	};
	let client = worker()?;
	let certificate = client.optimize_zx(&original, 53, Limits::default())?;
	verify_eq!(certificate.target(), &original)?;
	original
		.operations
		.first_mut()
		.ok_or_else(|| std::io::Error::other("operation missing"))?
		.gate = Gate::T;
	verify_that!(
		client.optimize_zx(&original, 53, Limits::default()),
		err(anything())
	)?;
	Ok(())
}
