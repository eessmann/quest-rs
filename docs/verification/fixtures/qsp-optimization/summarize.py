#!/usr/bin/env python3
"""Summarize a complete four-variant QSP campaign without modifying raw receipts."""
import argparse
import hashlib
import json
import math
import pathlib
import statistics

LABELS = ('baseline', 'a', 'ab', 'abc')
DISPLAY = ('Baseline', 'A', 'A+B', 'A+B+C')
WORKLOADS = (('binary64', 256, 53), ('binary64', 1024, 53),
             ('offline', 16, 128), ('offline', 16, 256),
             ('offline', 256, 128), ('offline', 256, 256))


def distribution(values):
    return {'median': statistics.median(values), 'min': min(values), 'max': max(values)}


def numeric(row, key):
    value = row.get(key)
    if type(value) not in (int, float) or not math.isfinite(value) or value < 0:
        raise ValueError(f'invalid nonnegative finite {key}: {value!r}')
    return value


def summarize(root):
    root = pathlib.Path(root)
    inputs = {}

    def read(path):
        raw = path.read_bytes()
        inputs[str(path.relative_to(root))] = hashlib.sha256(raw).hexdigest()
        return raw.decode('utf-8')

    completion = json.loads(read(root/'measurement-completion.json'))
    for key in ('all_trials_succeeded', 'sources_unchanged', 'workloads_valid'):
        if completion.get(key) is not True:
            raise ValueError(f'campaign completion gate failed: {key}')
    labels = completion.get('labels')
    if (not isinstance(labels, list) or len(labels) != 4 or set(labels) != set(LABELS)
            or completion.get('trials') != 3
            or completion.get('validated_workload_records') != 72 or completion.get('errors')):
        raise ValueError('expected a completed four-variant, three-trial, 72-record campaign')
    rows = {key: {label: [] for label in LABELS} for key in WORKLOADS}
    signatures = {}
    count = 0
    for label in LABELS:
        for trial in range(1, 4):
            prefix = root/label/f'trial-{trial}'
            receipt = json.loads(read(prefix.with_suffix('.json')))
            if receipt.get('exit_code') != 0 or receipt.get('status') in ('timeout', 'interrupted'):
                raise ValueError(f'unsuccessful trial receipt: {prefix}')
            observed = set()
            for line in read(prefix.with_suffix('.stdout')).splitlines():
                row = json.loads(line)
                if not isinstance(row, dict):
                    raise ValueError(f'workload record is not an object: {prefix}')
                key = (row.get('kind'), row.get('degree'), row.get('bits'))
                if key not in rows or key in observed or row.get('status') != 'ok':
                    raise ValueError(f'failed, duplicate, or unexpected workload {key}: {prefix}')
                observed.add(key)
                iterations = row.get('iterations')
                if type(iterations) is not int or iterations <= 0:
                    raise ValueError(f'invalid iterations: {prefix}')
                if key[0] == 'offline' and iterations != 1:
                    raise ValueError(f'offline fixture requires one measured solve: {prefix}')
                fingerprint, grid = row.get('export_fingerprint'), row.get('grid')
                if (not isinstance(fingerprint, str) or len(fingerprint) != 16
                        or any(c not in '0123456789abcdef' for c in fingerprint)
                        or type(grid) is not int or grid <= 0):
                    raise ValueError(f'invalid fingerprint/grid: {prefix}')
                signature = (fingerprint, grid)
                if key in signatures and signature != signatures[key]:
                    raise ValueError(f'fingerprint/grid differs across variants/trials for {key}: {prefix}')
                signatures[key] = signature
                # Stage getters already describe one offline completion. Binary64
                # completion counts/time and every outer sample span iterations.
                completion_iterations = iterations if key[0] == 'binary64' else 1
                normalized = {
                    'completion': {
                        'nanoseconds': numeric(row, 'completion_nanoseconds')/completion_iterations,
                        'allocations': numeric(row, 'completion_allocations')/completion_iterations,
                        'peak_extra_live_bytes': numeric(row, 'completion_peak_extra_live_bytes'),
                        'work_units': numeric(row, 'completion_work_units'),
                    },
                    'end_to_end': {
                        'nanoseconds': numeric(row, 'nanoseconds')/iterations,
                        'allocations': numeric(row, 'allocations')/iterations,
                        'peak_extra_live_bytes': numeric(row, 'peak_extra_live_bytes'),
                        'work_units': numeric(row, 'work_units'),
                    },
                }
                rows[key][label].append(normalized)
                count += 1
            if observed != set(WORKLOADS):
                raise ValueError(f'missing workloads: {prefix}')
    if count != 72:
        raise ValueError(f'expected 72 workload records, got {count}')
    workloads = []
    for key in WORKLOADS:
        variants = {}
        for label in LABELS:
            samples = rows[key][label]
            variants[label] = {
                stage: {metric: distribution([sample[stage][metric] for sample in samples])
                        for metric in ('nanoseconds', 'allocations', 'peak_extra_live_bytes', 'work_units')}
                for stage in ('completion', 'end_to_end')
            }
        fingerprint, grid = signatures[key]
        workloads.append({'kind': key[0], 'degree': key[1], 'bits': key[2],
                          'grid': grid, 'export_fingerprint': fingerprint, 'variants': variants})
    return {'schema_version': 1, 'labels': list(LABELS), 'trials': 3,
            'validated_workload_records': count, 'inputs_sha256': inputs,
            'normalization': 'Times and allocation counts per invocation; peak bytes and charged work are not divided by iterations.',
            'workloads': workloads}


