//! Fixed-storage local-rank facts from one completely successful prepared apply.
use crate::qsvt::matching::collective::RoutingStatistics;

/// Cumulative matching-router counters for a completed LCU or transform apply.
///
/// Only matching application routing is included. PREP, response/projector/native
/// internal, constructor, separate coordinator and MPI protocol traffic are excluded.
/// Counts are local-rank facts; global reductions require separate checked accounting.
#[derive(Debug, Clone, Copy)]
pub struct RoutingTelemetry {
	/// Source applies dispatched by the transform; zero for a direct LCU apply.
	pub source_queries: usize,
	/// Selected matching child applies actually dispatched across all source queries.
	pub child_events: usize,
	/// Sum counters and maximum peak fields across those child applies.
	pub routing: RoutingStatistics,
	/// True only when every included counter is exact under the inherited router model.
	/// False means the numbers are conservative lower bounds: a counter reached
	/// `usize::MAX` or aggregation overflowed. Excluded traffic is never a measured zero.
	pub exact: bool,
}
impl Default for RoutingTelemetry {
	fn default() -> Self {
		Self {
			source_queries: 0,
			child_events: 0,
			routing: RoutingStatistics::default(),
			exact: true,
		}
	}
}
const fn sum(left: usize, right: usize, exact: &mut bool) -> usize {
	if left == usize::MAX || right == usize::MAX {
		*exact = false;
	}
	match left.checked_add(right) {
		Some(value) if value < usize::MAX => value,
		_ => {
			*exact = false;
			usize::MAX
		}
	}
}
fn maximum(left: usize, right: usize, exact: &mut bool) -> usize {
	if left == usize::MAX || right == usize::MAX {
		*exact = false;
	}
	left.max(right)
}
impl RoutingTelemetry {
	fn routing(&mut self, next: RoutingStatistics) {
		let current = &mut self.routing;
		let exact = &mut self.exact;
		current.batches = sum(current.batches, next.batches, exact);
		current.local_pair_candidates = sum(
			current.local_pair_candidates,
			next.local_pair_candidates,
			exact,
		);
		current.maximum_batch_pairs =
			maximum(current.maximum_batch_pairs, next.maximum_batch_pairs, exact);
		current.maximum_routed_amplitudes = maximum(
			current.maximum_routed_amplitudes,
			next.maximum_routed_amplitudes,
			exact,
		);
		current.coordination_calls =
			sum(current.coordination_calls, next.coordination_calls, exact);
		current.indexed_reads = sum(current.indexed_reads, next.indexed_reads, exact);
		current.indexed_writes = sum(current.indexed_writes, next.indexed_writes, exact);
		current.point_to_point_sent_bytes = sum(
			current.point_to_point_sent_bytes,
			next.point_to_point_sent_bytes,
			exact,
		);
		current.point_to_point_received_bytes = sum(
			current.point_to_point_received_bytes,
			next.point_to_point_received_bytes,
			exact,
		);
	}
	pub(crate) fn child(&mut self, next: RoutingStatistics) {
		self.child_events = sum(self.child_events, 1, &mut self.exact);
		self.routing(next);
	}
	pub(crate) fn source(&mut self, next: Self) {
		self.exact &= next.exact;
		self.source_queries = sum(self.source_queries, 1, &mut self.exact);
		self.child_events = sum(self.child_events, next.child_events, &mut self.exact);
		self.routing(next.routing);
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::qsvt::matching::collective::RoutingStatistics;
	#[test]
	fn child_rollup_arithmetic_is_admitted_before_execution() {
		let cost = crate::qsvt::matching::MatchingExecutionCost {
			protocol_batches: 0,
			maximum_local_candidates: 0,
			maximum_rank_work: 37,
			aggregate_work: 37,
			application_bytes_per_rank: 0,
			aggregate_application_bytes: 0,
			routing_collective_calls: 0,
			native_dispatches: 0,
			native_state_elements: 0,
		};
		let mut resources = crate::qsvt::matching_lcu::MatchingLcuResources::default();
		assert!(resources.child(cost).is_ok());
		assert_eq!(resources.maximum_rank_work, 165);
	}

	#[test]
	fn routing_rollup_counts_every_event_and_uses_maxima_for_peaks() {
		let mut lcu = RoutingTelemetry::default();
		let first = RoutingStatistics {
			batches: 2,
			local_pair_candidates: 17,
			maximum_batch_pairs: 8,
			maximum_routed_amplitudes: 12,
			coordination_calls: 3,
			indexed_reads: 6,
			indexed_writes: 10,
			point_to_point_sent_bytes: 48,
			point_to_point_received_bytes: 24,
		};
		let second = RoutingStatistics {
			batches: 3,
			maximum_batch_pairs: 4,
			maximum_routed_amplitudes: 20,
			..first
		};
		lcu.child(first);
		lcu.child(second);
		assert_eq!(lcu.source_queries, 0);
		assert_eq!(lcu.child_events, 2);
		assert!(lcu.exact);
		assert_eq!(lcu.routing.batches, 5);
		assert_eq!(lcu.routing.local_pair_candidates, 34);
		assert_eq!(lcu.routing.maximum_batch_pairs, 8);
		assert_eq!(lcu.routing.maximum_routed_amplitudes, 20);
		assert_eq!(lcu.routing.coordination_calls, 6);
		assert_eq!(lcu.routing.indexed_reads, 12);
		assert_eq!(lcu.routing.indexed_writes, 20);
		assert_eq!(lcu.routing.point_to_point_sent_bytes, 96);
		assert_eq!(lcu.routing.point_to_point_received_bytes, 48);
		let mut transform = RoutingTelemetry::default();
		transform.source(lcu);
		transform.source(lcu);
		assert_eq!(transform.source_queries, 2);
		assert_eq!(transform.child_events, 4);
		assert_eq!(transform.routing.batches, 10);
		assert_eq!(transform.routing.maximum_routed_amplitudes, 20);
		assert_eq!(transform.routing.point_to_point_sent_bytes, 192);
		assert!(transform.exact);
	}

	#[test]
	fn inherited_limits_and_checked_overflow_remain_visible_lower_bounds() {
		let mut inherited = RoutingTelemetry::default();
		inherited.child(RoutingStatistics {
			indexed_reads: usize::MAX,
			..RoutingStatistics::default()
		});
		assert!(!inherited.exact);
		assert_eq!(inherited.routing.indexed_reads, usize::MAX);
		let mut sum = RoutingTelemetry::default();
		for bytes in [usize::MAX - 1, 2] {
			sum.child(RoutingStatistics {
				point_to_point_sent_bytes: bytes,
				..RoutingStatistics::default()
			});
		}
		assert!(!sum.exact);
		assert_eq!(sum.routing.point_to_point_sent_bytes, usize::MAX);
		assert_eq!(sum.child_events, 2);
		let mut transform = RoutingTelemetry::default();
		transform.source(sum);
		assert!(!transform.exact);
		assert_eq!(transform.routing.point_to_point_sent_bytes, usize::MAX);
		let empty = RoutingTelemetry::default();
		assert!(empty.exact);
		assert_eq!(empty.source_queries, 0);
		assert_eq!(empty.child_events, 0);
		assert_eq!(empty.routing.point_to_point_sent_bytes, 0);
	}
	#[test]
	fn reaching_the_counter_limit_is_conservatively_inexact() {
		let mut total = RoutingTelemetry::default();
		for bytes in [usize::MAX - 1, 1] {
			total.child(RoutingStatistics {
				point_to_point_received_bytes: bytes,
				..RoutingStatistics::default()
			});
		}
		assert_eq!(total.routing.point_to_point_received_bytes, usize::MAX);
		assert!(!total.exact);
	}
	#[test]
	fn every_inherited_counter_and_peak_limit_is_marked() {
		let fields: [fn(&mut RoutingStatistics); 9] = [
			|s| s.batches = usize::MAX,
			|s| s.local_pair_candidates = usize::MAX,
			|s| s.maximum_batch_pairs = usize::MAX,
			|s| s.maximum_routed_amplitudes = usize::MAX,
			|s| s.coordination_calls = usize::MAX,
			|s| s.indexed_reads = usize::MAX,
			|s| s.indexed_writes = usize::MAX,
			|s| s.point_to_point_sent_bytes = usize::MAX,
			|s| s.point_to_point_received_bytes = usize::MAX,
		];
		for field in fields {
			let mut input = RoutingStatistics::default();
			field(&mut input);
			let mut total = RoutingTelemetry::default();
			total.child(input);
			assert!(!total.exact);
		}
		let mut events = RoutingTelemetry {
			child_events: usize::MAX - 1,
			source_queries: usize::MAX - 1,
			..Default::default()
		};
		events.child(RoutingStatistics::default());
		events.source(RoutingTelemetry::default());
		assert_eq!(events.child_events, usize::MAX);
		assert_eq!(events.source_queries, usize::MAX);
		assert!(!events.exact);
	}
}
