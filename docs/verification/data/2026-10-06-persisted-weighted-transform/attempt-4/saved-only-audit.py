from pathlib import Path
import hashlib,json,math,stat,struct,runpy
ROOT=Path.cwd(); P=Path('/tmp'); OUT=P/'quest-persisted-weighted-campaign-4'
def sha(p):
 h=hashlib.sha256()
 with Path(p).open('rb')as f:
  for x in iter(lambda:f.read(65536),b''):h.update(x)
 return h.hexdigest()
def pairs(xs):
 d={}
 for k,v in xs:
  assert k not in d,('duplicate',k)
  d[k]=v
 return d
def finite(s):
 v=float(s); assert math.isfinite(v);return v
def dec(s):return json.loads(s,object_pairs_hook=pairs,parse_float=finite,parse_constant=lambda s:(_ for _ in()).throw(ValueError(s)))
def load(p):return dec(Path(p).read_text())
b=load(P/'quest-persisted-weighted-final-build-4-attestation.json');e=load(P/'quest-persisted-weighted-final-execution-4.json');r=load(OUT/'receipt.json')
paths=[P/f'quest-persisted-weighted-final-{stage}-4-{side}.json'for stage in ['build','execution']for side in ['before','after']];snaps=[load(p)for p in paths];assert all(s==snaps[0]for s in snaps);s=snaps[0]
assert all(sha(p)==b['before_sha256']==b['after_sha256']==e['before_sha256']==e['after_sha256']=='9dcf3171035b7a13bc0804ff33959175fbbe939032b771d23f4552dbe6d9c1b8'for p in paths)
files=s['source']['files'];assert len(files)==975==b['source_files'];assert len({x['path']for x in files})==975;h=hashlib.sha256()
current_snapshot_deltas=[]
for x in files:
 if sha(ROOT/x['path'])!=x['sha256']:current_snapshot_deltas.append(x['path'])
 h.update(x['path'].encode()+b'\0'+bytes.fromhex(x['sha256']))
