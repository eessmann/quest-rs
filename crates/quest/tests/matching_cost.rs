#![cfg(feature = "qsvt")]
use googletest::prelude::*;
use quest::qsvt::matching::MatchingExecutionCost;

#[gtest]
#[allow(
	clippy::arithmetic_side_effects,
	reason = "the independent differential uses fixed dimensions at most 512 and eight ranks"
)]
fn matching_cost_counts_every_protocol_round_and_separates_wire() -> googletest::Result<()> {
	for parts in [1usize, 2, 4, 8] {
		for flag in [1usize, 32, 256] {
			let cost = MatchingExecutionCost::admit(512, parts, flag, 9, 2, 17)?;
			let local = 512usize.checked_div(parts).ok_or(quest::Error::Overflow)?;
			let per_owner = if flag < local { local / 2 } else { local };
			let owners = if flag < local { parts } else { parts / 2 };
			let rounds = owners * per_owner.div_ceil(64);
			expect_eq!(cost.protocol_batches, rounds);
			expect_eq!(
				cost.application_bytes_per_rank,
				4 * rounds * (parts - 1) * 5128
			);
			expect_eq!(
				cost.aggregate_application_bytes,
				parts * cost.application_bytes_per_rank
			);
			expect_ge!(cost.native_dispatches, 6);
			expect_gt!(cost.maximum_rank_work, 0);
		}
	}
	Ok(())
}

#[gtest]
fn matching_cost_rejects_shape_and_overflow() {
	for (dimension, parts, flag) in [
		(512usize, 3usize, 1usize),
		(512, 2, 0),
		(512, 2, 512),
		(1, 2, 1),
	] {
		expect_true!(MatchingExecutionCost::admit(dimension, parts, flag, 9, 2, 17).is_err());
	}
	expect_true!(MatchingExecutionCost::admit(1usize << 60, 8, 1, 60, 2, usize::MAX).is_err());
}
