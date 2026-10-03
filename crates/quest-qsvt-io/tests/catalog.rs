use googletest::prelude::*;
use quest_qsvt_io::{CatalogSource, InverseCatalog, IoPolicy};
use sha2::{Digest, Sha256};
use std::{fmt::Write as _, io::Write};

fn digest_hex(bytes: &[u8]) -> Result<String> {
	let mut hex = String::new();
	for byte in bytes {
		write!(&mut hex, "{byte:02x}")?;
	}
	Ok(hex)
}

#[gtest]
fn official_dataset_preserves_all_original_binary64_coefficients() -> Result<()> {
	let catalog = InverseCatalog::bundled(IoPolicy::default())?;
	let hashes = [
		"dd6580db74e48b0005ea537c18ce45e5116d59d45e2c340850fcd608fdcee300",
		"bd95b78da4298ceed086a93ed94f88b470bff343c12e24e7a27cf4e5bb86da8f",
		"389a689cfc133464f9516cc16eed46b0566ee4639601f41aab827371c570b9c3",
		"749c65be2e4042ceda1e2675667656750ece9ba0d7a6bbefb803ce38520208d5",
		"ffbf061ded68b526a540bb952a5d70d256bd6d6504e2e4f24c4b3b5e3d59ac40",
		"4edf9d73f3e4c346c5310900b8208c1c7b9370cdf7a2e04a4a32b0428c9f4566",
		"53108b2c0782438e9fff9c345f1259b641c1d6f763e632187b3e361fe12c3f5e",
		"80c4f621f0e7e93196171e92ed4f6c0b148a084c3abf8e54b138c82f360a28ac",
		"0e2ae283e6ceb6388680e65f4c299c055ebd92fc4a3ffd258cc183ce20614dd4",
		"46de5762e7142bea19a23e61af23bc8c0ff201f009c7e240674a67de926fb303",
		"6dd6e6ddd6cf4119c6209f137e9c7c6a92a4eb816a66fc00d257f618bcb6c57c",
		"320bb628a3776622b88609bcf901440a93c1e172216cd7ab47057f5ec8cf051b",
		"640cf2d4b1b417b28f73099f016e50ccdc741ca81fed66c9806e136fea76ae90",
		"26e99513a5086474a40124469d4fc4b5084e74fbdcf5bbdf4996b0801a3b400d",
		"ef68ed5ee3bfcf0852cfc9515d65c5252d964e500de07a548db163ed6fd53557",
		"3917c5eb1836f68e165ec23cf061ed1cd101c1eb2c62b7935db185b053b9ef25",
		"b4b56369ebfccbc92f6b9bc0fd3235bfa3ff26a18646361d7a33dac1fa62abf0",
		"1606b23b9c744f55c152cbc79781a433cbfc42c28334210e8a3ece08b8743005",
		"485a5a065ec6ab73b668e50066f24f1a2bf697991a7616e83c15ae8f3376bc4e",
		"45d12828ffc6f61e72e68ee7be56c50ea2540600f61c3d9706e7e5ae2edcc905",
		"151f548a02e0196092af78c0f0faeae20173bcba936cbba4faff125bfa7f2557",
	];
	expect_eq!(catalog.families().len(), hashes.len());
	for (family, expected) in catalog.families().iter().zip(hashes) {
		let mut hash = Sha256::new();
		for value in family.coefficients() {
			hash.update(value.to_le_bytes());
		}
		expect_eq!(digest_hex(&hash.finalize())?, expected);
		let polynomial = family.polynomial(IoPolicy::default())?;
		expect_eq!(
			polynomial.coefficients().len(),
			family
				.degree()
				.checked_add(1)
				.ok_or_else(|| std::io::Error::other("degree overflow"))?
		);
		expect_true!(family.coefficients().iter().step_by(2).all(|v| *v == 0.0));
	}
	expect_eq!(
		catalog
			.families()
			.iter()
			.map(quest_qsvt_io::CatalogFamily::degree)
			.max(),
		Some(8105)
	);
	expect_true!(catalog.find(1500, 0.001).is_some());
	expect_true!(catalog.find(1500, 0.001_f64.next_up()).is_none());
	expect_true!(catalog.find(1499, 0.001).is_none());
	Ok(())
}

