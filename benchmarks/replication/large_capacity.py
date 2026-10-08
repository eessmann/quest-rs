#!/usr/bin/env python3
"""Run the original inverse gates and independent forward capacity lane locally.

This lane records resource admission and process memory, not paper timings.
Run after building the large_polynomial_acceptance release example. The separate
matched campaign validates actual timed outputs with its independent oracle.
"""
import argparse
import gzip
import hashlib
import json
import os
from pathlib import Path
import signal
import subprocess
import tempfile


def digest(path):
    value = hashlib.sha256()
    with Path(path).open('rb') as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b''):
            value.update(block)
    return value.hexdigest()


def atomic_json(path, value):
    temporary = path.with_suffix('.tmp')
    temporary.write_text(json.dumps(value, indent=2, allow_nan=False) + '\n')
    temporary.replace(path)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--corpus', type=Path, required=True)
    parser.add_argument('--native-regression', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--timeout', type=float, default=600)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=False)
    index = json.loads((args.native_regression / 'index.json').read_text())
    manifest = json.loads((args.corpus / 'manifest.json').read_text())
    rows = []
    identity = {'binary_sha256': digest(args.binary),
                'manifest_sha256': digest(args.corpus / 'manifest.json'),
                'native_index_sha256': digest(args.native_regression / 'index.json'),
                'profile': 'million-degree-inverse-forward-v1',
                'purpose': 'numerical regression and capacity; not performance comparison'}

    def execute(mode, name, degree, fixture):
        receipt = args.output / (name + '.json')
        memory = args.output / (name + '.rss.txt')
        command = ['/usr/bin/time', '-v', '-o', str(memory), str(args.binary),
                   mode, str(fixture), str(receipt)]
        with (args.output / (name + '.stderr')).open('wb') as errors:
            process = subprocess.Popen(command, stdout=subprocess.DEVNULL,
                                       stderr=errors, start_new_session=True)
            try:
                process.wait(timeout=args.timeout)
                row = json.loads(receipt.read_text()) if receipt.exists() else {'status': 'infrastructure_failure'}
            except subprocess.TimeoutExpired:
                os.killpg(process.pid, signal.SIGKILL)
                process.wait()
                row = {'status': 'timeout', 'deadline_seconds': args.timeout}
        measured_rss = None
        if memory.exists():
            for line in memory.read_text().splitlines():
                if 'Maximum resident set size (kbytes):' in line:
                    measured_rss = int(line.rsplit(':', 1)[1]) * 1024
        row.update(case_id=name, degree=degree, returncode=process.returncode,
                   fixture_sha256=digest(fixture), measured_process_peak_rss_bytes=measured_rss,
                   rss_scope='whole adapter including input loading; separate from modeled live memory')
        rows.append(row)
        atomic_json(args.output / 'summary.json', {**identity, 'rows': rows})
        print(json.dumps({key: row.get(key) for key in ('case_id', 'status', 'source_inverse_check', 'coefficient_backward_linf', 'measured_process_peak_rss_bytes')}), flush=True)

    for source in index['fixtures']:
        archive = args.native_regression / source['file']
        if digest(archive) != source['archive_sha256']:
            raise ValueError('native archive changed')
        # TMPDIR must be disk-backed project storage, as for native campaigns.
        with tempfile.TemporaryDirectory(prefix='nlft-source-') as temporary:
            fixture = Path(temporary) / 'input.json'
            fixture.write_bytes(gzip.decompress(archive.read_bytes()))
            if digest(fixture) != source['source_sha256']:
                raise ValueError('native source changed')
            execute('inverse', 'native_d' + str(source['degree']), source['degree'], fixture)
    for case in manifest['cases']:
        if case['family'] == 'independent_reflections':
            fixture = args.corpus / case['fixture']
            if digest(fixture) != case['fixture_sha256']:
                raise ValueError('independent fixture changed')
            execute('forward', case['id'], case['degree'], fixture)
    atomic_json(args.output / 'completion.json', {**identity, 'cases': len(rows),
                'complete': True, 'all_passed': all(row['status'] == 'ok' for row in rows),
                'summary_sha256': digest(args.output / 'summary.json')})
    return 0 if all(row['status'] == 'ok' for row in rows) else 1


if __name__ == '__main__':
    raise SystemExit(main())
