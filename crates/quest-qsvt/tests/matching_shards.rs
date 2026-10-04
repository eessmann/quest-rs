use googletest::prelude::*;
use quest_numerics::{SparseFormat, SparseLimits, SparseMatrix};
use quest_qsvt::{Complex64, MatchingEncoding, MatchingShard, NumericalPolicy};
use std::ops::Mul;

#[gtest]
fn matching_shards_own_coefficients_and_completion_without_parent_storage() -> quest_qsvt::Result<()>
{
	let policy = NumericalPolicy::default();
	let matrix = SparseMatrix::from_triplets(
		5,
		5,
		SparseFormat::Csr,
		vec![
			(1, 0, Complex64::new(1.0, 1.0)),
			(2, 1, Complex64::new(0.0, 2.0)),
			(4, 3, Complex64::new(1.0, 0.0)),
		],
		SparseLimits::default(),
	)
	.unwrap();
	let encoding = MatchingEncoding::from_sparse(&matrix, policy)?;
	let left = MatchingShard::from_encoding(&encoding, 0, 2, policy)?;
	let right = MatchingShard::from_encoding(&encoding, 1, 2, policy)?;
	drop(encoding);
	expect_eq!(
		left.records()
			.len()
			.checked_add(right.records().len())
			.ok_or(quest_qsvt::Error::Budget("record count overflow"))?,
		5
	);
	expect_eq!(left.column(0, 0)?.destination, 1);
	expect_eq!(right.column(0, 1)?.destination, 2);
	expect_eq!(left.column(0, 2)?.destination, 0);
	expect_eq!(right.column(0, 3)?.destination, 4);
	expect_eq!(left.column(0, 4)?.destination, 3);
	expect_eq!(left.column(0, 6)?.destination, 6);
	expect_eq!(left.column(0, 2)?.cosine, 0.0);
	expect_true!(left.column(0, 1).is_err());
	Ok(())
}

#[gtest]
fn matching_shard_admission_rejects_wrong_ownership_and_malformed_records() -> quest_qsvt::Result<()>
{
	let policy = NumericalPolicy::default();
	let matrix = SparseMatrix::from_triplets(
		1,
		1,
		SparseFormat::Csr,
		vec![(0, 0, Complex64::new(1.0, 0.0))],
		SparseLimits::default(),
	)
	.unwrap();
	let encoding = MatchingEncoding::from_sparse(&matrix, policy)?;
	let shard = MatchingShard::from_encoding(&encoding, 0, 1, policy)?;
	expect_true!(MatchingShard::from_parts(shard.header(), 0, 1, vec![], policy).is_err());
	let mut altered = shard.records().to_vec();
	if let Some(first) = altered.first_mut() {
		first.phase = first.phase.mul(-1.0);
	}
	expect_true!(MatchingShard::from_parts(shard.header(), 0, 1, altered, policy).is_err());
	let mut wrong_scale = shard.header();
	wrong_scale.alpha *= 2.0;
	expect_true!(
		MatchingShard::from_parts(wrong_scale, 0, 1, shard.records().to_vec(), policy).is_err()
	);
	expect_true!(MatchingShard::from_encoding(&encoding, 1, 1, policy).is_err());
	let mut records = shard.records().to_vec();
	records[0].phase = Complex64::new(f64::NAN, 0.0);
	expect_true!(MatchingShard::from_parts(shard.header(), 0, 1, records, policy).is_err());
	let mut records = shard.records().to_vec();
	records.push(records[0]);
	expect_true!(MatchingShard::from_parts(shard.header(), 0, 1, records, policy).is_err());
	Ok(())
}
