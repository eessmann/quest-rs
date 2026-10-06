#!/usr/bin/env python3
"""Admit the native-only trybuild profile without exposing Cargo configuration."""
import json
import os
from pathlib import Path
import re
import subprocess
import sys

UI_FILTER = 'binary(=compile_fail) and (package(=quest-compile) or package(=quest-rs))'
UI_FLAGS = ['--cfg', 'trybuild_no_target', '--cfg', 'trybuild', '--verbose',
            '-A', 'dead_code', '--diagnostic-width=140']


def prepare(receipt, feature_mode):
    version = subprocess.check_output([os.environ.get('RUSTC', 'rustc'), '-vV'], text=True)
    hosts = [line.removeprefix('host: ') for line in version.splitlines()
             if line.startswith('host: ')]
    if len(hosts) != 1 or not hosts[0]:
        raise ValueError('rustc did not identify one native host')
    host = hosts[0]
    # The nightly config command includes credentials in unrelated tables. Keep
    # its output only in memory and extract build.target; never forward stderr.
    query = subprocess.run(['cargo', '-Z', 'unstable-options', 'config', 'get', '--format=json'],
                           capture_output=True, text=True)
    if query.returncode:
        raise ValueError('Cannot query Cargo target selection; native UI needs nightly Cargo config JSON')
    try:
        configuration = json.loads(query.stdout)
        configured = configuration.get('build', {}).get('target')
    except (ValueError, AttributeError, TypeError):
        raise ValueError('Cargo configuration did not contain a valid target selection') from None
    selected = os.environ.get('CARGO_BUILD_TARGET') or configured or host
    if isinstance(selected, list):
        if len(selected) != 1:
            raise ValueError('Native UI requires one unambiguous Cargo target')
        selected = selected[0]
    if not isinstance(selected, str) or not selected:
        raise ValueError('Native UI requires a target triple')
    native = selected == host and not configured
    reason = ('native host with no configured target' if native else
              'cross-target selection retains the ordinary UI path' if selected != host else
              'Configured build.target suppresses host flags; remove it for the native UI stage')
    admission = dict(host=host, selected_target=selected, configured_target=configured,
                     native_ui=native, reason=reason)
    (receipt / 'native-ui-admission.json').write_text(json.dumps(admission, indent=2) + '\n')
    if not native:
        print('ordinary')
        return
    if feature_mode not in ('default', 'all'):
        raise ValueError('QUEST_UI_FEATURES must be default or all')
    if os.environ.get('TRYBUILD') == 'overwrite':
        raise ValueError('Native UI admission forbids overwriting expected diagnostics')
    encoded = os.environ.get('CARGO_ENCODED_RUSTFLAGS')
    flags = (encoded.split('\x1f') if encoded else []) if encoded is not None else \
        os.environ.get('RUSTFLAGS', '').split()
    flags += UI_FLAGS
    target = receipt / 'native-ui-target'
    profile = dict(schema_version=1, source_digest=os.environ['QUEST_SOURCE_DIGEST'],
                   host=host, selected_target=selected, feature_mode=feature_mode,
                   trybuild_version='1.0.121', rustflags=flags,
                   target_directory=str(target), base_profile='build-profile.json',
                   packages=['quest-compile', 'quest-rs'], test_binary='compile_fail',
                   filter_expression=UI_FILTER)
    (receipt / 'native-ui-profile.json').write_text(json.dumps(profile, indent=2) + '\n')
    (receipt / 'native-ui-rustflags').write_text('\x1f'.join(flags))
    print('native')


def counts(path):
    if not path.exists():
        return None
    text = path.read_text(errors='replace')
    summaries = re.findall(r'Summary \[\s*([0-9.]+)s\] (\d+) tests run: ([^\n]+)', text)
    if not summaries:
        return None
    seconds, total, tail = summaries[-1]
    result = dict(tests_run=int(total), elapsed_seconds=float(seconds))
    for field in ('passed', 'failed', 'skipped'):
        found = re.search(r'(\d+) ' + field, tail)
        result[field] = int(found[1]) if found else 0
    return result


def summarize(receipt):
    profile = receipt / 'native-ui-profile.json'
    workspace = (receipt / 'test-stage').read_text().strip() == 'workspace'
    result = dict(base=counts(receipt / 'tests.log'),
                  native_ui=counts(receipt / 'native-ui.log'),
                  base_filter_expression=f'not ({UI_FILTER})' if workspace and profile.exists() else None,
                  rerouted_binaries=['quest-compile::compile_fail', 'quest-rs::compile_fail']
                  if workspace and profile.exists() else [])
    result['base_tests_rerouted_to_native_ui'] = (
        result['native_ui']['tests_run'] if workspace and result['native_ui'] else 0)
    for name in ('nextest', 'native-ui', 'doctest'):
        status = receipt / (name + '-status')
        if status.exists():
            result[name + '_status'] = int(status.read_text())
    (receipt / 'test-routing.json').write_text(json.dumps(result, indent=2) + '\n')


if __name__ == '__main__':
    try:
        if sys.argv[1] == 'prepare':
            prepare(Path(sys.argv[2]), sys.argv[3])
        elif sys.argv[1] == 'summarize':
            summarize(Path(sys.argv[2]))
        else:
            raise ValueError('unknown native UI operation')
    except (ValueError, OSError, subprocess.SubprocessError) as error:
        print(f'Native UI admission: {error}', file=sys.stderr)
        sys.exit(2)
