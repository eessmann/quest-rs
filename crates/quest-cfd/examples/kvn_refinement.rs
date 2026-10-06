//! One complete five-coordinate classical `KvN` refinement experiment.
#![allow(
	clippy::arithmetic_side_effects,
	reason = "Bounded diagnostic differences and timing; solver admission checks inputs"
)]
use clap::Parser;
use quest_cfd::{
	CfdError, PeriodicBdm1, configuration::ConfigurationGrid,
	configuration_diagnostics::regularization_resolution, validation::compare_full_dg_transport,
};
use quest_numerics::SparseLimits;
use serde_json::json;

#[derive(Parser, serde::Serialize)]
struct Options {
	#[arg(long, default_value_t = 3)]
	cells: usize,
	#[arg(long, default_value_t = 1)]
	order: usize,
	#[arg(long, default_value_t = 1.0)]
	extent: f64,
	#[arg(long, default_value_t = 1.2)]
	width: f64,
	#[arg(long, default_value_t = 2)]
	minimum_support_samples: usize,
	#[arg(long, default_value_t = 0.001)]
	horizon: f64,
	#[arg(long, default_value_t = 4)]
	steps: u32,
	#[arg(long, default_value_t = 100_000)]
	max_dimension: usize,
	#[arg(long, default_value_t = 3_000_000)]
	max_entries: usize,
	#[arg(long, default_value_t = 268_435_456)]
	max_bytes: usize,
	#[arg(long, default_value_t = 2_000_000_000)]
	max_work: usize,
}
fn experiment(o: &Options) -> Result<serde_json::Value, CfdError> {
	let flow = PeriodicBdm1::assemble(0.01)?;
	let center = [0.15, -0.1, 0.07, 0.11, -0.04];
	let grid = ConfigurationGrid::uniform(
		flow.dimension(),
		-o.extent,
		o.extent,
		o.cells,
		o.order,
		o.max_dimension,
	)?;
	let sampling = regularization_resolution(&grid, &center, o.width, o.minimum_support_samples)?;
	let initial = grid.initial_bump(&center, o.width)?;
	let comparison = compare_full_dg_transport(
		&flow,
		&grid,
		&initial,
		o.horizon,
		o.steps,
		SparseLimits {
			max_dimension: o.max_dimension,
			max_entries: o.max_entries,
			max_bytes: o.max_bytes,
			max_work: o.max_work,
		},
	)?;
	let deterministic = flow.integrate_rk4(
		&center,
		o.horizon / f64::from(o.steps),
		usize::try_from(o.steps).map_err(|_| CfdError::InvalidInput("step index overflow"))?,
	)?;
	let deterministic_energy = flow.energy(&deterministic)?;
	let initial_regularization_mean_bias = comparison
		.initial
		.coordinate_means
		.iter()
		.zip(center)
		.fold(0_f64, |s, (x, y)| s.hypot(x - y));
	let trajectory_regularization_mean_bias = comparison
		.trajectory_mean_coordinates
		.iter()
		.zip(&deterministic)
		.fold(0_f64, |s, (x, y)| s.hypot(x - y));
	Ok(json!({
		"sampling":sampling,"comparison":comparison,
		"deterministic_reference": {"coordinates":deterministic,"kinetic_energy":deterministic_energy},
		"initial_sampled_regularization_coordinate_bias":initial_regularization_mean_bias,
		"trajectory_sampled_regularization_coordinate_bias":trajectory_regularization_mean_bias,
		"trajectory_sampled_regularization_energy_bias":(comparison.trajectory_mean_kinetic_energy-deterministic_energy).abs(),
	}))
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
	let options = Options::parse();
	let start = std::time::Instant::now();
	let outcome = experiment(&options);
	let (status, result, error) = match outcome {
		Ok(v) => ("classical_validation", Some(v), None),
		Err(e) => ("rejected", None, Some(e.to_string())),
	};
	println!(
		"{}",
		serde_json::to_string_pretty(&json!({
			"schema":"quest-cfd-full-kvn-refinement-v1","status":status,"parameters":options,"result":result,"error":error,
			"elapsed_seconds":start.elapsed().as_secs_f64(),"physical_dimension":5,"viscosity":0.01,
			"physical_source":"PeriodicBdm1 complete periodic two-triangle nonlinear system",
			"quantum_execution":false,"convergence_certified":false,
			"error_contract": {
				"comparison":"mass-weighted nodal probability weak coordinate and kinetic-energy errors versus the identical full DG trajectory ensemble",
				"physical_discretization":"held fixed; no continuum error bound",
				"regularization":"sampled deterministic-limit bias; mixed with initial quadrature error until configuration refinement",
				"configuration":"weak observable discrepancy at fixed bump width; includes unresolved periodic-domain effects",
				"domain":"outer-cell occupation only; no outward flux or leakage certificate",
				"time":"two independent classical RK4 references; step refinement measures reference integration sensitivity, not temporal DG error",
				"encoding_polynomial_sampling":"not executed in this classical experiment",
				"combined_bound":null
			}
		}))?
	);
	Ok(())
}
