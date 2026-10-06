from pathlib import Path
import hashlib,json,math,struct,types
R=Path('<workspace>'); T=Path('/tmp'); C=T/'quest-persisted-weighted-campaign-3'; D=C/'dataset'
artifacts=[]
def sha(b):return hashlib.sha256(b).hexdigest()
def raw(p,cap=4*1024*1024):
 with Path(p).open('rb') as f:b=f.read(cap+1)
 assert len(b)<=cap,(p,'over cap');return b
def record(p,cap=4*1024*1024):
 b=raw(p,cap);artifacts.append({'path':str(p),'bytes':len(b),'sha256':sha(b)});return b
def pairs(items):
 d={}
 for k,v in items:
  assert k not in d,('duplicate',k);d[k]=v
 return d
def decode(b):
 d=json.loads(b,object_pairs_hook=pairs,parse_constant=lambda x:(_ for _ in ()).throw(ValueError(x)))
 stack=[d]
 while stack:
  x=stack.pop()
  if isinstance(x,float):assert math.isfinite(x)
  elif isinstance(x,dict):stack.extend(x.values())
  elif isinstance(x,list):stack.extend(x)
 return d
def read(p,cap=4*1024*1024):return decode(record(p,cap))
def filehash(p):
 h=hashlib.sha256();n=0
 with Path(p).open('rb') as f:
  while b:=f.read(65536):h.update(b);n+=len(b)
 artifacts.append({'path':str(p),'bytes':n,'sha256':h.hexdigest()});return h.hexdigest()
execution=read(T/'quest-persisted-weighted-final-execution-3.json');build=read(T/'quest-persisted-weighted-final-build-3-attestation.json');receipt=read(C/'receipt.json')
assert sha(raw(C/'receipt.json'))==execution['receipt_sha256']
assert sha(raw(T/'quest-persisted-weighted-final-build-3-attestation.json'))==execution['build_attestation_sha256']==receipt['build_attestation_sha256']
assert build['status']=='stable-pinned' and build['build_exit_code']==0
assert execution['runner_exit_code']==0 and execution['status']=='one-campaign-outcome-retained'
assert receipt['status']==execution['campaign_status']=='incomplete-or-accuracy-failure'
assert receipt['final_provenance_errors']=={} and not receipt['supervisor_interrupted']
snapshots=[]
for name in ['build-3-before','build-3-after','execution-3-before','execution-3-after']:
 p=T/f'quest-persisted-weighted-final-{name}.json';b=record(p);snapshots.append(decode(b));assert sha(b)==execution['before_sha256']==execution['after_sha256']==build['before_sha256']==build['after_sha256']
assert all(s==snapshots[0] for s in snapshots)
snap=snapshots[0];source=snap['source'];assert len(source['files'])==975==build['source_files']
h=hashlib.sha256()
for entry in source['files']:h.update(entry['path'].encode()+b'\0'+bytes.fromhex(entry['sha256']))
assert h.hexdigest()==source['source_sha256']
for suffix in ['before','after']:
 p=T/f'quest-persisted-weighted-final-build-3-source-{suffix}.json';assert read(p)==source
 assert sha(raw(p))==build['source_manifest_sha256']
manifest_path=R/'.superpowers/sdd/2026-10-05-sparse-mathcore-cfd/persisted-weighted-transform-consumer-source-fingerprint-v2.json';manifest=read(manifest_path)
assert sha(raw(manifest_path))==execution['consumer_source_manifest_sha256']==build['consumer_source_manifest_sha256']==receipt['source_manifest_sha256']==snap['consumer_source_manifest_sha256']
source_map={x['path']:x['sha256'] for x in source['files']};assert len(source_map)==975
assert len(manifest['files'])==16
manifest_only=[]
for f in manifest['files']:
 if f['path'] in source_map:assert source_map[f['path']]==f['sha256']
 else:manifest_only.append(f['path'])
assert manifest_only==['docs/research/persisted-weighted-transform.md']
# The archived source map is authoritative; current moving source files are not checked.
for p in [T/'quest-persisted-weighted-final-binary-3',C/'binary']:
 assert filehash(p)==execution['binary_sha256']==build['binary_sha256']==receipt['binary_sha256']
