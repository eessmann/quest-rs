//! Experiment admission separates managed budgets from observed OS limits.
use serde_json::{Value, json};
use std::path::PathBuf;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Limit {
	Finite(usize),
	Unlimited,
}

#[cfg(test)]
mod tests {
	use super::*;

	fn args(words: &str) -> Result<Arguments> {
		Arguments::parse(words.split_whitespace().map(str::to_owned))
	}
	fn limits(soft: &str, hard: &str) -> ProcessLimits {
		ProcessLimits::parse(&format!("Max address space {soft} {hard} bytes\n")).unwrap()
	}

	#[test]
	fn legacy_capped_admission_requires_the_actual_equal_finite_limit() {
		let arguments = args("output 16 2 1000 100 200").unwrap();
		assert_eq!(arguments.placement, None);
		assert_eq!(arguments.threads, 1);
		assert_eq!(arguments.stack_allowance, 0);
		assert!(!arguments.explicit_threads);
		assert!(arguments.admit_limits(limits("1000", "1000")).is_ok());
		for observed in [
			limits("unlimited", "unlimited"),
			limits("1000", "unlimited"),
			limits("1000", "2000"),
			limits("2000", "2000"),
		] {
			assert!(arguments.admit_limits(observed).is_err(), "{observed:?}");
		}
	}

	#[test]
	fn scaling_retains_all_budgets_and_supports_real_unlimited_limits() {
		let arguments = args("--scaling output 64 3 100 200 1 2 2 8").unwrap();
		let observed = limits("unlimited", "unlimited");
		assert_eq!(arguments.mode, Mode::Scaling);
		assert_eq!(arguments.placement, Some((1, 2)));
		assert_eq!((arguments.rank_budget, arguments.node_budget), (100, 200));
		assert!(arguments.admit_limits(observed).is_ok());
		assert_eq!(arguments.rank_envelope(20, observed).unwrap(), 128);
		assert!(arguments.rank_envelope(usize::MAX, observed).is_err());
	}

	#[test]
	fn scaling_respects_finite_soft_limits_even_when_hard_is_unlimited() {
		let arguments = args("--scaling output 16 1 100 200 1 2 2 8").unwrap();
		assert_eq!(
			arguments
				.rank_envelope(20, limits("128", "unlimited"))
				.unwrap(),
			128
		);
		assert!(
			arguments
				.rank_envelope(20, limits("127", "unlimited"))
				.is_err()
		);
		assert!(arguments.rank_envelope(20, limits("127", "127")).is_err());
	}

	#[test]
	fn invalid_or_overflowing_budgets_are_rejected_before_mpi() {
		for words in [
			"--scaling output 16 1 0 200 1 2 2 8",
			"--scaling output 16 1 100 0 1 2 2 8",
			"--scaling output 16 1 201 200 1 2 2 8",
			"--scaling output 16 1 101 200 1 2 2 8",
			"--scaling output 16 1 100 200 0 2 2 8",
			"--scaling output 16 1 100 200 1 0 2 8",
			"--scaling output 16 1 100 200 1 3 2 8",
			"--scaling output 16 1 100 200 64 1 2 8",
			"--scaling output 16 1 100 200 1 2 0 8",
			"--scaling output 16 1 100 200 1 2 2 0",
			"--scaling output 16 1 100 200 1 2 2",
			"--scaling output 16 1 100 200 1 2",
			"--scaling output 16 1 100 200",
		] {
			assert!(args(words).is_err(), "{words}");
		}
		for words in [
			format!(
				"--scaling output 16 1 {} {} 1 1 2 8",
				usize::MAX,
				usize::MAX
			),
			format!("--scaling output 16 1 100 {} 1 2 2 8", usize::MAX),
			format!(
				"--scaling output 16 1 {} {} 1 2 1 8",
				usize::MAX,
				usize::MAX
			),
			format!("--scaling output 16 1 100 200 {} 2 2 8", usize::MAX),
		] {
			assert!(args(&words).is_err(), "{words}");
		}
	}

	#[test]
	fn limits_require_known_units_and_valid_soft_hard_order() {
		for contents in [
			"",
			"Max address space unlimited 100 bytes",
			"Max address space 200 100 bytes",
			"Max address space 100 100 kb",
			"Max address space 100 100 bytes extra",
		] {
			assert!(ProcessLimits::parse(contents).is_err(), "{contents}");
		}
		assert_eq!(limits("100", "unlimited").soft, Limit::Finite(100));
	}

