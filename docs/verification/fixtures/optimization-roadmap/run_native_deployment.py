#!/usr/bin/env python3
"""Build a direct downstream consumer and preserve native-mode outcomes."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--repository', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--target-dir', type=Path, required=True)
    parser.add_argument('--mpi-only', action='store_true', help='run two/four-rank SV/DM cases')
    args = parser.parse_args()
    repo, out, target = (p.resolve() for p in (args.repository, args.output, args.target_dir))
    out.mkdir(parents=True, exist_ok=False)
    (out / 'src').mkdir()
    source = Path(__file__).with_name('native_deployment.rs').read_bytes()
    (out / 'src/main.rs').write_bytes(source)
    (out / 'rust-toolchain.toml').write_bytes((repo / 'rust-toolchain.toml').read_bytes())
    (out / 'Cargo.toml').write_text('''[package]
name = "quest-roadmap-native-deployment"
version = "0.0.0"
edition = "2024"
[workspace]
[dependencies]
quest-sys = { path = SYS }
[build-dependencies]
quest-build = { path = BUILD }
'''.replace('SYS', json.dumps(str(repo / 'crates/quest-sys')))
        .replace('BUILD', json.dumps(str(repo / 'crates/quest-build'))))
    (out / 'build.rs').write_text('fn main() { quest_build::emit_final_target_runtime_paths().unwrap(); }\n')
    env = dict(os.environ, CARGO_BUILD_JOBS='4', OMP_NUM_THREADS='4')
    for name in ('LD_LIBRARY_PATH', 'LD_PRELOAD', 'LD_AUDIT'):
        env.pop(name, None)
    result = {'schema': 1, 'source_sha256': hashlib.sha256(source).hexdigest(),
              'repository': str(repo), 'started_unix': time.time(), 'cases': []}
    status = 1
    try:
        command = ['cargo', 'build', '--offline', '--manifest-path', str(out / 'Cargo.toml'),
                   '--target-dir', str(target)]
        result['build_command'] = command
        with (out / 'build.log').open('w') as log:
            build = subprocess.run(command, cwd=repo, env=env, stdout=log, stderr=subprocess.STDOUT)
        result['build_status'] = build.returncode
        if build.returncode:
            raise RuntimeError('build failed; see build.log')
        binary = target / 'debug/quest-roadmap-native-deployment'
        for name, command in [('readelf', ['readelf', '-d', str(binary)]),
                              ('ldd', ['ldd', str(binary)])]:
            run = subprocess.run(command, env=env, capture_output=True, text=True, timeout=30)
            (out / f'{name}.log').write_text(run.stdout + run.stderr)
            result[name + '_status'] = run.returncode
            if run.returncode or (name == 'ldd' and 'not found' in run.stdout):
                raise RuntimeError(f'{name} failed')
            if name == 'readelf' and '(RUNPATH)' not in run.stdout:
                raise RuntimeError('final executable lacks RUNPATH')
        configurations = [('mpi', nodes) for nodes in (2, 4)] if args.mpi_only else [(mode, 1) for mode in ('cpu', 'omp', 'gpu')]
        for mode, nodes in configurations:
            for kind in ('sv', 'dm'):
                case = {'mode': mode, 'kind': kind, 'nodes': nodes}
                command = [str(binary), mode, kind]
                if mode == 'mpi':
                    command = ['mpiexec', '-n', str(nodes), *command, str(nodes)]
                log_name = f'{mode}-{kind}-{nodes}.log'
                try:
                    run = subprocess.run(command, cwd=out, env=env,
                                         capture_output=True, text=True, timeout=60)
                    (out / log_name).write_text(run.stdout + run.stderr)
                    case['exit_code'] = run.returncode
                    case['status'] = 'complete' if run.returncode == 0 else 'failed'
                except subprocess.TimeoutExpired as error:
                    (out / log_name).write_bytes((error.stdout or b'') + (error.stderr or b''))
                    case['status'] = 'timeout'
                result['cases'].append(case)
        status = int(any(case['status'] != 'complete' for case in result['cases']))
    except Exception as error:
        result['error'] = str(error)
    finally:
        result['status'] = 'complete' if status == 0 else 'failed'
        result['finished_unix'] = time.time()
        (out / 'completion.json').write_text(json.dumps(result, indent=2) + '\n')
        print(json.dumps(result))
    return status


if __name__ == '__main__':
    raise SystemExit(main())
