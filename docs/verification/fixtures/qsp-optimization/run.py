#!/usr/bin/env python3
"""Build immutable QSP variants, then measure all variants without concurrent builds."""
import argparse, hashlib, json, os, pathlib, shutil, signal, subprocess, time, tomllib
import tomli_w


def render_manifest(source):
    dependencies = {"serde_json": "1"}
    for name in ["quest-polynomial", "quest-qsp"]:
        dependency = {"path": str(source / "crates" / name)}
        if name == "quest-qsp":
            dependency["features"] = ["offline-synthesis"]
        dependencies[name] = dependency
    return tomli_w.dumps({
        "package": {"name": "qsp-optimization-observer", "version": "0.0.0", "edition": "2024"},
        "workspace": {}, "dependencies": dependencies, "profile": {"release": {"lto": "thin"}},
    })


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def inventory(root):
    return {str(p.relative_to(root)): sha(p) for p in sorted(root.rglob('*'))
            if p.is_file() and p.suffix in ('.rs', '.toml', '.lock')}


def cargo_configs(source, env):
    """Record candidate Cargo config files, including target-specific flags."""
    home=pathlib.Path(env.get('CARGO_HOME',str(pathlib.Path.home()/'.cargo'))).resolve()
    directories=[home, *[p/'.cargo' for p in reversed([source,*source.parents])]]
    records=[]
    seen=set()
    for directory in directories:
        for name in ['config','config.toml']:
            path=directory/name
            if path in seen or not path.is_file():
                continue
            seen.add(path)
            config=tomllib.loads(path.read_text())
            records.append({'path':str(path),'sha256':sha(path),
                            'tables':{k:config[k] for k in ['build','target','unstable'] if k in config}})
    return records


def stop_process_group(process):
    # time(1) is the parent of the observer: kill the entire group on failure.
    try:
        os.killpg(process.pid, signal.SIGTERM)
    except ProcessLookupError:
        return
    try:
        process.wait(timeout=5)
    except subprocess.TimeoutExpired:
        pass
    # The leader can exit before its children. Escalate for the group even
    # when wait() succeeded; a missing group means all members already exited.
    try:
        os.killpg(process.pid, signal.SIGKILL)
    except ProcessLookupError:
        pass
    process.wait()


def record_run(command, cwd, env, prefix, timeout=None):
    if prefix.with_suffix('.json').exists():
        raise RuntimeError(f'receipt already exists: {prefix}')
    receipt = {'command': list(map(str, command)), 'cwd': str(cwd), 'started': time.time(),
               'environment': {key: env[key] for key in ['PATH', 'CARGO_BUILD_JOBS', 'CARGO_TARGET_DIR', 'RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS', 'OMP_NUM_THREADS'] if key in env}}
    with prefix.with_suffix('.stdout').open('w') as out, prefix.with_suffix('.stderr').open('w') as err:
        try:
            process = subprocess.Popen(command, cwd=cwd, env=env, stdout=out, stderr=err, start_new_session=True)
            receipt['exit_code'] = process.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            receipt['status'] = 'timeout'
            stop_process_group(process)
        except KeyboardInterrupt:
            receipt['status'] = 'interrupted'
            stop_process_group(process)
            raise
        finally:
            receipt['elapsed_seconds'] = time.time()-receipt['started']
            prefix.with_suffix('.json').write_text(json.dumps(receipt, indent=2)+'\n')
    return receipt.get('exit_code') == 0


