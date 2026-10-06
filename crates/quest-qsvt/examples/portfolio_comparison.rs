//! Bounded comparison of different whole-unitary constructions of one circulant.
#[path = "portfolio_comparison/support.rs"]
mod support;
fn main() -> quest_qsvt::Result<()> {
	let campaign = support::run()?;
	println!(
		"A=0.5 I4 + 0.125 i S4^2; same extracted block; whole-unitary equality is not asserted"
	);
	println!(
		"common stored fixture input_bytes={} source_seconds={}",
		campaign.input_bytes, campaign.source_seconds
	);
	println!(
		"counts use emitted portable primitives, not decomposed T gates; memory is modeled application payload, not RSS"
	);
	println!(
		"uniform matching compile-work/table accounting unavailable; portfolio resources report constructor-local counters (child preparation is additionally included in explicit preparation_gates)"
	);
	println!(
		"scheme,alpha,qubits,workspace,gates,preparation_gates,select_other_gates,precision_bits,oracle_queries,compile_work,retained_bytes,construction_peak_bytes,compile_seconds,block_extraction_seconds"
	);
	for receipt in &campaign.receipts {
		let costs = receipt.portfolio_resources;
		let model = |n: Option<usize>| n.map_or_else(|| "unavailable".into(), |n| n.to_string());
		println!(
			"{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
			receipt.scheme,
			receipt.alpha,
			receipt.qubits,
			receipt.workspace_qubits,
			receipt.elementary_gates,
			receipt.preparation_gates,
			receipt.select_and_other_gates,
			model(costs.map(|c| c.precision_bits)),
			model(costs.map(|c| c.oracle_queries)),
			model(costs.map(|c| c.compile_work)),
			receipt.retained_bytes,
			receipt.construction_peak_bytes,
			receipt.compile_seconds,
			receipt.block_extraction_seconds
		);
	}
	for receipt in campaign.receipts {
		println!("{receipt:?}");
	}
	Ok(())
}
