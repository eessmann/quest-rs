//! Root-only cold IO/construction, followed by an exact bounded frozen payload
//! broadcast and matched collective native execution. No distributed state gather.
mod wire;
use crate::{
    Context, EmbeddedArgs, Error, OverlapArgs, Result, Stage, TransformArgs, TransformRoute,
};
use num_complex::Complex64 as C;
use quest::{
    QubitCount,
    collective::{CollectiveEnvironment, MpiCommunicator, MpiRuntime},
};
use quest_qsvt::{NumericalPolicy, ValidatedTransform};
use quest_qsvt_io::{
    IoPolicy, QspInput,
    hdf5::{self, StoredBlockEncoding},
};
use serde_json::{Value, json};
use std::path::Path;

macro_rules! mpi {
    ($expression:expr) => {
        $expression.map_err(|source| {
            Error::Quest(quest::Error::Backend {
                operation: "coordinating application MPI",
                source,
            })
        })
    };
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Workflow {
    Embedded,
    Overlap,
}
struct FrozenInput {
    workflow: Workflow,
    route: TransformRoute,
    verification_tolerance: f64,
    block: StoredBlockEncoding,
    qsp: QspInput,
    input: Vec<C>,
    reference: Vec<C>,
    evidence: Value,
}
#[derive(Clone, Copy)]
struct Request<'a> {
    workflow: Workflow,
    encoding: &'a Path,
    input: &'a Path,
    reference: Option<&'a Path>,
    transform: &'a TransformArgs,
}
fn agree<T>(comm: &mut MpiCommunicator<'_>, result: Result<T>) -> Result<T> {
    if mpi!(comm.all_agree(result.is_ok()))? {
        result
    } else {
        result.and(Err(Error::Input(
            "another rank rejected distributed application admission",
        )))
    }
}

// A live admitted runtime must exist before the factory can create any worker.
// Factory failure is agreed on every rank before cold input or native entry.
fn root_resources<T>(
    runtime: &MpiRuntime,
    comm: &mut MpiCommunicator<'_>,
    factory: impl FnOnce() -> Result<T>,
) -> Result<Option<T>> {
    let supported = runtime.thread_multiple();
    agree(
        comm,
        if supported {
            Ok(())
        } else {
            Err(Error::Input(
                "MPI_THREAD_MULTIPLE is required before workers",
            ))
        },
    )?;
    let result = if mpi!(comm.rank())? == 0 {
        factory().map(Some)
    } else {
        Ok(None)
    };
    agree(comm, result)
}

pub fn application(cli: crate::Cli, start: std::time::Instant) -> Result<Value> {
    let (result, teardown_started) = {
        let mpi_started = std::time::Instant::now();
        let runtime = mpi!(MpiRuntime::initialize())?;
        let mut comm = mpi!(runtime.world())?;
        let mpi_seconds = mpi_started.elapsed().as_secs_f64();
        let pool_started = std::time::Instant::now();
        let pool = root_resources(&runtime, &mut comm, || {
            crate::workers::Pool::build(cli.workers, &cli.command)
        })?
        .unwrap_or_else(crate::workers::Pool::serial);
        let pool_seconds = pool_started.elapsed().as_secs_f64();
        // Declaration order keeps the owned pool inside the communicator/runtime scope.
        let result = cli.run_scoped(&pool, |command, context| {
            context.output_rank = mpi!(comm.rank())? == 0;
            context
                .timings
                .insert("mpi_environment".into(), json!(mpi_seconds));
            context
                .timings
                .insert("worker_pool".into(), json!(pool_seconds));
            match command {
                crate::Command::Embedded(args) => embedded(&args, context, &mut comm),
                crate::Command::Overlap(args) => overlap(&args, context, &mut comm),
                _ => Err(Error::Input("invalid distributed application")),
            }
        });
        (result, std::time::Instant::now())
    };
    crate::finalize_timing(result, start, teardown_started, "mpi_and_worker_teardown")
}

