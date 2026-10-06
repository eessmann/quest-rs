//! Explicit 2b aggregate allowance for one complete coarse P2 snapshot; no evolution.
use quest_cfd::cylinder_high_order::{
	CylinderGeometryExport, CylinderPhysicalLimits, CylinderPhysicalSnapshot,
	CylinderPhysicalSource, CylinderWorkflowResources,
};
use serde::Serialize;
use std::io::Write;
#[derive(Serialize)]
struct Completed<'a> {
	schema: &'static str,
	status: &'static str,
	geometry: CylinderGeometryExport<'a>,
	snapshot: CylinderPhysicalSnapshot,
}
#[derive(Serialize)]
struct Rejected {
	schema: &'static str,
	status: &'static str,
	error: String,
	resources: CylinderWorkflowResources,
}
#[allow(
	clippy::large_stack_arrays,
	reason = "The fixed 64 KiB output buffer is explicitly reserved in all workflow phases and prevents serializer growth"
)]
fn emit(value: &impl Serialize) -> Result<(), Box<dyn std::error::Error>> {
	let mut bytes = [0_u8; 65_536];
	let mut cursor = std::io::Cursor::new(bytes.as_mut_slice());
	serde_json::to_writer_pretty(&mut cursor, value)?;
	let length = usize::try_from(cursor.position())?;
	std::io::stdout()
		.lock()
		.write_all(bytes.get(..length).ok_or("bounded output range")?)?;
	println!();
	Ok(())
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
	if std::env::args().len() != 1 {
		return Err("fixed snapshot example accepts no arguments".into());
	}
	let limits = CylinderPhysicalLimits {
		max_work: 2_000_000_000,
		mesh: quest_cfd::physical_space::PhysicalMeshLimits {
			external_retained_bytes: 65_536,
			..Default::default()
		},
		..Default::default()
	};
	// Conservatively reserve the complete fixed JSON buffer across every numerical phase.
	let attempt = CylinderPhysicalSource::new_with_receipt(4, 1, 100, limits);
	let source = match attempt.outcome {
		Ok(source) => source,
		Err(e) => {
			return emit(&Rejected {
				schema: "quest-cylinder-p2-snapshot-v1",
				status: "rejected",
				error: e.to_string(),
				resources: attempt.resources,
			});
		}
	};
	let attempt = source.prepare_with_receipt();
	let mut prepared = match attempt.outcome {
		Ok(prepared) => prepared,
		Err(e) => {
			return emit(&Rejected {
				schema: "quest-cylinder-p2-snapshot-v1",
				status: "rejected",
				error: e.to_string(),
				resources: attempt.resources,
			});
		}
	};
	let initial = prepared.initial_state().to_vec();
	match prepared.snapshot_at(&initial, 0.) {
		Ok(snapshot) => emit(&Completed {
			schema: "quest-cylinder-p2-snapshot-v1",
			status: "completed",
			geometry: source.geometry_export(),
			snapshot,
		}),
		Err(e) => emit(&Rejected {
			schema: "quest-cylinder-p2-snapshot-v1",
			status: "rejected",
			error: e.to_string(),
			resources: prepared.resources().clone(),
		}),
	}
}