def instrument_source(source):
    # Benchmark-only instrumentation; never modifies the production checkout.
    # The offline intermediate stage is private, so bracket its one completion
    # call identically in all variants. Timings include this small hook equally.
    path = source/'crates/quest-qsp/src/offline/mod.rs'
    text = path.read_text()
    anchor = 'kernels::completion('
    assert text.count(anchor) == 1
    start = text.index(anchor)
    line = text.rfind('\n', 0, start)+1
    end = text.index(';', start)+1
    text = text[:line] + ('            let benchmark_phase = BenchmarkCompletionPhase::begin();\n'
           '            let benchmark_started = Instant::now();\n'
           '            let benchmark_work = context.work;\n') + text[line:end] + (
           '\n            BENCH_COMPLETION_NS.store(benchmark_started.elapsed().as_nanos() as u64, std::sync::atomic::Ordering::Relaxed);'
           '\n            BENCH_COMPLETION_WORK.store(context.work - benchmark_work, std::sync::atomic::Ordering::Relaxed);'
           '\n            drop(benchmark_phase);') + text[end:]
    text += '''
static BENCH_COMPLETION_NS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static BENCH_COMPLETION_WORK: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
static BENCH_COMPLETION_HOOKS: std::sync::OnceLock<(fn(), fn())> = std::sync::OnceLock::new();
/// Benchmark-only callbacks; registered once by the sequential observer.
pub fn benchmark_register_completion_hooks(begin: fn(), end: fn()) {
    assert!(BENCH_COMPLETION_HOOKS.set((begin, end)).is_ok());
}
struct BenchmarkCompletionPhase;
impl BenchmarkCompletionPhase {
    fn begin() -> Self {
        if let Some((begin, _)) = BENCH_COMPLETION_HOOKS.get() { begin(); }
        Self
    }
}
impl Drop for BenchmarkCompletionPhase {
    fn drop(&mut self) {
        if let Some((_, end)) = BENCH_COMPLETION_HOOKS.get() { end(); }
    }
}
/// Benchmark-only hook injected by the verification fixture into immutable copies.
pub fn benchmark_completion_observation() -> (u64, usize) {
    (BENCH_COMPLETION_NS.load(std::sync::atomic::Ordering::Relaxed),
     BENCH_COMPLETION_WORK.load(std::sync::atomic::Ordering::Relaxed))
}
'''
    path.write_text(text)
    path = source/'crates/quest-qsp/src/kernel.rs'
    text = path.read_text()
    anchor = 'if residual <= policy.response_tolerance / 8.0 {'
    assert text.count(anchor) == 1
    text = text.replace(anchor, anchor+'\n            BENCH_COMPLETION_WORK.store(policy.limits.max_work - remaining.limits.max_work, std::sync::atomic::Ordering::Relaxed);')
    anchor = 'Ok((gamma, work.work_used))'
    assert text.count(anchor) == 1
    text = text.replace(anchor, 'BENCH_INVERSE_WORK.store(work.work_used, std::sync::atomic::Ordering::Relaxed);\n    '+anchor)
    anchor = 'let coefficients = product_tree(controls, &mut Convolutions::new(policy, execution))?;'
    assert text.count(anchor) == 1
    text = text.replace(anchor, '''let mut benchmark_work = Convolutions::new(policy, execution);
    let coefficients = product_tree(controls, &mut benchmark_work)?;
    BENCH_RESPONSE_WORK.store(benchmark_work.work_used, std::sync::atomic::Ordering::Relaxed);
    drop(benchmark_work);''')
    text += '''
static BENCH_COMPLETION_WORK: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
static BENCH_INVERSE_WORK: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
static BENCH_RESPONSE_WORK: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
/// Benchmark-only completion work observation.
pub fn benchmark_completion_work() -> usize {
    BENCH_COMPLETION_WORK.load(std::sync::atomic::Ordering::Relaxed)
}
/// Charged inverse and independent response-reconstruction convolution work.
pub fn benchmark_synthesis_work() -> usize {
    BENCH_INVERSE_WORK.load(std::sync::atomic::Ordering::Relaxed)
        .checked_add(BENCH_RESPONSE_WORK.load(std::sync::atomic::Ordering::Relaxed))
        .expect("benchmark work count fits usize")
}
'''
    path.write_text(text)
    path=source/'crates/quest-qsp/src/lib.rs'
    path.write_text(path.read_text()+'\n#[doc(hidden)]\npub use kernel::{benchmark_completion_work, benchmark_synthesis_work};\n')


def identity(folder):
    return {'source': inventory(folder/'source'), 'package': inventory(folder/'package'),
            'binary': sha(folder/'observer')}


def validate_identity(folder):
    expected = json.loads((folder/'build-identity.json').read_text())
    sources = json.loads((folder/'sources.json').read_text())
    binary = json.loads((folder/'binary.json').read_text())
    current = identity(folder)
    if (current != expected or current['source'] != sources['instrumented']
            or current['binary'] != binary['sha256']
            or (folder/'observer').stat().st_size != binary['bytes']):
        raise ValueError(f'build identity mismatch: {folder.name}')
    return current


def build(args):
    output = args.output.resolve()
    folder = output/args.label
    folder.mkdir(parents=True, exist_ok=False)
    source = folder/'source'
    shutil.copytree(args.source.resolve(), source, ignore=shutil.ignore_patterns('.git', 'target', '.devenv*', '.superpowers', 'result*'))
    original = inventory(source)
    instrument_source(source)
    package=folder/'package'
    (package/'src').mkdir(parents=True)
    fixture=pathlib.Path(__file__).resolve().parent
    for name in ['main.rs','allocator.rs']:
        shutil.copyfile(fixture/name, package/'src'/name)
    (package/'Cargo.toml').write_text(render_manifest(source))
    shutil.copyfile(source/'Cargo.lock',package/'Cargo.lock')
    # Never share incremental Cargo state across immutable variants: path and
    # mtime-preserving copies can otherwise resolve stale workspace artifacts.
    env=dict(os.environ,CARGO_BUILD_JOBS='2',CARGO_TARGET_DIR=str(folder/'build'))
    (folder/'cargo-config.json').write_text(json.dumps({
        'cwd':str(source),'config_files':cargo_configs(source,env),
        'environment':{k:v for k,v in env.items() if k in [
            'CARGO_HOME','RUSTUP_HOME','RUSTUP_TOOLCHAIN','RUSTFLAGS','CARGO_ENCODED_RUSTFLAGS',
            'CARGO_BUILD_TARGET','CARGO_BUILD_RUSTFLAGS','RUSTC','RUSTC_WRAPPER','RUSTC_WORKSPACE_WRAPPER']
            or (k.startswith('CARGO_TARGET_') and k.endswith(('_RUSTFLAGS','_LINKER','_RUNNER')))}
    },indent=2)+'\n')
    with (folder/'toolchain.txt').open('w') as stream:
        subprocess.run(['rustc', '-Vv'], cwd=source, env=env, stdout=stream, check=True)
        subprocess.run(['cargo', '-V'], cwd=source, env=env, stdout=stream, check=True)
    (folder/'sources.json').write_text(json.dumps({'original':original,'instrumented':inventory(source),'fixture':{p.name:sha(p) for p in fixture.iterdir() if p.is_file()}},indent=2)+'\n')
    ok=record_run(['cargo','build','--release','--offline','--manifest-path',str(package/'Cargo.toml')],source,env,folder/'build')
    if not ok: raise SystemExit(f'build failed: {folder}')
    binary=folder/'observer'
    shutil.copyfile(folder/'build/release/qsp-optimization-observer',binary)
    binary.chmod(0o755)
    (folder/'binary.json').write_text(json.dumps({'sha256':sha(binary),'bytes':binary.stat().st_size},indent=2)+'\n')
    (folder/'build-identity.json').write_text(json.dumps(identity(folder),indent=2)+'\n')
    validate_identity(folder)
    print(binary)


