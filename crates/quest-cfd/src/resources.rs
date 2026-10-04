//! Exact arbitrary-width accounting for a full physical constraint chart.

use crate::CfdError;
use dashu_int::{UBig, ops::BitTest};

/// Resource inputs; the physical dimension must come from the full mesh constraint rank.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceRequest {
	/// Full local velocity coefficient count before constraint elimination.
	pub local_velocity_dimension: usize,
	/// Rank of all normal-trace, boundary and divergence constraints.
	pub constraint_rank: usize,
	/// Number of configuration DG coefficients on each independent axis.
	pub configuration_coefficients_per_axis: u32,
	/// Number of causal time elements.
	pub time_elements: u64,
	/// Number of temporal DG coefficients per time element.
	pub temporal_coefficients: u32,
	/// Total encoding/solver ancillas, including dilation and signal registers.
	pub auxiliary_qubits: usize,
	/// Exact byte budget for a complex-f64 statevector simulator.
	pub statevector_budget_bytes: String,
}

/// Exact sizes; admission is a memory check, never benchmark solution evidence.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ResourceEstimate {
	/// Full physical dimension; no independent mode is dropped.
	pub independent_dimension: usize,
	/// Full tensor configuration dimension as a decimal integer.
	pub configuration_dimension: String,
	/// Full causal DG history dimension as a decimal integer.
	pub history_dimension: String,
	/// Qubits for zero-padded logical history data.
	pub data_qubits: usize,
	/// Data plus supplied auxiliary qubits.
	pub total_qubits: usize,
	/// Exact bytes for the whole complex-f64 statevector, including auxiliaries.
	pub statevector_bytes: String,
	/// Whether this estimate fits the provided byte budget.
	pub fits_statevector_budget: bool,
	/// Always estimate-only: operator, oracle, accuracy and runtime admission are separate.
	pub status: String,
}

/// Account without narrowing exponentially large dimensions to machine integers.
///
/// # Errors
/// Rejects malformed counts and limits integer metadata itself to one mebibit.
#[allow(clippy::arithmetic_side_effects)]
pub fn estimate(request: &ResourceRequest) -> Result<ResourceEstimate, CfdError> {
	let independent_dimension = request
		.local_velocity_dimension
		.checked_sub(request.constraint_rank)
		.ok_or(CfdError::InvalidInput(
			"constraint rank exceeds local velocity dimension",
		))?;
	if independent_dimension == 0
		|| request.configuration_coefficients_per_axis < 2
		|| request.time_elements == 0
		|| request.temporal_coefficients == 0
	{
		return Err(CfdError::InvalidInput(
			"resource dimensions must be positive; configuration axis requires at least two coefficients",
		));
	}
	if request.statevector_budget_bytes.len() > 1_048_576 {
		return Err(CfdError::InvalidInput("resource budget metadata too large"));
	}
	let budget = request
		.statevector_budget_bytes
		.parse::<UBig>()
		.map_err(|_| CfdError::InvalidInput("byte budget must be an unsigned decimal integer"))?;
	let bits_per_axis = usize::try_from(request.configuration_coefficients_per_axis.bit_width())
		.map_err(|_| CfdError::InvalidInput("configuration bit count overflow"))?;
	let metadata_bound = independent_dimension
		.checked_mul(bits_per_axis)
		.and_then(|n| n.checked_add(request.auxiliary_qubits))
		.and_then(|n| n.checked_add(128))
		.ok_or(CfdError::InvalidInput("resource metadata width overflow"))?;
	if metadata_bound > 1_048_576 {
		return Err(CfdError::InvalidInput(
			"exact integer metadata exceeds the one-mebibit estimator budget",
		));
	}
	let configuration =
		UBig::from(request.configuration_coefficients_per_axis).pow(independent_dimension);
	let history = &configuration
		* UBig::from(request.time_elements)
		* UBig::from(request.temporal_coefficients);
	let data_qubits = (&history - UBig::ONE).bit_len();
	let total_qubits = data_qubits
		.checked_add(request.auxiliary_qubits)
		.ok_or(CfdError::InvalidInput("qubit count overflow"))?;
	let bytes = UBig::from(16_u8) << total_qubits;
	Ok(ResourceEstimate {
		independent_dimension,
		configuration_dimension: configuration.to_string(),
		history_dimension: history.to_string(),
		data_qubits,
		total_qubits,
		statevector_bytes: bytes.to_string(),
		fits_statevector_budget: bytes <= budget,
		status: "estimate-only; operator, accuracy and runtime admission required".to_owned(),
	})
}

