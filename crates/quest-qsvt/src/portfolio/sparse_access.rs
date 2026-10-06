//! Explicit local QROM sparse-access baseline with lookup, inverse ranks and values.
//!
//! Columns/slots map bijectively to rows/slots using bounded stored tables.
//! Lookup, register swaps and inverse unlookup implement the actual permutation.
//! This uses padded equal row/column sparsity S, with alpha=S max|Aij|; it does
//! not claim the asymmetric optimal oracle cost of a black-box sparse-access lemma.
#![allow(
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	reason = "Padded table dimensions, full physical width and simultaneous construction buffers are admitted"
)]
use super::{
	PortfolioLimits, PortfolioResources, Qrom, add, admit, bits, count_with_work, hash, mul,
	targets, word,
};
use crate::{
	Complex64, EncodingDescriptor, EncodingErrors, Error, ReplayEncoding, ReplayGate, ReplayKind,
	Result,
};
use std::sync::Arc;
#[derive(Clone, Copy, Debug)]
struct Value {
	word: u64,
	theta: f64,
	phase: f64,
}
#[derive(Debug)]
struct Data {
	column: Qrom,
	row: Qrom,
	values: Qrom,
	rotations: Vec<Value>,
	descriptor: EncodingDescriptor,
	resources: PortfolioResources,
	address: usize,
	slots: usize,
	precision: usize,
	error_bound: f64,
	max_bytes: usize,
}
/// Bounded stored-table baseline for a supplied local sparse matrix.
///
/// It performs four QROM queries per whole replay, with explicit magnitude/phase
/// precision and clean location/value scratch. It is not an MPI sparse producer.
#[derive(Clone, Debug)]
pub struct SparseAccessEncoding(Arc<Data>);
impl SparseAccessEncoding {
	/// # Errors
	/// Rejects precision, table, input capacity, complete construction work and gate budgets.
	#[allow(
		clippy::too_many_lines,
		reason = "Linear bounded-table compilation admits each simultaneous source/table phase before ownership transfer"
	)]
	pub fn new(
		matrix: &quest_numerics::SparseMatrix,
		precision: usize,
		l: PortfolioLimits,
	) -> Result<Self> {
		if !(2..=24).contains(&precision) {
			return Err(Error::Encoding(
				"sparse-access precision must be 2..=24 bits",
			));
		}
		let dimension = matrix
			.rows()
			.max(matrix.cols())
			.checked_next_power_of_two()
			.ok_or(Error::Budget("sparse-access system"))?;
		let system = usize::try_from(dimension.ilog2())
			.map_err(|_| Error::Budget("sparse-access system"))?;
		let input = matrix.retained_bytes()?;
		let mut peak = add(
			input,
			add(
				mul(matrix.nnz(), size_of::<(usize, usize, Complex64)>() * 2)?,
				mul(dimension, size_of::<usize>() * 4)?,
			)?,
		)?;
		peak = add(peak, 4096)?;
		let pre_work = add(
			mul(
				matrix.nnz(),
				add(
					usize::try_from(matrix.nnz().max(1).ilog2())
						.map_err(|_| Error::Budget("sparse sort work"))?,
					8,
				)?,
			)?,
			mul(dimension, 4)?,
		)?;
		if peak > l.max_bytes || pre_work > l.max_compile_work || dimension > l.max_table_entries {
			return Err(Error::Budget("sparse-access input/sorting"));
		}
		let mut entries = crate::matching::reserve(matrix.nnz())?;
		entries.extend(matrix.entries());
		entries.sort_unstable_by_key(|e| (e.0, e.1));
		let mut row_counts = crate::matching::reserve(dimension)?;
		row_counts.resize(dimension, 0usize);
		let mut column_counts = row_counts.clone();
		let mut beta = 0.0_f64;
		let mut source = hash([
			0x5350_4152_5345_4131,
			word(matrix.rows())?,
			word(matrix.cols())?,
		]);
		for &(row, column, value) in &entries {
			if !value.re.is_finite() || !value.im.is_finite() {
				return Err(Error::NonFinite);
			}
			row_counts[row] = add(row_counts[row], 1)?;
			column_counts[column] = add(column_counts[column], 1)?;
			beta = beta.max(value.norm());
			source = hash([
				source,
				word(row)?,
				word(column)?,
				value.re.to_bits(),
				value.im.to_bits(),
			]);
		}
		if !beta.is_finite() {
			return Err(Error::NonFinite);
		}
		if beta == 0.0 {
			beta = 1.0;
		}
		let sparsity = row_counts
			.iter()
			.chain(&column_counts)
			.copied()
			.max()
			.unwrap_or(1)
			.max(1)
			.checked_next_power_of_two()
			.ok_or(Error::Budget("sparse-access slots"))?;
		let slots =
			usize::try_from(sparsity.ilog2()).map_err(|_| Error::Budget("sparse-access slots"))?;
		let address = add(system, slots)?;
		let table = bits(address)?;
		let value_bits = add(mul(precision, 2)?, 1)?;
		let width = add(add(mul(address, 2)?, value_bits)?, 1)?;
		bits(width)?;
		let table_work = mul(
			table,
			add(add(mul(address, 16)?, mul(precision, 16)?)?, 128)?,
		)?;
		let compile_work = add(pre_work, table_work)?;
		peak = add(
			peak,
			mul(table, size_of::<u64>() * 12 + size_of::<Value>() * 2)?,
		)?;
		if peak > l.max_bytes
			|| mul(table, 3)? > l.max_table_entries
			|| compile_work > l.max_compile_work
		{
			return Err(Error::Budget("sparse-access table construction"));
		}
		let mut forward = crate::matching::reserve(table)?;
		forward.resize(table, usize::MAX);
		let mut reverse = forward.clone();
		let mut payload = crate::matching::reserve(table)?;
		payload.resize(table, 0u64);
		row_counts.fill(0);
		column_counts.fill(0);
		let scale = bits(precision)?;
		let mut largest_error = 0.0_f64;
		for &(row, column, value) in &entries {
			let from = column * sparsity + column_counts[column];
			let to = row * sparsity + row_counts[row];
			column_counts[column] += 1;
			row_counts[row] += 1;
			forward[from] = to;
			reverse[to] = from;
			let magnitude = quantize(
				(value.norm() / beta).clamp(0.0, 1.0)
					* f64::from(
						u32::try_from(scale).map_err(|_| Error::Budget("precision scale"))?,
					),
			)?;
			let phase = quantize(
				value.arg().rem_euclid(std::f64::consts::TAU) / std::f64::consts::TAU
					* f64::from(
						u32::try_from(scale).map_err(|_| Error::Budget("precision scale"))?,
					),
			)? % word(scale)?;
			let encoded = magnitude | (phase << (precision + 1));
			payload[from] = encoded;
			let decoded = Complex64::from_polar(
				f64::from(
					u32::try_from(magnitude).map_err(|_| Error::Budget("magnitude precision"))?,
				) / f64::from(u32::try_from(scale).map_err(|_| Error::Budget("precision scale"))?),
				std::f64::consts::TAU
					* f64::from(
						u32::try_from(phase).map_err(|_| Error::Budget("phase precision"))?,
					)
					/ f64::from(
						u32::try_from(scale).map_err(|_| Error::Budget("precision scale"))?,
					),
			) * beta;
			largest_error = largest_error.max((decoded - value).norm());
		}
		let mut next = 0;
		for (from, to) in forward.iter_mut().enumerate() {
			if *to == usize::MAX {
				while reverse[next] != usize::MAX {
					next += 1;
				}
				*to = next;
				reverse[next] = from;
			}
		}
		let mut ql = l;
		ql.max_bytes = l.max_bytes.saturating_sub(input);
		let column = Qrom::new(
			forward.into_iter().map(word).collect::<Result<Vec<_>>>()?,
			address,
			ql,
		)?;
		let row = Qrom::new(
			reverse.into_iter().map(word).collect::<Result<Vec<_>>>()?,
			address,
			ql,
		)?;
		let mut rotations = crate::matching::reserve(table)?;
		let mut unique = payload.clone();
		unique.sort_unstable();
		unique.dedup();
		for w in unique {
			let magnitude = w & (word(bits(precision + 1)?)? - 1);
			let phase = w >> (precision + 1);
			rotations.push(Value {
				word: w,
				theta: 2.0
					* (f64::from(
						u32::try_from(magnitude).map_err(|_| Error::Budget("magnitude word"))?,
					) / f64::from(
						u32::try_from(scale).map_err(|_| Error::Budget("precision scale"))?,
					))
					.acos(),
				phase: std::f64::consts::TAU
					* f64::from(u32::try_from(phase).map_err(|_| Error::Budget("phase word"))?)
					/ f64::from(
						u32::try_from(scale).map_err(|_| Error::Budget("precision scale"))?,
					),
			});
		}
		let values = Qrom::new(payload, value_bits, ql)?;
		let normalization = beta
			* f64::from(
				u32::try_from(sparsity).map_err(|_| Error::Budget("sparse normalization"))?,
			);
		let error_bound = largest_error
			* f64::from(u32::try_from(sparsity).map_err(|_| Error::Budget("sparse error"))?);
		let mut descriptor = super::structured_maps::descriptor(
			system,
			add(slots, 1)?,
			add(address, value_bits)?,
			matrix.rows(),
			matrix.cols(),
			normalization,
			source,
			hash([
				source,
				word(precision)?,
				word(sparsity)?,
				0x5152_4f4d_554e_4931,
			]),
		)?;
		// Deterministic stored coefficient discrepancy; binary64 synthesis remains uncertified.
		descriptor.errors = EncodingErrors {
			preparation: Some(0.0),
			encoding: None,
			binary64_parameters: true,
		};
		let retained = add(
			size_of::<Data>() + 64,
			add(
				add(
					column.resources().retained_bytes,
					row.resources().retained_bytes,
				)?,
				add(
					values.resources().retained_bytes,
					mul(rotations.capacity(), size_of::<Value>())?,
				)?,
			)?,
		)?;
		let resources = PortfolioResources {
			oracle_queries: 4,
			table_entries: mul(table, 3)?,
			precision_bits: precision,
			workspace_qubits: width - system,
			preparation_gates: mul(slots, 2)?,
			compile_work,
			retained_bytes: retained,
			construction_peak_bytes: peak.max(add(retained, 4096)?),
			..PortfolioResources::default()
		};
		admit(resources, l)?;
		let mut result = Self(Arc::new(Data {
			column,
			row,
			values,
			rotations,
			descriptor,
			resources,
			address,
			slots,
			precision,
			error_bound,
			max_bytes: l.max_bytes,
		}));
		let mut compile_work = result.0.resources.compile_work;
		let gates = count_with_work(&result, l, &mut compile_work)?;
		let data = Arc::get_mut(&mut result.0).ok_or(Error::Encoding("sparse recipe ownership"))?;
		data.resources.elementary_gates = gates;
		data.resources.compile_work = compile_work;
		admit(data.resources, l)?;
		Ok(result)
	}
	#[must_use]
	pub fn resources(&self) -> PortfolioResources {
		self.0.resources
	}
	/// Observed binary64 coefficient-discrepancy norm estimate; not a synthesis certificate.
	#[must_use]
	pub fn coefficient_error_estimate(&self) -> f64 {
		self.0.error_bound
	}
	/// Coherent column/slot -> row/row-slot XOR location lookup (or its unlookup).
	/// # Errors
	/// Rejects invalid controls/layouts before gate emission.
	pub fn visit_column_location(
		&self,
		address: &[usize],
		output: &[usize],
		unlookup: bool,
		v: &mut dyn FnMut(ReplayGate) -> Result<()>,
	) -> Result<()> {
		self.0
			.column
			.visit_lookup(address, output, 0, 0, unlookup, v)
	}
	/// Coherent row/slot -> column/column-slot XOR location lookup (or unlookup).
	/// # Errors
	/// Rejects invalid controls/layouts before gate emission.
	pub fn visit_row_location(
		&self,
		address: &[usize],
		output: &[usize],
		unlookup: bool,
		v: &mut dyn FnMut(ReplayGate) -> Result<()>,
	) -> Result<()> {
		self.0.row.visit_lookup(address, output, 0, 0, unlookup, v)
	}
	/// Column/slot indexed binary magnitude and phase XOR lookup or unlookup.
	/// Magnitudes use `precision+1` bits (including the endpoint one), followed by
	/// `precision` phase bits describing fractions of a full turn.
	/// # Errors
	/// Rejects malformed value/address operands before emitting gates.
	pub fn visit_value(
		&self,
		address: &[usize],
		output: &[usize],
		unlookup: bool,
		v: &mut dyn FnMut(ReplayGate) -> Result<()>,
	) -> Result<()> {
		self.0
			.values
			.visit_lookup(address, output, 0, 0, unlookup, v)
	}
	/// Bounded classical reference execution, without a dense whole-unitary table.
	/// # Errors
	/// Rejects state/gate work, state width and finite payloads before mutation.
	pub fn apply_reference(
		&self,
		state: &mut [Complex64],
		adjoint: bool,
		max_work: usize,
	) -> Result<()> {
		if add(
			mul(state.len(), size_of::<Complex64>())?,
			self.0.resources.retained_bytes,
		)? > self.0.max_bytes
			|| state.len() != bits(self.0.descriptor.layout.num_qubits)?
			|| mul(state.len(), self.0.resources.elementary_gates)? > max_work
			|| state.iter().any(|v| !v.re.is_finite() || !v.im.is_finite())
		{
			return Err(Error::Budget("sparse-access classical reference"));
		}
		self.visit_replay(adjoint, &mut |g| crate::owned_replay::apply_gate(state, g))
	}
	fn data(&self, adjoint: bool, v: &mut dyn FnMut(ReplayGate) -> Result<()>) -> Result<()> {
		let start = 1 + 2 * self.0.address;
		let mask = (bits(2 * self.0.precision + 1)? - 1) << start;
		let default = ReplayGate {
			kind: ReplayKind::Ry(if adjoint {
				-std::f64::consts::PI
			} else {
				std::f64::consts::PI
			}),
			target: Some(0),
			control_mask: 0,
			control_value: 0,
		};
		if !adjoint {
			v(default)?;
		}
		for ordinal in 0..self.0.rotations.len() {
			let index = if adjoint {
				self.0.rotations.len() - 1 - ordinal
			} else {
				ordinal
			};
			let r = self.0.rotations[index];
			let value = usize::try_from(r.word)
				.map_err(|_| Error::Budget("sparse value controls"))?
				<< start;
			let rotate = ReplayGate {
				kind: ReplayKind::Ry(if adjoint {
					std::f64::consts::PI - r.theta
				} else {
					r.theta - std::f64::consts::PI
				}),
				target: Some(0),
				control_mask: mask,
				control_value: value,
			};
			let phase = ReplayGate {
				kind: ReplayKind::Phase(if adjoint { -r.phase } else { r.phase }),
				target: None,
				control_mask: mask | 1,
				control_value: value,
			};
			if adjoint {
				v(phase)?;
				v(rotate)?;
			} else {
				v(rotate)?;
				v(phase)?;
			}
		}
		if adjoint {
			v(default)?;
		}
		Ok(())
	}
	fn permutation(
		&self,
		adjoint: bool,
		v: &mut dyn FnMut(ReplayGate) -> Result<()>,
	) -> Result<()> {
		let a = targets(1, self.0.address)?;
		let b = targets(1 + self.0.address, self.0.address)?;
		let (first, last) = if adjoint {
			(&self.0.row, &self.0.column)
		} else {
			(&self.0.column, &self.0.row)
		};
		first.visit_lookup(&a[..self.0.address], &b[..self.0.address], 0, 0, false, v)?;
		for ordinal in 0..self.0.address {
			let i = if adjoint {
				self.0.address - 1 - ordinal
			} else {
				ordinal
			};
			for (target, control) in [(a[i], b[i]), (b[i], a[i]), (a[i], b[i])] {
				v(ReplayGate {
					kind: ReplayKind::X,
					target: Some(target),
					control_mask: bits(control)?,
					control_value: bits(control)?,
				})?;
			}
		}
		last.visit_lookup(&a[..self.0.address], &b[..self.0.address], 0, 0, true, v)
	}
}
#[allow(
	clippy::cast_possible_truncation,
	clippy::cast_sign_loss,
	clippy::as_conversions,
	reason = "Validated finite nonnegative fixed-point payload fits within 25 bits before conversion"
)]
fn quantize(value: f64) -> Result<u64> {
	let rounded = value.round();
	if !rounded.is_finite() || !(0.0..=33_554_432.0).contains(&rounded) {
		return Err(Error::Encoding("fixed-point payload overflow"));
	}
	Ok(rounded as u64)
}
impl ReplayEncoding for SparseAccessEncoding {
	fn descriptor(&self) -> Result<EncodingDescriptor> {
		Ok(self.0.descriptor.clone())
	}
	fn retained_bytes(&self) -> Result<usize> {
		Ok(self.0.resources.retained_bytes)
	}
	fn visit_replay(
		&self,
		adjoint: bool,
		v: &mut dyn FnMut(ReplayGate) -> Result<()>,
	) -> Result<()> {
		for ordinal in 0..self.0.slots {
			let bit = if adjoint {
				self.0.slots - 1 - ordinal
			} else {
				ordinal
			};
			v(ReplayGate {
				kind: ReplayKind::H,
				target: Some(1 + bit),
				control_mask: 0,
				control_value: 0,
			})?;
		}
		let address = targets(1, self.0.address)?;
		let values = targets(1 + 2 * self.0.address, 2 * self.0.precision + 1)?;
		if adjoint {
			self.permutation(true, v)?;
		}
		self.0.values.visit_lookup(
			&address[..self.0.address],
			&values[..=2 * self.0.precision],
			0,
			0,
			false,
			v,
		)?;
		self.data(adjoint, v)?;
		self.0.values.visit_lookup(
			&address[..self.0.address],
			&values[..=2 * self.0.precision],
			0,
			0,
			true,
			v,
		)?;
		if !adjoint {
			self.permutation(false, v)?;
		}
		for ordinal in 0..self.0.slots {
			let bit = if adjoint {
				self.0.slots - 1 - ordinal
			} else {
				ordinal
			};
			v(ReplayGate {
				kind: ReplayKind::H,
				target: Some(1 + bit),
				control_mask: 0,
				control_value: 0,
			})?;
		}
		Ok(())
	}
}