assert sha(record(T/'quest-persisted-weighted-final-build-3-build.log'))==build['build_log_sha256']
compiler_raw=record(T/'quest-persisted-weighted-final-build-3-compiler-artifacts.jsonl');assert sha(compiler_raw)==build['compiler_artifacts_sha256']
compiler=[decode(line) for line in compiler_raw.splitlines()];locals_=sorted({str(Path(x['manifest_path']).relative_to(R)) for x in compiler if x.get('reason')=='compiler-artifact' and x.get('manifest_path','').startswith(str(R)+'/')})
assert locals_==build['actual_local_package_manifests'] and len(locals_)==16
example=[x for x in compiler if x.get('reason')=='compiler-artifact' and x['target']['name']=='persisted_weighted_transform'];assert len(example)==1
assert example[0]['profile']==build['profile'] and sorted(example[0]['features'])==build['features']
assert any(x.get('reason')=='build-finished' and x.get('success') is True for x in compiler)
assert sha(record(T/'quest-persisted-weighted-final-execution-3-runner.log'))==execution['runner_log_sha256']
for name,digest in receipt['immutable_inputs'].items():assert sha(record(D/name))==digest
assert len(receipt['immutable_inputs'])==29
freeze=read(D/'freeze.json');phasebytes=raw(D/'phases.json');phases=decode(phasebytes)
assert sha(phasebytes)==freeze['phase_sha256']
assert sha(b''.join(struct.pack('<d',x) for x in freeze['coefficients']))==freeze['coefficient_sha256']
assert freeze['degree']==33 and len(freeze['coefficients'])==len(phases['angles'])==34
assert phases['convention']=='pyqsp-wx-symmetric'
expected=[['publish',8],['compile',1],['replay',1],['replay',2],['replay',4],['replay',8],['split',4]]
assert receipt['requests']==expected and len(receipt['outcomes'])==7
rows=[];jobs=[]
for i,(o,req) in enumerate(zip(receipt['outcomes'],expected)):
 assert o['index']==i and [o['mode'],o['world_parts']]==req
 job=o['job'];assert job['caps']=={'address_space_bytes':2147483648,'process_file_size_limit':None,'read_chunk_bytes':65536,'seconds':180,'stderr_bytes':4194304,'stdout_bytes':4194304}
 for stream in ['stdout','stderr']:
  info=job[stream];b=record(C/info['path']);assert len(b)==info['bytes']==info['observed_bytes']<=4194304 and sha(b)==info['sha256'] and not info['truncated']
 assert not job['capture_incomplete'] and not job['driver_interrupted'] and not job['output_limit']
 if i>=3:
  assert o['status']=='timeout' and job['timed_out'] and job['returncode']==-9 and 180<=job['elapsed_seconds']<181
  assert job['stdout']['bytes']==job['stderr']['bytes']==0 and 'rows' not in o
 else:
  assert o['status']=='completed' and job['returncode']==0 and not job['timed_out']
  decoded=[]
  for line in raw(C/job['stdout']['path']).splitlines():
   index=decode(line);p=D/index['receipt'];b=record(p,131072);assert sha(b)==index['receipt_sha256'] and len(b)==index['receipt_bytes'];row=decode(b)
   assert row['world_rank']==index['world_rank'] and row['status']==index['status']=='completed';decoded.append(row)
  assert sorted(decoded,key=lambda x:x['world_rank'])==o['rows'];rows+=decoded
 jobs.append({'index':i,'mode':o['mode'],'world_parts':o['world_parts'],'status':o['status'],'elapsed_seconds':job['elapsed_seconds'],'stdout_bytes':job['stdout']['bytes'],'stderr_bytes':job['stderr']['bytes'],'job_stage_known':i<3})
assert len(rows)==10
# Use the exact historical, reviewed schema checker only if its bytes match the archived map.
runnerpath=R/'docs/verification/fixtures/persisted-weighted-transform/run.py';runner=raw(runnerpath);assert sha(runner)==source_map[str(runnerpath.relative_to(R))]==snap['runner_sha256']
ns={'__name__':'saved_only_review','__file__':str(runnerpath)};exec(compile(runner,str(runnerpath),'exec'),ns)
for o in receipt['outcomes'][:3]:
 assert ns['read_rows'](C/o['job']['stdout']['path'],D,o['mode'],o['world_parts'])==o['rows']
 for row in o['rows']:ns['validate_row'](row,o['mode'],o['world_parts'])
assert receipt['outcomes'][1]['rows'][0]['result']['freeze']==freeze
result=receipt['outcomes'][2]['rows'][0]['result'];assert result['freeze']==freeze
calls=result['calls'];assert [c['adjoint'] for c in calls]==[False,True,False]
metrics=[]
for c in calls:
 d=c['readout'];assert c['fresh_basis_rhs'] and c['status']=='accuracy-passed'
 assert d['relative_residual']<=.001 and d['relative_vector_error']<=.002 and abs(d['total_probability']-1)<=1e-10
 assert d['system_coordinates_visited']==32 and d['residual_operator']==('A†' if c['adjoint'] else 'A')
 assert c['routing']['source_queries']==66 and c['routing']['child_events']==198 and c['routing']['exact']
 assert c['routing']['routing']['sent_bytes']==c['routing']['routing']['received_bytes']==0
 assert math.isclose(d['recovered_norm_squared'],d['success_probability']*freeze['physical_rescaling']**2,rel_tol=1e-14)
 metrics.append({'index':c['index'],'adjoint':c['adjoint'],'residual':d['relative_residual'],'relative_vector_error':d['relative_vector_error'],'total_probability':d['total_probability'],'success_probability':d['success_probability'],'recovered_norm_squared':d['recovered_norm_squared'],'application_seconds':c['application']['seconds']})