assert set(current_snapshot_deltas)<={'crates/quest-cfd/NEXT_STEPS.md','crates/quest-cfd/README.md'}, current_snapshot_deltas
assert h.hexdigest()==s['source']['source_sha256']
for side in ['before','after']:assert load(P/f'quest-persisted-weighted-final-build-4-source-{side}.json')==s['source']
assert sha(P/'quest-persisted-weighted-final-build-4-source-before.json')==b['source_manifest_sha256']
for x in s['native']:assert sha(x['path'])==x['sha256']
for x in s['tools']:assert sha(x['resolved_command_executable'])==x['command_executable_sha256']
mfile=ROOT/'.superpowers/sdd/2026-10-05-sparse-mathcore-cfd/persisted-weighted-transform-consumer-source-readout-fix.json';m=load(mfile)
assert sha(mfile)==s['consumer_source_manifest_sha256']==b['consumer_source_manifest_sha256']==e['consumer_source_manifest_sha256']==r['source_manifest_sha256']=='dc69f1e3a848b6ebf368f6b0ac2a09e4573e5601cae0b06452ea38b352b8963c'
for x in m['files']:assert sha(ROOT/x['path'])==x['sha256']and(ROOT/x['path']).stat().st_size==x['bytes']
for p,k in [(P/'quest-persisted-weighted-build-execute-4.py','private_wrapper_sha256'),(P/'quest-programme-checkpoint-08-identity.py','source_identity_helper_sha256'),ROOT/'docs/verification/fixtures/persisted-weighted-transform/run.py'and(ROOT/'docs/verification/fixtures/persisted-weighted-transform/run.py','runner_sha256')]:assert sha(p)==s[k]
for suffix,k in [('compiler-artifacts.jsonl','compiler_artifacts_sha256'),('build.log','build_log_sha256')]:assert sha(P/f'quest-persisted-weighted-final-build-4-{suffix}')==b[k]
assert sha(P/'quest-persisted-weighted-final-build-4-attestation.json')==e['build_attestation_sha256']==r['build_attestation_sha256']=='fd59fe4164397f1abccdae411e292f2cd63897ce1c62ee8fc10dc1660748b3ec'
a=[dec(x)for x in(P/'quest-persisted-weighted-final-build-4-compiler-artifacts.jsonl').read_text().splitlines()];local=sorted({str(Path(x['manifest_path']).relative_to(ROOT))for x in a if x.get('reason')=='compiler-artifact'and Path(x['manifest_path']).is_relative_to(ROOT)});assert len(local)==16 and local==b['actual_local_package_manifests'];assert a[-1]=={'reason':'build-finished','success':True}
selected=[x for x in a if x.get('reason')=='compiler-artifact'and x['target']['name']=='persisted_weighted_transform'and x.get('executable')];assert len(selected)==1;selected=selected[0];assert selected['profile']==b['profile']and selected['features']==b['features']
identity='7acd0d38dc30a81aa299ad5704a28d7c2e0acec00f0d4a4a5f20f584759dcab9';binary=P/'quest-persisted-weighted-final-binary-4';assert sha(binary)==sha(OUT/'binary')==b['binary_sha256']==e['binary_sha256']==r['binary_sha256']==identity
assert stat.S_IMODE(binary.stat().st_mode)==0o555 and stat.S_IMODE((OUT/'binary').stat().st_mode)==0o500
assert sha(OUT/'receipt.json')==e['receipt_sha256']=='a9fb283e26c948dc2c2e5ff6871559fae968692173b54981515cc90fd19ea89f';assert sha(P/'quest-persisted-weighted-final-execution-4-runner.log')==e['runner_log_sha256']
requests=[['publish',8],['compile',1],['replay',1],['replay',2],['replay',4],['replay',8],['split',4]];assert r['requests']==requests and len(r['outcomes'])==7;assert e['outcomes']==[{k:x[k]for k in ['index','mode','world_parts','status']}for x in r['outcomes']]
assert r['status']==e['campaign_status']=='completed'and e['runner_exit_code']==0 and b['build_exit_code']==0;assert all(e[k]for k in ['before_after_equal','binary_unchanged','build_attestation_unchanged']);assert r['source_unchanged_after']and r['binary_unchanged_after']and not r['supervisor_interrupted']and r['final_provenance_errors']=={}
caps={'address_space_bytes':2147483648,'stdout_bytes':4194304,'stderr_bytes':4194304,'read_chunk_bytes':65536,'process_file_size_limit':None,'seconds':180}
ns=runpy.run_path(str(ROOT/'docs/verification/fixtures/persisted-weighted-transform/run.py'),run_name='saved_only_independent_audit');rawrows=0
for i,o in enumerate(r['outcomes']):
 assert o['index']==i and[o['mode'],o['world_parts']]==requests[i]and o['status']=='completed';j=o['job'];assert j['caps']==caps and j['returncode']==0 and not any(j[k]for k in ['timed_out','driver_interrupted','output_limit','capture_incomplete']);assert 0<=j['elapsed_seconds']<=r['elapsed_seconds']<=e['elapsed_seconds']<1500
 for stream in ['stdout','stderr']:
  z=j[stream];p=OUT/z['path'];assert p.stat().st_size==z['bytes']==z['observed_bytes']and sha(p)==z['sha256']and not z['truncated']and z['bytes']<=4194304
 assert j['stderr']['bytes']==0
 decoded=ns['read_rows'](OUT/j['stdout']['path'],OUT/'dataset',o['mode'],o['world_parts']);assert decoded==o['rows'];rawrows+=len(decoded)
 assert len(decoded)==o['world_parts']and sorted(row['world_rank']for row in decoded)==list(range(o['world_parts']))
 for row in decoded:assert row['status']=='completed'and row['error']is None and row['claims']=={'multihost_capacity':False,'rigorous_inverse_certificate':False,'uniform_native_error_known':False}
assert rawrows==28
for name,digest in r['immutable_inputs'].items():assert sha(OUT/'dataset'/name)==digest
assert len(r['immutable_inputs'])==29 and len(list((OUT/'dataset').rglob('*.h5')))==24
# Immutable publication is three manifests +24 HDF5 files; freeze and phases bring total29.
for row in r['outcomes'][0]['rows']:
 assert len(row['result']['publication']['terms'])==3
 for t in row['result']['publication']['terms']:
  assert t['local_generated_entries']==8 and t['header']['record_count']==64 and t['header']['alpha']==2
  manifest=load(OUT/'dataset'/f"term-{t['term']}/manifest.json");assert manifest['schema_version']==2 and len(manifest['buckets'])==8
  for z in t['buckets']:
   path=OUT/'dataset'/f"term-{t['term']}/matching-{z['bucket']:016x}.h5";assert sha(path)==bytes(z['sha256']).hex()and path.stat().st_size==z['file_bytes']==6808 and z['records']==8
