#!/usr/bin/env python3
"""Run matched project workloads serially in an explicitly granted quiet window."""
import argparse
import datetime
import hashlib
import json
import os
import pathlib
import platform
import subprocess
import shutil


def repository_inputs(repository):
    files = []
    for folder in sorted((repository / 'crates').iterdir()):
        if not folder.is_dir():
            continue
        for extension in ['*.rs', '*.cpp', '*.hpp', '*.h']:
            files.extend(folder.rglob(extension))
        if (folder / 'Cargo.toml').is_file():
            files.append(folder / 'Cargo.toml')
    for name in ['Cargo.toml', 'Cargo.lock', 'devenv.nix', 'devenv.yaml', 'devenv.lock', 'rust-toolchain.toml', 'rust-toolchain', '.cargo/config.toml', '.cargo/config', 'flake.nix', 'flake.lock']:
        if (repository / name).is_file():
            files.append(repository / name)
    return files


def hashes(paths):
    return {str(path): hashlib.sha256(path.read_bytes()).hexdigest() for path in sorted(set(paths))}


def write_hashes(values, path):
    with path.open('w') as receipt:
        for filename, digest in values.items():
            receipt.write(digest + '  ' + filename + '\n')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--repository', type=pathlib.Path, required=True)
    parser.add_argument('--baseline-repository', type=pathlib.Path, required=True)
    parser.add_argument('--output', type=pathlib.Path, required=True)
    parser.add_argument('--prepare-only', action='store_true', help='Write scratch manifests without building or timing')
    args = parser.parse_args()
    repositories = {'baseline': args.baseline_repository.resolve(), 'current': args.repository.resolve()}
    fixture = pathlib.Path(__file__).resolve().parent
    output = args.output.resolve()
    if output.exists() and any(output.iterdir()):
        parser.error('--output must be a new or empty directory; existing receipts are preserved')
    output.mkdir(parents=True, exist_ok=True)
    env = os.environ.copy()
    env['CARGO_BUILD_JOBS'] = '2'
    packages = {}
    (output / 'packages').mkdir()
    allocator = output / 'packages' / 'allocator.rs'
    allocator.write_bytes((fixture.parent / 'allocator.rs').read_bytes())
    source_files = [fixture / 'main.rs', fixture / 'run.py', fixture / 'README.md', fixture.parent / 'allocator.rs', allocator]
    for label, repository in repositories.items():
        compiler = 'quest-circuit' if (repository / 'crates/quest-circuit/Cargo.toml').is_file() else 'quest-compile'
        dependencies = {'quest': 'quest', 'quest-compile': compiler, 'quest-polynomial': 'quest-polynomial', 'quest-qsp': 'quest-qsp', 'quest-sys': 'quest-sys'}
        package = output / 'packages' / label
        package.mkdir(parents=True)
        name = 'project-performance-' + label
        manifest = '[package]\nname=' + json.dumps(name) + '\nversion="0.0.0"\nedition="2024"\n[workspace]\n[dependencies]\n'
        for alias, crate in dependencies.items():
            path = repository / 'crates' / crate
            if not (path / 'Cargo.toml').is_file():
                parser.error(f'missing crate manifest: {path}')
            native_name = 'quest-rs' if alias == 'quest' else crate
            manifest += alias + ' = { package=' + json.dumps(native_name) + ', path=' + json.dumps(str(path)) + ' }\n'
        manifest += 'faer = { version="=0.24.4", default-features=false, features=["std","linalg"] }\n'
        manifest += '[build-dependencies]\nquest-build = { path=' + json.dumps(str(repository / 'crates/quest-build')) + ' }\n'
        manifest += '[profile.release]\nlto="thin"\n[[bin]]\nname=' + json.dumps(name) + '\npath=' + json.dumps('main.rs') + '\n'
        (package / 'main.rs').write_bytes((fixture / 'main.rs').read_bytes())
        (package / 'Cargo.toml').write_text(manifest)
        (package / 'Cargo.lock').write_bytes((repository / 'Cargo.lock').read_bytes())
        (package / 'build.rs').write_text('fn main() { quest_build::emit_final_target_runtime_paths().expect("native final-target runtime path configuration"); }\n')
        packages[label] = (package, name)
        source_files.extend(package.iterdir())
        source_files.extend(repository_inputs(repository))
    write_hashes(hashes(source_files), output / 'sources.sha256')

    def frozen_inputs():
        files = [fixture / 'main.rs', fixture / 'run.py', fixture / 'README.md', fixture.parent / 'allocator.rs', allocator]
        for repository in repositories.values():
            files.extend(repository_inputs(repository))
        for package, _ in packages.values():
            files.extend(package / name for name in ['main.rs', 'Cargo.toml', 'build.rs'])
        return hashes(files)

    before = frozen_inputs()
    write_hashes(before, output / 'frozen-before.sha256')
    with (output / 'environment.txt').open('w') as receipt:
        receipt.write(datetime.datetime.now(datetime.timezone.utc).isoformat() + '\nrelease_lto=thin\njobs=2\n')
        for command in [['uname', '-a'], ['rustc', '-Vv'], ['cargo', '-V']]:
            subprocess.run(command, cwd=repositories['current'], stdout=receipt, stderr=receipt, check=True)
        for label, repository in repositories.items():
            receipt.write(label + '=' + str(repository) + '\n')
            root = subprocess.run(['git', 'rev-parse', '--show-toplevel'], cwd=repository, capture_output=True, text=True)
            if root.returncode == 0 and pathlib.Path(root.stdout.strip()).resolve() == repository:
                receipt.flush()
                subprocess.run(['git', 'rev-parse', 'HEAD'], cwd=repository, stdout=receipt, stderr=receipt, check=True)
            else:
                receipt.write('archive snapshot; identity is the frozen source hash manifest\n')
    if args.prepare_only:
        print('Prepared scratch manifests:', output)
        return

    def run(command, name):
        with (output / name).open('w') as receipt:
            subprocess.run(command, cwd=repositories['current'], env=env, stdout=receipt, stderr=subprocess.STDOUT, check=True)

    try:
        darwin = platform.system() == 'Darwin'
        timing = ['/usr/bin/time', '-l' if darwin else '-v']
        binaries = []
        for label, (package, name) in packages.items():
            target = output / 'build' / label
            env['CARGO_TARGET_DIR'] = str(target)
            command = ['cargo', 'build', '--release', '--manifest-path', str(package / 'Cargo.toml'), '--bin', name]
            run([*timing, *command], label + '-cold-build.log')
            run([*timing, *command], label + '-unchanged-build.log')
            os.utime(package / 'main.rs', None)
            run([*timing, *command], label + '-touched-source-build.log')
            binary = target / 'release' / name
            binaries.append((label, binary))
            run(['size', '-m' if darwin else '--format=SysV', str(binary)], label + '-sections.txt')
        with (output / 'resolved-locks.sha256').open('w') as receipt:
            for label, (package, _) in packages.items():
                lock = package / 'Cargo.lock'
                receipt.write(hashlib.sha256(lock.read_bytes()).hexdigest() + '  ' + label + '\n')
                shutil.copyfile(lock, output / (label + '-resolved-Cargo.lock'))
        with (output / 'executables.json').open('w') as receipt:
            json.dump({label: {'bytes': path.stat().st_size, 'sha256': hashlib.sha256(path.read_bytes()).hexdigest()} for label, path in binaries}, receipt, indent=2)
        for trial in range(1, 4):
            order = binaries if trial % 2 else list(reversed(binaries))
            for workload in ['qsp', 'compiler', 'native']:
                for label, binary in order:
                    with (output / f'{label}-{workload}-trial{trial}.csv').open('w') as receipt, (output / f'{label}-{workload}-trial{trial}.details').open('w') as diagnostic:
                        subprocess.run([*timing, str(binary), workload], cwd=repositories['current'], env=env, stdout=receipt, stderr=diagnostic, check=True)
    finally:
        after = frozen_inputs()
        write_hashes(after, output / 'frozen-after.sha256')
        if before != after:
            raise RuntimeError('Repository or fixture inputs changed during measurement; receipts are invalid')
    print('Receipts:', output)


if __name__ == '__main__':
    main()
