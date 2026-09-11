use crate::{Context, EmbeddedArgs, Error, OverlapArgs, Result, SolveArgs, Stage, TransformRoute};
use faer::{
    Mat, MatRef, Par,
    dyn_stack::{MemBuffer, MemStack},
};
use num_complex::Complex64 as C;
use quest::{Environment, QubitCount};
use quest_circuit::{NumericalOperator, OracleFragment, ProgramBuilder};
use quest_qsvt::{
    DenseEncodingBuilder, EncodingBuilder, Left, LogicalSpace, NumericalPolicy, ProjectedEncoding,
    Right, TransformBuilder, ValidatedTransform,
};
use quest_qsvt_io::{
    IoPolicy, QspInput,
    hdf5::{self, StoredBlockEncoding},
};
use serde_json::{Value, json};
use std::ops::{Add, Div, Mul, Sub};

pub fn encoding(block: &StoredBlockEncoding) -> Result<ProjectedEncoding> {
    let policy = NumericalPolicy::default();
    let width =
        usize::try_from(block.u().nrows().ilog2()).map_err(|_| Error::Budget("oracle width"))?;
    let mut builder = ProgramBuilder::new(width, 0)?;
    let targets = (0..width)
        .map(|index| builder.qubit(index))
        .collect::<quest_circuit::Result<Vec<_>>>()?;
    builder.numerical(
        NumericalOperator::from_view(block.u(), policy.matrix_policy())?,
        &targets,
        &[],
    )?;
    let oracle = OracleFragment::builder(builder.finish()?.bind(&[])?)
        .matrix_tolerance(1e-12)?
        .matrix_policy(policy.matrix_policy())
        .build()?;
    let [rows, cols] = block.original_dimensions();
    Ok(EncodingBuilder::new()
        .oracle(oracle)
        .left(LogicalSpace::<Left>::from_isometry(
            block.pi_left().subcols(0, rows),
            policy,
        )?)
        .right(LogicalSpace::<Right>::from_isometry(
            block.pi_right().subcols(0, cols),
            policy,
        )?)
        .normalization(block.alpha())?
        .policy(policy)
        .build()?)
}
pub fn transform(
    source: ProjectedEncoding,
    route: TransformRoute,
    input: QspInput,
    idle: usize,
) -> Result<ValidatedTransform> {
    let auxiliary = !matches!(route, TransformRoute::Standard | TransformRoute::Direct);
    let layout = quest_qsvt::OperandLayout::canonical(source.num_qubits(), auxiliary)?
        .with_idle_high_qubits(idle)?;
    let builder = TransformBuilder::new().encoding(source).operands(layout);
    match (route, input) {
        (TransformRoute::Standard, QspInput::Symmetric(phases)) => {
            Ok(builder.standard(phases).build()?)
        }
        (TransformRoute::Standard, QspInput::Laurent(phases)) => {
            Ok(builder.standard(phases).build()?)
        }
        (
            route,
            QspInput::GeneralizedAngles { controls, .. } | QspInput::GeneralizedMatrices(controls),
        ) => Ok(match route {
            TransformRoute::Standard => {
                return Err(Error::Input("standard route requires tagged Wx phases"));
            }
            TransformRoute::Direct => builder.direct(controls).build()?,
            TransformRoute::HermitianizedFull => builder.hermitianized_full(controls).build()?,
            TransformRoute::HermitianizedEven => builder.hermitianized_even(controls).build()?,
            TransformRoute::HermitianizedOdd => builder.hermitianized_odd(controls).build()?,
            TransformRoute::MultiplicationEven => builder.multiplication_even(controls).build()?,
            TransformRoute::MultiplicationOdd => builder.multiplication_odd(controls).build()?,
        }),
        _ => Err(Error::Input(
            "route and imported convention disagree; synthesize polynomial input explicitly first",
        )),
    }
}
pub fn transform_report(transform: &ValidatedTransform) -> Value {
    let counts = transform.query_counts();
    json!({"route":format!("{:?}",transform.route()),"degree":transform.degree(),
        "normalization":transform.normalization().get(), "qubits":transform.operands().num_qubits(),
        "source_convention":transform.convention(),
        "phase_conversion_roundoff_estimate":transform.evidence().phase_conversion_roundoff_estimate,
        "queries":{"semantic":counts.semantic,"source_forward":counts.source_forward,
            "source_adjoint":counts.source_adjoint,"retained_oracle_calls":counts.retained_oracle_calls},
        "imported_response_is_reciprocal_certificate":false})
}
fn norm(values: &[C]) -> f64 {
    values
        .iter()
        .fold(0.0_f64, |sum, z| sum.hypot(z.re).hypot(z.im))
}
pub fn vector(mut value: impl Iterator<Item = C>, length: usize) -> Result<Vec<C>> {
    if length
        .checked_mul(size_of::<C>())
        .is_none_or(|n| n > NumericalPolicy::default().max_bytes)
    {
        return Err(Error::Budget("vector storage"));
    }
    let mut output = Vec::new();
    output
        .try_reserve_exact(length)
        .map_err(|_| Error::Budget("vector allocation"))?;
    for _ in 0..length {
        let entry = value.next().ok_or(Error::Input("vector length"))?;
        if !entry.re.is_finite() || !entry.im.is_finite() {
            return Err(Error::Input("nonfinite computed vector"));
        }
        output.push(entry);
    }
    Ok(output)
}
pub fn matrix(rows: usize, cols: usize) -> Result<Mat<C>> {
    if rows
        .checked_next_multiple_of(4)
        .and_then(|r| r.checked_mul(cols))
        .and_then(|n| n.checked_mul(size_of::<C>()))
        .is_none_or(|n| n > NumericalPolicy::default().max_bytes)
    {
        return Err(Error::Budget("matrix storage"));
    }
    let mut value = Mat::new();
    value
        .try_reserve(rows, cols)
        .map_err(|_| Error::Budget("matrix allocation"))?;
    value.resize_with(rows, cols, |_, _| C::new(0.0, 0.0));
    Ok(value)
}
pub fn product(matrix: MatRef<'_, C>, values: &[C]) -> Result<Vec<C>> {
    if matrix.ncols() != values.len() {
        return Err(Error::Input("matrix/vector dimensions"));
    }
    vector(
        (0..matrix.nrows()).map(|r| {
            values
                .iter()
                .enumerate()
                .fold(C::new(0.0, 0.0), |sum, (c, z)| {
                    sum.add(matrix[(r, c)].mul(*z))
                })
        }),
        matrix.nrows(),
    )
}
struct Executed {
    raw: Vec<C>,
    normalized: Option<Vec<C>>,
    mass: quest::qsvt::MassObservation,
}
fn execute(
    transform: ValidatedTransform,
    input: &[C],
    normalize: bool,
    context: &mut Context<'_>,
) -> Result<Executed> {
    if input.len() != transform.input().logical_dimension() {
        return Err(Error::Input("logical input state dimension"));
    }
    if !norm(input).is_finite() {
        return Err(Error::Input("input norm overflow"));
    }
    let width = transform.operands().num_qubits();
    let amplitudes = context.measure("input_embedding", Stage::Construction, || {
        let basis = transform
            .input()
            .materialize_isometry(width, NumericalPolicy::default())?;
        product(basis.as_ref(), input)
    })?;
    let environment = context.measure("environment", Stage::Preparation, || {
        Ok(Environment::builder().build()?)
    })?;
    let admitted = context.measure("lowering_and_admission", Stage::Lowering, || {
        Ok(environment.qsvt().transform(transform).admit()?)
    })?;
    let (mut prepared, mut register) =
        context.measure("preparation", Stage::Preparation, || {
            let prepared = admitted.prepare()?;
            let mut register = environment.state_vector(QubitCount::new(width)?)?;
            register.init_pure(&amplitudes)?;
            Ok((prepared, register))
        })?;
    let result = context.measure("execution", Stage::Execution, || {
        Ok(prepared.run(&mut register)?)
    })?;
    context.measure("postselection_and_decode", Stage::Postselection, || {
        let mass = result.mass();
        let raw = result.logical_snapshot()?;
        let raw = vector((0..raw.nrows()).map(|r| raw[(r, 0)]), raw.nrows())?;
        let normalized = if normalize {
            let conditioned = result.condition()?;
            let snapshot = conditioned.logical_snapshot()?;
            Some(vector(
                (0..snapshot.nrows()).map(|r| snapshot[(r, 0)]),
                snapshot.nrows(),
            )?)
        } else {
            let _ = result.release();
            None
        };
        Ok(Executed {
            raw,
            normalized,
            mass,
        })
    })
}
pub fn native_dispatch_report(report: quest::qsvt::NativeDispatchReport) -> Value {
    json!({"scope":"successful_run_per_rank", "total":report.total(),
        "circuit":report.circuit(), "projection":report.projection(),
        "readout":report.readout(), "state_management":report.state_management(),
        "excludes":["setup", "run_admission", "MPI_agreement", "caller_initialization", "snapshots", "later_conditioning"],
        "partial_error_count_available":false})
}
pub fn mass_report(mass: quest::qsvt::MassObservation) -> Value {
    json!({"initial":mass.initial(),"input":mass.input(),"bridge":mass.bridge(),"retained":mass.retained(),"relative_success":mass.relative_success(),"native_dispatches":native_dispatch_report(mass.native_dispatches())})
}
pub fn embedded(args: &EmbeddedArgs, context: &mut Context<'_>) -> Result<Value> {
    if args.distributed {
        return Err(Error::Feature(
            "MPI application admission is required for distributed execution",
        ));
    }
    let output_path = args.output_state.as_ref().ok_or(Error::Input(
        "local embedded execution requires --output-state",
    ))?;

    let (stored, input) = context.measure("read", Stage::Construction, || {
        Ok((
            hdf5::read_block_encoding(&args.encoding, IoPolicy::default())?,
            hdf5::read_state_vector(&args.input_state, IoPolicy::default())?,
        ))
    })?;
    let (frozen, input_report) = crate::synthesis::freeze_input(&args.transform, context)?;
    let transform = context.measure("transform_construction", Stage::Construction, || {
        transform(encoding(&stored)?, args.transform.route, frozen, 0)
    })?;
    let mut report = transform_report(&transform);
    crate::set(&mut report, "input_synthesis", input_report)?;
    let executed = execute(
        transform,
        &input,
        args.normalized_output_state.is_some(),
        context,
    )?;
    crate::set(&mut report, "mass", mass_report(executed.mass))?;
    crate::set(
        &mut report,
        "logical_output_norm",
        json!(norm(&executed.raw)),
    )?;
    context.measure("write", Stage::Postselection, || {
        hdf5::write_state_vector(output_path, &executed.raw, IoPolicy::default())?;
        if let (Some(path), Some(values)) = (&args.normalized_output_state, &executed.normalized) {
            hdf5::write_state_vector(path, values, IoPolicy::default())?;
        }
        Ok(())
    })?;
    Ok(report)
}
pub fn overlap(args: &OverlapArgs, context: &mut Context<'_>) -> Result<Value> {
    if args.distributed {
        return Err(Error::Feature(
            "MPI application admission is required for distributed execution",
        ));
    }

    let (stored, input, reference) = context.measure("read", Stage::Construction, || {
        Ok((
            hdf5::read_block_encoding(&args.encoding, IoPolicy::default())?,
            hdf5::read_state_vector(&args.input_state, IoPolicy::default())?,
            hdf5::read_state_vector(&args.reference_state, IoPolicy::default())?,
        ))
    })?;
    let (frozen, input_report) = crate::synthesis::freeze_input(&args.transform, context)?;
    let transform = context.measure("transform_construction", Stage::Construction, || {
        transform(encoding(&stored)?, args.transform.route, frozen, 0)
    })?;
    let mut report = transform_report(&transform);
    crate::set(&mut report, "input_synthesis", input_report)?;
    let environment = context.measure("environment", Stage::Preparation, || {
        Ok(Environment::builder().build()?)
    })?;
    let admitted = context.measure("lowering_and_admission", Stage::Lowering, || {
        Ok(environment
            .qsvt()
            .transform(transform)
            .overlap()
            .input(input)
            .reference(reference)
            .admit()?)
    })?;
    let mut prepared =
        context.measure(
            "preparation",
            Stage::Preparation,
            || Ok(admitted.prepare()?),
        )?;
    let observed = context.measure("execution", Stage::Execution, || Ok(prepared.run()?))?;
    crate::set(
        &mut report,
        "overlap",
        json!([observed.overlap().re, observed.overlap().im]),
    )?;
    crate::set(
        &mut report,
        "normalized_overlap",
        json!(observed.normalized_overlap().map(|z| [z.re, z.im])),
    )?;
    crate::set(
        &mut report,
        "mass",
        json!({"retained":observed.retained_mass(),"active":observed.active_mass(),"real_zero":observed.real_zero_mass(),"imaginary_zero":observed.imaginary_zero_mass(),"transformed_norm":observed.transformed_norm(),"native_dispatches":native_dispatch_report(observed.native_dispatches())}),
    )?;
    Ok(report)
}
fn singular_values(matrix: MatRef<'_, C>) -> Result<(f64, f64, f64)> {
    use faer::linalg::svd::{ComputeSvdVectors, svd, svd_scratch};
    let n = matrix.nrows();
    if n == 0 || n != matrix.ncols() {
        return Err(Error::Input("solve requires a nonempty square matrix"));
    }
    let req = svd_scratch::<C>(
        n,
        n,
        ComputeSvdVectors::No,
        ComputeSvdVectors::No,
        Par::Seq,
        faer::Spec::default(),
    );
    if n.checked_mul(n)
        .and_then(|v| v.checked_add(n))
        .and_then(|v| v.checked_mul(size_of::<C>()))
        .and_then(|v| v.checked_add(req.size_bytes()))
        .is_none_or(|v| v > NumericalPolicy::default().max_bytes)
    {
        return Err(Error::Budget("sequential SVD workspace"));
    }
    let mut values = self::matrix(n, 1)?;
    let mut scratch =
        MemBuffer::try_new(req).map_err(|_| Error::Budget("SVD workspace allocation"))?;
    svd(
        matrix,
        values.col_mut(0).as_diagonal_mut(),
        None,
        None,
        Par::Seq,
        MemStack::new(&mut scratch),
        faer::Spec::default(),
    )
    .map_err(|_| Error::Input("SVD failed to converge"))?;
    let maximum = values[(0, 0)].re;
    let minimum = values[(n.saturating_sub(1), 0)].re;
    let threshold = maximum
        .mul(f64::from(
            u32::try_from(n).map_err(|_| Error::Budget("rank dimension"))?,
        ))
        .mul(f64::EPSILON);
    if !maximum.is_finite() || !minimum.is_finite() || maximum <= 0.0 || minimum <= threshold {
        return Err(Error::Input(
            "solve requires numerical full rank at sigma_max * dimension * binary64 epsilon",
        ));
    }
    Ok((maximum, minimum, threshold))
}
struct SolveInput {
    a: Mat<C>,
    b: Vec<C>,
    b_norm: f64,
    sigma_max: f64,
    sigma_min: f64,
    rank_threshold: f64,
}
fn read_solve(args: &SolveArgs, context: &mut Context<'_>) -> Result<SolveInput> {
    if !args.reciprocal_scale.is_finite() || args.reciprocal_scale <= 0.0 {
        return Err(Error::Input("positive finite reciprocal scale required"));
    }
    if args
        .residual_tolerance
        .is_some_and(|t| !t.is_finite() || t <= 0.0)
    {
        return Err(Error::Input(
            "positive finite physical residual tolerance required",
        ));
    }
    if !matches!(
        args.transform.route,
        TransformRoute::Standard
            | TransformRoute::HermitianizedOdd
            | TransformRoute::MultiplicationOdd
    ) {
        return Err(Error::Input(
            "solve requires standard or an odd singular-value route",
        ));
    }
    let (a, b) = context.measure("read", Stage::Construction, || {
        Ok((
            hdf5::read_matrix(&args.matrix, IoPolicy::default())?.into_dense(IoPolicy::default())?,
            hdf5::read_state_vector(&args.rhs, IoPolicy::default())?,
        ))
    })?;
    if a.nrows() != b.len() {
        return Err(Error::Input("right-hand side dimension"));
    }
    let b_norm = norm(&b);
    if !b_norm.is_finite() || b_norm <= 0.0 {
        return Err(Error::Input("finite nonzero right-hand side norm required"));
    }
    let (sigma_max, sigma_min, rank_threshold) =
        context.measure("svd", Stage::Construction, || singular_values(a.as_ref()))?;
    Ok(SolveInput {
        a,
        b,
        b_norm,
        sigma_max,
        sigma_min,
        rank_threshold,
    })
}
pub fn solve(args: &SolveArgs, context: &mut Context<'_>) -> Result<Value> {
    let SolveInput {
        a,
        b,
        b_norm,
        sigma_max,
        sigma_min,
        rank_threshold,
    } = read_solve(args, context)?;
    let normalized_b = vector(b.iter().map(|z| z.div(b_norm)), b.len())?;
    let (frozen, input_report) = crate::synthesis::freeze_input(&args.transform, context)?;
    let transform = context.measure("transform_construction", Stage::Construction, || {
        solve_transform(a.as_ref(), sigma_max, args, frozen)
    })?;
    let mut report = transform_report(&transform);
    crate::set(&mut report, "input_synthesis", input_report)?;
    let executed = execute(
        transform,
        &normalized_b,
        args.normalized_output_state.is_some(),
        context,
    )?;
    let scale = b_norm.div(sigma_max).div(args.reciprocal_scale);
    if !scale.is_finite() || scale <= 0.0 {
        return Err(Error::Input(
            "physical solution rescaling overflow or underflow",
        ));
    }
    let physical = vector(
        executed.raw.iter().map(|z| z.mul(scale)),
        executed.raw.len(),
    )?;
    let residual = context.measure("physical_residual", Stage::Postselection, || {
        let applied = product(a.as_ref(), &physical)?;
        let residual = vector(applied.iter().zip(&b).map(|(a, b)| a.sub(*b)), b.len())?;
        let absolute = norm(&residual);
        if !absolute.is_finite() {
            return Err(Error::Input("physical residual overflow"));
        }
        Ok(absolute)
    })?;
    let relative = residual.div(b_norm);
    crate::set(&mut report, "mass", mass_report(executed.mass))?;
    crate::set(&mut report, "sigma_max", json!(sigma_max))?;
    crate::set(&mut report, "sigma_min", json!(sigma_min))?;
    crate::set(
        &mut report,
        "numerical_rank_threshold",
        json!(rank_threshold),
    )?;
    crate::set(&mut report, "rhs_norm", json!(b_norm))?;
    crate::set(
        &mut report,
        "reciprocal_scale_premise",
        json!(args.reciprocal_scale),
    )?;
    crate::set(&mut report, "physical_rescaling", json!(scale))?;
    crate::set(
        &mut report,
        "physical_solution_norm",
        json!(norm(&physical)),
    )?;
    crate::set(&mut report, "absolute_physical_residual", json!(residual))?;
    crate::set(&mut report, "relative_physical_residual", json!(relative))?;
    crate::set(
        &mut report,
        "failed",
        json!(usize::from(
            args.residual_tolerance.is_some_and(|t| relative > t)
        )),
    )?;
    crate::set(
        &mut report,
        "residual_tolerance",
        json!(args.residual_tolerance),
    )?;
    context.measure("write", Stage::Postselection, || {
        hdf5::write_state_vector(&args.output_state, &physical, IoPolicy::default())?;
        if let (Some(path), Some(values)) = (&args.normalized_output_state, &executed.normalized) {
            hdf5::write_state_vector(path, values, IoPolicy::default())?;
        }
        Ok(())
    })?;
    Ok(report)
}

fn solve_transform(
    a: MatRef<'_, C>,
    sigma_max: f64,
    args: &SolveArgs,
    frozen: QspInput,
) -> Result<ValidatedTransform> {
    let encoding = DenseEncodingBuilder::new(a.adjoint(), NumericalPolicy::default())?
        .normalization(sigma_max)?
        .build()?;
    let transform = transform(encoding, args.transform.route, frozen, 0)?;
    if transform.input().logical_dimension() != a.nrows()
        || transform.output().logical_dimension() != a.ncols()
    {
        return Err(Error::Input(
            "selected route does not map the square logical spaces",
        ));
    }
    if matches!(args.transform.route, TransformRoute::Standard) && transform.degree() % 2 == 0 {
        return Err(Error::Input(
            "reciprocal standard route requires an odd degree",
        ));
    }
    Ok(transform)
}
