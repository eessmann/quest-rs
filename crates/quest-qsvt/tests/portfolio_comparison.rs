#![allow(
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	clippy::panic_in_result_fn,
	reason = "Independent fixed sixteen-entry fixture and exact scheme expectations"
)]
#[path = "../examples/portfolio_comparison/support.rs"]
mod support;
use quest_qsvt::Complex64;
#[test]
fn same_circulant_block_and_counted_costs_across_eligible_portfolio() -> quest_qsvt::Result<()> {
	let campaign = support::run()?;
	assert_eq!(campaign.receipts.len(), 8);
	for receipt in &campaign.receipts {
		assert_eq!(receipt.extracted.len(), 16);
		for i in 0..4 {
			for j in 0..4 {
				let expected = if i == j {
					Complex64::new(0.5, 0.)
				} else if i == (j + 2) % 4 {
					Complex64::new(0., 0.125)
				} else {
					Complex64::default()
				};
				assert!(
					(receipt.extracted[4 * i + j] - expected).norm() < 2e-12,
					"{} block {i},{j}",
					receipt.scheme
				);
			}
		}
		assert_eq!(
			receipt.elementary_gates,
			receipt.x + receipt.h + receipt.ry + receipt.phase
		);
		assert!(receipt.retained_bytes > 0);
		assert!(receipt.construction_peak_bytes >= receipt.retained_bytes);
		assert!(receipt.compile_seconds.is_finite() && receipt.compile_seconds >= 0.);
		assert!(receipt.control_occurrences >= receipt.controlled_gates);
		let expected_alpha =
			if ["uniform-matching", "scc-base", "sparse-qrom"].contains(&receipt.scheme) {
				1.
			} else {
				0.625
			};
		assert!(
			(receipt.alpha - expected_alpha).abs() < 2e-12,
			"{} alpha",
			receipt.scheme
		);
		if let Some(costs) = receipt.portfolio_resources {
			assert_eq!(costs.elementary_gates, receipt.elementary_gates);
			assert_eq!(costs.workspace_qubits, receipt.workspace_qubits);
		}
	}
	let qrom = campaign
		.receipts
		.iter()
		.find(|r| r.scheme == "sparse-qrom")
		.expect("explicit QROM receipt");
	let costs = qrom
		.portfolio_resources
		.expect("QROM modeled resource inventory");
	assert_eq!(costs.precision_bits, 2);
	assert_eq!(costs.oracle_queries, 4);
	assert!(costs.table_entries > 0);
	assert!(costs.compile_work > 0);
	assert!(campaign.source_seconds.is_finite());
	Ok(())
}