	#[test]
	fn scaling_receipts_cannot_claim_capacity_even_with_a_finite_limit() {
		let arguments = args("--scaling output 16 1 100 200 1 2 2 8").unwrap();
		for observed in [limits("unlimited", "unlimited"), limits("1000", "1000")] {
			let receipt = arguments
				.scaling_evidence(observed, observed, 128, 256)
				.unwrap();
			assert_eq!(receipt["schema_version"], 6);
			assert_eq!(receipt["evidence_kind"], "scaling-only");
			assert_eq!(receipt["capacity_closed"], false);
			assert!(receipt["whole_node_enforced_memory_cap_bytes"].is_null());
			assert!(receipt["whole_node_peak_bytes"].is_null());
			assert!(receipt["persistence_load_wire_bytes"].is_null());
			assert_eq!(receipt["modeled_node_rank_envelope_bytes"], 256);
			assert_eq!(
				receipt["process_address_space_limits_before"],
				receipt["process_address_space_limits_after"]
			);
		}
		let unlimited = limits("unlimited", "unlimited");
		let receipt = arguments
			.scaling_evidence(unlimited, unlimited, 128, 256)
			.unwrap();
		assert_eq!(
			receipt["process_address_space_limits_before"]["soft"],
			json!({"kind":"unlimited"})
		);
		assert!(
			args("output 16 1 1000 100 200")
				.unwrap()
				.scaling_evidence(limits("1000", "1000"), limits("1000", "1000"), 100, 200)
				.is_none()
		);
	}
}
impl Limit {
	fn parse(word: &str) -> Result<Self> {
		Ok(if word == "unlimited" {
			Self::Unlimited
		} else {
			Self::Finite(word.parse()?)
		})
	}
	const fn permits(self, bytes: usize) -> bool {
		match self {
			Self::Finite(limit) => bytes <= limit,
			Self::Unlimited => true,
		}
	}
	fn value(self) -> Value {
		match self {
			Self::Finite(bytes) => json!({"kind":"finite","bytes":bytes}),
			Self::Unlimited => json!({"kind":"unlimited"}),
		}
	}
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProcessLimits {
	pub soft: Limit,
	pub hard: Limit,
}
impl ProcessLimits {
	pub fn observe() -> Result<Self> {
		Self::parse(&std::fs::read_to_string("/proc/self/limits")?)
	}
	pub fn parse(contents: &str) -> Result<Self> {
		let mut words = contents
			.lines()
			.find_map(|line| line.strip_prefix("Max address space"))
			.ok_or("Linux address-space limit")?
			.split_whitespace();
		let soft = Limit::parse(words.next().ok_or("soft address-space limit")?)?;
		let hard = Limit::parse(words.next().ok_or("hard address-space limit")?)?;
		if words.next() != Some("bytes") || words.next().is_some() {
			return Err("invalid Linux address-space limit units".into());
		}
		if matches!((soft, hard), (Limit::Unlimited, Limit::Finite(_)))
			|| matches!((soft, hard), (Limit::Finite(s), Limit::Finite(h)) if s > h)
		{
			return Err("soft address-space limit exceeds hard limit".into());
		}
		Ok(Self { soft, hard })
	}
	pub fn equal_finite(self) -> Result<usize> {
		match (self.soft, self.hard) {
			(Limit::Finite(soft), Limit::Finite(hard)) if soft == hard => Ok(soft),
			_ => Err("capped mode requires equal finite hard/soft address-space limits".into()),
		}
	}
	fn value(self) -> Value {
		json!({"source":"/proc/self/limits: Max address space","soft":self.soft.value(),"hard":self.hard.value()})
	}
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
	Capped { expected: usize },
	Scaling,
}

#[derive(Debug)]
pub struct Arguments {
	pub mode: Mode,
	pub directory: PathBuf,
	pub dimension: usize,
	pub repetitions: usize,
	pub rank_budget: usize,
	pub node_budget: usize,
	pub placement: Option<(usize, usize)>,
	pub explicit_threads: bool,
	pub threads: usize,
	pub stack_bytes: usize,
	pub stack_allowance: usize,
}

fn next_number(args: &mut impl Iterator<Item = String>, label: &'static str) -> Result<usize> {
	Ok(args.next().ok_or(label)?.parse()?)
}

impl Arguments {
	pub fn parse(mut args: impl Iterator<Item = String>) -> Result<Self> {
		let first = args.next().ok_or("output directory or --scaling")?;
		let scaling = first == "--scaling";
		let directory = PathBuf::from(if scaling {
			args.next().ok_or("scaling output directory")?
		} else {
			first
		});
		let dimension = next_number(&mut args, "dimension")?;
		let repetitions = next_number(&mut args, "repetitions")?;
		let mode = if scaling {
			Mode::Scaling
		} else {
			Mode::Capped {
				expected: next_number(&mut args, "process AS cap")?,
			}
		};
		let rank_budget = next_number(&mut args, "managed rank budget")?;
		let node_budget = next_number(&mut args, "managed node budget")?;
		let placement = args
			.next()
			.map(|nodes| -> Result<(usize, usize)> {
				Ok((nodes.parse()?, next_number(&mut args, "ranks per node")?))
			})
			.transpose()?;
		let thread_argument = args.next().map(|v| v.parse::<usize>()).transpose()?;
		let threads = thread_argument.unwrap_or(1);
		let stack_argument = args.next().map(|v| v.parse::<usize>()).transpose()?;
		let stack_bytes = stack_argument.unwrap_or(8 * 1024 * 1024);
		let stack_allowance = threads
			.checked_sub(1)
			.and_then(|count| count.checked_mul(stack_bytes))
			.ok_or("OpenMP stack allowance overflow")?;
		if args.next().is_some()
			|| !(1..=1024).contains(&threads)
			|| !(1..=1_073_741_824).contains(&stack_bytes)
			|| dimension < 16
			|| dimension.checked_mul(1_048_576).is_none()
			|| !dimension.is_power_of_two()
			|| !(1..=8).contains(&repetitions)
			|| rank_budget == 0
			|| node_budget == 0
			|| rank_budget > node_budget
			|| rank_budget.checked_add(stack_allowance).is_none()
			|| (scaling
				&& (placement.is_none() || thread_argument.is_none() || stack_argument.is_none()))
		{
			return Err("invalid bounded experiment arguments".into());
		}
		if let Some((nodes, per_node)) = placement
			&& (nodes == 0
				|| per_node == 0
				|| nodes
					.checked_mul(per_node)
					.is_none_or(|p| p > 32 || !p.is_power_of_two())
				|| rank_budget
					.checked_mul(per_node)
					.is_none_or(|b| b > node_budget)
				|| stack_allowance
					.checked_mul(per_node)
					.and_then(|b| node_budget.checked_add(b))
					.is_none())
		{
			return Err("invalid managed node/placement envelope".into());
		}
		Ok(Self {
			mode,
			directory,
			dimension,
			repetitions,
			rank_budget,
			node_budget,
			placement,
			explicit_threads: thread_argument.is_some(),
			threads,
			stack_bytes,
			stack_allowance,
		})
	}

