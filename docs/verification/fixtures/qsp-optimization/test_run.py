"""Lightweight controller regressions; no Cargo builds or benchmark workloads."""
import contextlib
import io
import json
import os
import pathlib
import signal
import subprocess
import sys
import tempfile
import time
import types
import unittest

import run

WORKLOADS = [('binary64', 256, 53), ('binary64', 1024, 53), ('offline', 16, 128),
             ('offline', 16, 256), ('offline', 256, 128), ('offline', 256, 256)]


def records():
    return [dict(kind=k, degree=d, bits=b, status='ok', export_fingerprint='1234567890abcdef', grid=2048)
            for k, d, b in WORKLOADS]


def variant(root, label, rows):
    folder = root / label
    (folder / 'source').mkdir(parents=True)
    (folder / 'package').mkdir()
    (folder / 'source' / 'lib.rs').write_text('// original\n')
    (folder / 'package' / 'Cargo.lock').write_text('# resolved\n')
    observer = folder / 'observer'
    observer.write_text('#!' + sys.executable + '\nprint(' + repr('\n'.join(map(json.dumps, rows))) + ')\n')
    observer.chmod(0o755)
    (folder / 'sources.json').write_text(json.dumps({'instrumented': run.inventory(folder/'source')}))
    (folder / 'binary.json').write_text(json.dumps({'sha256': run.sha(observer), 'bytes': observer.stat().st_size}))
    identity = {'source': run.inventory(folder/'source'), 'package': run.inventory(folder/'package'),
                'binary': run.sha(observer)}
    (folder/'build-identity.json').write_text(json.dumps(identity))
    return folder


class ControllerTests(unittest.TestCase):
    def test_records_home_target_specific_rustflags(self):
        with tempfile.TemporaryDirectory() as d:
            root=pathlib.Path(d)
            home=root/'cargo-home'
            home.mkdir()
            (home/'config.toml').write_text('[target.x86_64-unknown-linux-gnu]\nrustflags=["-C", "target-cpu=native"]\n')
            source=root/'source'
            source.mkdir()
            observed=run.cargo_configs(source, {'CARGO_HOME':str(home)})
            entry=next(v for v in observed if v['path']==str(home/'config.toml'))
            self.assertEqual(entry['tables']['target']['x86_64-unknown-linux-gnu']['rustflags'],
                             ['-C', 'target-cpu=native'])

    def test_timeout_kills_child_when_time_leader_exits(self):
        # Removing group escalation after leader exit must make this fail.
        with tempfile.TemporaryDirectory() as d:
            root = pathlib.Path(d)
            pidfile = root/'pid'
            code = ('import pathlib,os,signal,time; signal.signal(signal.SIGTERM,signal.SIG_IGN); '
                    f'pathlib.Path({str(pidfile)!r}).write_text(str(os.getpid())); time.sleep(20)')
            result = run.record_run(['/usr/bin/time', '-v', sys.executable, '-c', code], root,
                                    dict(os.environ), root/'probe', timeout=0.3)
            self.assertFalse(result)
            pid = int(pidfile.read_text())
            def running():
                try:
                    # An adopted zombie is dead and no longer consumes resources.
                    return pathlib.Path(f'/proc/{pid}/stat').read_text().split(') ')[1][0] != 'Z'
                except FileNotFoundError:
                    return False
            try:
                for _ in range(20):
                    if not running(): break
                    time.sleep(0.01)
                self.assertFalse(running(), 'observer survived process-group cleanup')
            finally:
                if running(): os.kill(pid, signal.SIGKILL)

    def test_tampered_build_input_is_rejected_before_trials(self):
        for target in ['source/lib.rs', 'package/Cargo.lock', 'observer']:
            with self.subTest(target=target), tempfile.TemporaryDirectory() as d:
                root = pathlib.Path(d)
                folder = variant(root, 'baseline', records())
                with (folder/target).open('a') as stream: stream.write('\n# changed\n')
                with contextlib.redirect_stdout(io.StringIO()), self.assertRaises(SystemExit):
                    run.run(types.SimpleNamespace(output=root, labels=['baseline']))
                self.assertFalse((folder/'trial-1.json').exists())

    def test_invalid_workload_records_fail_completion(self):
        for problem in ['missing', 'error', 'fingerprint', 'grid', 'duplicate']:
            with self.subTest(problem=problem), tempfile.TemporaryDirectory() as d:
                root = pathlib.Path(d)
                variant(root, 'baseline', records())
                bad = records()
                if problem == 'missing': bad.pop()
                elif problem == 'duplicate': bad.append(bad[0])
                elif problem == 'error': bad[0]['status'] = 'error'
                elif problem == 'fingerprint': bad[0]['export_fingerprint'] = '0000000000000000'
                elif problem == 'grid': bad[0]['grid'] = 4096
                variant(root, 'a', bad)
                with contextlib.redirect_stdout(io.StringIO()), self.assertRaises(SystemExit):
                    run.run(types.SimpleNamespace(output=root, labels=['baseline', 'a']))
                completion = json.loads((root/'measurement-completion.json').read_text())
                self.assertFalse(completion['workloads_valid'])

    def test_matching_trials_succeed(self):
        with tempfile.TemporaryDirectory() as d:
            root = pathlib.Path(d)
            for label in ['baseline', 'a']: variant(root, label, records())
            with contextlib.redirect_stdout(io.StringIO()):
                run.run(types.SimpleNamespace(output=root, labels=['baseline', 'a']))
            completion = json.loads((root/'measurement-completion.json').read_text())
            self.assertTrue(completion['workloads_valid'])
            self.assertEqual(completion['validated_workload_records'], 36)

    def test_trial_drift_and_mutation_fail_completion(self):
        for problem in ['grid-drift', 'source-mutation']:
            with self.subTest(problem=problem), tempfile.TemporaryDirectory() as d:
                root = pathlib.Path(d)
                folder = variant(root, 'baseline', records())
                script = ('#!' + sys.executable + '\nimport pathlib,json\n'
                          'counter=pathlib.Path("counter")\n'
                          'n=int(counter.read_text())+1 if counter.exists() else 1\n'
                          'counter.write_text(str(n))\n'
                          f'rows={records()!r}\n')
                if problem == 'grid-drift':
                    script += 'rows[0]["grid"] += n\n'
                else:
                    script += 'pathlib.Path("source/lib.rs").write_text("// changed during trial")\n'
                script += 'print("\\n".join(map(json.dumps, rows)))\n'
                (folder/'observer').write_text(script)
                binary=json.loads((folder/'binary.json').read_text())
                binary.update(sha256=run.sha(folder/'observer'),bytes=(folder/'observer').stat().st_size)
                (folder/'binary.json').write_text(json.dumps(binary))
                saved=json.loads((folder/'build-identity.json').read_text())
                saved['binary']=binary['sha256']
                (folder/'build-identity.json').write_text(json.dumps(saved))
                with contextlib.redirect_stdout(io.StringIO()), self.assertRaises(SystemExit):
                    run.run(types.SimpleNamespace(output=root, labels=['baseline']))
                completion=json.loads((root/'measurement-completion.json').read_text())
                self.assertFalse(completion['workloads_valid' if problem == 'grid-drift' else 'sources_unchanged'])


if __name__ == '__main__':
    unittest.main()
