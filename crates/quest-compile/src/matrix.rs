pub use quest_language::matrix::{MatrixPolicy, NumericalOperator, UnitaryAdmission};
/// # Errors
/// Rejects invalid dimensions, nonfinite values, failed numerical admission, and allocation limits.
pub fn check_channel(
	kraus: &[NumericalOperator],
	tolerance: f64,
	policy: MatrixPolicy,
) -> crate::Result<()> {
	Ok(quest_language::matrix::check_channel(
		kraus, tolerance, policy,
	)?)
}
