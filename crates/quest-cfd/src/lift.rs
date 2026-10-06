//! Alternative full-coordinate lifts for the same global history problem.
/// Lift selection; Carleman introduces a separately assessed order truncation.
#[derive(
	Clone,
	Copy,
	Debug,
	Default,
	PartialEq,
	Eq,
	clap::ValueEnum,
	serde::Serialize,
	serde::Deserialize,
)]
#[serde(rename_all = "kebab-case")]
pub enum LiftKind {
	/// Configuration-space Koopman-von Neumann generator.
	#[default]
	Kvn,
	/// All normalized symmetric monomials through the requested order.
	Carleman,
}
