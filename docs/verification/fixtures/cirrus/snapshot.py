#!/usr/bin/env python3
"""Package a stable, content-addressed source snapshot without modifying Git."""
import argparse
import hashlib
import io
import json
import os
from pathlib import Path
import stat
import subprocess
import tarfile


MANIFEST = 'source-snapshot.json'


def source_paths(root):
    paths = subprocess.check_output(
        ['git', 'ls-files', '-z', '--cached', '--others', '--exclude-standard'], cwd=root
    ).decode().split('\0')
    return sorted(set(paths) - {''})


def collect(root):
    entries = []
    payload = []
    for name in source_paths(root):
        if name == MANIFEST:
            raise ValueError(f'{MANIFEST} is reserved for the generated manifest')
        path = root / name
        try:
            metadata = path.lstat()
        except FileNotFoundError:
            continue  # A tracked deletion belongs to the snapshot's state.
        if not stat.S_ISREG(metadata.st_mode):
            raise ValueError(f'unsupported snapshot entry: {name}')
        data = path.read_bytes()
        mode = 0o755 if metadata.st_mode & 0o111 else 0o644
        entries.append({'path': name, 'sha256': hashlib.sha256(data).hexdigest(),
                        'bytes': len(data), 'mode': mode})
        payload.append((name, data, mode))
    return entries, payload


def manifest_digest(entries):
    encoded = json.dumps(entries, sort_keys=True, separators=(',', ':')).encode()
    return hashlib.sha256(encoded).hexdigest()


def verify(root):
    """Admit exactly the manifest's files, allowing a read-only deployment."""
    root = root.resolve(strict=True)
    inventory = set()

    def walk_error(error):
        raise error

    for directory, directories, files in os.walk(root, onerror=walk_error, followlinks=False):
        for name in directories + files:
            path = Path(directory) / name
            mode = path.lstat().st_mode
            if not (stat.S_ISREG(mode) or stat.S_ISDIR(mode)):
                raise ValueError(f'unsupported snapshot entry: {path.relative_to(root)}')
        inventory.update((Path(directory) / name).relative_to(root).as_posix() for name in files)
    if MANIFEST not in inventory:
        raise ValueError('source snapshot manifest is missing')
    manifest = json.loads((root / MANIFEST).read_text())
    if (not isinstance(manifest, dict) or manifest.get('schema') != 'quest-source-snapshot-v1'
            or not isinstance(manifest.get('files'), list)):
        raise ValueError('unsupported source snapshot manifest')
    entries = manifest['files']
    if manifest_digest(entries) != manifest.get('digest'):
        raise ValueError('source snapshot manifest digest mismatch')
    expected = set()
    for entry in entries:
        if not isinstance(entry, dict) or set(entry) != {'path', 'sha256', 'bytes', 'mode'}:
            raise ValueError('invalid source snapshot file record')
        name = entry['path']
        if (not isinstance(name, str) or not name or name == MANIFEST
                or name.startswith('/') or any(part in ('', '.', '..') for part in name.split('/'))
                or name in expected or entry['mode'] not in (0o644, 0o755)
                or type(entry['bytes']) is not int or entry['bytes'] < 0):
            raise ValueError('invalid source snapshot path, size, or mode')
        expected.add(name)
    if inventory != expected | {MANIFEST}:
        raise ValueError('source snapshot inventory differs from the manifest')
    for entry in entries:
        path = root / entry['path']
        metadata = path.lstat()
        if (not stat.S_ISREG(metadata.st_mode) or metadata.st_size != entry['bytes']
                or metadata.st_mode & 0o111 != entry['mode'] & 0o111):
            raise ValueError(f"source snapshot size or executable mode changed: {entry['path']}")
        digest = hashlib.sha256()
        with path.open('rb') as stream:
            for chunk in iter(lambda: stream.read(1024 * 1024), b''):
                digest.update(chunk)
        if digest.hexdigest() != entry['sha256']:
            raise ValueError(f"source snapshot content changed: {entry['path']}")
    return manifest


def snapshot(root, output):
    root = root.resolve()
    revision = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root).decode().strip()
    entries, payload = collect(root)
    # Recheck the complete file inventory and executable modes as well as bytes.
    # A new source file created during collection must not disappear silently.
    if collect(root)[0] != entries or revision != subprocess.check_output(
            ['git', 'rev-parse', 'HEAD'], cwd=root).decode().strip():
        raise ValueError('source changed during snapshot; retry')
    digest = manifest_digest(entries)
    manifest = {'schema': 'quest-source-snapshot-v1', 'digest': digest,
                'base_revision': revision,
                'files': entries}
    output.mkdir(parents=True, exist_ok=True)
    archive = output / f'{digest}.tar.gz'
    with tarfile.open(archive, 'w:gz') as tar:
        for name, data, mode in payload:
            info = tarfile.TarInfo(f'source/{name}')
            info.size, info.mode, info.mtime = len(data), mode, 0
            tar.addfile(info, io.BytesIO(data))
        data = json.dumps(manifest, indent=2).encode() + b'\n'
        info = tarfile.TarInfo(f'source/{MANIFEST}')
        info.size, info.mode, info.mtime = len(data), 0o644, 0
        tar.addfile(info, io.BytesIO(data))
    receipt = {'digest': digest, 'archive': str(archive), 'files': len(entries),
               'archive_sha256': hashlib.sha256(archive.read_bytes()).hexdigest()}
    print(json.dumps(receipt))
    return receipt


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, default=Path.cwd())
    parser.add_argument('--output', type=Path)
    parser.add_argument('--verify', action='store_true', help='verify an extracted source snapshot')
    parser.add_argument('--digest-only', action='store_true', help='print only the verified digest')
    args = parser.parse_args()
    if args.verify:
        if args.output:
            parser.error('--output cannot be used with --verify')
        manifest = verify(args.root)
        print(manifest['digest'] if args.digest_only else json.dumps({
            'digest': manifest['digest'], 'files': len(manifest['files']), 'verified': True,
        }))
    else:
        if args.output is None or args.digest_only:
            parser.error('snapshot creation requires --output and does not accept --digest-only')
        snapshot(args.root, args.output)
