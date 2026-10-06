#![allow(
	clippy::panic_in_result_fn,
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	reason = "Small independent resource expectations and behavioral budget assertions"
)]
#[path = "../examples/portfolio_resource_curves/support.rs"]
mod support;
use quest_qsvt::Result;

#[test]
fn constructed_resource_curves_count_both_orientations_with_real_layouts() -> Result<()> {
	let campaign = support::run(support::Limits::default())?;
	assert_eq!(campaign.rows.len(), 24);
	for row in &campaign.rows {
		assert!(
			row.count_error.is_none(),
			"{} N{}",
			row.scheme,
			row.dimension
		);
		assert_eq!(row.forward.gates, row.adjoint.gates);
		assert_eq!(
			row.forward.gates,
			row.forward.primitives.iter().sum::<usize>()
		);
		assert_eq!(row.count_work, row.forward.gates + row.adjoint.gates);
		assert_eq!(row.descriptor.rows, row.dimension);
		assert_eq!(row.descriptor.cols, row.dimension);
		assert!(row.retained_bytes > 0);
		assert!(row.peak_bytes >= row.retained_bytes);
		assert!(row.count_seconds.is_finite());
		let alpha = if ["uniform-matching", "scc-base", "sparse-qrom"].contains(&row.scheme) {
			1.0
		} else {
			0.625
		};
		assert!((row.descriptor.normalization - alpha).abs() < 2e-12);
		if let Some(costs) = row.portfolio {
			assert_eq!(costs.elementary_gates, row.forward.gates);
			assert_eq!(
				costs.workspace_qubits,
				usize::try_from(row.descriptor.layout.workspace_mask.count_ones())
					.map_err(|_| quest_qsvt::Error::Budget("test workspace width"))?
			);
		}
	}
	Ok(())
}

#[test]
fn shared_count_budget_rejects_before_a_second_orientation_without_free_work() -> Result<()> {
	let normal = support::run(support::Limits::default())?;
	let mut limits = support::Limits {
		count_work: normal.rows[0].forward.gates,
		..support::Limits::default()
	};
	let restricted = support::run(limits)?;
	let row = &restricted.rows[0];
	assert!(row.count_error.is_some());
	assert_eq!(row.count_work, limits.count_work);
	assert_eq!(row.forward.gates, limits.count_work);
	assert_eq!(row.adjoint.gates, 0);
	assert!(row.retained_bytes > 0);
	assert_eq!(row.descriptor.rows, 4);
	limits.count_work = normal.rows[0].count_work - 1;
	let one_less = support::run(limits)?;
	let row = &one_less.rows[0];
	assert!(row.count_error.is_some());
	assert_eq!(row.count_work, limits.count_work);
	assert_eq!(row.adjoint.gates, normal.rows[0].adjoint.gates - 1);
	assert_eq!(row.observed_emissions, limits.count_work + 1);
	limits.count_work = 0;
	let none = support::run(limits)?;
	assert!(
		none.rows
			.iter()
			.all(|r| r.count_error.is_some() && r.count_work == 0)
	);
	Ok(())
}

#[test]
fn independent_gate_limit_and_constructor_byte_limit_remain_active() -> Result<()> {
	let limits = support::Limits {
		gates_per_orientation: 1,
		..support::Limits::default()
	};
	let partial = support::run(limits)?;
	for row in partial.rows {
		assert!(row.count_error.is_some());
		assert_eq!(row.forward.gates, 1);
		assert_eq!(row.adjoint.gates, 0);
		assert_eq!(row.observed_emissions, 2);
	}
	let limits = support::Limits {
		construction: quest_qsvt::portfolio::PortfolioLimits {
			max_bytes: 1,
			..quest_qsvt::portfolio::PortfolioLimits::default()
		},
		..support::Limits::default()
	};
	assert!(support::run(limits).is_err());
	Ok(())
}