EXPECTED_WORKLOADS = {('binary64', 256, 53), ('binary64', 1024, 53),
                      ('offline', 16, 128), ('offline', 16, 256),
                      ('offline', 256, 128), ('offline', 256, 256)}


def validate_records(path, reference):
    observed = {}
    for line in path.read_text().splitlines():
        row = json.loads(line)
        key = (row['kind'], row['degree'], row['bits'])
        if key not in EXPECTED_WORKLOADS or key in observed:
            raise ValueError(f'unexpected/duplicate workload {key}: {path}')
        if row.get('status') != 'ok':
            raise ValueError(f'failed workload {key}: {path}')
        value = (row['export_fingerprint'], row['grid'])
        if (not isinstance(value[0], str) or len(value[0]) != 16
                or any(c not in '0123456789abcdef' for c in value[0])
                or type(value[1]) is not int or value[1] <= 0):
            raise ValueError(f'invalid fingerprint/grid {key}: {path}')
        observed[key] = value
    if observed.keys() != EXPECTED_WORKLOADS:
        raise ValueError(f'missing workloads: {path}')
    if reference and observed != reference:
        raise ValueError(f'fingerprint/grid mismatch: {path}')
    reference.update(observed)
    return len(observed)


def run(args):
    output=args.output.resolve()
    completion_path=output/'measurement-completion.json'
    if completion_path.exists():
        raise SystemExit(f'completion receipt already exists: {completion_path}')
    env=dict(os.environ)
    folders=[output/label for label in args.labels]
    complete={'all_trials_succeeded':False,'sources_unchanged':False,
              'workloads_valid':False,'validated_workload_records':0,
              'labels':args.labels,'trials':3,'errors':[]}
    reference={}
    before={}
    all_ok=True
    try:
        if not folders or len(set(args.labels)) != len(folders):
            raise ValueError('labels must be nonempty and unique')
        # Validate *all* variants against saved build identities before any run.
        before={p.name:validate_identity(p) for p in folders}
        for trial in range(1,4):
            for folder in (folders if trial%2 else list(reversed(folders))):
                print(f'trial {trial}: {folder.name}',flush=True)
                prefix=folder/f'trial-{trial}'
                ok=record_run(['/usr/bin/time','-v',str(folder/'observer')],folder,env,prefix,timeout=3600)
                all_ok=all_ok and ok
                try:
                    complete['validated_workload_records']+=validate_records(prefix.with_suffix('.stdout'),reference)
                except (OSError, ValueError, KeyError, TypeError) as error:
                    complete['errors'].append(str(error))
        complete['all_trials_succeeded']=all_ok
        complete['workloads_valid']=(not complete['errors'] and
            complete['validated_workload_records']==3*len(folders)*len(EXPECTED_WORKLOADS))
        complete['sources_unchanged']=all(before[p.name]==validate_identity(p) for p in folders)
    except (OSError, ValueError, KeyError, TypeError, RuntimeError, KeyboardInterrupt) as error:
        complete['errors'].append(str(error) or type(error).__name__)
    completion_path.write_text(json.dumps(complete,indent=2)+'\n')
    print(json.dumps(complete),flush=True)
    if not all(complete[k] for k in ['all_trials_succeeded','sources_unchanged','workloads_valid']):
        raise SystemExit(1)


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    subs=parser.add_subparsers(dest='action',required=True)
    b=subs.add_parser('build'); b.add_argument('label'); b.add_argument('source',type=pathlib.Path); b.add_argument('output',type=pathlib.Path)
    r=subs.add_parser('run'); r.add_argument('output',type=pathlib.Path); r.add_argument('labels',nargs='+')
    args=parser.parse_args()
    (build if args.action=='build' else run)(args)
if __name__=='__main__': main()
