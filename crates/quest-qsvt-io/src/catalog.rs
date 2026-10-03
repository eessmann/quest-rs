use crate::{Complex64, Error, IoPolicy, Result};
use hdf5_metno::{
	File, Group, LinkType,
	plist::dataset_create::Layout,
	types::{FloatSize, TypeDescriptor},
};
use quest_polynomial::{Chebyshev, Polynomial};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
	fmt::Write as _,
	io::{Read, Write},
	path::Path,
};

// These are literal placeholders in the upstream path template.
#[allow(clippy::literal_string_with_formatting_args)]
const COEFFICIENT_PATH: &str = "/poly/{epsilon}/{kappa}";

fn digest_hex(bytes: &[u8]) -> Result<String> {
	let capacity = bytes
		.len()
		.checked_mul(2)
		.ok_or(Error::Budget("catalog digest"))?;
	let mut hex = String::new();
	hex.try_reserve_exact(capacity)
		.map_err(|_| Error::Budget("catalog digest allocation"))?;
	for byte in bytes {
		write!(&mut hex, "{byte:02x}").map_err(|_| Error::Format("catalog digest formatting"))?;
	}
	Ok(hex)
}

fn family_storage(count: usize, policy: IoPolicy) -> Result<usize> {
	let bytes = count
		.checked_mul(size_of::<CatalogFamily>())
		.ok_or(Error::Budget("catalog family storage"))?;
	if bytes > policy.max_bytes || isize::try_from(bytes).is_err() {
		return Err(Error::Budget("catalog family storage"));
	}
	Ok(bytes)
}

/// Dataset provenance and declared schema. Labels and hashes are not certificates.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogSource {
	pub schema_version: u32,
	pub dataset: String,
	pub source_page: String,
	pub download_url: String,
	pub resolved_url: String,
	pub retrieved_on: String,
	pub file: String,
	pub size_bytes: u64,
	pub sha256: String,
	pub coefficient_path: String,
	pub basis: String,
	pub coefficient_order: String,
	pub target: String,
	pub kappas: Vec<u32>,
	pub epsilons: Vec<String>,
	pub author: String,
	pub license: String,
	pub license_url: String,
}
impl CatalogSource {
	fn validate(&self, policy: IoPolicy) -> Result<()> {
		if self.schema_version != 1
			|| self.dataset != "inverse"
			|| self.basis != "Chebyshev"
			|| self.coefficient_order != "ascending"
			|| self.coefficient_path != COEFFICIENT_PATH
			|| self.target != "1 / (2 * kappa * x)"
			|| self.sha256.len() != 64
			|| !self.sha256.bytes().all(|b| b.is_ascii_hexdigit())
			|| self.kappas.is_empty()
			|| self.epsilons.is_empty()
		{
			return Err(Error::Format("unsupported inverse catalog manifest"));
		}
		let bytes =
			usize::try_from(self.size_bytes).map_err(|_| Error::Budget("catalog source size"))?;
		if bytes == 0 || bytes > policy.max_bytes || isize::try_from(bytes).is_err() {
			return Err(Error::Budget("catalog source size"));
		}
		let count = self
			.kappas
			.len()
			.checked_mul(self.epsilons.len())
			.ok_or(Error::Budget("catalog families"))?;
		if count > policy.max_coefficients {
			return Err(Error::Budget("catalog families"));
		}
		family_storage(count, policy)?;
		for (index, kappa) in self.kappas.iter().enumerate() {
			if *kappa == 0 || self.kappas.iter().take(index).any(|prior| prior == kappa) {
				return Err(Error::Format("invalid or duplicate catalog kappa"));
			}
		}
		for (index, label) in self.epsilons.iter().enumerate() {
			let epsilon = label
				.parse::<f64>()
				.map_err(|_| Error::Format("catalog epsilon label"))?;
			if !epsilon.is_finite()
				|| epsilon <= 0.0
				|| epsilon.to_string() != *label
				|| self.epsilons.iter().take(index).any(|prior| prior == label)
			{
				return Err(Error::Format("invalid or duplicate catalog epsilon"));
			}
		}
		Ok(())
	}
}

