//! Fixed short-window complete prepared dynamics; no developed-cycle or forward-error claim.
use super::{
	CylinderPhysicalSnapshot, CylinderWorkflowResources, PreparedCylinderPhysical, add, invalid,
	mul,
};
use crate::{
	CfdError,
	reference_rk4::{self, Rk4Progress},
};
use serde::{Deserialize, Serialize};
const END: f64 = 0.0001;
const RESERVE: usize = 64 * 1024;
/// Provenance of the complete supplied chart state, checked when claiming the prepared origin.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum CylinderInitialCondition {
	PreparedMinimumMassCompatible,
	SuppliedComplete,
}
/// The bounded campaign admits only two, four or eight steps over [0,1e-4].
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CylinderEvolutionRequest {
	pub steps: u32,
	pub initial_condition: CylinderInitialCondition,
}
/// Integrated physical energy at an accepted complete state, never a pressure snapshot.
#[derive(Clone, Copy, Debug, Serialize)]
pub struct CylinderEnergySample {
	pub time: f64,
	pub integrated_energy: f64,
}
/// Completed integration and final original physical observations; accuracy is separately assessed.
#[derive(Debug, Serialize)]
#[allow(
	clippy::struct_excessive_bools,
	reason = "Separate public scientific claims must not conflate execution, quadratic action, forward accuracy and response resolution"
)]
pub struct CylinderEvolutionReport {
	pub dt: f64,
	pub end_time: f64,
	pub initial_integrated_energy: f64,
	pub state_change_norm: f64,
	pub quadratic_action_norm: f64,
	pub relative_quadratic_action: f64,
	pub quadratic_action_resolved: bool,
	pub quadratic_probe_calls: u32,
	pub integration_completed: bool,
	pub forward_accuracy_certified: bool,
	pub resolved_full_minus_linear_response: bool,
	pub snapshot: CylinderPhysicalSnapshot,
}
/// Rejections retain progress, complete last accepted state and conservatively charged work.
#[derive(Debug)]
pub struct CylinderEvolutionAttempt {
	pub request: CylinderEvolutionRequest,
	pub outcome: Result<CylinderEvolutionReport, CfdError>,
	pub progress: Rk4Progress,
	pub last_state: Vec<f64>,
	pub energy_samples: Vec<CylinderEnergySample>,
	/// Initial and accepted-step energy callbacks only; final snapshot queries are separately charged as a bundle.
	pub energy_calls_attempted: u32,
	/// Direct final quadratic-action probes, separate from RK4 and internal pressure-query work.
	pub diagnostic_drift_calls_attempted: u32,
	pub resources: CylinderWorkflowResources,
}
fn reserve<T>(count: usize) -> Result<Vec<T>, CfdError> {
	let mut out = Vec::new();
	out.try_reserve_exact(count).map_err(|_| invalid())?;
	Ok(out)
}
fn norm(values: impl Iterator<Item = f64>) -> Result<f64, CfdError> {
	let n = values.fold(0_f64, f64::hypot);
	if n.is_finite() { Ok(n) } else { Err(invalid()) }
}
impl PreparedCylinderPhysical<'_> {
	/// Evolve every coordinate under the actual lifting, retaining any failed attempt.
	/// All integration/diagnostic work is conservatively charged on entry; callback counts
	/// record actual attempts separately. The final snapshot retains its own single charge.
	#[must_use]
	#[allow(
		clippy::too_many_lines,
		reason = "A single admitted transaction retains all partial state, numerical phases and the final snapshot ledger"
	)]
	pub fn evolve_with_receipt(
		&mut self,
		initial: &[f64],
		request: CylinderEvolutionRequest,
	) -> CylinderEvolutionAttempt {
		let mut progress = Rk4Progress::default();
		let mut state = Vec::new();
		let mut samples = Vec::new();
		let mut energy_calls = 0;
		let mut probe_calls = 0;
		let outcome = (|| {
			progress.failure_phase = Some("evolution preflight");
			let m = self.source.model.dimension();
			let n = self.source.model.diagnostics().local_velocity_dimension;
			if ![2, 4, 8].contains(&request.steps)
				|| m != 54
				|| n != 192
				|| self.source.evidence.requested_sectors != 4
				|| self.source.geometry.cells.len() != 16
				|| initial.len() != m
				|| initial.iter().any(|x| !x.is_finite())
				|| END > self.source.manifest.time_window[1]
			{
				return Err(invalid());
			}
			if matches!(
				request.initial_condition,
				CylinderInitialCondition::PreparedMinimumMassCompatible
			) && initial
				.iter()
				.zip(&self.initial)
				.any(|(x, y)| x.to_bits() != y.to_bits())
			{
				return Err(invalid());
			}
			let steps = usize::try_from(request.steps).map_err(|_| invalid())?;
			let d = self.flow.resources().drift_work;
			let integration_work = add(mul(add(4, mul(5, steps)?)?, d)?, mul(mul(64, m)?, steps)?)?;
			let pressure = self.flow.pressure_query_resources()?.ok_or_else(invalid)?;
			self.flow.admit_label_force(
				m,
				"cylinder",
				crate::physical_space::MechanicalTractionLimits::default(),
			)?;
			let observations = add(mul(8, d)?, mul(self.source.model.cell_count(), 1024)?)?;
			let complete_work = add(integration_work, add(pressure.work, observations)?)?;
			let peak = add(
				self.source
					.resources
					.peak_bytes
					.max(pressure.peak_bytes)
					.max(self.flow.resources().peak_bytes),
				RESERVE,
			)?;
			self.resources.plan(
				"complete evolution plus final snapshot",
				complete_work,
				peak,
			)?;
			// One reserve holds persistent progress, input-accessible payload and worst simultaneous
			// helper/probe vectors. Physical callback scratch is already in the complete source peak.
			samples = reserve(add(steps, 1)?)?;
			let persistent = add(
				size_of::<CylinderEvolutionAttempt>(),
				add(
					mul(samples.capacity(), size_of::<CylinderEnergySample>())?,
					mul(m, 64)?,
				)?,
			)?;
			let helper_bytes = RESERVE.checked_sub(persistent).ok_or_else(invalid)?;
			if mul(m, 64)? > helper_bytes {
				return Err(invalid());
			}
			let mut initial_copy = reserve(m)?;
			initial_copy.extend_from_slice(initial);
			if mul(initial_copy.capacity(), 64)? > helper_bytes {
				return Err(invalid());
			}
			state = initial_copy;
			self.resources.complete(integration_work)?;
			progress.failure_phase = Some("initial physical energy");
			energy_calls += 1;
			let initial_energy = self.flow.energy(0., &state)?;
			samples.push(CylinderEnergySample {
				time: 0.,
				integrated_energy: initial_energy,
			});
			let dt = END / f64::from(request.steps);
			let attempt = reference_rk4::integrate(
				std::mem::take(&mut state),
				dt,
				request.steps,
				helper_bytes,
				|time, values| self.flow.drift(time, values),
				|_, time, values| {
					energy_calls += 1;
					let energy = self.flow.energy(time, values)?;
					samples.push(CylinderEnergySample {
						time,
						integrated_energy: energy,
					});
					Ok(())
				},
			);
			state = attempt.state;
			progress = attempt.progress;
			attempt.outcome?;
			progress.failure_phase = Some("quadratic-action probes");
			let mut negative = reserve(m)?;
			negative.extend(state.iter().map(|x| -x));
			let mut zero = reserve(m)?;
			zero.resize(m, 0.);
			let mut call = |values: &[f64]| {
				probe_calls += 1;
				self.flow.drift(END, values)
			};
			let audit = |vectors: &[&Vec<f64>]| -> Result<(), CfdError> {
				let bytes = vectors
					.iter()
					.try_fold(persistent, |sum, v| add(sum, mul(v.capacity(), 8)?))?;
				if bytes > RESERVE {
					Err(invalid())
				} else {
					Ok(())
				}
			};
			audit(&[&state, &negative, &zero])?;
			let plus = call(&state)?;
			audit(&[&state, &negative, &zero, &plus])?;
			let minus = call(&negative)?;
			audit(&[&state, &negative, &zero, &plus, &minus])?;
			let constant = call(&zero)?;
			audit(&[&state, &negative, &zero, &plus, &minus, &constant])?;
			let action = norm(
				plus.iter()
					.zip(&minus)
					.zip(&constant)
					.map(|((p, m), c)| 0.5 * (p + m - 2. * c)),
			)?;
			let relative = action / norm(plus.iter().copied())?.max(1.);
			let change = norm(state.iter().zip(initial).map(|(a, b)| a - b))?;
			drop(negative);
			drop(zero);
			drop(plus);
			drop(minus);
			drop(constant);
			progress.failure_phase = Some("final original snapshot");
			let mut snapshot = self.snapshot_at(&state, END)?;
			snapshot.method = "classical complete P2 RK4 trajectory; fixed short window";
			progress.failure_phase = None;
			Ok(CylinderEvolutionReport {
				dt,
				end_time: END,
				initial_integrated_energy: initial_energy,
				state_change_norm: change,
				quadratic_action_norm: action,
				relative_quadratic_action: relative,
				quadratic_action_resolved: relative > 1e-10,
				quadratic_probe_calls: probe_calls,
				integration_completed: true,
				forward_accuracy_certified: false,
				resolved_full_minus_linear_response: false,
				snapshot,
			})
		})();
		CylinderEvolutionAttempt {
			request,
			outcome,
			progress,
			last_state: state,
			energy_samples: samples,
			energy_calls_attempted: energy_calls,
			diagnostic_drift_calls_attempted: probe_calls,
			resources: self.resources.clone(),
		}
	}
}
