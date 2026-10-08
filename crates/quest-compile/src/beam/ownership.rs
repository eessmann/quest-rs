//! Candidate ownership, retained budget leases, and evidence identity.
#[cfg(feature = "workers")]
use super::{ApproxHistory, Arc, ExactHistory, RBig};
use super::{
	BoundRegion, BudgetCategory, BudgetLease, BudgetLedger, CostComponents, Error, QuantumRegion,
	Result, RoundingStatus,
};

pub(super) fn reserve_candidate(
	program: &QuantumRegion,
	ledger: &BudgetLedger,
) -> Result<BudgetLease> {
	let bytes = program
		.retained_bytes()?
		.checked_mul(4)
		.and_then(|amount| amount.checked_add(1024 * 1024))
		.ok_or(Error::Budget("beam candidate storage"))?;
	ledger.reserve(
		BudgetCategory::Candidate,
		0,
		u64::try_from(bytes).map_err(|_| Error::Budget("beam candidate storage"))?,
	)
}

pub(super) struct Candidate {
	pub(super) source: Box<QuantumRegion>,
	pub(super) bound: BoundRegion,
	pub(super) score: CostComponents,
	pub(super) terminal_bound: Option<BoundRegion>,
	pub(super) rounding: RoundingStatus,
	pub(super) allowances: Vec<BudgetLease>,
	pub(super) expanded: bool,
	pub(super) bound_only: bool,
	#[cfg(feature = "workers")]
	pub(super) exact_history: Option<Arc<ExactHistory>>,
	#[cfg(feature = "workers")]
	pub(super) approx_history: Option<Arc<ApproxHistory>>,
	#[cfg(feature = "workers")]
	pub(super) error_bound: Option<RBig>,
}

#[cfg(feature = "workers")]
pub(super) fn same_history<T>(left: Option<&Arc<T>>, right: Option<&Arc<T>>) -> bool {
	match (left, right) {
		(None, None) => true,
		(Some(left), Some(right)) => Arc::ptr_eq(left, right),
		_ => false,
	}
}

#[cfg(feature = "workers")]
pub(super) fn same_evidence(left: &Candidate, right: &Candidate) -> bool {
	same_history(left.exact_history.as_ref(), right.exact_history.as_ref())
		&& same_history(left.approx_history.as_ref(), right.approx_history.as_ref())
		&& left.error_bound == right.error_bound
}

#[cfg(not(feature = "workers"))]
pub(super) const fn same_evidence(_left: &Candidate, _right: &Candidate) -> bool {
	true
}