/// Owned inverse coefficients decoded from the official `PennyLane` HDF5 layout.
/// No HDF5 handles or temporary files are retained; callers control its lifetime.
#[derive(Debug)]
pub struct InverseCatalog {
	source: CatalogSource,
	families: Vec<CatalogFamily>,
}
impl InverseCatalog {
	/// Decode the bundled, unchanged official HDF5 dataset offline.
	/// A secure temporary file is required during decoding and removed before return.
	/// `max_bytes` bounds both the source file and aggregate decoded coefficient storage;
	/// `max_coefficients` bounds each family and the number of families.
	/// # Errors
	/// Reports temporary IO, digest, schema, finite-value, parity and budget errors.
	pub fn bundled(policy: IoPolicy) -> Result<Self> {
		let source = serde_json::from_str(include_str!("../data/pennylane/source.json"))?;
		Self::decode(
			&include_bytes!("../data/pennylane/inverse.h5")[..],
			source,
			policy,
		)
	}
	/// Snapshot and validate an external HDF5 file against its supplied manifest.
	/// The file is read into secure temporary storage while its digest is checked,
	/// so later replacement of the original path cannot change admitted data.
	/// # Errors
	/// Reports source/temporary IO, digest, schema, finite-value, parity and budget errors.
	pub fn open(path: impl AsRef<Path>, source: CatalogSource, policy: IoPolicy) -> Result<Self> {
		source.validate(policy)?;
		let file = std::fs::File::open(path)?;
		if file.metadata()?.len() != source.size_bytes {
			return Err(Error::Format("catalog source size mismatch"));
		}
		Self::decode(file, source, policy)
	}
	fn decode(reader: impl Read, source: CatalogSource, policy: IoPolicy) -> Result<Self> {
		source.validate(policy)?;
		let mut reader = reader.take(
			source
				.size_bytes
				.checked_add(1)
				.ok_or(Error::Budget("catalog source size"))?,
		);
		let mut temporary = tempfile::NamedTempFile::new()?;
		let mut hash = Sha256::new();
		let mut buffer = [0_u8; 8192];
		let mut length = 0_u64;
		loop {
			let read = reader.read(&mut buffer)?;
			if read == 0 {
				break;
			}
			length = length
				.checked_add(u64::try_from(read).map_err(|_| Error::Budget("catalog source size"))?)
				.ok_or(Error::Budget("catalog source size"))?;
			if length > source.size_bytes {
				return Err(Error::Format("catalog source size mismatch"));
			}
			let chunk = buffer
				.get(..read)
				.ok_or(Error::Format("catalog source read count"))?;
			hash.update(chunk);
			temporary.write_all(chunk)?;
		}
		if length != source.size_bytes
			|| digest_hex(&hash.finalize())? != source.sha256.to_ascii_lowercase()
		{
			return Err(Error::Format("catalog source size or SHA-256 mismatch"));
		}
		temporary.flush()?;
		let file = File::open(temporary.path())?;
		let result = read_families(&file, &source, policy);
		// Close/remove on both admission outcomes. Preserve the admission error when
		// more than one operation fails; RAII provides cleanup on earlier IO failures.
		let closed = file.close();
		let removed = temporary.close();
		let families = result?;
		closed?;
		removed?;
		Ok(Self { source, families })
	}
	#[must_use]
	pub fn families(&self) -> &[CatalogFamily] {
		&self.families
	}
	/// Select exact labels without substituting a nearby family.
	#[must_use]
	pub fn find(&self, kappa: u32, epsilon: f64) -> Option<&CatalogFamily> {
		self.families
			.iter()
			.find(|family| family.kappa == kappa && family.epsilon.to_bits() == epsilon.to_bits())
	}
	#[must_use]
	pub const fn source(&self) -> &CatalogSource {
		&self.source
	}
}

fn require_hard_links(group: &Group) -> Result<()> {
	let mut contained = true;
	group.iter_visit_default(|_, info| {
		contained &= info.link_type == LinkType::Hard;
		Ok(())
	})?;
	if !contained {
		return Err(Error::Format(
			"catalog cannot depend on symbolic or external links",
		));
	}
	Ok(())
}

