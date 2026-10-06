//! Owned sparse matching shards; no shard retains its source matching plan.
use crate::{Complex64, Error, MatchingEncoding, NumericalPolicy, Result};
use std::ops::{Add, Mul, Sub};

/// Common scalar manifest of the unitary and its original logical matrix.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MatchingHeader {
	pub rows: usize,
	pub cols: usize,
	pub system_qubits: usize,
	pub color_qubits: usize,
	pub num_colors: usize,
	pub beta: f64,
	pub alpha: f64,
	/// Source fingerprint; provenance only, not a cryptographic integrity proof.
	pub source_identity: u64,
	/// Number of completed sparse columns in the original encoding.
	pub record_count: usize,
	/// Commutative digest of completed column fields; accidental integrity only.
	pub record_digest: u64,
}
impl MatchingHeader {
	/// Derive the scalar manifest without retaining the complete encoding.
	/// # Errors
	/// Rejects record-count or field representation overflow.
	pub fn from_encoding(encoding: &MatchingEncoding) -> Result<Self> {
		let mut record_count = 0usize;
		let mut record_digest = 0u64;
		visit_columns(encoding, |column| {
			record_count = record_count
				.checked_add(1)
				.ok_or(Error::Budget("matching record count"))?;
			record_digest = record_digest.wrapping_add(column_digest(column)?);
			Ok(())
		})?;
		Ok(Self {
			rows: encoding.rows(),
			cols: encoding.cols(),
			system_qubits: encoding.system_qubits(),
			color_qubits: encoding.color_qubits(),
			num_colors: encoding.num_colors(),
			beta: encoding.beta(),
			alpha: encoding.normalization().get(),
			source_identity: encoding.source_identity(),
			record_count,
			record_digest,
		})
	}

	/// # Errors
	/// Rejects invalid dimensions, widths, normalizations and color padding.
	pub fn validate(self) -> Result<()> {
		let system = self.system_dimension()?;
		let colors = 1usize
			.checked_shl(
				u32::try_from(self.color_qubits).map_err(|_| Error::Budget("shard color width"))?,
			)
			.ok_or(Error::Budget("shard color width"))?;
		if self.rows == 0
			|| self.cols == 0
			|| self.rows.max(self.cols) > system
			|| colors != self.num_colors
			|| !self.beta.is_finite()
			|| self.beta <= 0.0
			|| !self.alpha.is_finite()
			|| self.alpha <= 0.0
		{
			return Err(Error::Encoding("invalid matching shard manifest"));
		}
		let color_count = u32::try_from(colors)
			.map_err(|_| Error::Budget("matching color count representation"))?;
		if self.alpha.to_bits() != self.beta.mul(f64::from(color_count)).to_bits() {
			return Err(Error::Encoding("noncanonical matching normalization"));
		}
		self.num_qubits()?;
		Ok(())
	}
	/// # Errors
	/// Rejects width overflow.
	pub fn system_dimension(self) -> Result<usize> {
		1usize
			.checked_shl(
				u32::try_from(self.system_qubits)
					.map_err(|_| Error::Budget("shard system width"))?,
			)
			.ok_or(Error::Budget("shard system width"))
	}
	/// # Errors
	/// Rejects width overflow.
	pub fn num_qubits(self) -> Result<usize> {
		let width = self
			.system_qubits
			.checked_add(self.color_qubits)
			.and_then(|n| n.checked_add(1))
			.ok_or(Error::Budget("shard width"))?;
		1usize
			.checked_shl(u32::try_from(width).map_err(|_| Error::Budget("shard width"))?)
			.ok_or(Error::Budget("shard width"))?;
		Ok(width)
	}
}

/// One completed permutation column and its flag-sector two-by-two rotation.
/// Missing columns are identity permutations with zero success coefficient.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MatchingColumn {
	pub color: usize,
	pub source: usize,
	pub destination: usize,
	pub cosine: f64,
	pub sine: f64,
	pub phase: Complex64,
}
impl MatchingColumn {
	/// Apply the whole flag rotation and phase, or their adjoint, to arbitrary
	/// flag amplitudes. Permutation routing is the caller's separate obligation.
	#[must_use]
	pub fn rotate(self, pair: [Complex64; 2], adjoint: bool) -> [Complex64; 2] {
		if adjoint {
			let success = pair[0].mul(self.phase.conj());
			[
				success.mul(self.cosine).add(pair[1].mul(self.sine)),
				pair[1].mul(self.cosine).sub(success.mul(self.sine)),
			]
		} else {
			[
				(pair[0].mul(self.cosine).sub(pair[1].mul(self.sine))).mul(self.phase),
				pair[0].mul(self.sine).add(pair[1].mul(self.cosine)),
			]
		}
	}
}

