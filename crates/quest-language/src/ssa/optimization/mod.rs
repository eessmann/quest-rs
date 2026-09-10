//! Bounded exact classical transformations. Results become visible only after re-verification.
mod cleanup;
mod constants;
use super::{Program, VerifiedProgram};
use crate::semantic::{CompileLimits, SemanticError};
/// Independent hard limits for analysis and the returned executable representation.
#[derive(Debug, Clone, Copy)]
pub struct OptimizationLimits {
    pub compile: CompileLimits,
    pub work: usize,
    pub rounds: usize,
    pub storage_bytes: usize,
}
impl Default for OptimizationLimits {
    fn default() -> Self {
        Self {
            compile: CompileLimits::default(),
            work: 10_000_000,
            rounds: 256,
            storage_bytes: 64 * 1024 * 1024,
        }
    }
}
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct OptimizationReport {
    pub constants_folded: usize,
    pub branches_simplified: usize,
    pub unreachable_blocks_removed: usize,
    pub common_expressions_removed: usize,
    pub dead_instructions_removed: usize,
    pub analysis_rounds: usize,
    pub work: usize,
}
pub type OptimizationError = SemanticError;
pub(super) struct Budget {
    remaining: usize,
    limit: usize,
}
impl Budget {
    pub fn tick(&mut self) -> Result<(), SemanticError> {
        self.remaining = self.remaining.checked_sub(1).ok_or_else(|| {
            SemanticError::limit(
                crate::ResourceKind::OptimizationWork,
                self.limit.saturating_add(1),
                self.limit,
                "optimization work budget exceeded",
            )
        })?;
        Ok(())
    }
}
/// Consume an immutable verified program and publish only a reverified transformed result.
///
/// No program clone is required. Failure consumes the input without exposing a partial candidate.
/// Export syntax, sources and host captures remain with their external owning wrapper.
///
/// # Errors
/// Reports exhausted analysis budgets or independent verification failures.
pub fn optimize(
    program: VerifiedProgram,
    limits: OptimizationLimits,
) -> Result<(VerifiedProgram, OptimizationReport), OptimizationError> {
    preflight(&program, limits)?;
    let mut program = program.into_unverified();
    let mut report = OptimizationReport::default();
    let mut budget = Budget {
        remaining: limits.work,
        limit: limits.work,
    };
    let lattice = constants::analyze(&program, limits, &mut budget, &mut report)?;
    constants::apply(&mut program, &lattice, &mut budget, &mut report)?;
    drop(lattice);
    cleanup::predecessors(&mut program)?;
    let before = program.blocks.len();
    program = crate::semantic::cfg::prune(program)?;
    report.unreachable_blocks_removed = before.saturating_sub(program.blocks.len());
    cleanup::common_expressions(&mut program, &mut budget, &mut report)?;
    cleanup::dead_instructions(&mut program, &mut budget, &mut report)?;
    report.work = limits.work.saturating_sub(budget.remaining);
    let compile = CompileLimits {
        storage_bytes: limits.compile.storage_bytes.min(limits.storage_bytes),
        ..limits.compile
    };
    let verified = program.verify(compile)?;
    Ok((verified, report))
}
fn preflight(program: &VerifiedProgram, limits: OptimizationLimits) -> Result<(), SemanticError> {
    let nodes = program.blocks().iter().try_fold(0usize, |count, block| {
        count
            .checked_add(block.arguments.len())
            .and_then(|count| count.checked_add(block.instructions.len()))
            .ok_or_else(|| SemanticError::budget("optimizer node count overflow"))
    })?;
    let working = program
        .retained_bytes()?
        .checked_mul(2)
        .and_then(|size| {
            nodes
                .checked_mul(512)
                .and_then(|nodes| size.checked_add(nodes))
        })
        .ok_or_else(|| SemanticError::budget("optimizer storage estimate overflow"))?;
    if working > limits.storage_bytes {
        return Err(SemanticError::limit(
            crate::ResourceKind::StorageBytes,
            working,
            limits.storage_bytes,
            "optimizer working storage budget exceeded",
        ));
    }
    Ok(())
}
pub(super) fn scalar_equal(
    left: crate::classical::ScalarValue,
    right: crate::classical::ScalarValue,
) -> bool {
    use crate::classical::ScalarType;
    if left.ty() != right.ty() {
        return false;
    }
    match left.ty() {
        ScalarType::Bool => left.to_bool() == right.to_bool(),
        ScalarType::Float(_) => left.to_f64().map(f64::to_bits) == right.to_f64().map(f64::to_bits),
        _ => left.raw_bits() == right.raw_bits(),
    }
}