assert calls[0]['readout']==calls[2]['readout']
assert result['cumulative_max_rank_work']==sum(c['admission']['max_rank_work'] for c in calls)<=result['fixed_declared_three_apply_work_ceiling']==24000000000
for c,comparison,rollup in zip(calls,receipt['outcomes'][2]['scalar_comparisons'],receipt['outcomes'][2]['routing_rollups'][0]['calls']):
 d=c['readout'];assert comparison['finite_polynomial_success_difference']==d['success_probability']-result['finite_polynomial_success_target'];assert comparison['reciprocal_ideal_success_difference']==d['success_probability']-result['reciprocal_ideal_success_target'];assert comparison['recovered_norm_squared_difference']==d['recovered_norm_squared']-32/13
 assert rollup['routing']==c['routing']['routing'] and rollup['source_queries']==c['routing']['source_queries'] and rollup['child_events']==c['routing']['child_events']
# Independent scalar recurrence and 2x2 Wx product from saved coefficients/phases only.
x=math.sqrt(26)/8/freeze['alpha'];a,b=0.,0.
for coefficient in freeze['coefficients'][:0:-1]:a,b=coefficient+2*x*a-b,a
poly=freeze['coefficients'][0]+x*a-b
assert abs(poly*poly-freeze['finite_polynomial_success'])<1e-15
assert abs((freeze['scale']/x)**2-freeze['reciprocal_ideal_success'])<1e-15
assert abs(1/(freeze['alpha']*freeze['scale'])-freeze['physical_rescaling'])<1e-13
sig=math.sqrt(1-x*x);u=[[complex(math.cos(phases['angles'][0]),math.sin(phases['angles'][0])),0j],[0j,complex(math.cos(phases['angles'][0]),-math.sin(phases['angles'][0]))]]
def mul(a,b):return [[sum(a[i][k]*b[k][j] for k in range(2)) for j in range(2)] for i in range(2)]
for phi in phases['angles'][1:]:
 e=complex(math.cos(phi),math.sin(phi));u=mul(u,[[x*e,1j*sig*e.conjugate()],[1j*sig*e,x*e.conjugate()]])
assert abs(u[0][0].imag-poly)<1e-11
predicted_error=abs(poly*x/freeze['scale']-1)
assert max(abs(m['residual']-predicted_error) for m in metrics)<1e-12
headers=[json.loads(raw(D/f'term-{i}/manifest.json')) for i in range(3)]
assert all(h['schema_version']==2 and h['construction']=='completed-matching-columns-v2' for h in headers)
assert len({h['header'][7] for h in headers})==3 and len({h['header'][9] for h in headers})==3
summary={'schema':'persisted-weighted-attempt3-independent-audit-v1','verdict':'Approved saved-artifact integrity and bounded P1 numerical evidence; overall campaign remains incomplete due to four timeouts.','read_only_saved_artifacts':True,'physical_or_native_runs':0,'source_builds':0,'current_tree_not_used_as_historical_identity':True,'build_source_files':975,'actual_compiler_local_packages':16,'historical_consumer_files':16,'consumer_paths_outside_compiler_source_scope':manifest_only,'immutable_dataset_files':29,'rank_receipts':10,'raw_streams':14,'before_after_snapshot_sha256':execution['before_sha256'],'binary_sha256':execution['binary_sha256'],'elapsed_wrapper_seconds':execution['elapsed_seconds'],'elapsed_driver_seconds':receipt['elapsed_seconds'],'outcomes':jobs,'p1_calls':metrics,'phase_scalar_reconstruction':{'x':x,'polynomial':poly,'wx_symmetric_imaginary_response':u[0][0].imag,'absolute_response_difference':abs(u[0][0].imag-poly),'predicted_relative_error':predicted_error},'source_identities':[h['header'][7] for h in headers],'record_digests':[h['header'][9] for h in headers],'timed_out_job_stage_unknown':True,'tag_mismatch_is_separate_source_diagnosis':True,'uniform_native_inverse_error_certificate':False,'multihost_capacity_acceptance':False,'cumulative_p1_work':result['cumulative_max_rank_work'],'artifacts':artifacts}
out=R/'.superpowers/sdd/2026-10-05-sparse-mathcore-cfd/persisted-weighted-attempt3-independent-audit.json';out.write_text(json.dumps(summary,indent=2,allow_nan=False)+'\n');print(json.dumps({k:v for k,v in summary.items() if k!='artifacts'},indent=2));print('AUDIT_SHA256',sha(out.read_bytes()))
