use crate::{Context, EmbeddedArgs, Error, OverlapArgs, Result, SolveArgs, Stage, TransformRoute};
use faer::{
    Mat, MatRef, Par,
    dyn_stack::{MemBuffer, MemStack},
};
use num_complex::Complex64 as C;
use quest::{Environment, QubitCount};
use quest_compile::{NumericalOperator, OracleFragment, QuantumRegionBuilder};
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
    let mut builder = QuantumRegionBuilder::new(width, 0)?;
    let targets = (0..width)
        .map(|index| builder.qubit(index))
        .collect::<quest_compile::Result<Vec<_>>>()?;
    builder.numerical(
        NumericalOperator::from_view(block.u(), policy.matrix_policy())
            .map_err(quest_qsvt::Error::from)?,
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
#[cfg(feature = "certification")]
fn certified_response<V: quest_qsvt::ResponseArgument>(
    certificate: quest_qsp::certification::Certified<quest_qsp::UnitCircleResponse>,
) -> Result<quest_qsvt::RouteResponse<V>> {
    // The explicitly chosen CLI route assigns Hermitian x or Gram y meaning to
    // this coefficient transfer. It is not a Laurent variable substitution.
    let (offset, length) = certificate.candidate().source_storage();
    let start =
        usize::try_from(offset).map_err(|_| Error::Input("negative compiled source support"))?;
    let end = start
        .checked_add(length)
        .ok_or(Error::Input("compiled source span overflow"))?;
    let coefficients = certificate
        .candidate()
        .target()
        .get(start..end)
        .ok_or(Error::Input("compiled route source span"))?
        .to_vec();
    let polynomial = quest_polynomial::Polynomial::new(
        quest_polynomial::Laurent::new(offset),
        coefficients,
        quest_polynomial::Limits::default(),
    )
    .map_err(quest_qsvt_io::Error::from)?;
    Ok(
        quest_qsvt::RouteTarget::<V>::from_unit_circle_coefficients(polynomial)?
            .bind_certified(certificate)?,
    )
}
#[cfg(feature = "certification")]
fn compiled_transform(
    builder: TransformBuilder<quest_qsvt::SuppliedEncoding>,
    route: TransformRoute,
    compiled: quest_qsvt_io::CompiledInput,
) -> Result<ValidatedTransform> {
    use quest_qsp::artifact::LoadedCertified;
    use quest_qsvt::{GramArgument, HermitianArgument};
    match compiled.into_certified() {
        LoadedCertified::RealParityWx(certificate) => {
            if !matches!(route, TransformRoute::Standard) {
                return Err(Error::Input(
                    "compiled Wx phases require the standard route",
                ));
            }
            let converted = certificate.certify_projector_phases(certificate.report().policy())?;
            Ok(builder.certified_standard(converted).build()?)
        }
        LoadedCertified::UnitCircleResponse(certificate) => Ok(match route {
            TransformRoute::Auto | TransformRoute::Standard => {
                return Err(Error::Input(
                    "compiled unit-circle controls require a generalized route",
                ));
            }
            TransformRoute::Direct => builder
                .direct(certified_response::<HermitianArgument>(certificate)?)
                .build()?,
            TransformRoute::HermitianizedFull => builder
                .hermitianized_full(certified_response::<HermitianArgument>(certificate)?)
                .build()?,
            TransformRoute::HermitianizedEven => builder
                .hermitianized_even(
                    certified_response::<HermitianArgument>(certificate)?.even_component(),
                )
                .build()?,
            TransformRoute::HermitianizedOdd => builder
                .hermitianized_odd(
                    certified_response::<HermitianArgument>(certificate)?.odd_component(),
                )
                .build()?,
            TransformRoute::MultiplicationEven => builder
                .multiplication_even(certified_response::<GramArgument>(certificate)?)
                .build()?,
            TransformRoute::MultiplicationOdd => builder
                .multiplication_odd(certified_response::<GramArgument>(certificate)?)
                .build()?,
        }),
    }
}
/// A deterministic convention route; admission failures never cause a route retry.
pub fn resolve_route(route: TransformRoute, input: &QspInput) -> TransformRoute {
    if !matches!(route, TransformRoute::Auto) {
        return route;
    }
    match input {
        QspInput::Symmetric(_) | QspInput::Laurent(_) => TransformRoute::Standard,
        #[cfg(feature = "certification")]
        QspInput::Compiled(compiled) => match compiled.certified() {
            quest_qsp::artifact::LoadedCertified::RealParityWx(_) => TransformRoute::Standard,
            quest_qsp::artifact::LoadedCertified::UnitCircleResponse(_) => {
                TransformRoute::HermitianizedFull
            }
        },
        _ => TransformRoute::HermitianizedFull,
    }
}
fn read_encoding(
    encoding_path: Option<&std::path::Path>,
    matrix_path: Option<&std::path::Path>,
    alpha: Option<f64>,
) -> Result<ProjectedEncoding> {
    match (encoding_path, matrix_path, alpha) {
        (Some(path), None, None) => {
            encoding(&hdf5::read_block_encoding(path, IoPolicy::default())?)
        }
        (None, Some(path), Some(alpha)) => {
            let matrix =
                hdf5::read_matrix(path, IoPolicy::default())?.into_dense(IoPolicy::default())?;
            Ok(
                DenseEncodingBuilder::new(matrix.as_ref(), NumericalPolicy::default())?
                    .normalization(alpha)?
                    .build()?,
            )
        }
        _ => Err(Error::Input(
            "supply an encoding or a matrix with explicit alpha",
        )),
    }
}
pub fn transform(
    source: ProjectedEncoding,
    route: TransformRoute,
    input: QspInput,
    idle: usize,
) -> Result<ValidatedTransform> {
    let route = resolve_route(route, &input);
    let auxiliary = !matches!(route, TransformRoute::Standard | TransformRoute::Direct);
    let layout = quest_qsvt::OperandLayout::canonical(source.num_qubits(), auxiliary)?
        .with_idle_high_qubits(idle)?;
    let builder = TransformBuilder::new().encoding(source).operands(layout);
    #[cfg(feature = "certification")]
    if let QspInput::Compiled(compiled) = input {
        return compiled_transform(builder, route, compiled);
    }
    match (route, input) {
        (TransformRoute::Standard, QspInput::Symmetric(phases)) => {
            Ok(builder.standard(phases).build()?)
        }
        (TransformRoute::Standard, QspInput::Laurent(phases)) => {
            Ok(builder.standard(phases).build()?)
        }
        (route, qsp @ (QspInput::GeneralizedAngles(_) | QspInput::GeneralizedMatrices(_))) => {
            let controls = match qsp {
                QspInput::GeneralizedAngles(angles) => angles.into_controls(),
                QspInput::GeneralizedMatrices(controls) => controls,
                _ => return Err(Error::Input("generalized controls missing")),
            };
            Ok(match route {
                TransformRoute::Auto | TransformRoute::Standard => {
                    return Err(Error::Input("standard route requires tagged Wx phases"));
                }
                TransformRoute::Direct => builder
                    .direct(
                        quest_qsvt::RouteResponse::<quest_qsvt::HermitianArgument>::imported(
                            controls,
                        ),
                    )
                    .build()?,
                TransformRoute::HermitianizedFull => {
                    builder
                        .hermitianized_full(quest_qsvt::RouteResponse::<
                            quest_qsvt::HermitianArgument,
                        >::imported(controls))
                        .build()?
                }
                TransformRoute::HermitianizedEven => builder
                    .hermitianized_even(
                        quest_qsvt::RouteResponse::<quest_qsvt::HermitianArgument>::imported(
                            controls,
                        )
                        .even_component(),
                    )
                    .build()?,
                TransformRoute::HermitianizedOdd => builder
                    .hermitianized_odd(
                        quest_qsvt::RouteResponse::<quest_qsvt::HermitianArgument>::imported(
                            controls,
                        )
                        .odd_component(),
                    )
                    .build()?,
                TransformRoute::MultiplicationEven => builder
                    .multiplication_even(
                        quest_qsvt::RouteResponse::<quest_qsvt::GramArgument>::imported(controls),
                    )
                    .build()?,
                TransformRoute::MultiplicationOdd => builder
                    .multiplication_odd(
                        quest_qsvt::RouteResponse::<quest_qsvt::GramArgument>::imported(controls),
                    )
                    .build()?,
            })
        }
        _ => Err(Error::Input(
            "route and imported convention disagree; synthesize polynomial input explicitly first",
        )),
    }
}
pub fn transform_report(transform: &ValidatedTransform) -> Value {
    #[cfg(feature = "certification")]
    let certified_projector = transform.projector_certificate().is_some();
    #[cfg(not(feature = "certification"))]
    let certified_projector = false;
    let response_evidence =
        transform
            .route_meaning()
            .map_or("none", |meaning| match meaning.evidence() {
                quest_qsvt::ResponseEvidence::Imported => "imported",
                quest_qsvt::ResponseEvidence::Candidate(_) => "candidate",
                #[cfg(feature = "certification")]
                quest_qsvt::ResponseEvidence::Certified(_) => "certified",
            });
    let counts = transform.query_counts();
    json!({"certified_projector_payload":certified_projector,"response_evidence":response_evidence,"route":format!("{:?}",transform.route()),"degree":transform.degree(),
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
    physical: Option<Vec<C>>,
    raw: Vec<C>,
    normalized: Option<Vec<C>>,
    mass: quest::qsvt::MassObservation,
}
fn execute(
    transform: ValidatedTransform,
    input: &[C],
    normalize: bool,
    physical_input: bool,
    physical_output: bool,
    context: &mut Context<'_>,
) -> Result<Executed> {
    let width = transform.operands().num_qubits();
    let input_dimension = if physical_input {
        1usize
            .checked_shl(u32::try_from(width).map_err(|_| Error::Budget("register width"))?)
            .ok_or(Error::Budget("register width"))?
    } else {
        transform.input().logical_dimension()
    };
    if input.len() != input_dimension {
        return Err(Error::Input(
            "input state dimension for selected logical/physical mode",
        ));
    }
    if !norm(input).is_finite() {
        return Err(Error::Input("input norm overflow"));
    }
    let amplitudes = context.measure("input_embedding", Stage::Construction, || {
        if physical_input {
            return vector(input.iter().copied(), input.len());
        }
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
        let physical = if physical_output {
            Some({
                let state = result.physical_snapshot()?;
                vector((0..state.nrows()).map(|r| state[(r, 0)]), state.nrows())?
            })
        } else {
            None
        };
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
            physical,
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
    if args.output_state.is_none() && args.physical_output_state.is_none() {
        return Err(Error::Input(
            "local embedded execution requires a logical or physical output path",
        ));
    }

    let (stored, input) = context.measure("read", Stage::Construction, || {
        Ok((
            read_encoding(args.encoding.as_deref(), args.matrix.as_deref(), args.alpha)?,
            hdf5::read_state_vector(&args.input_state, IoPolicy::default())?,
        ))
    })?;
    let (frozen, input_report) = crate::synthesis::freeze_input(&args.transform, context)?;
    let transform = context.measure("transform_construction", Stage::Construction, || {
        transform(stored, args.transform.route, frozen, 0)
    })?;
    let mut report = transform_report(&transform);
    crate::set(&mut report, "input_synthesis", input_report)?;
    let executed = execute(
        transform,
        &input,
        args.normalized_output_state.is_some(),
        args.physical_input,
        args.physical_output_state.is_some(),
        context,
    )?;
    crate::set(
        &mut report,
        "input_state_space",
        json!(if args.physical_input {
            "physical-register"
        } else {
            "logical"
        }),
    )?;
    crate::set(&mut report, "mass", mass_report(executed.mass))?;
    crate::set(
        &mut report,
        "logical_output_norm",
        json!(norm(&executed.raw)),
    )?;
    context.measure("write", Stage::Postselection, || {
        if let Some(path) = &args.output_state {
            hdf5::write_state_vector(path, &executed.raw, IoPolicy::default())?;
        }
        if let (Some(path), Some(values)) = (&args.physical_output_state, &executed.physical) {
            hdf5::write_state_vector(path, values, IoPolicy::default())?;
        }
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
            read_encoding(args.encoding.as_deref(), args.matrix.as_deref(), args.alpha)?,
            hdf5::read_state_vector(&args.input_state, IoPolicy::default())?,
            hdf5::read_state_vector(&args.reference_state, IoPolicy::default())?,
        ))
    })?;
    let (frozen, input_report) = crate::synthesis::freeze_input(&args.transform, context)?;
    let transform = context.measure("transform_construction", Stage::Construction, || {
        transform(stored, args.transform.route, frozen, 0)
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
    let rows = matrix.nrows();
    let cols = matrix.ncols();
    let n = rows.min(cols);
    if n == 0 {
        return Err(Error::Input("solve requires a nonempty matrix"));
    }
    let entry_scale = (0..rows)
        .flat_map(|r| (0..cols).map(move |c| matrix[(r, c)].re.abs().max(matrix[(r, c)].im.abs())))
        .fold(0.0_f64, f64::max);
    if !entry_scale.is_finite() || entry_scale <= 0.0 {
        return Err(Error::Input("solve requires numerical full rank"));
    }
    let mut normalized = self::matrix(rows, cols)?;
    for r in 0..rows {
        for c in 0..cols {
            normalized[(r, c)] = matrix[(r, c)].div(entry_scale);
        }
    }
    let req = svd_scratch::<C>(
        rows,
        cols,
        ComputeSvdVectors::No,
        ComputeSvdVectors::No,
        Par::Seq,
        faer::Spec::default(),
    );
    if rows
        .checked_next_multiple_of(4)
        .and_then(|r| r.checked_mul(cols))
        .and_then(|v| v.checked_add(n.checked_next_multiple_of(4)?))
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
        normalized.as_ref(),
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
    let dimension =
        f64::from(u32::try_from(rows.max(cols)).map_err(|_| Error::Budget("rank dimension"))?);
    let relative_threshold = maximum.mul(dimension.mul(f64::EPSILON));
    if !maximum.is_finite()
        || !minimum.is_finite()
        || maximum <= 0.0
        || minimum <= relative_threshold
    {
        return Err(Error::Input(
            "solve requires numerical full rank at sigma_max * dimension * binary64 epsilon",
        ));
    }
    let threshold = relative_threshold.mul(entry_scale);
    let maximum = maximum.mul(entry_scale);
    let minimum = minimum.mul(entry_scale);
    if !maximum.is_finite() || minimum <= 0.0 {
        return Err(Error::Input("singular values overflow or underflow"));
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
        TransformRoute::Auto
            | TransformRoute::Standard
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
    let input = read_solve(args, context)?;
    let (frozen, input_report) = crate::synthesis::freeze_input(&args.transform, context)?;
    finish_solve(args, context, input, frozen, input_report)
}
pub fn catalog_solve(args: &crate::CatalogSolveArgs, context: &mut Context<'_>) -> Result<Value> {
    let family = quest_qsvt_io::find_catalog_family(args.kappa, args.epsilon).ok_or(
        Error::Input("no exact catalogue family matches kappa and epsilon"),
    )?;
    let solve = SolveArgs {
        matrix: args.matrix.clone(),
        rhs: args.rhs.clone(),
        output_state: args.output_state.clone(),
        normalized_output_state: args.normalized_output_state.clone(),
        residual_tolerance: args.residual_tolerance,
        reciprocal_scale: family.reciprocal_scale(),
        transform: crate::TransformArgs {
            qsp: std::path::PathBuf::new(),
            synthesize_input: true,
            algorithm: args.algorithm,
            certify_input: args.certify,
            input_tolerance: args.tolerance,
            route: TransformRoute::Standard,
        },
    };
    let input = read_solve(&solve, context)?;
    let (frozen, report, _) =
        crate::synthesis::freeze_catalog(args, input.sigma_max, input.sigma_min, context)?;
    finish_solve(&solve, context, input, frozen, report)
}
fn finish_solve(
    args: &SolveArgs,
    context: &mut Context<'_>,
    input: SolveInput,
    frozen: QspInput,
    input_report: Value,
) -> Result<Value> {
    let SolveInput {
        a,
        b,
        b_norm,
        sigma_max,
        sigma_min,
        rank_threshold,
    } = input;
    let normalized_b = vector(b.iter().map(|z| z.div(b_norm)), b.len())?;
    let transform = context.measure("transform_construction", Stage::Construction, || {
        solve_transform(a.as_ref(), sigma_max, args, frozen)
    })?;
    let mut report = transform_report(&transform);
    crate::set(&mut report, "input_synthesis", input_report)?;
    let executed = execute(
        transform,
        &normalized_b,
        args.normalized_output_state.is_some(),
        false,
        false,
        context,
    )?;
    let scale = positive_ratio(b_norm, sigma_max, args.reciprocal_scale);
    if !scale.is_finite() || scale <= 0.0 {
        return Err(Error::Input(
            "physical solution rescaling overflow or underflow",
        ));
    }
    let physical = vector(
        executed.raw.iter().map(|z| z.mul(scale)),
        executed.raw.len(),
    )?;
    let (relative, residual) =
        context.measure("physical_residual", Stage::Postselection, || {
            physical_residual(a.as_ref(), &physical, &normalized_b, sigma_max, b_norm)
        })?;
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

fn physical_residual(
    a: MatRef<'_, C>,
    physical: &[C],
    normalized_b: &[C],
    sigma_max: f64,
    b_norm: f64,
) -> Result<(f64, f64)> {
    // Evaluate the actual rounded exported x in normalized coordinates. Exponent
    // arithmetic avoids overflow in the scale ratio and the physical products.
    let mut scaled_a = matrix(a.nrows(), a.ncols())?;
    for r in 0..a.nrows() {
        for c in 0..a.ncols() {
            scaled_a[(r, c)] = a[(r, c)].div(sigma_max);
        }
    }
    let scaled_physical = vector(
        physical.iter().map(|z| {
            C::new(
                signed_product_ratio(z.re, sigma_max, b_norm),
                signed_product_ratio(z.im, sigma_max, b_norm),
            )
        }),
        physical.len(),
    )?;
    let applied = product(scaled_a.as_ref(), &scaled_physical)?;
    let residual = vector(
        applied.iter().zip(normalized_b).map(|(a, b)| a.sub(*b)),
        normalized_b.len(),
    )?;
    let relative = norm(&residual);
    let absolute = relative.mul(b_norm);
    if !relative.is_finite() || !absolute.is_finite() || !norm(physical).is_finite() {
        return Err(Error::Input("physical solution or residual norm overflow"));
    }
    Ok((relative, absolute))
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
    let route = if matches!(args.transform.route, TransformRoute::Auto) {
        match resolve_route(TransformRoute::Auto, &frozen) {
            TransformRoute::Standard => TransformRoute::Standard,
            _ => TransformRoute::HermitianizedOdd,
        }
    } else {
        args.transform.route
    };
    let transform = transform(encoding, route, frozen, 0)?;
    if transform.input().logical_dimension() != a.nrows()
        || transform.output().logical_dimension() != a.ncols()
    {
        return Err(Error::Input(
            "selected route does not map the rectangular inverse logical spaces",
        ));
    }
    if matches!(route, TransformRoute::Standard) && transform.degree() % 2 == 0 {
        return Err(Error::Input(
            "reciprocal standard route requires an odd degree",
        ));
    }
    Ok(transform)
}

#[cfg(all(test, feature = "certification"))]
mod compiled_tests {
    use super::*;
    use quest_polynomial::Basis;
    fn certificate(
        offset: i32,
        coefficients: Vec<C>,
    ) -> googletest::Result<quest_qsp::certification::Certified<quest_qsp::UnitCircleResponse>>
    {
        let source = quest_polynomial::Polynomial::new(
            quest_polynomial::Laurent::new(offset),
            coefficients,
            quest_polynomial::Limits::default(),
        )?;
        let candidate = quest_qsp::SynthesisBuilder::new()
            .unit_circle_response(&source)?
            .admit()?
            .complete()?
            .synthesize()?;
        Ok(quest_qsp::certification::CertificationBuilder::new()
            .candidate(candidate)
            .policy(quest_qsp::certification::CertificationPolicy::default())?
            .certify()?)
    }
    #[test]
    fn route_binding_preserves_empty_source_identity_and_absolute_support() -> googletest::Result<()>
    {
        let empty = certified_response::<quest_qsvt::HermitianArgument>(certificate(0, vec![])?)?;
        googletest::verify_eq!(
            empty
                .meaning()
                .unit_circle_source()
                .ok_or_else(|| std::io::Error::other("missing source"))?
                .coefficients(),
            []
        )?;
        let zero = certified_response::<quest_qsvt::GramArgument>(certificate(
            0,
            vec![C::new(0.0, 0.0)],
        )?)?;
        googletest::verify_eq!(
            zero.meaning()
                .target()
                .ok_or_else(|| std::io::Error::other("missing target"))?
                .coefficients()
                .len(),
            1
        )?;
        let offset = certified_response::<quest_qsvt::HermitianArgument>(certificate(
            2,
            vec![C::new(0.2, 0.0)],
        )?)?;
        googletest::verify_eq!(
            offset
                .meaning()
                .target()
                .ok_or_else(|| std::io::Error::other("missing target"))?
                .coefficients(),
            &[C::new(0.0, 0.0), C::new(0.0, 0.0), C::new(0.2, 0.0)]
        )?;
        googletest::verify_eq!(
            offset
                .meaning()
                .unit_circle_source()
                .ok_or_else(|| std::io::Error::other("missing source"))?
                .basis()
                .offset(),
            2
        )?;

        Ok(())
    }
}

#[cfg(test)]
mod numerical_regressions {
    use super::*;
    #[test]
    fn rank_admission_preserves_large_and_small_full_rank_matrices() -> googletest::Result<()> {
        for scale in [1e308, 1e-308] {
            let a = Mat::from_fn(2, 2, |r, c| C::new(if r == c { scale } else { 0.0 }, 0.0));
            let (maximum, minimum, _) = singular_values(a.as_ref())?;
            googletest::verify_eq!(maximum, scale)?;
            googletest::verify_eq!(minimum, scale)?;
        }
        Ok(())
    }
    #[test]
    fn rectangular_full_rank_is_admitted_but_rectangular_rank_deficiency_is_rejected()
    -> googletest::Result<()> {
        for (rows, cols) in [(3, 2), (2, 3)] {
            let a = Mat::from_fn(rows, cols, |r, c| C::new(f64::from(r == c), 0.0));
            let _ = singular_values(a.as_ref())?;
            let a = Mat::from_fn(rows, cols, |r, c| C::new(f64::from(r == 0 && c == 0), 0.0));
            googletest::verify_true!(singular_values(a.as_ref()).is_err())?;
        }
        Ok(())
    }
}

// Exact exponent decomposition avoids intermediate overflow/underflow in ratios.
fn positive_parts(x: f64) -> (f64, i32) {
    if x < f64::MIN_POSITIVE {
        let (mantissa, exponent) = positive_parts(x.mul(4_503_599_627_370_496.0));
        return (mantissa, exponent.saturating_sub(52));
    }
    let bits = x.to_bits();
    let word = bits.to_be_bytes();
    let exponent = u16::from_be_bytes([word[0], word[1]]) >> 4;
    (
        f64::from_bits((bits & ((1u64 << 52) - 1)) | (1023u64 << 52)),
        i32::from(exponent).saturating_sub(1023),
    )
}
fn apply_exponent(mut value: f64, mut exponent: i32) -> f64 {
    while exponent > 512 {
        value = value.mul(2.0_f64.powi(512));
        exponent = exponent.saturating_sub(512);
    }
    while exponent < -512 {
        value = value.mul(2.0_f64.powi(-512));
        exponent = exponent.saturating_add(512);
    }
    value.mul(2.0_f64.powi(exponent))
}
fn positive_ratio(a: f64, b: f64, c: f64) -> f64 {
    let (am, ae) = positive_parts(a);
    let (bm, be) = positive_parts(b);
    let (cm, ce) = positive_parts(c);
    apply_exponent(am.div(bm).div(cm), ae.saturating_sub(be).saturating_sub(ce))
}

#[cfg(test)]
mod scale_regressions {
    use super::*;
    #[test]
    fn physical_scale_has_no_intermediate_underflow_or_overflow() -> googletest::Result<()> {
        googletest::verify_that!(
            positive_ratio(1e-300, 1e100, 1e-200),
            googletest::matchers::near(1e-200, 1e-215)
        )?;
        googletest::verify_that!(
            positive_ratio(1e300, 1e-100, 1e200),
            googletest::matchers::near(1e200, 1e185)
        )?;
        googletest::verify_eq!(
            positive_ratio(f64::from_bits(1), f64::from_bits(1), 1.0),
            1.0
        )?;
        Ok(())
    }
}

fn signed_product_ratio(value: f64, numerator: f64, denominator: f64) -> f64 {
    if value == 0.0 {
        return value;
    }
    let (vm, ve) = positive_parts(value.abs());
    let (nm, ne) = positive_parts(numerator);
    let (dm, de) = positive_parts(denominator);
    apply_exponent(vm.mul(nm).div(dm), ve.saturating_add(ne).saturating_sub(de)).copysign(value)
}
