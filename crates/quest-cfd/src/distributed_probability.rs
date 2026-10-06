//! Probability-weighted diagonal reductions; a child module of distributed history.
#![allow(
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	clippy::suboptimal_flops,
	reason = "Checked counts, finite scalar reductions and admitted fixed-size protocol frames"
)]
use super::{PreparedHistoryInverse, agree, capacity, common, native, plus, times};
use crate::{
	CfdError,
	probability_observation::{DiagonalRange, HistoryProjection, SamplingPlan},
};
use quest::collective::CollectiveRegister;
/// Additional readout/callback payload admission; parent rank/node caps also apply.
#[derive(Clone, Copy, Debug)]
pub struct ProbabilityReadoutLimits {
	pub chunk_amplitudes: usize,
	pub max_work: usize,
	pub max_transport_bytes: usize,
	pub max_bytes: usize,
	pub callback_retained_bytes: usize,
	pub callback_query_bytes: usize,
	pub callback_query_work: usize,
}
impl Default for ProbabilityReadoutLimits {
	fn default() -> Self {
		Self {
			chunk_amplitudes: 256,
			max_work: 100_000_000,
			max_transport_bytes: 100_000_000,
			max_bytes: 64 * 1024 * 1024,
			callback_retained_bytes: 0,
			callback_query_bytes: 0,
			callback_query_work: 1,
		}
	}
}
/// Same source identity, range, projection and declarations must be supplied on every rank.
/// The callback must be deterministic and pure, and must not perform collectives.
#[derive(Clone, Copy, Debug)]
pub struct ProbabilityReadoutRequest {
	pub range: DiagonalRange,
	pub projection: HistoryProjection,
	pub source_identity: u64,
	pub limits: ProbabilityReadoutLimits,
}
/// Conservative global scalar-work/transport bounds and additional rank payload.
#[derive(Clone, Copy, Debug, serde::Serialize)]
pub struct ProbabilityReadoutResources {
	pub amplitude_passes: usize,
	pub callback_queries_bound: usize,
	pub work_bound: usize,
	pub transport_byte_bound: usize,
	pub local_extra_bytes: usize,
	pub maximum_rank_envelope_bytes: usize,
}
/// Exact simulator reductions in binary64; none of these values certify a shot probability.
#[derive(Clone, Copy, Debug, serde::Serialize)]
pub struct ProbabilityObservation {
	/// Squared norm of the whole register; retained to expose normalization drift.
	pub total_probability_mass: f64,
	/// Born probabilities normalized by the whole-register squared norm.
	pub inverse_success_probability: f64,
	pub selected_probability: f64,
	pub selected_probability_given_inverse: Option<f64>,
	/// Unnormalized selected mass and diagonal moments before physical rescaling.
	pub selected_probability_mass: f64,
	pub weighted_first_moment: f64,
	pub weighted_second_moment: f64,
	pub conditional_expectation: Option<f64>,
	pub conditional_variance: Option<f64>,
	/// scale² times selected mass/first moment. These are not probability-normalized means.
	pub physical_selected_squared_norm: f64,
	pub physical_scaled_quadratic_functional: f64,
	pub projection: HistoryProjection,
	pub range: DiagonalRange,
	pub source_identity: u64,
	pub resources: ProbabilityReadoutResources,
	pub readout_seconds: f64,
}
/// Repeated full-circuit cost estimate, not a measurement execution record.
#[derive(Clone, Copy, Debug, serde::Serialize)]
pub struct RepeatedSamplingCost {
	pub attempted_state_preparations: u64,
	pub attempted_inverse_applications: u64,
	pub state_preparation_elementary_gates: u64,
	pub inverse_matching_oracle_calls: u64,
	pub measured_register_bits: u64,
	pub classical_attempt_selection_work: u64,
	pub classical_selected_sample_work: u64,
	pub quantum_measurements_executed: bool,
}
fn sums(
	env: &quest::collective::CollectiveEnvironment<'_, '_>,
	local: [f64; 5],
) -> Result<[f64; 5], CfdError> {
	let mut result = [0.; 5];
	let mut lane = env.communicator().collective_lane().map_err(native)?;
	for peer in 0..env.size().map_err(native)? {
		let mut packet = [0; 40];
		for (slot, x) in packet.as_chunks_mut::<8>().0.iter_mut().zip(local) {
			slot.copy_from_slice(&x.to_le_bytes());
		}
		lane.broadcast_bytes(peer, &mut packet).map_err(native)?;
		for (slot, x) in packet.as_chunks::<8>().0.iter().zip(&mut result) {
			*x += f64::from_le_bytes(*slot);
		}
	}
	drop(lane);
	agree(
		env,
		if result.iter().all(|x| x.is_finite()) {
			Ok(result)
		} else {
			Err(CfdError::InvalidInput("probability reduction overflow"))
		},
	)
}
fn hash(mut current: u64, word: u64) -> u64 {
	for byte in word.to_le_bytes() {
		current = (current ^ u64::from(byte)).wrapping_mul(0x100_0000_01b3);
	}
	current
}
impl PreparedHistoryInverse<'_, '_, '_> {
	/// Conditional diagonal expectations from successful logical amplitudes, in bounded chunks.
	///
	/// Generic coefficient projections do not describe a physical time. A temporal-node
	/// descriptor must match this prepared history exactly and denotes its specified
	/// one-sided nodal trace; arbitrary-time interference is not reconstructed here.
	/// The callback runs twice on each successful logical index in that projection,
	/// including zero amplitudes. It returns (selected,value), and must be pure/deterministic,
	/// carry the declared common source identity, and never perform MPI operations.
	/// A replay digest checks consistency but does not prove callback semantics.
	///
	/// Sampling probabilities are simulator observations only. Physical rescaling assumes
	/// the supplied register is the corresponding inverse output; this method cannot prove
	/// that state provenance. No measurement, projection or register mutation occurs.
	/// # Errors
	/// Collectively rejects incompatible metadata, budget/callback/range failures, zero
	/// whole-state norm, replay mismatch and nonfinite moments or physical rescaling.
	#[allow(
		clippy::too_many_lines,
		reason = "Keep two admitted read passes and collective failure ordering visible"
	)]
	pub fn reduce_probability_observable(
		&self,
		state: &CollectiveRegister<'_, '_, '_>,
		request: ProbabilityReadoutRequest,
		select: impl Fn(usize) -> Result<(bool, f64), CfdError>,
	) -> Result<ProbabilityObservation, CfdError> {
		let started = std::time::Instant::now();
		let env = self.environment;
		let limits = request.limits;
		self.admit_register(state)?;
		let local = state.deployment().local_amplitudes();
		let chunk = limits.chunk_amplitudes.min(local);
		let parts = usize::try_from(env.size().map_err(native)?)
			.map_err(|_| CfdError::InvalidInput("probability ranks"))?;
		let word = |x: usize| {
			u64::try_from(x).map_err(|_| CfdError::InvalidInput("probability metadata width"))
		};
		let words = agree(
			env,
			(|| {
				Ok([
					request.range.lower().to_bits(),
					request.range.upper().to_bits(),
					request.source_identity,
					word(local)?,
					word(chunk)?,
					word(limits.max_work)?,
					word(limits.max_transport_bytes)?,
					word(limits.max_bytes)?,
					word(limits.callback_retained_bytes)?,
					word(limits.callback_query_bytes)?,
					word(limits.callback_query_work)?,
				])
			})(),
		)?;
		common(env, &words)?;
		let projection_words = match request.projection {
			HistoryProjection::CoefficientProjection => Ok([0; 14]),
			HistoryProjection::TemporalNode(node) => node.metadata_words(),
		};
		let projection_words = agree(env, projection_words)?;
		common(env, &projection_words)?;
		agree(
			env,
			if chunk == 0
				|| limits.callback_query_work == 0
				|| request.source_identity == 0
				|| matches!(request.projection,HistoryProjection::TemporalNode(node) if !node.matches(self.history_semantics))
			{
				Err(CfdError::InvalidInput(
					"probability projection, source or query policy",
				))
			} else {
				Ok(())
			},
		)?;
		let mut resources = agree(
			env,
			(|| {
				let chunks = local.div_ceil(chunk);
				let control = times(times(parts, parts)?, plus(8192, times(chunks, 2048)?)?)?;
				let global = times(local, parts)?;
				let callback_queries_bound = times(self.header.rows, 2)?;
				let work_bound = plus(
					plus(
						times(times(global, 2)?, 128)?,
						times(callback_queries_bound, limits.callback_query_work)?,
					)?,
					control,
				)?;
				let local_extra_bytes = plus(
					plus(limits.callback_retained_bytes, limits.callback_query_bytes)?,
					plus(
						times(chunk, 32)?,
						4096 + size_of::<ProbabilityObservation>(),
					)?,
				)?;
				if work_bound > limits.max_work
					|| control > limits.max_transport_bytes
					|| local_extra_bytes > limits.max_bytes
				{
					return Err(CfdError::InvalidInput("probability readout budget"));
				}
				Ok(ProbabilityReadoutResources {
					amplitude_passes: 2,
					callback_queries_bound,
					work_bound,
					transport_byte_bound: control,
					local_extra_bytes,
					maximum_rank_envelope_bytes: 0,
				})
			})(),
		)?;
		resources.maximum_rank_envelope_bytes = capacity(
			env,
			plus(self.external_bytes, resources.local_extra_bytes),
			&self.limits,
		)?;
		let _reservation = env
			.reserve_external_bytes(resources.local_extra_bytes)
			.map_err(native)?;
		let start = times(state.deployment().rank(), local)?;
		let mut first_hash = 0;
		let mut global = [0.; 5];
		let mut variance = 0.;
		for pass in 0..2 {
			let mut own = [0.; 5];
			let mut digest = 0xcbf2_9ce4_8422_2325;
			let mean = if global[2] > 0. {
				global[3] / global[2]
			} else {
				0.
			};
			for offset in (0..local).step_by(chunk) {
				let values = state
					.read_local_amplitudes(offset, (local - offset).min(chunk))
					.map_err(native)?;
				agree(env,std::panic::catch_unwind(std::panic::AssertUnwindSafe(||{
     for (index,z) in values.iter().enumerate(){
      let mass=z.norm_sqr();let physical=plus(start,plus(offset,index)?)?;
      if pass==0{own[0]+=mass;}
      let logical=physical/2;
      if physical%2==0&&logical<self.header.rows{
       if pass==0{own[1]+=mass;}
       if matches!(request.projection,HistoryProjection::TemporalNode(node) if !node.contains(logical)){continue;}
       let (selected,value)=select(logical)?;
       if !value.is_finite()||(selected&&!request.range.contains(value)){return Err(CfdError::InvalidInput("diagonal observation outside finite range"));}
       digest=hash(hash(hash(digest,word(logical)?),u64::from(selected)),value.to_bits());
       if selected{
        if pass==0{own[2]+=mass;own[3]+=mass*value;own[4]+=mass*(value*value);}
        else{let centered=value-mean;own[0]+=mass*(centered*centered);}
       }
      }
     }
     if own.iter().any(|x|!x.is_finite()){return Err(CfdError::InvalidInput("probability moments overflow"));}
     Ok(())
    })).unwrap_or(Err(CfdError::InvalidInput("probability callback panicked"))))?;
			}
			if pass == 0 {
				first_hash = digest;
				global = sums(env, own)?;
			} else {
				agree(
					env,
					if first_hash == digest {
						Ok(())
					} else {
						Err(CfdError::InvalidInput(
							"probability callback changed between passes",
						))
					},
				)?;
				variance = sums(env, own)?[0];
			}
		}
		let scale = self.report.physical_rescaling;
		let scaled_norm = global[2] * scale * scale;
		let scaled_functional = global[3] * scale * scale;
		let expectation = (global[2] > 0.).then(|| global[3] / global[2]);
		let conditional_variance = (global[2] > 0.).then(|| variance / global[2]);
		agree(
			env,
			if global[0] <= 0.
				|| ![scaled_norm, scaled_functional]
					.iter()
					.all(|x| x.is_finite())
				|| expectation.is_some_and(|x| !x.is_finite())
				|| conditional_variance.is_some_and(|x| !x.is_finite())
			{
				Err(CfdError::InvalidInput(
					"probability normalization or physical scaling overflow",
				))
			} else {
				Ok(())
			},
		)?;
		Ok(ProbabilityObservation {
			total_probability_mass: global[0],
			inverse_success_probability: global[1] / global[0],
			selected_probability: global[2] / global[0],
			selected_probability_given_inverse: (global[1] > 0.).then(|| global[2] / global[1]),
			selected_probability_mass: global[2],
			weighted_first_moment: global[3],
			weighted_second_moment: global[4],
			conditional_expectation: expectation,
			conditional_variance,
			physical_selected_squared_norm: scaled_norm,
			physical_scaled_quadratic_functional: scaled_functional,
			projection: request.projection,
			range: request.range,
			source_identity: request.source_identity,
			resources,
			readout_seconds: started.elapsed().as_secs_f64(),
		})
	}
	/// Count fresh coherent preparation and complete inverse repetitions for a sampling plan.
	/// The pure planner's caller probability/range premises and i.i.d. contract still apply.
	/// This does not bind or verify those premises to this source. Source evaluation work is
	/// caller-declared: selection/decoding is charged on every attempt, and observable
	/// evaluation on the first N selected outcomes; all trials measure flags.
	/// # Errors
	/// Rejects integer overflow or an absent selected-observable work declaration.
	pub fn sampling_cost(
		&self,
		plan: &SamplingPlan<'_>,
		selection_query_work: u64,
		sample_query_work: u64,
	) -> Result<RepeatedSamplingCost, CfdError> {
		if sample_query_work == 0 || selection_query_work == 0 {
			return Err(CfdError::InvalidInput("sampling callback work"));
		}
		let cv = |x| u64::try_from(x).map_err(|_| CfdError::InvalidInput("sampling cost count"));
		let mul = |a: u64, b: u64| {
			a.checked_mul(b)
				.ok_or(CfdError::InvalidInput("sampling cost overflow"))
		};
		Ok(RepeatedSamplingCost {
			attempted_state_preparations: plan.attempted_shots,
			attempted_inverse_applications: plan.attempted_shots,
			state_preparation_elementary_gates: mul(
				plan.attempted_shots,
				cv(self.report.rhs_preparation.elementary_gates)?,
			)?,
			inverse_matching_oracle_calls: mul(
				plan.attempted_shots,
				mul(2, cv(self.report.polynomial_degree)?)?,
			)?,
			measured_register_bits: mul(plan.attempted_shots, cv(self.report.qubits)?)?,
			classical_attempt_selection_work: mul(plan.attempted_shots, selection_query_work)?,
			classical_selected_sample_work: mul(plan.selected_shots, sample_query_work)?,
			quantum_measurements_executed: false,
		})
	}
}
