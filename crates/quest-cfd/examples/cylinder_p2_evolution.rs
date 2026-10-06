//! One fixed complete evolution row, with no retry or resource relaxation.
use quest_cfd::cylinder_high_order::{
	CylinderEnergySample, CylinderEvolutionReport, CylinderEvolutionRequest,
	CylinderInitialCondition, CylinderPhysicalLimits, CylinderPhysicalSource,
	CylinderWorkflowResources, Rk4Progress,
};
use serde::Serialize;
use std::io::Write;
#[derive(Serialize)]
struct Output<'a> {
	schema: &'static str,
	status: &'static str,
	request: CylinderEvolutionRequest,
	resources: &'a CylinderWorkflowResources,
	progress: Option<&'a Rk4Progress>,
	last_state: &'a [f64],
	energy_samples: &'a [CylinderEnergySample],
	energy_calls_attempted: u32,
	diagnostic_drift_calls_attempted: u32,
	report: Option<&'a CylinderEvolutionReport>,
	error: Option<String>,
	temporal_accuracy_assessed: bool,
}
#[allow(
	clippy::large_stack_arrays,
	reason = "The complete fixed serializer buffer is declared as external live storage before construction"
)]
fn emit(output: &Output<'_>) -> Result<(), Box<dyn std::error::Error>> {
	let mut bytes = [0_u8; 65_536];
	let mut cursor = std::io::Cursor::new(bytes.as_mut_slice());
	serde_json::to_writer_pretty(&mut cursor, output)?;
	let length = usize::try_from(cursor.position())?;
	std::io::stdout()
		.lock()
		.write_all(bytes.get(..length).ok_or("output range")?)?;
	println!();
	Ok(())
}
fn rejected(
	request: CylinderEvolutionRequest,
	resources: &CylinderWorkflowResources,
	error: &quest_cfd::CfdError,
) -> Result<(), Box<dyn std::error::Error>> {
	emit(&Output {
		schema: "quest-cylinder-p2-evolution-v1",
		status: "rejected",
		request,
		resources,
		progress: None,
		last_state: &[],
		energy_samples: &[],
		energy_calls_attempted: 0,
		diagnostic_drift_calls_attempted: 0,
		report: None,
		error: Some(error.to_string()),
		temporal_accuracy_assessed: false,
	})
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
	let mut args = std::env::args().skip(1);
	let steps = args
		.next()
		.ok_or("expected one step count: 2, 4 or 8")?
		.parse::<u32>()?;
	if args.next().is_some() || ![2, 4, 8].contains(&steps) {
		return Err("expected exactly one step count: 2, 4 or 8".into());
	}
	let request = CylinderEvolutionRequest {
		steps,
		initial_condition: CylinderInitialCondition::PreparedMinimumMassCompatible,
	};
	let source = CylinderPhysicalSource::new_with_receipt(
		4,
		1,
		100,
		CylinderPhysicalLimits {
			max_work: 2_000_000_000,
			mesh: quest_cfd::physical_space::PhysicalMeshLimits {
				external_retained_bytes: 65_536,
				..Default::default()
			},
			..Default::default()
		},
	);
	let source = match source.outcome {
		Ok(value) => value,
		Err(error) => return rejected(request, &source.resources, &error),
	};
	let prepared = source.prepare_with_receipt();
	let mut prepared = match prepared.outcome {
		Ok(value) => value,
		Err(error) => return rejected(request, &prepared.resources, &error),
	};
	let initial = prepared.initial_state().to_vec();
	let result = prepared.evolve_with_receipt(&initial, request);
	emit(&Output {
		schema: "quest-cylinder-p2-evolution-v1",
		status: if result.outcome.is_ok() {
			"completed"
		} else {
			"failed-attempt"
		},
		request,
		resources: &result.resources,
		progress: Some(&result.progress),
		last_state: &result.last_state,
		energy_samples: &result.energy_samples,
		energy_calls_attempted: result.energy_calls_attempted,
		diagnostic_drift_calls_attempted: result.diagnostic_drift_calls_attempted,
		report: result.outcome.as_ref().ok(),
		error: result.outcome.as_ref().err().map(ToString::to_string),
		temporal_accuracy_assessed: false,
	})
}
