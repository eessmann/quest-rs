from pathlib import Path
import importlib.util,json,hashlib
folder=Path('<private-artifacts>/quest-mpi-fsize-diagnostic')
path=Path('<workspace>/docs/verification/fixtures/persisted-weighted-transform/run.py')
s=importlib.util.spec_from_file_location('runner',path);r=importlib.util.module_from_spec(s);s.loader.exec_module(r)
job=r.collect(['/usr/lib64/mpich/bin/mpiexec','-n','8',str(folder/'init')],folder/'capture',seconds=30)
job['scope']='minimal MPI_Init_thread/Finalize only; no QuEST or consumer generation/synthesis/application'
job['runner_sha256']=hashlib.sha256(path.read_bytes()).hexdigest()
job['minimal_binary_sha256']=hashlib.sha256((folder/'init').read_bytes()).hexdigest()
(folder/'capture-receipt.json').write_text(json.dumps(job,indent=2)+'\n')
print(json.dumps(job))
if job['returncode']!=0 or job['timed_out'] or job['output_limit']:raise SystemExit(1)