fn members_match(group: &Group, mut expected: Vec<String>) -> Result<()> {
	require_hard_links(group)?;
	let mut actual = group.member_names()?;
	actual.sort();
	expected.sort();
	if actual != expected {
		return Err(Error::Format("catalog groups disagree with manifest"));
	}
	Ok(())
}
fn read_families(
	file: &File,
	source: &CatalogSource,
	policy: IoPolicy,
) -> Result<Vec<CatalogFamily>> {
	let count = source
		.kappas
		.len()
		.checked_mul(source.epsilons.len())
		.ok_or(Error::Budget("catalog families"))?;
	let mut retained = family_storage(count, policy)?;
	require_hard_links(file)?;
	let poly = file.group("/poly")?;
	members_match(&poly, source.epsilons.clone())?;
	let mut families = Vec::new();
	families
		.try_reserve_exact(count)
		.map_err(|_| Error::Budget("catalog family allocation"))?;
	for label in &source.epsilons {
		let group = poly.group(label)?;
		members_match(
			&group,
			source.kappas.iter().map(ToString::to_string).collect(),
		)?;
		let epsilon = label
			.parse::<f64>()
			.map_err(|_| Error::Format("catalog epsilon"))?;
		for &kappa in &source.kappas {
			let data = group.dataset(&kappa.to_string())?;
			let properties = data.create_plist()?;
			if !matches!(
				properties.get_layout()?,
				Layout::Compact | Layout::Contiguous | Layout::Chunked
			) || !properties.get_external()?.is_empty()
			{
				return Err(Error::Format(
					"catalog coefficients must be stored within the source file",
				));
			}
			let shape = data.shape();
			let [length] = shape.as_slice() else {
				return Err(Error::Format(
					"catalog coefficients must be one-dimensional",
				));
			};
			if *length < 2 || *length > policy.max_coefficients {
				return Err(Error::Budget("catalog coefficient count"));
			}
			if *length % 2 != 0
				|| data.dtype()?.to_descriptor()? != TypeDescriptor::Float(FloatSize::U8)
			{
				return Err(Error::Format(
					"catalog requires odd-degree float64 coefficients",
				));
			}
			let bytes = length
				.checked_mul(size_of::<f64>())
				.ok_or(Error::Budget("catalog coefficient bytes"))?;
			retained = retained
				.checked_add(bytes)
				.ok_or(Error::Budget("catalog aggregate storage"))?;
			if retained > policy.max_bytes || isize::try_from(retained).is_err() {
				return Err(Error::Budget("catalog aggregate storage"));
			}
			let mut values: Vec<f64> = Vec::new();
			values
				.try_reserve_exact(*length)
				.map_err(|_| Error::Budget("catalog coefficient allocation"))?;
			values.resize(*length, 0.0);
			if data.as_reader().read_into_raw(&mut values)? != *length {
				return Err(Error::Format("catalog coefficient read count"));
			}
			if values.iter().any(|value| !value.is_finite()) {
				return Err(Error::NonFinite);
			}
			if values.iter().step_by(2).any(|value| *value != 0.0) || values.last() == Some(&0.0) {
				return Err(Error::Format(
					"catalog requires exact odd polynomial support",
				));
			}
			families.push(CatalogFamily {
				kappa,
				epsilon,
				degree: length
					.checked_sub(1)
					.ok_or(Error::Format("catalog degree"))?,
				coefficients: values.into_boxed_slice(),
			});
		}
	}
	families.sort_by(|a, b| a.kappa.cmp(&b.kappa).then(a.epsilon.total_cmp(&b.epsilon)));
	Ok(families)
}

/// Owned `PennyLane` inverse family. Labels are provenance, not certificates.
#[derive(Debug)]
pub struct CatalogFamily {
	kappa: u32,
	epsilon: f64,
	degree: usize,
	coefficients: Box<[f64]>,
}
impl CatalogFamily {
	#[must_use]
	pub const fn kappa(&self) -> u32 {
		self.kappa
	}
	#[must_use]
	pub const fn epsilon_label(&self) -> f64 {
		self.epsilon
	}
	#[must_use]
	pub const fn degree(&self) -> usize {
		self.degree
	}
	#[must_use]
	pub fn coefficients(&self) -> &[f64] {
		&self.coefficients
	}
	#[must_use]
	pub fn reciprocal_scale(&self) -> f64 {
		0.5 / f64::from(self.kappa)
	}
	/// Materialize exact decoded binary64 coefficients as a Chebyshev polynomial.
	/// # Errors
	/// Rejects coefficient/allocation budgets or polynomial admission failures.
	pub fn polynomial(&self, policy: IoPolicy) -> Result<Polynomial<Chebyshev>> {
		let count = self.coefficients.len();
		if count > policy.max_coefficients {
			return Err(Error::Budget("catalog coefficients"));
		}
		policy.check(count, 1)?;
		let mut values = Vec::new();
		values
			.try_reserve_exact(count)
			.map_err(|_| Error::Budget("catalog polynomial allocation"))?;
		values.extend(
			self.coefficients
				.iter()
				.map(|&value| Complex64::new(value, 0.0)),
		);
		Ok(Polynomial::new(
			Chebyshev,
			values,
			policy.polynomial_limits(),
		)?)
	}
}