#[gtest]
fn budgets_and_failed_external_sources_do_not_poison_future_loads() -> Result<()> {
	expect_true!(
		InverseCatalog::bundled(IoPolicy {
			max_bytes: 1,
			..IoPolicy::default()
		})
		.is_err()
	);
	expect_true!(
		InverseCatalog::bundled(IoPolicy {
			max_coefficients: 8105,
			..IoPolicy::default()
		})
		.is_err()
	);
	let source: CatalogSource =
		serde_json::from_str(include_str!("../data/pennylane/source.json"))?;
	let mut file = tempfile::NamedTempFile::new()?;
	file.write_all(include_bytes!("../data/pennylane/inverse.h5"))?;
	file.flush()?;
	let external = InverseCatalog::open(file.path(), source.clone(), IoPolicy::default())?;
	expect_eq!(external.families().len(), 21);
	let mut corrupt_source = source.clone();
	corrupt_source.sha256 = "0".repeat(64);
	expect_true!(InverseCatalog::open(file.path(), corrupt_source, IoPolicy::default()).is_err());
	let mut corrupt_schema = source;
	corrupt_schema.basis = "Monomial".into();
	expect_true!(InverseCatalog::open(file.path(), corrupt_schema, IoPolicy::default()).is_err());
	drop(file);
	// Retained coefficients remain usable after the external file is removed.
	expect_true!(
		external
			.find(5, 0.1)
			.ok_or_else(|| std::io::Error::other("missing family"))?
			.polynomial(IoPolicy::default())
			.is_ok()
	);
	expect_true!(InverseCatalog::bundled(IoPolicy::default()).is_ok());
	Ok(())
}

fn source_for(path: &std::path::Path, kappas: Vec<u32>) -> Result<CatalogSource> {
	let mut source: CatalogSource =
		serde_json::from_str(include_str!("../data/pennylane/source.json"))?;
	let bytes = std::fs::read(path)?;
	source.size_bytes = u64::try_from(bytes.len())?;
	source.sha256 = digest_hex(&Sha256::digest(&bytes))?;
	source.kappas = kappas;
	source.epsilons = vec!["0.1".into()];
	Ok(source)
}

fn fixture(values: &[f64], shape: &[usize]) -> Result<tempfile::NamedTempFile> {
	let temporary = tempfile::NamedTempFile::new()?;
	let file = hdf5_metno::File::create(temporary.path())?;
	file.create_group("/poly/0.1")?;
	file.new_dataset::<f64>()
		.shape(shape)
		.create("/poly/0.1/5")?
		.write_raw(values)?;
	file.close()?;
	Ok(temporary)
}

#[gtest]
fn malformed_rank_nonfinite_parity_and_support_are_rejected() -> Result<()> {
	for (values, shape) in [
		(vec![0.0, 1.0], vec![1, 2]),
		(vec![0.0, f64::NAN], vec![2]),
		(vec![1.0, 1.0], vec![2]),
		(vec![0.0, 1.0, 0.0, 0.0], vec![4]),
	] {
		let temporary = fixture(&values, &shape)?;
		let source = source_for(temporary.path(), vec![5])?;
		expect_true!(InverseCatalog::open(temporary.path(), source, IoPolicy::default()).is_err());
	}
	let temporary = fixture(&[0.0, 1.0], &[2])?;
	let file = hdf5_metno::File::open_rw(temporary.path())?;
	file.unlink("/poly/0.1/5")?;
	file.new_dataset::<i64>()
		.shape(2)
		.create("/poly/0.1/5")?
		.write_raw(&[0_i64, 1])?;
	file.close()?;
	let source = source_for(temporary.path(), vec![5])?;
	expect_true!(InverseCatalog::open(temporary.path(), source, IoPolicy::default()).is_err());
	Ok(())
}