def render_markdown(summary):
    lines = ['# QSP optimization measurements', '',
             'All 72 workload records passed the campaign gates and independent summary validation. '
             'Accepted grids and export fingerprints match across all four variants and three trials.', '',
             'Values below are medians of three trials. Per-invocation min/max and exact grids, '
             'fingerprints, and input receipt hashes are available in `summary.json`. '
             'Raw trial receipts remain unchanged.', '',
             '## Timing', '',
             'Each cell is **completion / end-to-end**, in milliseconds per invocation.', '']

    def heading():
        return ['| Workload | ' + ' | '.join(DISPLAY) + ' |', '|---|' + '---:|'*4]

    def name(workload):
        return f"{workload['kind']} d={workload['degree']}, {workload['bits']} bits"

    lines += heading()
    for workload in summary['workloads']:
        values = []
        for label in LABELS:
            variant = workload['variants'][label]
            values.append(' / '.join(f"{variant[stage]['nanoseconds']['median']/1e6:.4f}"
                                     for stage in ('completion', 'end_to_end')))
        lines.append('| ' + name(workload) + ' | ' + ' | '.join(values) + ' |')
    for stage, title in [('completion', 'Completion resources'), ('end_to_end', 'End-to-end resources')]:
        lines += ['', '## ' + title, '',
                  'Each cell is **allocations / peak additional live bytes / charged work units**.', '']
        lines += heading()
        for workload in summary['workloads']:
            values = []
            for label in LABELS:
                measured = workload['variants'][label][stage]
                values.append(' / '.join(f"{measured[key]['median']:,.3f}".rstrip('0').rstrip('.')
                                         for key in ('allocations', 'peak_extra_live_bytes', 'work_units')))
            lines.append('| ' + name(workload) + ' | ' + ' | '.join(values) + ' |')
    lines += ['', 'Allocation counts and times are normalized by the measured iteration count; '
              'peaks are sample maxima and are never divided. Counters cover Rust allocations, '
              'excluding preexisting live storage, allocator metadata, stacks, native allocation, '
              'and transient realloc overlap.', '',
              'Binary64 end-to-end starts from an admitted target; offline end-to-end includes '
              'admission, export, and independent certification. Charged work follows each backend\'s '
              'accounting; offline solve work excludes separate certification accounting. '
              'Compare variants within a workload. Instrumentation and fixture checks are included in timings.', '']
    return '\n'.join(lines)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('campaign', type=pathlib.Path)
    args = parser.parse_args()
    try:
        summary = summarize(args.campaign.resolve())
    except (ValueError, OSError, KeyError, TypeError) as error:
        parser.exit(1, f'Cannot summarize campaign: {error}\n')
    (args.campaign/'summary.json').write_text(json.dumps(summary, indent=2, allow_nan=False)+'\n')
    (args.campaign/'results.md').write_text(render_markdown(summary))
    print(args.campaign/'summary.json')
    print(args.campaign/'results.md')


if __name__ == '__main__':
    main()
