#!/usr/bin/env python3
"""Exercise the real coordinator and launch scripts with unavailable tools stubbed."""
import json
from pathlib import Path
import subprocess
import unittest

import test_snapshot as fixture_support

HERE = fixture_support.HERE


class RuntimeCoordinator(unittest.TestCase):
    def setUp(self):
        self.fixture = fixture_support.SnapshotAdmission(methodName='runTest')
        self.fixture.setUp()
        self.addCleanup(self.fixture.doCleanups)
        for name in ('test-rust.sh', 'runtime-env.sh', 'native_ui.py'):
            path = self.fixture.root / 'docs/verification/fixtures/cirrus' / name
            path.write_text((HERE / name).read_text() if (HERE / name).exists()
                            else '#!/bin/bash\nexit 88\n')
            path.chmod(0o755)
        self.fixture.git('add', '.')
        self.source, self.digest = self.fixture.extract()
        self.environment = self.fixture.build_environment(self.source)
        self.environment['QUEST_COMPILER'] = 'gnu'
        (self.fixture.base / 'bin/rustc').write_text(
            '#!/bin/sh\nprintf "rustc fixture\nhost: x86_64-unknown-linux-gnu\n"\n')
        self.environment['SLURM_JOB_NUM_NODES'] = '8'
        self.environment['SLURM_NTASKS_PER_NODE'] = '99'
        self.environment['SLURM_TASKS_PER_NODE'] = '99(x8)'
        build = self.fixture.run_build(self.environment)
        self.assertEqual(build.returncode, 0, build.stderr)
        marker, = (self.fixture.base / 'campaign').rglob('build-complete')
        self.environment['QUEST_BUILD_RECEIPT'] = str(marker.parent)
        self.environment['SLURM_CPUS_PER_TASK'] = '288'
        for directory in ('native/gnu', 'native/cray', f'targets/cray/{self.digest}'):
            (self.fixture.base / 'campaign' / directory).mkdir(parents=True, exist_ok=True)
        self.script = self.source / 'docs/verification/fixtures/cirrus/test-rust.sh'

    def run_coordinator(self, script=None):
        return subprocess.run(['bash', '--noprofile', '--norc', str(script or self.script)],
                              env=self.environment, capture_output=True, text=True)

    def test_spooled_coordinator_uses_verified_build_and_shared_tmpdir(self):
        capture = self.fixture.base / 'coordinator.jsonl'
        cargo = self.fixture.base / 'bin/cargo'
        cargo.write_text('#!/usr/bin/env python3\nimport json,os,sys\n'
                         f'open({str(capture)!r},"a").write(json.dumps(dict('
                         'args=sys.argv[1:], env=dict(os.environ)))+"\\n")\n')
        spool = self.fixture.base / 'spool/slurm_script'
        spool.parent.mkdir()
        spool.write_text(self.script.read_text())
        result = self.run_coordinator(spool)
        self.assertEqual(result.returncode, 0, result.stderr)
        received, doctests = [json.loads(line) for line in capture.read_text().splitlines()]
        self.assertEqual(received['args'], ['nextest', 'run', '--locked', '--offline', '--workspace',
                                           '--all-features', '--test-threads=1', '--no-fail-fast',
                                           '--success-output=immediate'])
        self.assertEqual(doctests['args'], ['test', '--locked', '--offline', '--workspace',
                                          '--all-features', '--doc', '--', '--test-threads=1'])
        environment = received['env']
        self.assertEqual(environment['QUEST_MPI_LAUNCHER'], 'slurm')
        self.assertEqual(environment['QUEST_MPI_LAUNCHER_EXECUTABLE'], str(self.script))
        self.assertEqual(environment['CARGO_TARGET_DIR'],
                         str(self.fixture.base / 'campaign/targets/gnu' / self.digest))
        self.assertEqual(environment['OMP_NUM_THREADS'], '288')
        self.assertEqual(environment['OMP_PLACES'], 'cores')
        self.assertEqual(environment['CARGO_BUILD_JOBS'], '8')
        self.assertTrue(Path(environment['TMPDIR']).is_relative_to(self.fixture.base / 'campaign'))
        completion, = (self.fixture.base / 'campaign').rglob('tests-complete')
        self.assertEqual(completion.read_text().strip(), self.digest)

    def select_cray(self):
        self.environment['QUEST_COMPILER'] = 'cray'
        result = self.fixture.run_build(self.environment)
        self.assertEqual(result.returncode, 0, result.stderr)
        marker, = (self.fixture.base / 'campaign/receipts/cray').rglob('build-complete')
        self.environment['QUEST_BUILD_RECEIPT'] = str(marker.parent)

    def capture_ui_commands(self, config_target=None, ui_status=0):
        capture = self.fixture.base / 'ui-commands.jsonl'
        self.environment['QUEST_TEST_UI_CAPTURE'] = str(capture)
        self.environment['QUEST_TEST_CONFIG_TARGET'] = json.dumps(config_target)
        self.environment['QUEST_TEST_UI_STATUS'] = str(ui_status)
        (self.fixture.base / 'bin/cargo').write_text("#!/usr/bin/env python3\nimport json,os,sys\nargs=sys.argv[1:]\nif 'config' in args:\n    target=json.loads(os.environ['QUEST_TEST_CONFIG_TARGET'])\n    print(json.dumps({'build':{'target':target}} if target is not None else {}))\n    sys.exit(0)\nwith open(os.environ['QUEST_TEST_UI_CAPTURE'],'a') as output:\n    output.write(json.dumps({'args':args,'env':dict(os.environ)})+'\\n')\nif args[:2] == ['nextest','run']:\n    ui='--test' in args and 'compile_fail' in args\n    status=int(os.environ['QUEST_TEST_UI_STATUS']) if ui else 0\n    print('Summary [ 1.000s] 3 tests run: 3 passed, 0 skipped' if ui else\n          'Summary [ 2.000s] 9 tests run: 9 passed, 3 skipped')\n    sys.exit(status)\n")
        return capture

    def test_cray_workspace_routes_ui_with_native_flags_and_unchanged_assertions(self):
        self.environment['CARGO_ENCODED_RUSTFLAGS'] = '--cfg\x1fargument="with spaces"'
        self.select_cray()
        capture = self.capture_ui_commands()
        result = self.run_coordinator()
        self.assertEqual(result.returncode, 0, result.stderr)
        base, ui, docs = [json.loads(line) for line in capture.read_text().splitlines()]
        self.assertIn('-E', base['args'])
        self.assertEqual(base['args'][base['args'].index('-E')+1],
                         'not (binary(=compile_fail) and (package(=quest-compile) or package(=quest-rs)))')
        self.assertEqual(ui['args'], ['nextest','run','--locked','--offline','-p','quest-compile',
                         '-p','quest-rs','--all-features','--test','compile_fail','--test-threads=1',
                         '--no-fail-fast','--success-output=immediate'])
        flags=ui['env']['CARGO_ENCODED_RUSTFLAGS'].split('\x1f')
        self.assertEqual(flags[:2], ['--cfg','argument="with spaces"'])
        self.assertEqual(flags[-8:], ['--cfg','trybuild_no_target','--cfg','trybuild',
                         '--verbose','-A','dead_code','--diagnostic-width=140'])
        self.assertNotEqual(ui['env']['CARGO_TARGET_DIR'], base['env']['CARGO_TARGET_DIR'])
        self.assertEqual(docs['env']['CARGO_TARGET_DIR'], base['env']['CARGO_TARGET_DIR'])
        self.assertNotIn('TRYBUILD',ui['env'])
        receipt, = (self.fixture.base/'campaign').rglob('native-ui-status')
        self.assertEqual(receipt.read_text().strip(),'0')
        counts=json.loads((receipt.parent/'test-routing.json').read_text())
        self.assertEqual(counts['native_ui']['tests_run'],3)
        self.assertEqual(counts['base']['skipped'],3)

    def test_native_ui_stage_default_features_and_failure_are_explicit(self):
        self.select_cray()
        self.environment.update(QUEST_TEST_STAGE='native-ui',QUEST_UI_FEATURES='default',
                                SLURM_JOB_NUM_NODES='1',CARGO_BUILD_TARGET='x86_64-unknown-linux-gnu')
        capture=self.capture_ui_commands(ui_status=33)
        result=self.run_coordinator()
        self.assertEqual(result.returncode,33,result.stderr)
        ui,=[json.loads(line) for line in capture.read_text().splitlines()]
        self.assertNotIn('--all-features',ui['args'])
        self.assertNotIn('CARGO_BUILD_TARGET',ui['env'])
        self.assertFalse(list((self.fixture.base/'campaign').rglob('tests-complete')))
        status,=(self.fixture.base/'campaign').rglob('native-ui-status')
        self.assertEqual(status.read_text().strip(),'33')
        counts=json.loads((status.parent/'test-routing.json').read_text())
        self.assertIsNone(counts['base_filter_expression'])
        self.assertEqual(counts['base_tests_rerouted_to_native_ui'],0)
        self.assertEqual(counts['native_ui']['tests_run'],3)

    def test_ui_failure_still_runs_doctests_and_blocks_workspace_completion(self):
        self.select_cray()
        capture=self.capture_ui_commands(ui_status=33)
        result=self.run_coordinator()
        self.assertEqual(result.returncode,33,result.stderr)
        base,ui,docs=[json.loads(line) for line in capture.read_text().splitlines()]
        self.assertIn('--doc',docs['args'])
        self.assertFalse(list((self.fixture.base/'campaign').rglob('tests-complete')))
        status,=(self.fixture.base/'campaign').rglob('native-ui-status')
        self.assertEqual((status.parent/'doctest-status').read_text().strip(),'0')

    def test_native_ui_rejects_diagnostic_overwrite(self):
        self.select_cray()
        self.environment.update(QUEST_TEST_STAGE='native-ui', TRYBUILD='overwrite')
        capture=self.capture_ui_commands()
        result=self.run_coordinator()
        self.assertNotEqual(result.returncode,0)
        self.assertIn('forbids overwriting',result.stderr)
        self.assertFalse(capture.exists())

    def test_cross_target_keeps_original_path_and_native_ui_rejects_it(self):
        self.select_cray()
        capture=self.capture_ui_commands(config_target='aarch64-unknown-linux-gnu')
        result=self.run_coordinator()
        self.assertEqual(result.returncode,0,result.stderr)
        base,docs=[json.loads(line) for line in capture.read_text().splitlines()]
        self.assertNotIn('-E',base['args'])
        self.environment['QUEST_TEST_STAGE']='native-ui'
        result=self.run_coordinator()
        self.assertNotEqual(result.returncode,0)
        self.assertEqual(len(capture.read_text().splitlines()),2)

    def test_ambiguous_or_configured_host_target_cannot_admit_native_ui(self):
        self.select_cray()
        self.environment['QUEST_TEST_STAGE']='native-ui'
        for target in (['x86_64-unknown-linux-gnu','aarch64-unknown-linux-gnu'],
                       'x86_64-unknown-linux-gnu'):
            with self.subTest(target=target):
                capture=self.capture_ui_commands(config_target=target)
                result=self.run_coordinator()
                self.assertNotEqual(result.returncode,0)
                self.assertFalse(capture.exists())

    def test_wrong_build_digest_prevents_any_test_execution(self):
        (Path(self.environment['QUEST_BUILD_RECEIPT']) / 'build-complete').write_text('wrong\n')
        log = self.fixture.base / 'cargo.log'
        before = log.read_text()
        self.assertNotEqual(self.run_coordinator().returncode, 0)
        self.assertEqual(log.read_text(), before)
        self.assertEqual(list((self.fixture.base / 'campaign').rglob('tests-complete')), [])

    def test_doctests_run_and_keep_their_status_after_nextest_failure(self):
        capture = self.fixture.base / 'failed-nextest.jsonl'
        cargo = self.fixture.base / 'bin/cargo'
        cargo.write_text('#!/usr/bin/env python3\nimport json,sys\n'
                         f'open({str(capture)!r},"a").write(json.dumps(sys.argv[1:])+"\\n")\n'
                         'sys.exit(33 if sys.argv[1] == "nextest" else 0)\n')
        result = self.run_coordinator()
        self.assertEqual(result.returncode, 33)
        first, second = [json.loads(line) for line in capture.read_text().splitlines()]
        self.assertEqual(first[:2], ['nextest', 'run'])
        self.assertIn('--doc', second)
        status, = (self.fixture.base / 'campaign').rglob('nextest-status')
        self.assertEqual(status.read_text().strip(), '33')
        self.assertEqual((status.parent / 'doctest-status').read_text().strip(), '0')
        self.assertFalse((status.parent / 'tests-complete').exists())

    def test_build_receipt_cannot_select_a_different_native_installation(self):
        (Path(self.environment['QUEST_BUILD_RECEIPT']) / 'native-prefix').write_text('/wrong/native\n')
        log = self.fixture.base / 'cargo.log'
        before = log.read_text()
        self.assertNotEqual(self.run_coordinator().returncode, 0)
        self.assertEqual(log.read_text(), before)

    def test_test_failure_and_source_tampering_never_publish_completion(self):
        for field in ('QUEST_TEST_CARGO_STATUS', 'QUEST_TEST_TAMPER'):
            with self.subTest(field=field):
                self.environment[field] = '1'
                self.assertNotEqual(self.run_coordinator().returncode, 0)
                self.assertEqual(list((self.fixture.base / 'campaign').rglob('tests-complete')), [])
                self.environment.pop(field)

    def test_step_uses_one_rank_per_node_and_allocated_physical_cores(self):
        capture = self.fixture.base / 'srun.json'
        srun = self.fixture.base / 'bin/srun'
        srun.write_text('#!/usr/bin/env python3\nimport json,os,sys\n'
                        f'open({str(capture)!r},"w").write(json.dumps(dict('
                        'args=sys.argv[1:], env=dict(os.environ))))\n')
        srun.chmod(0o755)
        for allocated in (2, 8):
            for ranks in (1, 2, 4, 8):
                with self.subTest(allocated=allocated, ranks=ranks):
                    self.environment['SLURM_JOB_NUM_NODES'] = str(allocated)
                    result = subprocess.run([str(self.script), '--mpi-step', f'--ntasks={ranks}',
                                             '/bin/echo', 'argument with spaces'],
                                            env=self.environment, capture_output=True, text=True)
                    if ranks > allocated:
                        self.assertNotEqual(result.returncode, 0)
                        continue
                    self.assertEqual(result.returncode, 0, result.stderr)
                    received = json.loads(capture.read_text())
                    self.assertIn(f'--nodes={ranks}', received['args'])
                    self.assertIn(f'--ntasks={ranks}', received['args'])
                    for option in ('--kill-on-bad-exit=1', '--hint=nomultithread',
                                   '--cpu-bind=cores', '--cpus-per-task=288',
                                   '--distribution=block:block', '--ntasks-per-node=1', '--exclusive'):
                        self.assertIn(option, received['args'])
                    self.assertEqual(received['args'][-2:], ['/bin/echo', 'argument with spaces'])
                    self.assertNotIn('SLURM_NTASKS_PER_NODE', received['env'])
                    self.assertNotIn('SLURM_TASKS_PER_NODE', received['env'])
                    self.assertEqual(received['env']['SRUN_CPUS_PER_TASK'], '288')

    def test_small_allocation_requires_explicit_smoke_stage(self):
        self.environment['SLURM_JOB_NUM_NODES'] = '2'
        log = self.fixture.base / 'cargo.log'
        before = log.read_text()
        self.assertNotEqual(self.run_coordinator().returncode, 0)
        self.assertEqual(log.read_text(), before)
        self.environment['QUEST_TEST_STAGE'] = 'smoke'
        capture = self.fixture.base / 'smoke.jsonl'
        cargo = self.fixture.base / 'bin/cargo'
        cargo.write_text('#!/usr/bin/env python3\nimport json,sys\n'
                         f'open({str(capture)!r},"a").write(json.dumps(sys.argv[1:])+"\\n")\n')
        result = self.run_coordinator()
        self.assertEqual(result.returncode, 0, result.stderr)
        commands = [json.loads(line) for line in capture.read_text().splitlines()]
        self.assertEqual(len(commands), 5)
        self.assertTrue(all(args[:2] == ['nextest', 'run'] and '-E' in args
                            and '--workspace' not in args for args in commands))
        self.assertIn('test(=mpi_collective_preflight_rejects_mismatch_on_every_rank)', commands[2])
        stage, = (self.fixture.base / 'campaign').rglob('test-stage')
        self.assertEqual(stage.read_text().strip(), 'smoke')

    def test_cirrus_batch_rank_zero_is_admitted_without_removing_markers(self):
        self.environment.update(SLURM_PROCID='0', SLURM_LOCALID='0', SLURM_NODEID='0', SLURM_JOBID='42')
        capture = self.fixture.base / 'batch-environment.json'
        cargo = self.fixture.base / 'bin/cargo'
        cargo.write_text('#!/usr/bin/env python3\nimport json,os\n'
                         f'open({str(capture)!r},"w").write(json.dumps(dict(os.environ)))\n')
        for steps in ({}, {'SLURM_STEP_ID': 'batch'}, {'SLURM_STEPID': 'batch'},
                      {'SLURM_STEP_ID': 'batch', 'SLURM_STEPID': 'batch'}):
            with self.subTest(steps=steps):
                self.environment.pop('SLURM_STEP_ID', None)
                self.environment.pop('SLURM_STEPID', None)
                self.environment.update(steps)
                result = self.run_coordinator()
                self.assertEqual(result.returncode, 0, result.stderr)
                observed = json.loads(capture.read_text())
                self.assertEqual(observed['SLURM_PROCID'], '0')
                self.assertEqual(observed['SLURM_LOCALID'], '0')
                self.assertEqual(observed['SLURM_NODEID'], '0')
                for key, value in steps.items():
                    self.assertEqual(observed[key], value)

    def test_coordinator_rejects_step_ids_and_mpi_ranks(self):
        baseline = dict(self.environment)
        for markers in ({'SLURM_PROCID': '0', 'SLURM_STEP_ID': '0'},
                        {'SLURM_PROCID': '0', 'SLURM_STEPID': '5'},
                        {'SLURM_PROCID': '0', 'SLURM_STEP_ID': 'batch', 'SLURM_STEPID': '5'},
                        {'SLURM_STEP_ID': '0'}, {'SLURM_PROCID': '1'},
                        {'SLURM_LOCALID': '0'}, {'SLURM_STEP_ID': 'batch'},
                        {'SLURM_PROCID': '0', 'SLURM_LOCALID': '1'},
                        {'SLURM_PROCID': '0', 'SLURM_NODEID': '1'},
                        {'SLURM_PROCID': '0', 'SLURM_JOBID': '43'},
                        {'SLURM_PROCID': '0', 'SLURM_STEP_ID': ''},
                        {'SLURM_PROCID': '0', 'PMI_RANK': '0'},
                        {'SLURM_PROCID': '0', 'QUEST_MPI_SUPERVISED_CHILD': '1'}):
            with self.subTest(markers=markers):
                self.environment = dict(baseline, **markers)
                self.assertNotEqual(self.run_coordinator().returncode, 0)
        self.assertEqual(list((self.fixture.base / 'campaign').rglob('tests-complete')), [])


if __name__ == '__main__':
    unittest.main()
