//! Explicit bounded XOR QROM; table traversal and every controlled-X are charged.
use super::{PortfolioLimits, PortfolioResources, add, admit, bits, mul};
use crate::{Error, ReplayGate, ReplayKind, Result};
use std::sync::Arc;
#[derive(Debug)]
struct Data {
	words: Arc<[u64]>,
	address: usize,
	output: usize,
	resources: PortfolioResources,
}
/// |address,output> -> |address,output XOR table[address]>.
/// Padded addresses hold zero; lookup/unlookup remain unitary on dirty outputs.
#[derive(Clone, Debug)]
pub struct Qrom(Arc<Data>);
impl Qrom {
	/// # Errors
	/// Rejects empty tables, insufficient output width, retained input capacity and budgets.
	pub fn new(words: Vec<u64>, output_bits: usize, l: PortfolioLimits) -> Result<Self> {
		if words.is_empty() {
			return Err(Error::Encoding("empty QROM"));
		}
		let dimension = words
			.len()
			.checked_next_power_of_two()
			.ok_or(Error::Budget("QROM address"))?;
		let address =
			usize::try_from(dimension.ilog2()).map_err(|_| Error::Budget("QROM address"))?;
		bits(add(address, output_bits)?)?;
		let output_dimension =
			u64::try_from(bits(output_bits)?).map_err(|_| Error::Budget("QROM word"))?;
		let retained = add(
			add(size_of::<Data>(), 64)?,
			mul(dimension, size_of::<u64>())?,
		)?;
		let peak = add(
			add(retained, mul(words.capacity(), size_of::<u64>())?)?,
			add(mul(dimension, size_of::<u64>())?, 4096)?,
		)?;
		let compile_work = mul(dimension, output_bits.max(1))?;
		admit(
			PortfolioResources {
				table_entries: dimension,
				compile_work,
				retained_bytes: retained,
				construction_peak_bytes: peak,
				..PortfolioResources::default()
			},
			l,
		)?;
		if words.iter().any(|w| *w >= output_dimension) {
			return Err(Error::Encoding("QROM output width"));
		}
		let gates = words.iter().try_fold(0, |n, w| {
			add(
				n,
				usize::try_from(w.count_ones()).map_err(|_| Error::Budget("QROM gate count"))?,
			)
		})?;
		let resources = PortfolioResources {
			elementary_gates: gates,
			oracle_queries: 1,
			table_entries: dimension,
			workspace_qubits: output_bits,
			compile_work,
			retained_bytes: retained,
			construction_peak_bytes: peak,
			..PortfolioResources::default()
		};
		admit(resources, l)?;
		let mut padded = crate::matching::reserve(dimension)?;
		padded.extend(words);
		padded.resize(dimension, 0);
		Ok(Self(Arc::new(Data {
			words: Arc::from(padded),
			address,
			output: output_bits,
			resources,
		})))
	}
	#[must_use]
	pub fn address_bits(&self) -> usize {
		self.0.address
	}
	#[must_use]
	pub fn output_bits(&self) -> usize {
		self.0.output
	}
	#[must_use]
	pub fn resources(&self) -> PortfolioResources {
		self.0.resources
	}
	/// Classical metadata inspection, not a free coherent lookup.
	#[must_use]
	pub fn words(&self) -> &[u64] {
		&self.0.words
	}
	/// One actual query or literal reversed unquery, with signed outer controls.
	/// # Errors
	/// Rejects invalid/overlapping operands before emitting any gate.
	pub fn visit_lookup(
		&self,
		address: &[usize],
		output: &[usize],
		mask: usize,
		value: usize,
		unlookup: bool,
		v: &mut dyn FnMut(ReplayGate) -> Result<()>,
	) -> Result<()> {
		if address.len() != self.0.address || output.len() != self.0.output || value & !mask != 0 {
			return Err(Error::Encoding("QROM operand widths/controls"));
		}
		let mut occupied = mask;
		for target in address.iter().chain(output) {
			let bit = bits(*target)?;
			if occupied & bit != 0 {
				return Err(Error::Encoding("QROM operand overlap"));
			}
			occupied |= bit;
		}
		let mut address_mask = mask;
		for target in address {
			address_mask |= bits(*target)?;
		}
		for ordinal in 0..self.0.words.len() {
			let index = if unlookup {
				self.0
					.words
					.len()
					.checked_sub(ordinal)
					.and_then(|n| n.checked_sub(1))
					.ok_or(Error::Budget("QROM reverse"))?
			} else {
				ordinal
			};
			let word = *self
				.0
				.words
				.get(index)
				.ok_or(Error::Encoding("QROM table coverage"))?;
			let mut address_value = value;
			for (bit, target) in address.iter().enumerate() {
				if index & bits(bit)? != 0 {
					address_value |= bits(*target)?;
				}
			}
			for ordinal in 0..output.len() {
				let bit = if unlookup {
					output
						.len()
						.checked_sub(ordinal)
						.and_then(|n| n.checked_sub(1))
						.ok_or(Error::Budget("QROM reverse word"))?
				} else {
					ordinal
				};
				if word
					& u64::try_from(bits(bit)?).map_err(|_| Error::Budget("QROM output word"))?
					!= 0
				{
					v(ReplayGate {
						kind: ReplayKind::X,
						target: Some(
							*output
								.get(bit)
								.ok_or(Error::Encoding("QROM output coverage"))?,
						),
						control_mask: address_mask,
						control_value: address_value,
					})?;
				}
			}
		}
		Ok(())
	}
}
