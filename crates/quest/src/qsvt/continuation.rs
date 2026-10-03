//! A projected continuation is admitted and owned as one state.
use super::projection::{AdmittedProjection, PreparedProjection};
use crate::execution::{AdmittedPlan, PreparedRegion};

pub enum Continuation<B, P> {
	Direct,
	Projected { bridge: B, program: P },
}
pub type AdmittedContinuation<'env> = Continuation<AdmittedProjection<'env>, AdmittedPlan<'env>>;
pub type PreparedContinuation<'env> = Continuation<PreparedProjection<'env>, PreparedRegion<'env>>;
impl<B, P> Continuation<B, P> {
	pub const fn bridge(&self) -> Option<&B> {
		match self {
			Self::Direct => None,
			Self::Projected { bridge, .. } => Some(bridge),
		}
	}
	pub const fn program(&self) -> Option<&P> {
		match self {
			Self::Direct => None,
			Self::Projected { program, .. } => Some(program),
		}
	}
	pub const fn program_mut(&mut self) -> Option<&mut P> {
		match self {
			Self::Direct => None,
			Self::Projected { program, .. } => Some(program),
		}
	}
}
impl<'env> AdmittedContinuation<'env> {
	pub fn new(
		resources: &'env crate::environment::RuntimeResources,
		transform: &quest_qsvt::ValidatedTransform,
		plan: impl FnOnce(&quest_compile::BoundRegion) -> crate::Result<quest_compile::RegionPlan>,
	) -> super::Result<Self> {
		match transform.continuation_stage() {
			quest_qsvt::TransformContinuation::Direct => Ok(Self::Direct),
			quest_qsvt::TransformContinuation::Projected { bridge, program } => {
				let program = resources.admit_plan(plan(program)?)?;
				let bridge = AdmittedProjection::new(resources, bridge)?;
				Ok(Self::Projected { bridge, program })
			}
		}
	}
	pub fn materialize(self) -> super::Result<PreparedContinuation<'env>> {
		match self {
			Self::Direct => Ok(Continuation::Direct),
			Self::Projected { bridge, program } => {
				let program = program.materialize()?;
				let bridge = bridge.materialize()?;
				Ok(Continuation::Projected { bridge, program })
			}
		}
	}
}
