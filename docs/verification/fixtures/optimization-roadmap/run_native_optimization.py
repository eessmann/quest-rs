#!/usr/bin/env python3
"""Measure native search, preparation, and completed execution without Cargo loader overrides."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time

CORPORA = ('qft6', 'pauli_evolution6', 'qsvt_oracle6', 'clifford_t6', 'signed_controls6')
STAGES = ('unchanged', 'existing_combined', 'terminal', 'terminal_reuse100', 'beam')


def expected_keys(mode, nodes):
    cases = CORPORA + (() if mode == 'mpi' else ('branch_heavy6',))
    ranks = range(nodes) if mode == 'mpi' else (None,)
    return {(f'{case}/rank{rank}' if rank is not None else case, f'{stage}/{component}', sample)
            for case in cases for rank in ranks for stage in STAGES
            for component, count in (('search', 1), ('preparation', 5), ('warm_execution', 5))
            for sample in range(count)}


def validate_sample_keys(samples, mode, nodes):
    actual = [(row['case'], row['stage'], row['sample']) for row in samples]
    expected = expected_keys(mode, nodes)
    if len(actual) != len(expected) or set(actual) != expected:
        raise ValueError(f'missing, duplicate, or unexpected {mode} sample keys')


def max_rank_rows(samples, nodes):
    grouped = {}
    for row in samples:
        case, separator, rank_text = row['case'].rpartition('/rank')
        if not separator or not rank_text.isdecimal():
            raise ValueError('missing MPI rank label')
        rank = int(rank_text)
        if rank < 0 or rank >= nodes:
            raise ValueError('MPI rank outside expected range')
        key = (case, row['stage'], row['sample'])
        group = grouped.setdefault(key, {})
        if rank in group:
            raise ValueError('duplicate MPI rank measurement')
        group[rank] = row
    maxima = []
    for (case, stage, sample), group in grouped.items():
        if set(group) != set(range(nodes)):
            raise ValueError('missing MPI rank measurement')
        completions = {row.get('detail', {}).get('completion') for row in group.values()}
        if len(completions) != 1:
            raise ValueError('inconsistent MPI rank optimizer completion')
        maxima.append({'case': case, 'stage': stage, 'sample': sample,
                       'elapsed_ns': max(row['elapsed_ns'] for row in group.values()),
                       'status': 'complete' if all(row.get('status') == 'complete' for row in group.values()) else 'failed',
                       'detail': {'completion': next(iter(completions))},
                       'ranks': nodes, 'rank_ids': sorted(group)})
    return maxima


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--repository', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--target-dir', type=Path, required=True)
    parser.add_argument('--mpi-only', action='store_true')
    parser.add_argument('--modes', nargs='+', choices=['cpu', 'omp', 'gpu'], default=['cpu', 'omp', 'gpu'])
    args = parser.parse_args()
    repo, out, target = (p.resolve() for p in (args.repository, args.output, args.target_dir))
    out.mkdir(parents=True, exist_ok=False)
    (out / 'src').mkdir()
    fixtures = Path(__file__).parent
    for original, destination in [('native_mpi_optimization.rs' if args.mpi_only else 'native_optimization.rs', 'main.rs'), ('baseline_compiler.rs', 'baseline_compiler.rs')]:
        (out / 'src' / destination).write_bytes((fixtures / original).read_bytes())
    (out / 'rust-toolchain.toml').write_bytes((repo / 'rust-toolchain.toml').read_bytes())
    manifest = '''[package]
name = "quest-roadmap-native-optimization"
version = "0.0.0"
edition = "2024"
[workspace]
[dependencies]
quest = { package = "quest-rs", path = QUEST, features = ["mpi"] }
quest-circuit = { path = CIRCUIT }
quest-sys = { path = SYS }
serde_json = "1"
[build-dependencies]
quest-build = { path = BUILD }
'''
    for token, crate in [('QUEST','quest'), ('CIRCUIT','quest-circuit'), ('SYS','quest-sys'), ('BUILD','quest-build')]:
        manifest = manifest.replace(token, json.dumps(str(repo / 'crates' / crate)))
    (out / 'Cargo.toml').write_text(manifest)
    (out / 'build.rs').write_text('fn main() { quest_build::emit_final_target_runtime_paths().unwrap(); }\n')
    env = dict(os.environ, CARGO_BUILD_JOBS='4', OMP_NUM_THREADS='4')
    for name in ('LD_LIBRARY_PATH', 'LD_PRELOAD', 'LD_AUDIT'):
        env.pop(name, None)
    result = {'schema':1, 'started_unix':time.time(), 'cases':[], 'expected_samples_per_rank':275 if args.mpi_only else 330,
              'sources':{p.name:hashlib.sha256(p.read_bytes()).hexdigest() for p in (out/'src').iterdir()}}
    diff = subprocess.run(['git','diff','--binary','HEAD'],cwd=repo,capture_output=True)
    (out/'working-tree.diff').write_bytes(diff.stdout)
    result['tracked_diff_sha256']=hashlib.sha256(diff.stdout).hexdigest()
    result['commit']=subprocess.check_output(['git','rev-parse','HEAD'],cwd=repo,text=True).strip()
    untracked=subprocess.check_output(['git','ls-files','--others','--exclude-standard'],cwd=repo,text=True)
    result['untracked_source_sha256']={name:hashlib.sha256((repo/name).read_bytes()).hexdigest() for name in untracked.splitlines() if (repo/name).is_file()}
    status=1
    try:
        with (out/'build.log').open('w') as log:
            build=subprocess.run(['cargo','build','--offline','--release','--manifest-path',str(out/'Cargo.toml'),'--target-dir',str(target)],cwd=repo,env=env,stdout=log,stderr=subprocess.STDOUT)
        result['build_status']=build.returncode
        if build.returncode:
            raise RuntimeError('build failed; see build.log')
        binary=target/'release/quest-roadmap-native-optimization'
        for name,command in [('readelf',['readelf','-d',str(binary)]),('ldd',['ldd',str(binary)])]:
            probe=subprocess.run(command,env=env,capture_output=True,text=True,timeout=30)
            (out/f'{name}.log').write_text(probe.stdout+probe.stderr)
            if probe.returncode or (name=='ldd' and 'not found' in probe.stdout) or (name=='readelf' and '(RUNPATH)' not in probe.stdout):
                raise RuntimeError(f'{name} loader check failed')
        configurations=[('mpi','sv',nodes) for nodes in (2,4)] if args.mpi_only else [(mode,kind,1) for mode in args.modes for kind in ('sv','dm')]
        for mode,kind,nodes in configurations:
                case={'mode':mode,'kind':kind,'nodes':nodes}
                label=f'{mode}-{kind}-{nodes}'
                command=[str(binary),mode,kind]
                if mode=='mpi':
                    command=['mpiexec','-n',str(nodes),str(binary)]
                try:
                    with (out/f'{label}.jsonl').open('w') as stdout, (out/f'{label}.log').open('w') as stderr:
                        run=subprocess.run(command,cwd=out,env=env,stdout=stdout,stderr=stderr,timeout=600)
                    rows=[json.loads(line) for line in (out/f'{label}.jsonl').read_text().splitlines()]
                    samples=[row for row in rows if 'sample' in row]
                    case['samples']=len(samples)
                    case['failed_records']=sum(row.get('status')=='failed' for row in rows)
                    case['optimizer_stop_records']=[{'case':row['case'],'stage':row['stage'],'sample':row['sample'],
                                                     'completion':row.get('detail',{}).get('completion')}
                                                    for row in samples if row.get('detail',{}).get('completion') not in (None,'Complete')]
                    case['exhausted_records']=[row for row in case['optimizer_stop_records']
                                               if row['completion'] in ('Exhausted','WorkLimit','StorageLimit','CandidateLimit','RoundLimit','WorkerLimit')]
                    case['exit_code']=run.returncode
                    validate_sample_keys(samples, mode, nodes)
                    if mode=='mpi':
                        maxima=max_rank_rows(samples, nodes)
                        (out/f'{label}-max-rank.jsonl').write_text(''.join(json.dumps(row)+'\n' for row in maxima))
                    case['status']='complete' if run.returncode==0 and len(samples)==result['expected_samples_per_rank']*nodes and not case['failed_records'] else 'failed'
                except subprocess.TimeoutExpired:
                    case['status']='timeout'
                except Exception as error:
                    case['status']='failed'
                    case['error']=str(error)
                result['cases'].append(case)
        status=int(any(case['status']!='complete' for case in result['cases']))
    except Exception as error:
        result['error']=str(error)
    finally:
        result['status']='complete' if status==0 else 'failed'
        result['finished_unix']=time.time()
        (out/'completion.json').write_text(json.dumps(result,indent=2)+'\n')
        print(json.dumps(result))
    return status


if __name__=='__main__':
    raise SystemExit(main())
