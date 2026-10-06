"""Focused strict protocol tests; synthetic rows never become execution evidence."""
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location("weighted_runner", Path(__file__).with_name("run.py"))
runner = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(runner)

class Contract(unittest.TestCase):
    def rejected(self):
        return {"schema": runner.ROW_SCHEMA, "mode": "replay", "world_rank": 0,
                "world_parts": 1, "rank": 0, "parts": 1, "status": "rejected",
                "phase": "load/bridge", "error": "missing manifest", "stages": {},
                "result": None, "fixed": runner.FIXED.copy(), "claims": runner.CLAIMS.copy(),
                "memory": {k: 1024 for k in runner.MEMORY}, "environment_accounted_bytes": 1048576}

    def test_strict_rejected_real_shape_and_claims(self):
        runner.validate_row(self.rejected(), "replay", 1)
        for key, value in [("status", "completed"), ("schema", "unknown"), ("error", None),
                           ("world_rank", True), ("memory", {}), ("result", {"status": "completed"})]:
            row = self.rejected(); row[key] = value
            with self.subTest(key=key), self.assertRaises(ValueError):
                runner.validate_row(row, "replay", 1)
        row = self.rejected(); row["claims"]["rigorous_inverse_certificate"] = True
        with self.assertRaises(ValueError): runner.validate_row(row, "replay", 1)

    def test_genuine_saved_early_rejection_receipts(self):
        folder=Path(__file__).with_name('fixtures')
        provenance=json.loads((folder/'provenance.json').read_text())
        for entry in provenance['fixtures']:
            path=folder/entry['file']
            self.assertEqual(runner.digest(path),entry['sha256'])
            row=runner.decode(path.read_bytes());runner.validate_row(row,'replay',1)
            self.assertEqual(row['status'],'rejected')
        row=runner.decode((folder/'loader-rejected.json').read_bytes())
        self.assertEqual(row['stages']['completed_children'],0)
        self.assertIn('No such file',row['error'])

    def test_genuine_completed_compile_allows_zero_single_rank_transport_only(self):
        import copy,hashlib
        folder=Path(__file__).with_name('fixtures')
        provenance=runner.decode((folder/'compile-completed-provenance.json').read_bytes())
        raw=runner.bounded_read(folder/provenance['file'],runner.RECEIPT_BYTES)
        self.assertEqual(len(raw),provenance['bytes'])
        self.assertEqual(hashlib.sha256(raw).hexdigest(),provenance['sha256'])
        row=runner.decode(raw)
        self.assertEqual(row['status'],'completed')
        self.assertEqual(row['parts'],1)
        self.assertTrue(all(c['application_payload_ceiling']==0 for c in row['stages']['children']))
        runner.validate_row(row,'compile',1)
        for value in [-1,0.0,False,True,8388609]:
            invalid=copy.deepcopy(row);invalid['stages']['children'][0]['application_payload_ceiling']=value
            with self.subTest(value=value),self.assertRaises(ValueError):runner.validate_row(invalid,'compile',1)
        invalid=copy.deepcopy(row);invalid['stages']['children'][0]['constructor_work']=0
        with self.assertRaises(ValueError):runner.validate_row(invalid,'compile',1)
        with self.assertRaises(ValueError):runner.children(row['stages'],2)

    def test_boolean_substitution_is_not_numeric_policy(self):
        for section,key in [('claims','rigorous_inverse_certificate'),('fixed','response')]:
            row=self.rejected();row[section][key]=0 if section=='claims' else True
            with self.assertRaises(ValueError):runner.validate_row(row,'replay',1)
        row=self.rejected();row['parts']=True
        with self.assertRaises(ValueError):runner.validate_row(row,'replay',1)

    def completed_replay(self):
        import hashlib, struct, math
        row=self.rejected();row['status']='completed';row['error']=None
        stage={'seconds':.1,'memory':row['memory'].copy()}
        header={'rows':32,'cols':32,'record_count':64,'num_colors':2,'color_qubits':1,'system_qubits':5,'beta':1.,'alpha':2.,'source_identity':1,'record_digest':2}
        children=[]
        for i in range(3):
            child={'term':i,'header':header.copy(),'load':stage,'bridge':stage}
            child.update({k:1 for k in ['returned_source_bytes','source_clone_bytes','actual_target_bytes','native_preparation_scratch_allowance','whole_rank_admitted_ceiling','planned_whole_rank_peak','whole_node_admitted_ceiling','constructor_work','application_payload_ceiling']})
            children.append(child)
        row['stages']={'children':children,'stage':stage,'normalization':1.25,'source_identity':4,'construction_identity':5}
        sigma=math.sqrt(26)/8;lo=math.nextafter(sigma,-math.inf);hi=math.nextafter(sigma,math.inf);scale=.2
        freeze={'schema':'quest-persisted-weighted-phase-freeze-v1','dimension':32,'max_degree':81,'reciprocal_tolerance':1e-4,'degree':1,'alpha':1.25,'scale':scale,'error_bound':1e-5,'certificate':None,'source_identity':4,'construction_identity':5,'coefficients':[0.,.5],'coefficient_sha256':hashlib.sha256(struct.pack('<dd',0.,.5)).hexdigest(),'phase_sha256':'0'*64,'spectrum_lower':lo,'spectrum_upper':hi,'physical_rescaling':1/(1.25*scale),'synthesis':{'algorithm':'InverseNlftDivideConquer','backend':'Scalar','max_completion_grid':16384,'response_tolerance':1e-11,'contractivity_margin':1e-12,'limits':{'length':16384,'bytes':33554432,'work':1000000000},'completion_grid':128,'completion_residual':1e-12,'reconstruction_residual':1e-12},'finite_polynomial_success':(.5*sigma/1.25)**2,'reciprocal_ideal_success':(1.25*scale/sigma)**2,'coefficient_rounding_bound':0.,'constructor_retained_polynomial_bytes':512,'spectral_evidence':'synthetic test shape only','phase_format':'existing QspInput::Symmetric JSON','construction':{}}
        calls=[]
        for i in range(3):
            calls.append({'index':i,'adjoint':i==1,'fresh_basis_rhs':True,'coherent_rhs_identity_gates':0,'application':stage,'readout_stage':stage,'status':'accuracy-passed','local_simulator_initialization':{'max_rank_work_ceiling':4000000,'stage':stage},'readout':{'system_coordinates_visited':32,'total_probability':1.,'success_probability':.3,'relative_vector_error':1e-4,'relative_residual':1e-4,'recovered_norm_squared':32/13},'routing':{'source_queries':2,'child_events':6,'exact':True,'routing':{k:1 for k in runner.ROUTING}},'admission':{k:1 for k in ['max_rank_work','aggregate_work','native_dispatches','aggregate_application_payload_ceiling','control_calls','preflight_work','managed_rank_peak_bytes']}})
        row['result']={'status':'completed','freeze':freeze,'calls':calls,'rigorous_inverse_certificate':None,'native_os_overhead_bytes':None,'cumulative_max_rank_work':3,'fixed_declared_three_apply_work_ceiling':24000000000,'phase_import':stage,'transform_construction':stage}
        return row

    def test_known_complete_shape_and_missing_required_execution_evidence(self):
        import copy
        row=self.completed_replay();runner.validate_row(row,'replay',1)
        edits=[lambda r:r['result'].update(calls=[]),lambda r:r['result']['calls'][0].pop('routing'),lambda r:r['result']['calls'][0].pop('application'),lambda r:r['result']['calls'][0].pop('local_simulator_initialization'),lambda r:r['result']['calls'][0]['routing'].update(source_queries=0),lambda r:r['result']['calls'][0]['readout'].update(relative_residual=-1),lambda r:r['result']['calls'][0]['readout'].update(success_probability=0),lambda r:r['result']['freeze'].update(finite_polynomial_success=1),lambda r:r['stages'].update(children=[])]
        for edit in edits:
            changed=copy.deepcopy(row);edit(changed)
            with self.assertRaises(ValueError):runner.validate_row(changed,'replay',1)

    def test_optional_uncomputed_candidate_diagnostic_stays_unknown(self):
        row=self.completed_replay();row['result']['freeze']['synthesis']['reconstruction_residual']=None
        runner.validate_row(row,'replay',1)
        row['result']['freeze']['synthesis'].pop('reconstruction_residual')
        with self.assertRaises(ValueError):runner.validate_row(row,'replay',1)

    def test_bounded_snapshot_and_terminal_provenance_failure(self):
        with tempfile.TemporaryDirectory() as folder:
            path=Path(folder)/'huge';path.write_bytes(b'x'*1025)
            with self.assertRaises(ValueError):runner.bounded_read(path,1024)
            path.unlink()
            with self.assertRaises(OSError):runner.bounded_read(path,1024)
            report={'status':'completed','outcomes':[{'status':'completed'}]}
            runner.final_provenance(report,lambda:runner.bounded_read(path,1024),lambda:True)
            self.assertEqual(report['status'],'provenance-failure')
            self.assertIsNone(report['source_unchanged_after'])
            runner.atomic(Path(folder)/'receipt.json',report)
            self.assertEqual(json.loads((Path(folder)/'receipt.json').read_text())['outcomes'][0]['status'],'completed')

    def test_finite_norm_drift_retains_typed_accuracy_failure(self):
        row=self.completed_replay();row['status']='accuracy-failure';row['result']['status']='accuracy-failure'
        row['result']['calls'][0]['status']='accuracy-failure';row['result']['calls'][0]['readout']['total_probability']=1.001
        runner.validate_row(row,'replay',1)
        row['status']='completed';row['result']['status']='completed'
        with self.assertRaises(ValueError):runner.validate_row(row,'replay',1)

    def test_typed_nested_objects_and_underflow_become_admission_errors(self):
        import copy
        original=self.completed_replay()
        for edit in [lambda r:r['stages']['children'].__setitem__(0,[]),lambda r:r['result']['calls'].__setitem__(0,[]),lambda r:r['result']['freeze'].update(alpha=1e-300,scale=1e-300)]:
            row=copy.deepcopy(original);edit(row)
            with self.assertRaises(ValueError):runner.validate_row(row,'replay',1)
        row=copy.deepcopy(original);row['status']='accuracy-failure';row['result']['status']='accuracy-failure';row['result']['calls'][1]=[]
        with self.assertRaises(ValueError):runner.validate_row(row,'replay',1)
        with tempfile.TemporaryDirectory() as folder:
            folder=Path(folder);receipt=folder/'receipt-replay-world1-rank0.json';receipt.write_bytes(b'[]')
            index={'schema':runner.INDEX_SCHEMA,'mode':'replay','world_parts':1,'world_rank':0,'receipt':receipt.name,'receipt_bytes':2,'receipt_sha256':runner.digest(receipt)}
            stdout=folder/'stdout';stdout.write_text(json.dumps(index)+'\n')
            with self.assertRaises(ValueError):runner.read_rows(stdout,folder,'replay',1)

    def test_full_driver_malformed_prerequisite_retains_all_seven_requests(self):
        import hashlib, os, subprocess, sys
        with tempfile.TemporaryDirectory() as folder:
            folder=Path(folder);launcher=folder/'mpiexec';launcher.write_text('#!/bin/sh\nshift\nshift\nexec "$@"\n');launcher.chmod(0o700)
            binary=folder/'fake';binary.write_text('#!/usr/bin/env python3\nprint('+repr('{"status":1e999}')+')\n');binary.chmod(0o700)
            manifest=folder/'source.json';manifest.write_text(json.dumps({'schema':'quest-persisted-weighted-consumer-source-v1','files':[{'path':'fake','bytes':binary.stat().st_size,'sha256':runner.digest(binary)}]}))
            attestation=folder/'build.json';attestation.write_text(json.dumps({'binary_sha256':runner.digest(binary),'consumer_source_manifest_sha256':runner.digest(manifest),'scope':'synthetic malformed-child test only'}))
            out=folder/'out';env=os.environ.copy();env['PATH']=str(folder)+os.pathsep+env['PATH']
            child=subprocess.run([sys.executable,str(Path(__file__).with_name('run.py')),'--binary',str(binary),'--binary-sha256',runner.digest(binary),'--source-manifest',str(manifest),'--repository',str(folder),'--build-attestation',str(attestation),'--output-directory',str(out)],env=env,capture_output=True,timeout=10,check=False)
            self.assertEqual(child.returncode,0,child.stderr)
            receipt=json.loads((out/'receipt.json').read_text());self.assertEqual(len(receipt['outcomes']),7)
            self.assertEqual(receipt['outcomes'][0]['status'],'malformed-or-admission-failure')
            self.assertTrue(all(row['status']=='not-run' for row in receipt['outcomes'][1:]))

    def test_capture_limits_do_not_apply_to_child_transport_files(self):
        import sys
        with tempfile.TemporaryDirectory() as folder:
            folder=Path(folder);transport=folder/'transport'
            command=[sys.executable,'-c','import os,sys;f=os.open(sys.argv[1],os.O_CREAT|os.O_RDWR,0o600);os.ftruncate(f,4337664);os.close(f);print("initialized")',str(transport)]
            job=runner.collect(command,folder/'job',seconds=5)
            self.assertEqual(job['returncode'],0)
            self.assertEqual(transport.stat().st_size,4337664)
            self.assertEqual((folder/'job.stdout').read_bytes(),b'initialized\n')
            self.assertFalse(job['output_limit'])

    def test_capture_exact_boundaries_drain_both_streams_without_buffering_them(self):
        import sys
        with tempfile.TemporaryDirectory() as folder:
            folder=Path(folder)
            command=[sys.executable,'-c','import os;chunk=b"a"*65536\nfor _ in range(64):\n os.write(1,chunk);os.write(2,chunk)']
            job=runner.collect(command,folder/'job',seconds=5)
            self.assertEqual(job['returncode'],0)
            self.assertFalse(job['output_limit'])
            for name in ['stdout','stderr']:
                self.assertEqual(job[name]['bytes'],runner.OUTPUT_BYTES)
                self.assertFalse(job[name]['truncated'])

    def test_capture_overflow_kills_descendant_and_retains_bounded_prefix(self):
        import sys
        with tempfile.TemporaryDirectory() as folder:
            folder=Path(folder)
            code='import os,time\npid=os.fork()\nif pid==0:time.sleep(60)\nelse:\n print(pid,flush=True)\n chunk=b"x"*65536\n for _ in range(65):os.write(2,chunk)\n time.sleep(60)'
            job=runner.collect([sys.executable,'-c',code],folder/'job',seconds=5)
            self.assertTrue(job['output_limit'])
            self.assertFalse(job['timed_out'])
            self.assertEqual(job['stderr']['bytes'],runner.OUTPUT_BYTES)
            self.assertTrue(job['stderr']['truncated'])
            self.assertLess(job['returncode'],0)
            descendant=int((folder/'job.stdout').read_text().strip())
            try:state=Path(f'/proc/{descendant}/stat').read_text().split(') ',1)[1].split()[0]
            except FileNotFoundError:state='gone'
            self.assertIn(state,('Z','gone'))
            self.assertIsNone(runner.ACTIVE_CHILD)

    def test_capture_stdout_overrun_is_explicit_even_if_child_finishes(self):
        import sys
        with tempfile.TemporaryDirectory() as folder:
            folder=Path(folder)
            job=runner.collect([sys.executable,'-c','import os;chunk=b"x"*65536\nfor _ in range(65):os.write(1,chunk)'],folder/'job',seconds=5)
            self.assertTrue(job['output_limit'])
            self.assertEqual(job['stdout']['bytes'],runner.OUTPUT_BYTES)
            self.assertTrue(job['stdout']['truncated'])
            self.assertGreater(job['stdout']['observed_bytes'],runner.OUTPUT_BYTES)

    def test_capture_setup_error_kills_and_reaps_registered_child(self):
        import sys
        original_selector=runner.selectors.DefaultSelector;original_popen=runner.subprocess.Popen
        children=[]
        class BrokenRegistration:
            def __init__(self):self.inner=original_selector();self.calls=0
            def register(self,*args):
                self.calls+=1
                if self.calls==2:raise OSError('injected registration failure')
                return self.inner.register(*args)
            def get_map(self):return self.inner.get_map()
            def close(self):self.inner.close()
        def spawn(*args,**kwargs):
            child=original_popen(*args,**kwargs);children.append(child);return child
        try:
            runner.selectors.DefaultSelector=BrokenRegistration;runner.subprocess.Popen=spawn
            with tempfile.TemporaryDirectory() as folder, self.assertRaises(OSError):
                runner.collect([sys.executable,'-c','import time;time.sleep(60)'],Path(folder)/'job',seconds=5)
            self.assertEqual(len(children),1)
            self.assertLess(children[0].returncode,0)
            self.assertIsNone(runner.ACTIVE_CHILD)
        finally:
            runner.selectors.DefaultSelector=original_selector;runner.subprocess.Popen=original_popen

    def test_capture_escaped_descendant_reports_incomplete_eof_and_closes_pipes(self):
        import os,signal,sys
        with tempfile.TemporaryDirectory() as folder:
            folder=Path(folder);descendant=None
            code='import os,time\npid=os.fork()\nif pid==0:\n os.setsid();time.sleep(60)\nelse:print(pid,flush=True)'
            try:
                job=runner.collect([sys.executable,'-c',code],folder/'job',seconds=.15)
                descendant=int((folder/'job.stdout').read_text().strip())
                self.assertTrue(job['timed_out'])
                self.assertTrue(job['capture_incomplete'])
                self.assertTrue(job['stdout']['truncated'])
                self.assertIsNone(runner.ACTIVE_CHILD)
                self.assertLess(job['elapsed_seconds'],5)
            finally:
                if descendant is None and (folder/'job.stdout').exists():
                    text=(folder/'job.stdout').read_text().strip()
                    if text:descendant=int(text)
                if descendant is not None:
                    try:os.kill(descendant,signal.SIGKILL)
                    except ProcessLookupError:pass

    def test_full_driver_output_limit_retains_all_seven_requests(self):
        import os,subprocess,sys
        with tempfile.TemporaryDirectory() as folder:
            folder=Path(folder);launcher=folder/'mpiexec';launcher.write_text('#!/bin/sh\nshift\nshift\nexec "$@"\n');launcher.chmod(0o700)
            binary=folder/'fake';binary.write_text('#!/usr/bin/env python3\nimport os\nchunk=b"x"*65536\nfor _ in range(65):os.write(1,chunk)\n');binary.chmod(0o700)
            manifest=folder/'source.json';manifest.write_text(json.dumps({'schema':'quest-persisted-weighted-consumer-source-v1','files':[{'path':'fake','bytes':binary.stat().st_size,'sha256':runner.digest(binary)}]}))
            attestation=folder/'build.json';attestation.write_text(json.dumps({'binary_sha256':runner.digest(binary),'consumer_source_manifest_sha256':runner.digest(manifest)}))
            out=folder/'out';env=os.environ.copy();env['PATH']=str(folder)+os.pathsep+env['PATH']
            child=subprocess.run([sys.executable,str(Path(__file__).with_name('run.py')),'--binary',str(binary),'--binary-sha256',runner.digest(binary),'--source-manifest',str(manifest),'--repository',str(folder),'--build-attestation',str(attestation),'--output-directory',str(out)],env=env,capture_output=True,timeout=10,check=False)
            self.assertEqual(child.returncode,0,child.stderr)
            receipt=json.loads((out/'receipt.json').read_text())
            self.assertEqual(len(receipt['outcomes']),7)
            self.assertEqual(receipt['outcomes'][0]['status'],'output-limit')
            self.assertTrue(receipt['outcomes'][0]['job']['stdout']['truncated'])
            self.assertTrue(all(row['status']=='not-run' for row in receipt['outcomes'][1:]))

    def test_capture_job_timeout_preserves_prefix_and_reaps_child(self):
        import sys
        with tempfile.TemporaryDirectory() as folder:
            folder=Path(folder)
            job=runner.collect([sys.executable,'-c','import time;print("progress",flush=True);time.sleep(60)'],folder/'job',seconds=.15)
            self.assertTrue(job['timed_out'])
            self.assertFalse(job['output_limit'])
            self.assertLess(job['returncode'],0)
            self.assertEqual((folder/'job.stdout').read_bytes(),b'progress\n')
            self.assertIsNone(runner.ACTIVE_CHILD)

    def test_external_supervisor_kills_registered_child_group_and_preserves_capture(self):
        import os, subprocess, sys, time
        with tempfile.TemporaryDirectory() as folder:
            folder=Path(folder);child=folder/'child.py'
            child.write_text('import os,time\npid=os.fork()\nif pid==0: time.sleep(60)\nelse:\n print(pid,flush=True)\n time.sleep(60)\n')
            helper=folder/'supervisor.py';runner_path=Path(__file__).with_name('run.py').resolve()
            helper.write_text('import importlib.util,sys\nfrom pathlib import Path\ns=importlib.util.spec_from_file_location("r",'+repr(str(runner_path))+')\nr=importlib.util.module_from_spec(s);s.loader.exec_module(r)\nr.install_supervisor()\np=Path(sys.argv[1])\nreceipt={"outcomes":[{"status":"prior-checkpoint"}]}\nr.atomic(p/"receipt.json",receipt)\njob=r.collect([sys.executable,str(p/"child.py")],p/"job",seconds=30)\nreceipt["outcomes"].append(job)\nr.atomic(p/"receipt.json",receipt)\n')
            supervised=subprocess.run(['timeout','--kill-after=2s','1s',sys.executable,str(helper),str(folder)],capture_output=True,timeout=5,check=False)
            self.assertEqual(supervised.returncode,124,supervised.stderr)
            receipt=json.loads((folder/'receipt.json').read_text())
            self.assertEqual(receipt['outcomes'][0]['status'],'prior-checkpoint')
            self.assertTrue(receipt['outcomes'][1]['driver_interrupted'])
            self.assertTrue(receipt['outcomes'][1]['timed_out'])
            descendant=int((folder/'job.stdout').read_text().strip())
            try:state=Path(f'/proc/{descendant}/stat').read_text().split(') ',1)[1].split()[0]
            except FileNotFoundError:state='gone'
            self.assertIn(state,('Z','gone'),'descendant process still runs after supervisor timeout')

    def test_term_at_receipt_boundary_retains_all_outcomes_without_more_children(self):
        import subprocess, sys
        with tempfile.TemporaryDirectory() as folder:
            folder=Path(folder);helper=folder/'boundary.py'
            runner_path=Path(__file__).with_name('run.py').resolve()
            helper.write_text('import importlib.util,json,os,signal,sys\nfrom pathlib import Path\ns=importlib.util.spec_from_file_location("r",'+repr(str(runner_path))+')\nr=importlib.util.module_from_spec(s);s.loader.exec_module(r)\np=Path(sys.argv[1])\nb=p/"binary";b.write_bytes(b"fake never executed")\nm=p/"source.json";m.write_text("{}")\na=p/"build.json";a.write_text(json.dumps({"binary_sha256":"0"*64,"consumer_source_manifest_sha256":"0"*64}))\nr.source_check=lambda *_:"0"*64\nr.digest=lambda _:"0"*64\ncounts=[]\ndef collect(*args,**kwargs):\n counts.append(1)\n return {"timed_out":True,"returncode":-9}\nr.collect=collect\nold=r.atomic\ninterrupted=[]\ndef boundary(path,value):\n if not interrupted:\n  interrupted.append(1);os.kill(os.getpid(),signal.SIGTERM)\n old(path,value)\nr.atomic=boundary\nsys.argv=["run.py","--binary",str(b),"--binary-sha256","0"*64,"--source-manifest",str(m),"--repository",str(p),"--output-directory",str(p/"out"),"--build-attestation",str(a)]\nr.main()\n(p/"count.json").write_text(json.dumps(len(counts)))\n')
            child=subprocess.run([sys.executable,str(helper),str(folder)],capture_output=True,timeout=5,check=False)
            self.assertEqual(child.returncode,0,child.stderr)
            receipt=json.loads((folder/'out/receipt.json').read_text())
            self.assertEqual(receipt['status'],'driver-timeout')
            self.assertEqual(len(receipt['outcomes']),7)
            self.assertEqual(receipt['outcomes'][0]['status'],'timeout')
            self.assertTrue(all(row['status']=='not-run' for row in receipt['outcomes'][1:]))
            self.assertEqual(json.loads((folder/'count.json').read_text()),1)

    def test_pending_term_during_spawn_registration_is_caught_and_child_reaped(self):
        import subprocess, sys
        with tempfile.TemporaryDirectory() as folder:
            folder=Path(folder);helper=folder/'pending.py'
            runner_path=Path(__file__).with_name('run.py').resolve()
            helper.write_text('import importlib.util,os,signal,sys\nfrom pathlib import Path\ns=importlib.util.spec_from_file_location("r",'+repr(str(runner_path))+')\nr=importlib.util.module_from_spec(s);s.loader.exec_module(r)\nr.install_supervisor()\noriginal=r.subprocess.Popen\nchildren=[]\ndef spawn(*args,**kwargs):\n p=original(*args,**kwargs)\n children.append(p)\n os.kill(os.getpid(),signal.SIGTERM)\n return p\nr.subprocess.Popen=spawn\np=Path(sys.argv[1])\njob=r.collect([sys.executable,"-c","import time;time.sleep(60)"],p/"pending",seconds=30)\njob["registration_cleared"]=r.ACTIVE_CHILD is None\njob["direct_child_reaped"]=children[0].returncode is not None\nr.atomic(p/"receipt.json",job)\n')
            child=subprocess.run([sys.executable,str(helper),str(folder)],capture_output=True,timeout=5,check=False)
            self.assertEqual(child.returncode,0,child.stderr)
            receipt=json.loads((folder/'receipt.json').read_text())
            self.assertTrue(receipt['driver_interrupted'])
            self.assertTrue(receipt['registration_cleared'])
            self.assertTrue(receipt['direct_child_reaped'])
            self.assertLess(receipt['returncode'],0)

    def test_parser_bounds_duplicates_nonfinite_and_nesting(self):
        for text in ['{"x":1,"x":2}', '{"x":NaN}', '{"x":1e999}', '['*80+'0'+']'*80,
                     ' '* (runner.RECEIPT_BYTES+1)]:
            with self.subTest(text=text[:50]), self.assertRaises(ValueError): runner.decode(text.encode())
        self.assertEqual(runner.decode(b'{"finite":1}'), {"finite": 1})

    def test_fixed_seven_requests_and_no_parameter_tuning(self):
        self.assertEqual(runner.REQUESTS, [("publish",8),("compile",1),("replay",1),
                        ("replay",2),("replay",4),("replay",8),("split",4)])
        self.assertEqual(runner.JOB_SECONDS,180)
        self.assertEqual(runner.AS_BYTES,2147483648)
        self.assertEqual(runner.OUTPUT_BYTES,4194304)

    def test_partial_prefix_and_geometry_of_rank_coverage(self):
        row=self.rejected();row["stages"]={"children":[],"completed_children":0,"attempt_term":0}
        runner.validate_row(row,"replay",1)
        row["stages"]["completed_children"]=3
        with self.assertRaises(ValueError):runner.validate_row(row,"replay",1)
        rows=[self.rejected(),self.rejected()]
        with self.assertRaises(ValueError):runner.validate_coverage(rows,"replay",2)

    def test_telemetry_checked_rollup_and_missing_current_receipts(self):
        router={k:1 for k in runner.ROUTING}
        receipts=[{"source_queries":2,"child_events":6,"exact":True,"routing":router.copy()} for _ in range(2)]
        merged=runner.rollup(receipts)
        self.assertEqual(merged["routing"]["sent_bytes"],2)
        self.assertEqual(merged["routing"]["maximum_batch_pairs"],1)
        receipts[0]["routing"]["sent_bytes"]=runner.U64_MAX
        self.assertFalse(runner.rollup(receipts)["exact"])
        with self.assertRaises(ValueError):runner.rollup([None])

    def test_subprocess_malformed_outputs_preserve_valid_failure(self):
        with tempfile.TemporaryDirectory() as root:
            root=Path(root)
            child=root/'bad.py';child.write_text('#!/usr/bin/env python3\nprint("{\\\"status\\\":1e999}")\n')
            child.chmod(0o700)
            saved=runner.collect([str(child)],root/'job',seconds=3)
            self.assertEqual(saved['returncode'],0)
            with self.assertRaises(ValueError):runner.decode((root/'job.stdout').read_bytes())
            runner.atomic(root/'saved.json',{'status':'malformed','job':saved})
            self.assertEqual(json.loads((root/'saved.json').read_text())['status'],'malformed')

if __name__=='__main__': unittest.main()
