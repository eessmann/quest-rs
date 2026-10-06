//! Bounded actual-source portable resource curves, not accuracy or native performance claims.
#[path = "portfolio_resource_curves/support.rs"]
mod support;
fn main() -> quest_qsvt::Result<()> {
	let mut limits = support::Limits::default();
	let mut args = std::env::args().skip(1);
	while let Some(argument) = args.next() {
		let value = args
			.next()
			.ok_or(quest_qsvt::Error::Encoding("expected count allowance"))?
			.parse::<usize>()
			.map_err(|_| quest_qsvt::Error::Encoding("invalid count allowance"))?;
		match argument.as_str() {
			"--count-work" => limits.count_work = value,
			"--gates-per-orientation" => limits.gates_per_orientation = value,
			_ => return Err(quest_qsvt::Error::Encoding("unknown resource-curve option")),
		}
	}
	println!(
		"A_N=.5 I_N+.125i S_N^(N/2); N=4,8,16,32; portable primitives only; no state or gate stream retained"
	);
	println!(
		"limits={limits:?}; one count allowance shared across forward/adjoint per source; constructors retain their independent limits"
	);
	println!(
		"memory=constructor-local managed payload envelopes; shared CSR/receipt storage/RSS not added; counters unavailable where source exposes none"
	);
	let campaign = support::run(limits)?;
	for source in campaign.sources {
		println!("source={source:?}");
	}
	println!(
		"N,scheme,status,alpha,qubits,workspace,forward_gates,adjoint_gates,count_work,observed_emissions,prep_per_orientation,retained_bytes,construction_peak_bytes,construction_seconds,count_seconds"
	);
	for row in campaign.rows {
		println!(
			"{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
			row.dimension,
			row.scheme,
			if row.count_error.is_none() {
				"counted"
			} else {
				"count_rejected"
			},
			row.descriptor.normalization,
			row.descriptor.layout.num_qubits,
			row.descriptor.layout.workspace_mask.count_ones(),
			row.forward.gates,
			row.adjoint.gates,
			row.count_work,
			row.observed_emissions,
			row.preparation_gates,
			row.retained_bytes,
			row.peak_bytes,
			row.construction_seconds,
			row.count_seconds
		);
		println!(
			"descriptor={:?}; inventory_forward={:?}; inventory_adjoint={:?}; portfolio={:?}; count_error={:?}",
			row.descriptor, row.forward, row.adjoint, row.portfolio, row.count_error
		);
	}
	Ok(())
}