fn read(request: &Request<'_>, context: &mut Context<'_>) -> Result<FrozenInput> {
    let (block, input, reference) = context.measure("root_read", Stage::Construction, || {
        Ok((
            hdf5::read_block_encoding(request.encoding, IoPolicy::default())?,
            hdf5::read_state_vector(request.input, IoPolicy::default())?,
            request
                .reference
                .map(|path| hdf5::read_state_vector(path, IoPolicy::default()))
                .transpose()?
                .unwrap_or_default(),
        ))
    })?;
    let (qsp, evidence) = crate::synthesis::freeze_input(request.transform, context)?;
    Ok(FrozenInput {
        verification_tolerance: request.transform.input_tolerance,
        workflow: request.workflow,
        route: crate::execution::resolve_route(request.transform.route, &qsp),
        block,
        qsp,
        input,
        reference,
        evidence,
    })
}
fn broadcast(comm: &mut MpiCommunicator<'_>, root: Option<Vec<u8>>) -> Result<Vec<u8>> {
    let rank = mpi!(comm.rank())?;
    let mut bytes = root.unwrap_or_default();
    let mut length = u64::try_from(bytes.len())
        .map_err(|_| Error::Budget("wire length"))?
        .to_le_bytes();
    mpi!(comm.broadcast_bytes(0, &mut length))?;
    let length = agree(
        comm,
        usize::try_from(u64::from_le_bytes(length)).map_err(|_| Error::Budget("wire length")),
    )?;
    let allocation = (|| {
        if length > IoPolicy::default().max_bytes {
            return Err(Error::Budget("wire length"));
        }
        if rank != 0 {
            bytes
                .try_reserve_exact(length)
                .map_err(|_| Error::Budget("broadcast allocation"))?;
            bytes.resize(length, 0);
        }
        Ok(())
    })();
    agree(comm, allocation)?;
    for chunk in bytes.chunks_mut(1_048_576) {
        mpi!(comm.broadcast_bytes(0, chunk))?;
    }
    Ok(bytes)
}
fn input(
    comm: &mut MpiCommunicator<'_>,
    request: &Request<'_>,
    context: &mut Context<'_>,
) -> Result<FrozenInput> {
    let root = if mpi!(comm.rank())? == 0 {
        read(request, context)
            .and_then(|input| wire::encode(&input))
            .map(Some)
    } else {
        Ok(None)
    };
    let root = agree(comm, root)?;
    let bytes = context.measure("frozen_broadcast", Stage::Construction, || {
        broadcast(comm, root)
    })?;
    let decoded = agree(comm, wire::decode(&bytes))?;
    agree(
        comm,
        if decoded.workflow == request.workflow {
            Ok(decoded)
        } else {
            Err(Error::Input(
                "MPI ranks selected different application workflows",
            ))
        },
    )
}
fn construct(
    comm: &mut MpiCommunicator<'_>,
    input: FrozenInput,
    context: &mut Context<'_>,
) -> Result<(ValidatedTransform, Vec<C>, Vec<C>, Value)> {
    let size = usize::try_from(mpi!(comm.size())?).map_err(|_| Error::Input("MPI size"))?;
    let idle = if size.is_power_of_two() {
        usize::try_from(size.ilog2()).map_err(|_| Error::Budget("MPI width"))
    } else {
        Err(Error::Input("MPI size must be a power of two"))
    };
    let idle = agree(comm, idle)?;
    let result = context.measure("transform_construction", Stage::Construction, || {
        crate::execution::transform(
            crate::execution::encoding(&input.block)?,
            input.route,
            input.qsp,
            if input.workflow == Workflow::Embedded {
                idle
            } else {
                0
            },
        )
    });
    let transform = agree(comm, result)?;
    agree(
        comm,
        if transform.input().logical_dimension() == input.input.len()
            && (input.workflow != Workflow::Overlap
                || transform.output().logical_dimension() == input.reference.len())
        {
            Ok(())
        } else {
            Err(Error::Input(
                "distributed logical input/reference dimension",
            ))
        },
    )?;
    Ok((transform, input.input, input.reference, input.evidence))
}
fn embedded(
    args: &EmbeddedArgs,
    context: &mut Context<'_>,
    comm: &mut MpiCommunicator<'_>,
) -> Result<Value> {
    if args.output_state.is_some()
        || args.normalized_output_state.is_some()
        || args.physical_output_state.is_some()
        || args.physical_input
        || args.matrix.is_some()
        || args.alpha.is_some()
    {
        return Err(Error::Input("distributed full-state output is unsupported"));
    }
    run(
        Request {
            workflow: Workflow::Embedded,
            encoding: args.encoding.as_deref().ok_or(Error::Input(
                "distributed execution requires an imported encoding",
            ))?,
            input: &args.input_state,
            reference: None,
            transform: &args.transform,
        },
        context,
        comm,
    )
}
fn overlap(
    args: &OverlapArgs,
    context: &mut Context<'_>,
    comm: &mut MpiCommunicator<'_>,
) -> Result<Value> {
    if args.matrix.is_some() || args.alpha.is_some() {
        return Err(Error::Input(
            "distributed execution requires an imported encoding",
        ));
    }
    run(
        Request {
            workflow: Workflow::Overlap,
            encoding: args.encoding.as_deref().ok_or(Error::Input(
                "distributed execution requires an imported encoding",
            ))?,
            input: &args.input_state,
            reference: Some(&args.reference_state),
            transform: &args.transform,
        },
        context,
        comm,
    )
}
fn run(
    request: Request<'_>,
    context: &mut Context<'_>,
    comm: &mut MpiCommunicator<'_>,
) -> Result<Value> {
    let rank = mpi!(comm.rank())?;
    let size = mpi!(comm.size())?;
    context.output_rank = rank == 0;
    let input = input(comm, &request, context)?;
    let (transform, input, reference, evidence) = construct(comm, input, context)?;
    let mut report = crate::execution::transform_report(&transform);
    crate::set(&mut report, "input_synthesis", evidence)?;
    crate::set(
        &mut report,
        "distributed",
        json!({"rank":rank,"ranks":size,"root_io_and_synthesis":true,"full_state_gather":false}),
    )?;
    match request.workflow {
        Workflow::Embedded => embedded_run(comm, transform, &input, &mut report, context)?,
        Workflow::Overlap => overlap_run(comm, transform, input, reference, &mut report, context)?,
    }
    Ok(report)
}
fn embedded_run(
    comm: &mut MpiCommunicator<'_>,
    transform: ValidatedTransform,
    input: &[C],
    report: &mut Value,
    context: &mut Context<'_>,
) -> Result<()> {
    let width = transform.operands().num_qubits();
    let amplitudes = if mpi!(comm.rank())? == 0 {
        context.measure("root_input_embedding", Stage::Construction, || {
            let basis = transform
                .input()
                .materialize_isometry(width, NumericalPolicy::default())?;
            crate::execution::product(basis.as_ref(), input).map(Some)
        })
    } else {
        Ok(None)
    };
    let amplitudes = agree(comm, amplitudes)?;
    let count = agree(comm, QubitCount::new(width).map_err(Error::from))?;
    let environment = context.measure("environment", Stage::Preparation, || {
        Ok(CollectiveEnvironment::builder(comm)?.build()?)
    })?;
    let (mut prepared, mut register) = context.measure(
        "collective_admission_and_preparation",
        Stage::Preparation,
        || {
            let prepared = environment.qsvt().transform(transform).prepare()?;
            let mut register = environment.state_vector(count)?;
            register.init_pure_from_root(0, amplitudes.as_deref())?;
            Ok((prepared, register))
        },
    )?;
    let result = context.measure("execution", Stage::Execution, || {
        Ok(prepared.run(&mut register)?)
    })?;
    crate::set(report, "mass", crate::execution::mass_report(result.mass()))?;
    let _ = result.release();
    Ok(())
}
fn overlap_run(
    comm: &MpiCommunicator<'_>,
    transform: ValidatedTransform,
    input: Vec<C>,
    reference: Vec<C>,
    report: &mut Value,
    context: &mut Context<'_>,
) -> Result<()> {
    let environment = context.measure("environment", Stage::Preparation, || {
        Ok(CollectiveEnvironment::builder(comm)?.build()?)
    })?;
    let mut prepared = context.measure(
        "collective_admission_and_preparation",
        Stage::Preparation,
        || {
            Ok(environment
                .qsvt()
                .transform(transform)
                .overlap()
                .input(input)
                .reference(reference)
                .prepare()?)
        },
    )?;
    let observed = context.measure("execution", Stage::Execution, || Ok(prepared.run()?))?;
    crate::set(
        report,
        "overlap",
        json!([observed.overlap().re, observed.overlap().im]),
    )?;
    crate::set(
        report,
        "normalized_overlap",
        json!(observed.normalized_overlap().map(|z| [z.re, z.im])),
    )?;
    crate::set(
        report,
        "mass",
        json!({"retained":observed.retained_mass(),"active":observed.active_mass(),"real_zero":observed.real_zero_mass(),"imaginary_zero":observed.imaginary_zero_mass(),"transformed_norm":observed.transformed_norm(),"native_dispatches":crate::execution::native_dispatch_report(observed.native_dispatches())}),
    )?;
    Ok(())
}

