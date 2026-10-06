#![cfg(feature = "distributed")]
use quest_cfd::box_kvn_history::BoxKvnHistoryLimits;
#[test]
fn source_admission_has_separate_rank_aggregate_disk_and_call_limits() {
	let l = BoxKvnHistoryLimits::default();
	assert!(l.max_drift_calls > 0);
	assert!(l.max_rank_work > 0);
	assert!(l.max_aggregate_work > 0);
	assert!(l.max_local_disk_bytes > 0);
	assert!(l.max_global_disk_bytes > 0);
	assert!(l.max_node_disk_bytes > 0);
}