/// Sparse input-column partition.
///
/// Ownership is `source.checked_rem(parts) == Some(rank)`, for
/// every color, including completion-only columns. Only scalar manifest data
/// are replicated. Untouched coordinates and padded dummy colors are implicit.
#[derive(Debug, Clone)]
pub struct MatchingShard {
	header: MatchingHeader,
	rank: usize,
	parts: usize,
	records: Vec<MatchingColumn>,
}
impl MatchingShard {
	/// Summarize immutable local columns for collective manifest admission.
	/// This additive fingerprint detects accidental corruption, not adversarial changes.
	/// # Errors
	/// Rejects column-field representation overflow.
	pub fn summarize_records(records: &[MatchingColumn]) -> Result<(usize, u64)> {
		Ok((records.len(), digest(records)?))
	}

	/// Snapshot this partition from an existing plan, retaining no parent storage.
	/// This is a preprocessing convenience; distributed applications distribute
	/// these owned shards before releasing the complete source plan.
	/// # Errors
	/// Rejects invalid ownership, malformed source metadata and storage limits.
	pub fn from_encoding(
		encoding: &MatchingEncoding,
		rank: usize,
		parts: usize,
		policy: NumericalPolicy,
	) -> Result<Self> {
		if parts == 0 || !parts.is_power_of_two() || rank >= parts {
			return Err(Error::Encoding("invalid matching shard ownership"));
		}
		let count = encoding
			.matchings()
			.iter()
			.map(|matching| {
				matching
					.permutation()
					.iter()
					.filter(|&&(source, _)| source.checked_rem(parts) == Some(rank))
					.count()
			})
			.sum();
		let bytes = storage_bytes(count)?
			.checked_add(crate::RECORD_FINGERPRINT_SCRATCH_BYTES)
			.ok_or(Error::Budget("matching hash scratch"))?;
		if bytes > policy.max_bytes {
			return Err(Error::Budget("matching shard storage"));
		}
		let header = MatchingHeader::from_encoding(encoding)?;
		let mut records = Vec::new();
		records
			.try_reserve_exact(count)
			.map_err(|_| Error::Budget("matching shard allocation"))?;
		visit_columns(encoding, |column| {
			if column.source.checked_rem(parts) == Some(rank) {
				records.push(column);
			}
			Ok(())
		})?;
		Self::from_parts(header, rank, parts, records, policy)
	}
	/// Admit already-owned serialized records. This validates local records;
	/// global permutation closure/bijection must be admitted by the collective
	/// runtime before any native mutation.
	/// # Errors
	/// Rejects ownership, duplicates, nonfinite/nonunitary parameters and budgets.
	pub fn from_parts(
		header: MatchingHeader,
		rank: usize,
		parts: usize,
		mut records: Vec<MatchingColumn>,
		policy: NumericalPolicy,
	) -> Result<Self> {
		header.validate()?;
		if parts == 0 || !parts.is_power_of_two() || rank >= parts {
			return Err(Error::Encoding("invalid matching shard ownership"));
		}
		let peak = storage_bytes(records.capacity())?
			.checked_add(if parts == 1 {
				crate::RECORD_FINGERPRINT_SCRATCH_BYTES
			} else {
				0
			})
			.ok_or(Error::Budget("matching hash scratch"))?;
		if peak > policy.max_bytes {
			return Err(Error::Budget("matching shard storage"));
		}
		let system = header.system_dimension()?;
		for record in &records {
			if record.color >= header.num_colors
				|| record.source >= system
				|| record.destination >= system
				|| record.source.checked_rem(parts) != Some(rank)
				|| !record.cosine.is_finite()
				|| !record.sine.is_finite()
				|| record.cosine < 0.0
				|| record.sine < 0.0
				|| record
					.cosine
					.mul_add(record.cosine, record.sine.mul(record.sine))
					.sub(1.0)
					.abs()
					> 1e-12
				|| !record.phase.re.is_finite()
				|| !record.phase.im.is_finite()
				|| record.phase.norm_sqr().sub(1.0).abs() > 1e-12
			{
				return Err(Error::Encoding("invalid matching shard column"));
			}
		}
		records.sort_unstable_by_key(|record| (record.color, record.source));
		if records
			.windows(2)
			.any(|pair| matches!(pair, [a, b] if a.color == b.color && a.source == b.source))
		{
			return Err(Error::Encoding("duplicate matching shard column"));
		}
		if records.len() > header.record_count
			|| (parts == 1
				&& (records.len() != header.record_count
					|| digest(&records)? != header.record_digest))
		{
			return Err(Error::Encoding("matching payload differs from manifest"));
		}
		Ok(Self {
			header,
			rank,
			parts,
			records,
		})
	}
	#[must_use]
	pub const fn header(&self) -> MatchingHeader {
		self.header
	}
	#[must_use]
	pub const fn rank(&self) -> usize {
		self.rank
	}
	#[must_use]
	pub const fn parts(&self) -> usize {
		self.parts
	}
	#[must_use]
	pub fn records(&self) -> &[MatchingColumn] {
		&self.records
	}
	/// Summary of owned records for collective payload integrity admission.
	/// # Errors
	/// Rejects unrepresentable field values.
	pub fn payload_summary(&self) -> Result<(usize, u64)> {
		Ok((self.records.len(), digest(&self.records)?))
	}
	/// # Errors
	/// Rejects byte-accounting overflow.
	pub fn storage_bytes(&self) -> Result<usize> {
		storage_bytes(self.records.capacity())
	}
	/// Lookup this owner's column, including implicit zero/dummy columns.
	/// # Errors
	/// Rejects indices outside the manifest or owned by a different partition.
	pub fn column(&self, color: usize, source: usize) -> Result<MatchingColumn> {
		if color >= self.header.num_colors
			|| source >= self.header.system_dimension()?
			|| source.checked_rem(self.parts) != Some(self.rank)
		{
			return Err(Error::Encoding("matching column outside shard ownership"));
		}
		Ok(self
			.records
			.binary_search_by_key(&(color, source), |record| (record.color, record.source))
			.ok()
			.and_then(|index| self.records.get(index))
			.copied()
			.unwrap_or_else(|| zero_column(color, source)))
	}
}
const fn zero_column(color: usize, source: usize) -> MatchingColumn {
	MatchingColumn {
		color,
		source,
		destination: source,
		cosine: 0.0,
		sine: 1.0,
		phase: Complex64::new(1.0, 0.0),
	}
}
fn storage_bytes(count: usize) -> Result<usize> {
	count
		.checked_mul(size_of::<MatchingColumn>())
		.and_then(|n| n.checked_add(size_of::<MatchingShard>()))
		.ok_or(Error::Budget("matching shard bytes"))
}

