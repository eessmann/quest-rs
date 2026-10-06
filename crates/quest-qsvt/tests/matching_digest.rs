#![allow(
	clippy::panic_in_result_fn,
	reason = "Bounded regression tests use assertions for semantic failures and Result only for fixture construction"
)]
use quest_qsvt::{Complex64, MatchingColumn, MatchingHeader, MatchingShard, NumericalPolicy};

fn columns(n: usize, imaginary: f64) -> Vec<MatchingColumn> {
	(0..n)
		.map(|source| MatchingColumn {
			color: 0,
			source,
			destination: source ^ 1,
			cosine: 1.0,
			sine: 0.0,
			phase: Complex64::new(0.0, imaginary),
		})
		.collect()
}

#[test]
fn opposite_phases_do_not_cancel_in_completed_record_admission() -> quest_qsvt::Result<()> {
	for n in [4_usize, 8, 32] {
		let original = columns(n, 1.0);
		let changed = columns(n, -1.0);
		let (count, digest) = MatchingShard::summarize_records(&original)?;
		let header = MatchingHeader {
			rows: n,
			cols: n,
			system_qubits: usize::try_from(n.trailing_zeros())
				.map_err(|_| quest_qsvt::Error::Budget("test width"))?,
			color_qubits: 0,
			num_colors: 1,
			beta: 1.0,
			alpha: 1.0,
			source_identity: 42,
			record_count: count,
			record_digest: digest,
		};
		assert!(
			MatchingShard::from_parts(header, 0, 1, original, NumericalPolicy::default()).is_ok()
		);
		assert!(
			MatchingShard::from_parts(header, 0, 1, changed, NumericalPolicy::default()).is_err(),
			"opposite phases admitted for dimension {n}"
		);
	}
	Ok(())
}

#[test]
fn digest_retains_signed_zero_and_partition_order_independence() -> quest_qsvt::Result<()> {
	let mut original = columns(32, 1.0);
	let digest = MatchingShard::summarize_records(&original)?.1;
	original.reverse();
	assert_eq!(MatchingShard::summarize_records(&original)?.1, digest);
	for parts in [1, 2, 4, 8] {
		let mut sum = 0_u64;
		for rank in 0..parts {
			let shard: Vec<_> = original
				.iter()
				.copied()
				.filter(|c| c.source.checked_rem(parts) == Some(rank))
				.collect();
			sum = sum.wrapping_add(MatchingShard::summarize_records(&shard)?.1);
		}
		assert_eq!(sum, digest);
	}
	for c in &mut original {
		c.phase.re = -0.0;
	}
	assert_ne!(MatchingShard::summarize_records(&original)?.1, digest);
	Ok(())
}

#[test]
fn digest_work_counts_padded_blocks_and_rejects_overflow() -> quest_qsvt::Result<()> {
	assert_eq!(quest_qsvt::record_fingerprint_work(4)?, 8192);
	assert_eq!(quest_qsvt::record_fingerprint_work(7)?, 16_384);
	assert!(quest_qsvt::record_fingerprint_work(usize::MAX).is_err());
	// The hash state/output plus a 64-word u32 compression schedule fit the fixed allowance.
	let modeled_scratch = size_of::<sha2::Sha256>()
		.checked_add(352)
		.ok_or(quest_qsvt::Error::Budget("test scratch"))?;
	assert!(modeled_scratch <= quest_qsvt::RECORD_FINGERPRINT_SCRATCH_BYTES);
	let same_words = [0_u64, 1, 2, 3];
	// Independently computed with Python hashlib.sha256 on explicit LE bytes.
	assert_eq!(
		quest_qsvt::record_fingerprint(1, same_words),
		7_852_582_459_109_447_654
	);
	assert_ne!(
		quest_qsvt::record_fingerprint(1, same_words),
		quest_qsvt::record_fingerprint(2, same_words)
	);
	Ok(())
}