	pub fn admit_limits(&self, observed: ProcessLimits) -> Result<()> {
		if let Mode::Capped { expected } = self.mode
			&& observed.equal_finite()? != expected
		{
			return Err("observed address-space cap differs from capped-mode argument".into());
		}
		Ok(())
	}

	pub fn rank_envelope(&self, baseline: usize, observed: ProcessLimits) -> Result<usize> {
		let envelope = baseline
			.checked_add(self.rank_budget)
			.and_then(|bytes| bytes.checked_add(self.stack_allowance))
			.ok_or("MPI baseline, managed rank budget and OpenMP stacks overflow")?;
		if !observed.soft.permits(envelope) {
			return Err(
				"MPI baseline, managed rank budget and OpenMP stacks exceed process limit".into(),
			);
		}
		Ok(envelope)
	}

	pub fn scaling_evidence(
		&self,
		before: ProcessLimits,
		after: ProcessLimits,
		rank_envelope: usize,
		node_envelope: usize,
	) -> Option<Value> {
		(self.mode == Mode::Scaling).then(|| json!({
			"schema_version":6,"evidence_kind":"scaling-only","capacity_closed":false,
			"capacity_status":"not assessed: no verified whole-node enforcement",
			"process_address_space_limits_before":before.value(),
			"process_address_space_limits_after":after.value(),
			"managed_budget_scope":"application-managed admission budgets; not OS memory enforcement",
			"modeled_rank_envelope_bytes":rank_envelope,
			"modeled_node_rank_envelope_bytes":node_envelope,
			"modeled_envelope_scope":"MPI baseline address space plus managed rank budgets and configured OpenMP worker stacks; rank processes only, not a measured peak or whole-node cap",
			"whole_node_enforced_memory_cap_bytes":null,"whole_node_peak_bytes":null,
			"hugetlb_enforcement_coverage":"unmeasured",
			"node_memory_sampling_scope":"sampled sum of known MPI rank processes only; excludes coordinator, launcher, helpers and filesystem cache",
			"persistence_load_wire_bytes":null
		}))
	}
}