#[cfg(all(test, feature = "rayon"))]
mod tests {
    use super::*;
    use googletest::prelude::*;

    #[gtest]
    fn mpi_admission_precedes_root_worker_spawns() -> googletest::Result<()> {
        const NAME: &str = "distributed::tests::mpi_admission_precedes_root_worker_spawns";
        if std::env::var("QUEST_WORKER_ORDER_TEST").as_deref() != Ok(NAME) {
            let output = std::process::Command::new("timeout")
                .args(["60s", "mpiexec", "-n", "2"])
                .arg(std::env::current_exe()?)
                .args(["--exact", NAME, "--nocapture", "--test-threads=1"])
                .env("QUEST_WORKER_ORDER_TEST", NAME)
                .output()?;
            expect_true!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            return Ok(());
        }
        let runtime = mpi!(MpiRuntime::initialize())?;
        let mut comm = mpi!(runtime.world())?;
        let mut spawned = 0_usize;
        let pool = root_resources(&runtime, &mut comm, || {
            Ok(rayon::ThreadPoolBuilder::new()
                .num_threads(2)
                .spawn_handler(|thread| {
                    // Runs at actual OS worker creation, before the thread starts.
                    if !runtime.thread_multiple() {
                        return Err(std::io::Error::other("workers preceded MPI admission"));
                    }
                    spawned = spawned.saturating_add(1);
                    std::thread::Builder::new().spawn(|| thread.run())?;
                    Ok(())
                })
                .build()?)
        })?;
        if mpi!(comm.rank())? == 0 {
            expect_that!(spawned, eq(2));
            expect_that!(
                pool.as_ref().map(rayon::ThreadPool::current_num_threads),
                some(eq(2))
            );
        } else {
            expect_that!(spawned, eq(0));
            expect_true!(pool.is_none());
        }
        drop(pool);
        // An ordinary invalid worker scope fails only on root, then the same
        // admission agreement releases every rank without entering native work.
        let rejected = root_resources(&runtime, &mut comm, || {
            crate::workers::Pool::build(
                crate::Workers::Count(
                    std::num::NonZeroUsize::new(2).ok_or(Error::Input("test count"))?,
                ),
                &crate::Command::Catalog {
                    command: crate::CatalogCommand::List(crate::FamilySelection::default()),
                },
            )
        });
        expect_true!(rejected.is_err());
        expect_true!(mpi!(comm.all_agree(true))?);
        expect_true!(mpi!(runtime.is_active())?);
        Ok(())
    }
}
