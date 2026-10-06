#![allow(
	clippy::panic_in_result_fn,
	reason = "Assertions express regression outcomes after fallible setup"
)]
use quest_numerics::{
	Complex64,
	sparse_stream::{SparseEntry, StreamBuilder, StreamLimits},
};

#[test]
fn streamed_duplicates_follow_ordinals_and_remove_canonical_zeros() -> quest_numerics::Result<()> {
	let mut builder = StreamBuilder::new(2, 3, StreamLimits::default())?;
	for (row, column, ordinal, re) in [
		(1, 2, 9, -1e16),
		(0, 1, 2, -3.0),
		(1, 2, 10, 1.0),
		(1, 2, 8, 1e16),
		(0, 1, 1, 3.0),
	] {
		builder.push(SparseEntry {
			row,
			column,
			ordinal,
			value: Complex64::new(re, 0.0),
		})?;
	}
	let entries = builder
		.finish()?
		.collect::<quest_numerics::Result<Vec<_>>>()?;
	assert_eq!(
		entries,
		vec![SparseEntry {
			row: 1,
			column: 2,
			ordinal: 8,
			value: Complex64::new(1.0, 0.0)
		}]
	);
	Ok(())
}

#[test]
fn malformed_local_csr_and_duplicate_ordinals_reject() -> quest_numerics::Result<()> {
	use quest_numerics::sparse_stream::LocalCsr;
	let values = [Complex64::new(1.0, 0.0)];
	assert!(LocalCsr::new(0, 1, &[], &[], &[], &[], &[0]).is_err());
	assert!(LocalCsr::new(2, 2, &[2], &[], &[], &[], &[0, 0]).is_err());
	assert!(LocalCsr::new(2, 2, &[1], &[0], &values, &[7], &[0, 2]).is_err());
	assert!(LocalCsr::new(2, 2, &[1], &[2], &values, &[7], &[0, 1]).is_err());
	let csr = LocalCsr::new(2, 2, &[1], &[0], &values, &[7], &[0, 1])?;
	assert_eq!(
		csr.entries().collect::<quest_numerics::Result<Vec<_>>>()?,
		vec![SparseEntry {
			row: 1,
			column: 0,
			ordinal: 7,
			value: *values
				.first()
				.ok_or(quest_numerics::Error::Length("value"))?
		}]
	);
	let mut builder = StreamBuilder::new(2, 2, StreamLimits::default())?;
	for _ in 0..2 {
		builder.push(SparseEntry {
			row: 1,
			column: 0,
			ordinal: 7,
			value: *values
				.first()
				.ok_or(quest_numerics::Error::Length("value"))?,
		})?;
	}
	assert!(
		builder
			.finish()?
			.next()
			.ok_or(quest_numerics::Error::Length("expected duplicate error"))?
			.is_err()
	);
	Ok(())
}
#[test]
fn stream_rejects_unadmitted_growth_nonfinite_and_work() -> quest_numerics::Result<()> {
	let entry = SparseEntry {
		row: 0,
		column: 0,
		ordinal: 0,
		value: Complex64::new(1.0, 0.0),
	};
	let mut builder = StreamBuilder::new(
		1,
		1,
		StreamLimits {
			buffer_entries: 1,
			..StreamLimits::default()
		},
	)?;
	builder.push(entry)?;
	assert!(
		builder
			.push(SparseEntry {
				ordinal: 1,
				..entry
			})
			.is_err()
	);
	assert!(
		StreamBuilder::new(
			1,
			1,
			StreamLimits {
				max_bytes: 0,
				..StreamLimits::default()
			}
		)
		.is_err()
	);
	let mut builder = StreamBuilder::new(
		1,
		1,
		StreamLimits {
			max_work: 1,
			..StreamLimits::default()
		},
	)?;
	builder.push(entry)?;
	assert!(builder.finish().is_err());
	let mut builder = StreamBuilder::new(1, 1, StreamLimits::default())?;
	assert!(
		builder
			.push(SparseEntry {
				value: Complex64::new(f64::NAN, 0.0),
				..entry
			})
			.is_err()
	);
	for ordinal in 0..2 {
		builder.push(SparseEntry {
			ordinal,
			value: Complex64::new(f64::MAX, 0.0),
			..entry
		})?;
	}
	assert!(
		builder
			.finish()?
			.next()
			.ok_or(quest_numerics::Error::Length("expected overflow error"))?
			.is_err()
	);
	Ok(())
}