fn visit_columns(
	encoding: &MatchingEncoding,
	mut visit: impl FnMut(MatchingColumn) -> Result<()>,
) -> Result<()> {
	for (color, matching) in encoding.matchings().iter().enumerate() {
		for &(source, destination) in matching.permutation() {
			let mut column = zero_column(color, source);
			column.destination = destination;
			if let Some(edge) = matching.entry(source) {
				let (sine, cosine) = edge.theta.mul(0.5).sin_cos();
				column.cosine = cosine;
				column.sine = sine;
				column.phase = Complex64::from_polar(1.0, edge.phase);
			}
			visit(column)?;
		}
	}
	Ok(())
}
fn digest(records: &[MatchingColumn]) -> Result<u64> {
	records.iter().try_fold(0u64, |digest, &column| {
		Ok(digest.wrapping_add(column_digest(column)?))
	})
}
fn column_digest(column: MatchingColumn) -> Result<u64> {
	let words = [
		u64::try_from(column.color).map_err(|_| Error::Budget("digest color"))?,
		u64::try_from(column.source).map_err(|_| Error::Budget("digest source"))?,
		u64::try_from(column.destination).map_err(|_| Error::Budget("digest destination"))?,
		column.cosine.to_bits(),
		column.sine.to_bits(),
		column.phase.re.to_bits(),
		column.phase.im.to_bits(),
	];
	Ok(crate::record_fingerprint(0x4d43_4f4c_554d_4e32, words))
}