f=load(OUT/'dataset/freeze.json');phases=load(OUT/'dataset/phases.json');assert r['outcomes'][1]['rows'][0]['result']['freeze']==f
assert f['degree']==33 and len(f['coefficients'])==34 and len(phases['angles'])==34 and f['certificate']is None and f['max_degree']==81
assert sha(OUT/'dataset/phases.json')==f['phase_sha256']and hashlib.sha256(b''.join(struct.pack('<d',c)for c in f['coefficients'])).hexdigest()==f['coefficient_sha256']
# Independent trigonometric Chebyshev sum, not the implementation's recurrence.
sigma=math.sqrt(26)/8;alpha=f['alpha'];theta=math.acos(sigma/alpha);response=math.fsum(c*math.cos(i*theta)for i,c in enumerate(f['coefficients']));ratio=response*f['physical_rescaling']*sigma
assert abs(alpha-1.25)<1e-15 and f['spectrum_lower']<=sigma<=f['spectrum_upper'];assert f['physical_rescaling']==1/(alpha*f['scale']);assert abs(response*response-f['finite_polynomial_success'])<1e-15;assert abs((alpha*f['scale']/sigma)**2-f['reciprocal_ideal_success'])<1e-15
flat=[];comparisons=[]
for o in r['outcomes'][2:]:
 p=2 if o['mode']=='split'else o['world_parts'];rows=o['rows'];assert len(rows)==o['world_parts']
 for row in rows:
  assert row['parts']==p and row['rank']==(row['world_rank']%2 if o['mode']=='split'else row['world_rank']);x=row['result'];assert x['freeze']==f and x['rigorous_inverse_certificate']is None and x['native_os_overhead_bytes']is None and len(x['calls'])==3
  assert x['construction']['source_identity']==f['source_identity']and x['construction']['construction_identity']==f['construction_identity']and x['construction']['normalization']==alpha
  assert len(x['construction']['children'])==3
  for term,c in enumerate(x['construction']['children']):assert c['term']==term and c['header']==r['outcomes'][0]['rows'][0]['result']['publication']['terms'][term]['header']and c['native_source_identity']==c['header']['source_identity']and c['planned_whole_rank_peak']<=64*1024*1024
  assert x['cumulative_max_rank_work']==sum(c['admission']['max_rank_work']for c in x['calls'])<=24_000_000_000
  for i,c in enumerate(x['calls']):
   assert c['index']==i and c['adjoint']==(i==1)and c['fresh_basis_rhs']and c['coherent_rhs_identity_gates']==0 and c['status']=='accuracy-passed';q=c['routing'];assert q['source_queries']==66 and q['child_events']==198 and q['exact'];assert q['routing']['sent_bytes']==q['routing']['received_bytes'];assert q['routing']['maximum_batch_pairs']<=64 and q['routing']['maximum_routed_amplitudes']<=128
   z=c['readout'];assert z['system_coordinates_visited']==32 and z['residual_operator']==('A†'if i==1 else 'A')and z['relative_residual']<=1e-3 and z['relative_vector_error']<=2e-3 and abs(z['total_probability']-1)<1e-10
   assert abs(z['relative_residual']-abs(ratio-1))<1e-12 and abs(z['relative_vector_error']-abs(ratio-1))<1e-12 and abs(z['success_probability']-response**2)<1e-13 and abs(z['recovered_norm_squared']-ratio*ratio*32/13)<1e-12
   assert 0<z['success_probability']<z['total_probability']and z['local_chunks']==1024//p//64 and z['indexed_reads']==(32 if p==1 else 16//p)and z['local_pair_payload_sent_bytes']==(0 if p==1 else 16384//p)and z['managed_envelope_bytes']==1048576 and z['admitted_work_floor']<=4_000_000
   assert c['admission']['managed_rank_peak_bytes']<=64*1024*1024 and c['admission']['aggregate_work']==p*c['admission']['max_rank_work']
   flat.append({'mode':o['mode'],'world_parts':o['world_parts'],'parts':p,'world_rank':row['world_rank'],'call':i,'adjoint':i==1,'relative_residual':z['relative_residual'],'relative_vector_error':z['relative_vector_error'],'success_probability':z['success_probability'],'total_probability':z['total_probability'],'failure_mass':z['total_probability']-z['success_probability'],'routing':q,'admitted_rank_work':c['admission']['max_rank_work'],'managed_rank_peak_bytes':c['admission']['managed_rank_peak_bytes']})
 for i in range(3):
  reference=r['outcomes'][2]['rows'][0]['result']['calls'][i]['readout'];diff=max(abs(row['result']['calls'][i]['readout'][k]-reference[k])for row in rows for k in ['relative_residual','relative_vector_error','success_probability','total_probability','recovered_norm_squared']);assert diff<1e-12;comparisons.append({'mode':o['mode'],'world_parts':o['world_parts'],'call':i,'maximum_readout_delta_from_p1':diff})
