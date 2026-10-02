//! Bounded exact classical transformations. Results become visible only after re-verification.
mod cleanup;
mod constants;
use quest_language::semantic::{CompileLimits, SemanticError};
use quest_language::ssa::{Program, SnapshotId, VerifiedProgram};
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OptimizationReport {
    pub input_snapshot: SnapshotId,
    pub output_snapshot: SnapshotId,
    pub constants_folded: usize,
    pub branches_simplified: usize,
    pub unreachable_blocks_removed: usize,
    pub common_expressions_removed: usize,
    pub dead_instructions_removed: usize,
    pub analysis_rounds: usize,
    pub work: usize,
}
impl OptimizationReport {
    const fn new(snapshot: SnapshotId) -> Self {
        Self {
            input_snapshot: snapshot,
            output_snapshot: snapshot,
            constants_folded: 0,
            branches_simplified: 0,
            unreachable_blocks_removed: 0,
            common_expressions_removed: 0,
            dead_instructions_removed: 0,
            analysis_rounds: 0,
            work: 0,
        }
    }
}
pub(super) struct Budget {
    remaining: usize,
    limit: usize,
}
impl Budget {
    pub fn tick(&mut self) -> Result<(), SemanticError> {
        self.remaining = self.remaining.checked_sub(1).ok_or_else(|| {
            SemanticError::limit(
                quest_language::ResourceKind::OptimizationWork,
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
) -> Result<(VerifiedProgram, OptimizationReport), SemanticError> {
    preflight(&program, limits)?;
    let mut report = OptimizationReport::new(program.snapshot());
    let mut program = program.into_unverified();
    let mut budget = Budget {
        remaining: limits.work,
        limit: limits.work,
    };
    let lattice = constants::analyze(&program, limits, &mut budget, &mut report)?;
    constants::apply(&mut program, &lattice, &mut budget, &mut report)?;
    drop(lattice);
    cleanup::predecessors(&mut program)?;
    let before = program.blocks.len();
    program = quest_language::semantic::cfg::prune(program)?;
    report.unreachable_blocks_removed = before.saturating_sub(program.blocks.len());
    cleanup::common_expressions(&mut program, &mut budget, &mut report)?;
    cleanup::dead_instructions(&mut program, &mut budget, &mut report)?;
    report.work = limits.work.saturating_sub(budget.remaining);
    let compile = CompileLimits {
        storage_bytes: limits.compile.storage_bytes.min(limits.storage_bytes),
        ..limits.compile
    };
    let verified = program.verify(compile)?;
    report.output_snapshot = verified.snapshot();
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
            quest_language::ResourceKind::StorageBytes,
            working,
            limits.storage_bytes,
            "optimizer working storage budget exceeded",
        ));
    }
    Ok(())
}
pub(super) fn scalar_equal(
    left: quest_language::classical::ScalarValue,
    right: quest_language::classical::ScalarValue,
) -> bool {
    use quest_language::classical::ScalarType;
    if left.ty() != right.ty() {
        return false;
    }
    match left.ty() {
        ScalarType::Bool => left.to_bool() == right.to_bool(),
        ScalarType::Float(_) => left.to_f64().map(f64::to_bits) == right.to_f64().map(f64::to_bits),
        _ => left.raw_bits() == right.raw_bits(),
    }
}