#[gtest]
fn aggregate_budget_counts_every_retained_family_even_for_shared_storage() -> Result<()> {
	let mut values = vec![0.0; 2000];
	*values
		.get_mut(1999)
		.ok_or_else(|| std::io::Error::other("fixture length"))? = 1.0;
	let temporary = fixture(&values, &[2000])?;
	let kappas = vec![5, 50, 100, 250, 500, 1000, 1500];
	let file = hdf5_metno::File::open_rw(temporary.path())?;
	let group = file.group("/poly/0.1")?;
	for kappa in kappas.iter().skip(1) {
		group.link_hard("5", &kappa.to_string())?;
	}
	drop(group);
	file.close()?;
	let source = source_for(temporary.path(), kappas)?;
	let policy = IoPolicy {
		max_bytes: usize::try_from(source.size_bytes)?,
		..IoPolicy::default()
	};
	expect_true!(matches!(
		InverseCatalog::open(temporary.path(), source.clone(), policy),
		Err(quest_qsvt_io::Error::Budget("catalog aggregate storage"))
	));
	let catalog = InverseCatalog::open(temporary.path(), source, IoPolicy::default())?;
	expect_eq!(catalog.families().len(), 7);
	temporary.close()?;
	expect_eq!(
		catalog
			.find(5, 0.1)
			.ok_or_else(|| std::io::Error::other("missing family"))?
			.coefficients()
			.len(),
		2000
	);
	Ok(())
}

#[gtest]
fn symbolic_coefficient_links_are_rejected_before_reading() -> Result<()> {
	let temporary = fixture(&[0.0, 1.0], &[2])?;
	let file = hdf5_metno::File::open_rw(temporary.path())?;
	file.relink("/poly/0.1/5", "/saved")?;
	file.link_soft("/saved", "/poly/0.1/5")?;
	file.close()?;
	let source = source_for(temporary.path(), vec![5])?;
	expect_true!(matches!(
		InverseCatalog::open(temporary.path(), source, IoPolicy::default()),
		Err(quest_qsvt_io::Error::Format(
			"catalog cannot depend on symbolic or external links"
		))
	));
	Ok(())
}

#[gtest]
fn external_raw_and_virtual_coefficients_are_rejected_before_reading() -> Result<()> {
	let mut raw = tempfile::NamedTempFile::new()?;
	raw.write_all(&0.0_f64.to_le_bytes())?;
	raw.write_all(&1.0_f64.to_le_bytes())?;
	raw.flush()?;
	let raw_path = raw
		.path()
		.to_str()
		.ok_or_else(|| std::io::Error::other("raw path encoding"))?;
	let external = tempfile::NamedTempFile::new()?;
	let file = hdf5_metno::File::create(external.path())?;
	file.create_group("/poly/0.1")?;
	file.new_dataset::<f64>()
		.shape(2)
		.external(raw_path, 0, 16)
		.create("/poly/0.1/5")?;
	file.close()?;
	let virtual_source = fixture(&[0.0, 1.0], &[2])?;
	let virtual_path = virtual_source
		.path()
		.to_str()
		.ok_or_else(|| std::io::Error::other("virtual path encoding"))?;
	let virtual_file = tempfile::NamedTempFile::new()?;
	let file = hdf5_metno::File::create(virtual_file.path())?;
	file.create_group("/poly/0.1")?;
	file.new_dataset::<f64>()
		.shape(2)
		.virtual_map(virtual_path, "/poly/0.1/5", 2, .., 2, ..)
		.create("/poly/0.1/5")?;
	file.close()?;
	for temporary in [external, virtual_file] {
		let source = source_for(temporary.path(), vec![5])?;
		expect_true!(matches!(
			InverseCatalog::open(temporary.path(), source, IoPolicy::default()),
			Err(quest_qsvt_io::Error::Format(
				"catalog coefficients must be stored within the source file"
			))
		));
	}
	Ok(())
}

#[gtest]
fn family_storage_budget_rejects_oversized_manifest_before_hdf5_admission() -> Result<()> {
	let temporary = fixture(&[0.0, 1.0], &[2])?;
	let source = source_for(temporary.path(), (1..=10000).collect())?;
	let policy = IoPolicy {
		max_bytes: usize::try_from(source.size_bytes)?,
		..IoPolicy::default()
	};
	expect_true!(matches!(
		InverseCatalog::open(temporary.path(), source, policy),
		Err(quest_qsvt_io::Error::Budget("catalog family storage"))
	));
	Ok(())
}
