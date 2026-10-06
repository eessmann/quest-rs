#!/usr/bin/env python3
"""Capacity coordinator admission and receipt tests without cluster execution."""
import json
from pathlib import Path
import shlex
import subprocess
import unittest

import test_snapshot as fixture_support

HERE = fixture_support.HERE


class CapacityCoordinator(unittest.TestCase):
    def test_batch_interpreter_initializes_login_environment(self):
        interpreter = shlex.split((HERE / 'capacity-job.sh').read_text().splitlines()[0][2:])
        result = subprocess.run([interpreter[0], '--noprofile', '--norc', *interpreter[1:], '-c',
                                 'shopt -q login_shell'], capture_output=True, text=True)
        self.assertEqual(result.returncode, 0,
                         'Slurm must initialize modules without inherited shell functions')

    def setUp(self):
        self.fixture = fixture_support.SnapshotAdmission(methodName='runTest')
        self.fixture.setUp()
        self.addCleanup(self.fixture.doCleanups)
        helpers = self.fixture.root / 'docs/verification/fixtures/cirrus'
        (helpers / 'capacity-job.sh').write_text((HERE / 'capacity-job.sh').read_text()
            if (HERE / 'capacity-job.sh').exists() else '#!/bin/bash\nexit 88\n')
        (helpers / 'capacity.py').write_text('''import argparse,hashlib,json,os,pathlib,sys
parser=argparse.ArgumentParser()
parser.add_argument('--output',type=pathlib.Path)
parser.add_argument('--executable',type=pathlib.Path)
args,rest=parser.parse_known_args()
args.output.mkdir()
(args.output/'invocation.json').write_text(json.dumps(dict(args=sys.argv[1:],env=dict(os.environ))))
if os.environ.get('QUEST_TEST_CAPACITY_FAIL'): sys.exit(17)
(args.output/'completion.json').write_text(json.dumps(dict(complete=True,capacity_closed=False,
    executable_sha256=hashlib.sha256(args.executable.read_bytes()).hexdigest())))
if os.environ.get('QUEST_TEST_CAPACITY_TAMPER'):
    (pathlib.Path(os.environ['QUEST_SNAPSHOT'])/'input.rs').write_text('changed during capacity')
''')
        self.fixture.git('add', '.')
        self.source, self.digest = self.fixture.extract()
        self.environment = self.fixture.build_environment(self.source)
        self.environment['SLURM_JOB_NUM_NODES'] = '8'
        build = self.fixture.run_build(self.environment)
        self.assertEqual(build.returncode, 0, build.stderr)
        receipt, = (self.fixture.base / 'campaign').rglob('build-complete')
        self.environment['QUEST_BUILD_RECEIPT'] = str(receipt.parent)
        self.environment['SLURM_CPUS_PER_TASK'] = '288'
        self.executable = self.fixture.base / 'campaign/targets/cray' / self.digest / 'debug/examples/sparse_capacity'
        self.executable.parent.mkdir(parents=True)
        self.executable.write_text('#!/bin/sh\nexit 0\n')
        self.executable.chmod(0o755)
        (self.fixture.base / 'campaign/native/cray').mkdir(parents=True)
        self.script = self.source / 'docs/verification/fixtures/cirrus/capacity-job.sh'

    def run_campaign(self, script=None):
        return subprocess.run(['bash', '--noprofile', '--norc', str(script or self.script)],
                              env=self.environment, capture_output=True, text=True)

    def test_spooled_capacity_job_records_parameters_without_claiming_capacity_closed(self):
        self.environment.update(QUEST_CAPACITY_START_DIMENSION='1048576',
                                QUEST_CAPACITY_MAX_DIMENSION='1048576',
                                QUEST_CAPACITY_MAX_CASES='1')
        spool = self.fixture.base / 'spool/slurm_script'
        spool.parent.mkdir()
        spool.write_text(self.script.read_text())
        result = self.run_campaign(spool)
        self.assertEqual(result.returncode, 0, result.stderr)
        completion, = (self.fixture.base / 'campaign').rglob('completion.json')
        self.assertIs(json.loads(completion.read_text())['capacity_closed'], False)
        invocation = json.loads((completion.parent / 'invocation.json').read_text())
        values = dict(zip(invocation['args'][::2], invocation['args'][1::2]))
        for key, value in {'--nodes':'8', '--ranks-per-node':'1', '--threads':'288',
                           '--start-dimension':'1048576', '--max-dimension':'1048576',
                           '--process-as-mib':'8192', '--model-rank-mib':'4096',
                           '--executable':str(self.executable)}.items():
            self.assertEqual(values[key], value)
        self.assertEqual(invocation['env']['PYTHONDONTWRITEBYTECODE'], '1')
        self.assertEqual(invocation['env']['SRUN_CPUS_PER_TASK'], '288')
        marker, = (self.fixture.base / 'campaign').rglob('capacity-run-complete')
        self.assertEqual(marker.read_text().strip(), self.digest)

    def test_failed_or_tampered_campaign_never_publishes_success(self):
        for variable in ('QUEST_TEST_CAPACITY_FAIL', 'QUEST_TEST_CAPACITY_TAMPER'):
            self.environment[variable] = '1'
            self.assertNotEqual(self.run_campaign().returncode, 0)
            self.assertEqual(list((self.fixture.base / 'campaign').rglob('capacity-run-complete')), [])
            self.environment.pop(variable)

    def test_wrong_build_receipt_stops_before_capacity_execution(self):
        (Path(self.environment['QUEST_BUILD_RECEIPT']) / 'build-complete').write_text('wrong\n')
        self.assertNotEqual(self.run_campaign().returncode, 0)
        self.assertEqual(list((self.fixture.base / 'campaign').rglob('invocation.json')), [])

    def test_cirrus_batch_rank_zero_is_admitted_without_removing_markers(self):
        self.environment.update(SLURM_PROCID='0', SLURM_LOCALID='0', SLURM_NODEID='0', SLURM_JOBID='42')
        for steps in ({}, {'SLURM_STEP_ID': 'batch'}, {'SLURM_STEPID': 'batch'},
                      {'SLURM_STEP_ID': 'batch', 'SLURM_STEPID': 'batch'}):
            with self.subTest(steps=steps):
                self.environment.pop('SLURM_STEP_ID', None)
                self.environment.pop('SLURM_STEPID', None)
                self.environment.update(steps)
                before = set((self.fixture.base / 'campaign').rglob('artifacts/invocation.json'))
                result = self.run_campaign()
                self.assertEqual(result.returncode, 0, result.stderr)
                invocation, = set((self.fixture.base / 'campaign').rglob('artifacts/invocation.json')) - before
                observed = json.loads(invocation.read_text())['env']
                for marker in ('SLURM_PROCID', 'SLURM_LOCALID', 'SLURM_NODEID'):
                    self.assertEqual(observed[marker], '0')
                for key, value in steps.items():
                    self.assertEqual(observed[key], value)

    def test_coordinator_rejects_step_ids_and_mpi_ranks(self):
        baseline = dict(self.environment)
        for markers in ({'SLURM_PROCID': '0', 'SLURM_STEP_ID': '0'},
                        {'SLURM_PROCID': '0', 'SLURM_STEPID': '5'},
                        {'SLURM_PROCID': '0', 'SLURM_STEP_ID': 'batch', 'SLURM_STEPID': '5'},
                        {'SLURM_STEPID': '0'}, {'SLURM_PROCID': '1'},
                        {'SLURM_NODEID': '0'}, {'SLURM_STEPID': 'batch'},
                        {'SLURM_PROCID': '0', 'SLURM_LOCALID': '1'},
                        {'SLURM_PROCID': '0', 'SLURM_NODEID': '1'},
                        {'SLURM_PROCID': '0', 'SLURM_JOBID': '43'},
                        {'SLURM_PROCID': '0', 'SLURM_STEP_ID': ''},
                        {'SLURM_PROCID': '0', 'PMI_RANK': '0'},
                        {'SLURM_PROCID': '0', 'QUEST_MPI_SUPERVISED_CHILD': '1'}):
            with self.subTest(markers=markers):
                self.environment = dict(baseline, **markers)
                self.assertNotEqual(self.run_campaign().returncode, 0)
        self.assertEqual(list((self.fixture.base / 'campaign').rglob('capacity-run-complete')), [])


if __name__ == '__main__':
    unittest.main()