result={'schema':'quest-persisted-weighted-attempt-4-independent-audit-v1','verdict':'approved scoped saved seven-job completion and direct residual/readout evidence','native_or_scientific_reruns':0,'source_files':975,'workspace_source_aggregate_sha256':h.hexdigest(),'snapshot_file_sha256':sha(paths[0]),'consumer_source_manifest_sha256':sha(mfile),'mutable_cargo_artifact_current_sha256':sha(selected['executable']),'mutable_cargo_artifact_note':'Cargo target is reused by later broad gates; immutable pinned binary copies are authoritative for this saved run','current_later_documentation_deltas':current_snapshot_deltas,'consumer_files':len(m['files']),'native_install_files':len(s['native']),'tool_executable_records':len(s['tools']),'actual_local_package_manifests':local,'binary_sha256':identity,'build_attestation_sha256':sha(P/'quest-persisted-weighted-final-build-4-attestation.json'),'raw_receipt_sha256':sha(OUT/'receipt.json'),'execution_attestation_sha256':sha(P/'quest-persisted-weighted-final-execution-4.json'),'raw_rank_rows':rawrows,'independent_replay_calls':len(flat),'jobs':7,'publication_input_files':27,'freeze_phase_files':2,'immutable_files':29,'phase_sha256':f['phase_sha256'],'degree':33,'alpha':alpha,'nominal_sigma':sigma,'independent_polynomial_response':response,'independent_relative_polynomial_residual':abs(ratio-1),'ideal_inverse_norm_squared':32/13,'nominal_finite_success':response**2,'ideal_reciprocal_success':f['reciprocal_ideal_success'],'native_failure_mass_range':[min(c['failure_mass']for c in flat),max(c['failure_mass']for c in flat)],'replay_calls':flat,'cross_rank_layout_readout_comparisons':comparisons,'caps':caps,'elapsed_seconds':{'campaign_driver':r['elapsed_seconds'],'outer_execution':e['elapsed_seconds'],'build':b['elapsed_seconds']},'qualifications':['975-file workspace superset distinct from actual16-local-package compiler closure; neither hashes external registry/dependent library closure','rustc/cargo hashes identify rustup launcher; actual reported versions/profile preserved separately','three calls each use fresh computational basis RHS; not an accumulated U-U† round-trip or general dense input preparation','full32-coordinate scalar residuals validated; no saved global quantum-state vector available to independently recompute native amplitude coordinates','source H=.625I-.125iX encodes A†; forward solves A=.625I+.125iX, standalone adjoint solves A†','nominal analytic sigma/polynomial diagnostics not uniform source/native/phase error certificate; certificate and native-overhead remain null','telemetry exact only for named router counters, excludes PREP/projectors/coordinator/native/MPI protocol','managed cap not RSS quota;2GiB process AS and recorded RSS separate; local MPI not multihost evidence','historical attempts1/2/3 preserved; no change to caps/tolerances or old bytes']}
with(P/'quest-persisted-weighted-attempt-4-independent-audit-results.json').open('x')as out:json.dump(result,out,indent=2,allow_nan=False);out.write('\n')
print(json.dumps({k:v for k,v in result.items()if k not in ('replay_calls','actual_local_package_manifests','cross_rank_layout_readout_comparisons')},indent=2))