/// Exact power expressions and rigorous qubit bounds without materializing giant integers.
#[derive(Clone, Debug, serde::Serialize)]
pub struct SymbolicEstimate {
	pub independent_dimension: usize,
	pub configuration_dimension: String,
	pub history_dimension: String,
	/// Inclusive bounds, identical when the configuration axis is a power of two.
	pub data_qubits_lower: String,
	pub data_qubits_upper: String,
	pub total_qubits_lower: String,
	pub total_qubits_upper: String,
	pub statevector_bytes_lower: String,
	pub statevector_bytes_upper: String,
	pub exceeds_statevector_budget: bool,
	pub status: String,
}
/// Retain the entire exponential dimension symbolically when decimal output is excessive.
///
/// No coordinate is dropped. Bounds use exact integer floor/ceiling logarithms;
/// non-power-of-two axes can give loose but rigorous qubit intervals.
/// # Errors
/// Rejects malformed ranks/counts/budget strings; never allocates the lifted tensor.
#[allow(
	clippy::arithmetic_side_effects,
	reason = "Only exact arbitrary-width arithmetic and checked machine dimensions"
)]
pub fn estimate_symbolic(request: &ResourceRequest) -> Result<SymbolicEstimate, CfdError> {
	let m = request
		.local_velocity_dimension
		.checked_sub(request.constraint_rank)
		.filter(|m| *m > 0)
		.ok_or(CfdError::InvalidInput(
			"positive complete kernel dimension required",
		))?;
	let axis = request.configuration_coefficients_per_axis;
	if axis < 2 || request.time_elements == 0 || request.temporal_coefficients == 0 {
		return Err(CfdError::InvalidInput("invalid symbolic tensor dimensions"));
	}
	if request.statevector_budget_bytes.len() > 1_048_576 {
		return Err(CfdError::InvalidInput("resource budget metadata too large"));
	}
	let budget = request
		.statevector_budget_bytes
		.parse::<UBig>()
		.map_err(|_| CfdError::InvalidInput("invalid byte budget"))?;
	let temporal = UBig::from(request.time_elements) * UBig::from(request.temporal_coefficients);
	let temporal_bits = (&temporal - UBig::ONE).bit_len();
	let floor = axis.ilog2();
	let ceil = if axis.is_power_of_two() {
		floor
	} else {
		floor + 1
	};
	let lower = UBig::from(m) * UBig::from(floor) + UBig::from(temporal_bits);
	let upper = UBig::from(m) * UBig::from(ceil) + UBig::from(temporal_bits);
	let total_lower = &lower + UBig::from(request.auxiliary_qubits);
	let total_upper = &upper + UBig::from(request.auxiliary_qubits);
	// Byte requirement is 2^(total_qubits+4). Comparing exponents avoids the giant allocation.
	let exceeds = &total_lower + UBig::from(4_u8) >= UBig::from(budget.bit_len());
	Ok(SymbolicEstimate {
		independent_dimension: m,
		configuration_dimension: format!("{axis}^{m}"),
		history_dimension: format!("{axis}^{m} * {temporal}"),
		data_qubits_lower: lower.to_string(),
		data_qubits_upper: upper.to_string(),
		total_qubits_lower: total_lower.to_string(),
		total_qubits_upper: total_upper.to_string(),
		statevector_bytes_lower: format!("16 * 2^{total_lower}"),
		statevector_bytes_upper: format!("16 * 2^{total_upper}"),
		exceeds_statevector_budget: exceeds,
		status: "symbolic full-resource estimate; no operator or quantum execution".to_owned(),
	})
}
