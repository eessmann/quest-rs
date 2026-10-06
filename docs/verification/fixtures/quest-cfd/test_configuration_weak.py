"""Strict fixed-protocol parser and negative process regressions."""
import importlib.util
from pathlib import Path
import unittest
import copy
import json
import subprocess
import sys
import tempfile
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("weak_runner", Path(__file__).with_name("configuration_weak.py"))
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)

class Protocol(unittest.TestCase):
    def fixture(self,name='complete'):
        return runner.decode_report(Path(__file__).with_name('configuration_weak_fixtures').joinpath(name+'.json').read_text())

    def test_genuine_three_outcomes(self):
        for name,index,status in [('complete',0,'completed'),('sampling',3,'sampling-rejected'),('work',4,'diagnostic-rejected'),('grid_failure',0,'construction-rejected')]:
            row=self.fixture(name)
            self.assertEqual(runner.validate(row,runner.ROWS[index])['status'],status)

    def test_impossible_complete_receipts_reject(self):
        cases=[(('initial','maximum_coefficient_probability'),0.),
               (('source','resources','external_bytes'),0),
               (('diagnostic_resources','result_bytes'),0),
               (('input_resources','statistics_capacity_bytes'),0)]
        for path,value in cases:
            row=self.fixture();owner=row
            for key in path[:-1]:owner=owner[key]
            owner[path[-1]]=value
            with self.subTest(path=path),self.assertRaises(ValueError):
                runner.validate(row,runner.ROWS[0])

    def test_whole_live_receipts_cannot_lose_earlier_phases(self):
        for case in range(3):
            row=self.fixture()
            if case==0:
                row['source']['resources']['constructor_peak_bytes']=1
                row['source']['resources']['peak_bytes']=1
            elif case==1:row['diagnostic_resources']['retained_source_bytes']=1
            else:row['input_resources']['peak_bytes']=1
            with self.subTest(case=case),self.assertRaises(ValueError):
                runner.validate(row,runner.ROWS[0])

    def prefix(self,phase):
        row=self.fixture();row.update(status='construction-rejected',phase=phase,error='injected test failure',initial=None,diagnostic=None,validated_rows=0,visited_rows=0)
        ir=row['input_resources'];ir['statistics_capacity_bytes']=0
        if phase!='input diagnostics':ir['state_capacity_bytes']=0;ir['declared_query_extra_bytes']=0
        if phase=='grid':
            row.update(represented_grid=None,sampling=None,source=None,diagnostic_resources=None)
            for k in ('grid_bytes','metadata_capacity_bytes','support_capacity_bytes','declared_source_external_bytes'):ir[k]=0
            ir['peak_bytes']=196608
        elif phase=='source construction':row['source']=None;row['diagnostic_resources']=None
        else:row['diagnostic_resources']=copy.deepcopy(row['source']['resources'])
        return row

    def test_explicit_failed_phase_prefixes_and_partial_contraction(self):
        for phase in ('grid','source construction','state construction','input diagnostics'):
            row=self.prefix(phase)
            self.assertEqual(runner.validate(row,runner.ROWS[0])['status'],'construction-rejected')
            row['visited_rows']=1
            with self.assertRaises(ValueError):runner.validate(row,runner.ROWS[0])
        row=self.fixture();row.update(status='numerical-failure',phase='contraction',error='injected failure',diagnostic=None,visited_rows=3)
        row['diagnostic_resources']['physical_calls_attempted']=64
        self.assertEqual(runner.validate(row,runner.ROWS[0])['status'],'numerical-failure')
        row['diagnostic_resources']['physical_calls_attempted']=60
        with self.assertRaises(ValueError):runner.validate(row,runner.ROWS[0])
        row=self.fixture();row.update(status='diagnostic-rejected',phase='state validation',error='injected failure',diagnostic=None,validated_rows=15,visited_rows=0)
        r=row['diagnostic_resources'];r.update(physical_calls=58,physical_calls_attempted=58,physical_work=5800000,supported_rows=0)
        self.assertEqual(runner.validate(row,runner.ROWS[0])['status'],'diagnostic-rejected')
        row['visited_rows']=1
        with self.assertRaises(ValueError):runner.validate(row,runner.ROWS[0])

    def test_every_numeric_leaf_requires_numeric_not_bool_or_string(self):
        original=self.fixture()
        paths=[]
        def visit(value,path=()):
            if type(value) in (int,float):paths.append(path)
            elif type(value) is dict:
                for key,item in value.items():visit(item,path+(key,))
            elif type(value) is list:
                for key,item in enumerate(value):visit(item,path+(key,))
        visit(original)
        for path in paths:
            for value in ('corrupt',True):
                row=copy.deepcopy(original);owner=row
                for key in path[:-1]:owner=owner[key]
                owner[path[-1]]=value
                with self.subTest(path=path,value=value),self.assertRaises((ValueError,TypeError)):
                    runner.validate(row,runner.ROWS[0])

    def test_parser_and_identity_failures_cannot_enter_comparisons(self):
        for text in ('{"x":1,"x":2}','{"x":NaN}','{"x":1e999}','['*1100+'0'+']'*1100):
            with self.assertRaises(ValueError):runner.decode_report(text)
        row=self.fixture(); row['initial']['ensemble_sha256']='not-a-hash'
        with self.assertRaises(ValueError):runner.validate(row,runner.ROWS[0])
        rows={runner.ROWS[0]:self.fixture(),runner.ROWS[4]:self.fixture('work')}
        self.assertTrue(all(x['status']=='unavailable' for x in runner.comparisons(rows)))
        bad=self.fixture('sampling');bad['source']={}
        with self.assertRaises(ValueError):runner.validate(bad,runner.ROWS[3])

    def test_fake_malformed_children_keep_all_seven_requests(self):
        with tempfile.TemporaryDirectory() as folder:
            binary=Path(folder)/'fake';output=Path(folder)/'result'
            binary.write_text("#!/usr/bin/python3\nprint('{\"status\":1e999}')\n");binary.chmod(0o700)
            run=subprocess.run([sys.executable,'-B',runner.__file__,str(binary),str(output)],capture_output=True,timeout=10)
            self.assertEqual(run.returncode,0,run.stderr.decode())
            receipt=json.loads((output/'receipt.json').read_text())
            self.assertEqual([r['row_id'] for r in receipt['rows']],list(runner.ROWS))
            self.assertTrue(all(not r['validated'] for r in receipt['rows']))

    def test_bounded_timeout_and_output_cap_preserve_process_failure(self):
        for program,timeout,expected in [("import time; time.sleep(5)",.03,'timeout'),("print('x'*5000000)",5,'exited')]:
            with tempfile.TemporaryDirectory() as folder:
                binary=Path(folder)/'fake';binary.write_text('#!/usr/bin/python3\n'+program+'\n');binary.chmod(0o700)
                row,parsed=runner.collect(binary,runner.ROWS[0],Path(folder),timeout=timeout)
                self.assertEqual(row['process_status'],expected);self.assertIsNone(parsed);self.assertFalse(row['validated'])
                self.assertLessEqual((Path(folder)/row['stdout']).stat().st_size,runner.FILE_BYTES)

    def test_malformed_timing_is_unavailable_but_raw_identity_retained(self):
        for timing in ('0.1 -2\n','corrupt 123\n','inf 123\n','0.1 0\n'):
            payload=json.dumps(self.fixture()).encode()
            class FakeChild:
                def __init__(self,args,stdout,**kwargs):
                    stdout.write(payload)
                    Path(args[args.index('-o')+1]).write_text(timing)
                def wait(self,timeout):return 0
            with tempfile.TemporaryDirectory() as folder,patch.object(runner.subprocess,'Popen',FakeChild):
                row,parsed=runner.collect(Path('fake'),runner.ROWS[0],Path(folder))
                self.assertIsNone(row['peak_rss_kib']);self.assertIsNone(row['child_elapsed_seconds'])
                self.assertEqual(row['timing_sha256'],runner.sha256(Path(folder)/row['timing']))
                self.assertIsNotNone(parsed)

    def test_first_physical_callback_failure_is_numerical_without_visited_rows(self):
        row=self.fixture();row.update(status='numerical-failure',phase='contraction',error='injected failure',diagnostic=None,visited_rows=0)
        row['diagnostic_resources']['physical_calls_attempted']=59
        self.assertEqual(runner.validate(row,runner.ROWS[0])['status'],'numerical-failure')
        row['status']='diagnostic-rejected'
        with self.assertRaises(ValueError):runner.validate(row,runner.ROWS[0])

    def test_exact_finite_seven_row_matrix(self):
        self.assertEqual(len(runner.ROWS), 7)
        self.assertEqual(len(set(runner.ROWS)), 7)
        with self.assertRaises(ValueError):
            runner.validate({"status": "completed"}, runner.ROWS[0])

if __name__ == "__main__":
    unittest.main()
