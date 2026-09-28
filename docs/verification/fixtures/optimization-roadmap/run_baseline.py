#!/usr/bin/env python3
"""Preserve a reproducible compiler baseline, including failed measurements."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time


def expected_keys(fixture):
    cases = ['qft6', 'pauli_evolution6', 'qsvt_oracle6', 'clifford_t6', 'signed_controls6']
    stages = {
        'baseline': ['construction', 'unchanged', 'exact', 'linear', 'parity', 'fusion', 'existing_combined'],
        'roadmap': ['schedule', 'terminal', 'linear_candidate', 'parity_candidate', 'beam'],
        'workers': ['zx_baseline', 'zx_expanded', 'beam_workers'],
    }[fixture]
    pairs = [(case, stage) for case in cases for stage in stages]
    if fixture == 'baseline':
        pairs.extend(('branch_heavy6', stage) for stage in ['construction', 'unchanged', 'classical', 'quantum', 'existing_combined'])
    elif fixture == 'roadmap':
        pairs.extend(('branch_heavy6', stage) for stage in ['quantum_flow', 'terminal', 'beam'])
        pairs.extend(('shared_symbolic6', stage) for stage in ['construction', 'binding'])
    else:
        pairs.extend((f'mitm_exact{qubits}', 'mitm_exact') for qubits in [1, 2])
        pairs.extend((case, 'mitm_approximate') for case in ['mitm_dyadic', 'mitm_pi', 'mitm_affine'])
    return {(case, stage, sample) for case, stage in pairs for sample in range(5)}


def validate_sample_keys(samples, fixture):
    keys = [(row['case'], row['stage'], row['sample']) for row in samples]
    if len(keys) != len(set(keys)) or set(keys) != expected_keys(fixture):
        raise ValueError('missing, duplicate or unexpected compiler sample keys')


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--repository', type=Path, required=True)
    parser.add_argument('--worker', type=Path)
    parser.add_argument('--fixture', choices=['baseline', 'roadmap', 'workers'], default='baseline')
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--target-dir', type=Path, required=True)
    args = parser.parse_args()
    if args.fixture == 'workers' and (args.worker is None or not args.worker.is_file()):
        parser.error('--worker must identify an existing worker binary')
    repo, out = args.repository.resolve(), args.output.resolve()
    out.mkdir(parents=True, exist_ok=False)
    (out / 'src').mkdir()
    source = Path(__file__).with_name('roadmap_workers.rs' if args.fixture == 'workers' else args.fixture + '_compiler.rs').read_bytes()
    if args.fixture != 'baseline':
        (out / 'src/baseline_compiler.rs').write_bytes(Path(__file__).with_name('baseline_compiler.rs').read_bytes())
    (out / 'src/main.rs').write_bytes(source)
    (out / 'rust-toolchain.toml').write_bytes((repo / 'rust-toolchain.toml').read_bytes())
    (out / 'Cargo.toml').write_text('''[package]
name = "quest-roadmap-baseline"
version = "0.0.0"
edition = "2024"
[workspace]
[dependencies]
quest-circuit = { path = REPO, default-features = false }
serde_json = "1"
'''.replace('REPO', json.dumps(str(repo / 'crates/quest-circuit'))))
    if args.fixture == 'workers':
        package = (out / 'Cargo.toml').read_text().replace('default-features = false', 'default-features = false, features = ["workers"]')
        for name in ['quest-math', 'quest-optimizer-client', 'quest-optimizer-protocol']:
            package += '\n' + name + ' = { path = ' + json.dumps(str(repo / 'crates' / name)) + ' }\n'
        (out / 'Cargo.toml').write_text(package)
    env = dict(os.environ, CARGO_BUILD_JOBS='4')
    if args.worker is not None:
        env['QUEST_ROADMAP_WORKER'] = str(args.worker.resolve())
    manifest = {'schema': 1, 'source_sha256': hashlib.sha256(source).hexdigest(),
                'repository': str(repo), 'expected_samples': {'baseline': 200, 'roadmap': 150, 'workers': 100}[args.fixture], 'fixture': args.fixture, 'started_unix': time.time()}
    head = subprocess.run(['git', 'rev-parse', 'HEAD'], cwd=repo, text=True, capture_output=True)
    manifest['commit'] = head.stdout.strip() if head.returncode == 0 else None
    diff = subprocess.run(['git', 'diff', '--binary', 'HEAD'], cwd=repo, capture_output=True)
    (out / 'working-tree.diff').write_bytes(diff.stdout)
    manifest['tracked_diff_sha256'] = hashlib.sha256(diff.stdout).hexdigest()
    untracked = subprocess.run(['git', 'ls-files', '--others', '--exclude-standard'], cwd=repo, text=True, capture_output=True)
    manifest['untracked_source_sha256'] = {name: hashlib.sha256((repo / name).read_bytes()).hexdigest() for name in untracked.stdout.splitlines() if (repo / name).is_file()}
    if args.fixture != 'baseline':
        manifest['corpus_source_sha256'] = hashlib.sha256((out / 'src/baseline_compiler.rs').read_bytes()).hexdigest()
    if args.worker is not None:
        manifest['worker_sha256'] = hashlib.sha256(args.worker.read_bytes()).hexdigest()
    status = 1
    try:
        command = ['cargo', 'build', '--offline', '--release', '--manifest-path', str(out / 'Cargo.toml'),
                   '--target-dir', str(args.target_dir.resolve())]
        manifest['build_command'] = command
        with (out / 'build.log').open('w') as log:
            build = subprocess.run(command, cwd=repo, env=env, stdout=log, stderr=subprocess.STDOUT)
        manifest['build_status'] = build.returncode
        if build.returncode != 0:
            raise RuntimeError('build failed; see build.log')
        with (out / 'results.jsonl').open('w') as stdout, (out / 'diagnostics.log').open('w') as stderr:
            run = subprocess.run([str(args.target_dir.resolve() / 'release/quest-roadmap-baseline')],
                                 cwd=out, env=env, stdout=stdout, stderr=stderr)
        manifest['run_status'] = run.returncode
        rows = [json.loads(line) for line in (out / 'results.jsonl').read_text().splitlines()]
        samples = [row for row in rows if 'sample' in row]
        manifest['samples'] = len(samples)
        manifest['failed_samples'] = sum(row['status'] != 'complete' for row in samples)
        validate_sample_keys(samples, args.fixture)
        status = int(run.returncode != 0 or len(samples) != manifest['expected_samples'] or manifest['failed_samples'] != 0)
    except Exception as error:
        manifest['error'] = str(error)
    finally:
        manifest['status'] = 'complete' if status == 0 else 'failed'
        manifest['finished_unix'] = time.time()
        (out / 'completion.json').write_text(json.dumps(manifest, indent=2) + '\n')
        print(json.dumps(manifest))
    return status


if __name__ == '__main__':
    raise SystemExit(main())
