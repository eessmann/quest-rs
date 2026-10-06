#!/usr/bin/env python3
"""Fixed seven-job local protocol. Execution needs a separately approved build pin.

Only bounded receipt metadata is gathered. Native states, matrix sources and gate
streams remain distributed. This runner is not a multi-host capacity certificate.
"""
import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import resource
import selectors
import shutil
import signal
import struct
import subprocess
import tempfile
import time

REQUESTS = [("publish",8),("compile",1),("replay",1),("replay",2),("replay",4),("replay",8),("split",4)]
JOB_SECONDS = 180
DRIVER_SECONDS = 1500
AS_BYTES = 2147483648
OUTPUT_BYTES = 4194304
RECEIPT_BYTES = 131072
U64_MAX = (1 << 64) - 1
ROW_SCHEMA = "quest-persisted-weighted-transform-row-v1"
INDEX_SCHEMA = "quest-persisted-weighted-transform-index-v1"
MEMORY = {"rss_endpoint_bytes","rss_high_water_bytes","address_space_endpoint_bytes","address_space_high_water_bytes"}
ROUTING = {"batches","local_pair_candidates","maximum_batch_pairs","maximum_routed_amplitudes","coordination_calls","indexed_reads","indexed_writes","sent_bytes","received_bytes"}
PEAKS = {"maximum_batch_pairs","maximum_routed_amplitudes"}
CLAIMS = {"multihost_capacity":False,"uniform_native_error_known":False,"rigorous_inverse_certificate":False}
FIXED = {"dimension":32,"origin_ranks":8,"buckets":8,"register_qubits":10,
         "targets":[0,9,8,7,6,5,1],"selectors":[2,3],"response":4,"weights":[.25,.25,-.125],
         "rank_bytes":67108864,"node_bytes":536870912,"load_bytes":1048576,
         "conversion_bytes":4194304,"compile_bytes":50331648,"readout_bytes":1048576,
         "readout_work":4000000,"reciprocal_tolerance":1e-4,"max_degree":81,
         "direct_residual_target":1e-3,"relative_vector_target":2e-3}


ACTIVE_CHILD = None
SUPERVISOR_INTERRUPTED = False

class DriverTimeout(TimeoutError):
    pass


def kill_group(process):
    try:os.killpg(process.pid,signal.SIGKILL)
    except ProcessLookupError:
        try:process.kill()
        except ProcessLookupError:pass


def install_supervisor():
    def terminated(_signal,_frame):
        global SUPERVISOR_INTERRUPTED
        SUPERVISOR_INTERRUPTED=True
        if ACTIVE_CHILD is not None:
            kill_group(ACTIVE_CHILD)
            raise DriverTimeout('external1500s driver supervisor terminated this run')
        # With no child wait to interrupt, allow bounded receipt/finalization
        # code to retain progress; the loop observes the flag before spawning.
    signal.signal(signal.SIGTERM,terminated)


def require(condition, reason):
    if not condition: raise ValueError(reason)


def integer(value, low=0, high=U64_MAX):
    require(type(value) is int and low <= value <= high, "bounded integer")
    return value


def number(value, low=0):
    require(type(value) in (int,float) and math.isfinite(value) and value >= low, "finite scalar")
    return value


def exact_values(value,expected):
    require(type(value) is type(expected),'exact fixed value type')
    if type(expected) is dict:
        require(set(value)==set(expected),'fixed keys')
        for key in expected:exact_values(value[key],expected[key])
    elif type(expected) is list:
        require(len(value)==len(expected),'fixed list width')
        for left,right in zip(value,expected):exact_values(left,right)
    else:require(value==expected,'fixed value')


def finite_tree(value):
    stack=[(value,0)];visited=0
    while stack:
        item,depth=stack.pop();visited+=1
        require(depth<=32 and visited<=16384,"JSON structure bound")
        if isinstance(item,dict): stack.extend((v,depth+1) for v in item.values())
        elif isinstance(item,list): stack.extend((v,depth+1) for v in item)
        elif type(item) is float: require(math.isfinite(item),"finite JSON")


def decode(raw,cap=RECEIPT_BYTES):
    require(len(raw)<=cap,"JSON byte cap")
    def pairs(items):
        result={}
        for key,value in items:
            require(key not in result,"duplicate JSON key")
            result[key]=value
        return result
    def floating(text):
        value=float(text);require(math.isfinite(value),"overflow JSON exponent");return value
    try:
        value=json.loads(raw,object_pairs_hook=pairs,parse_float=floating,
                         parse_constant=lambda _: (_ for _ in ()).throw(ValueError("nonfinite JSON")))
        finite_tree(value)
        return value
    except (RecursionError,UnicodeError,OverflowError,json.JSONDecodeError) as exc:
        raise ValueError("malformed bounded JSON") from exc


def bounded_read(path,cap):
    with Path(path).open('rb') as stream:
        require(os.fstat(stream.fileno()).st_size<=cap,'file byte admission')
        raw=stream.read(cap+1)
    require(len(raw)<=cap,'growing file byte cap')
    return raw


def final_provenance(report,source_checker,binary_checker):
    errors={}
    for key,check in [('source_unchanged_after',source_checker),('binary_unchanged_after',binary_checker)]:
        try:
            report[key]=check()
            if report[key] is not True:errors[key]='identity changed'
        except (ValueError,OSError,KeyError,TypeError,ArithmeticError) as exc:
            report[key]=None;errors[key]=str(exc)[:4096]
    report['final_provenance_errors']=errors
    if errors:report['status']='provenance-failure'


def digest(path):
    h=hashlib.sha256()
    with open(path,'rb') as stream:
        for chunk in iter(lambda:stream.read(65536),b''):h.update(chunk)
    return h.hexdigest()


def sha(value):
    require(type(value) is str and len(value)==64 and all(c in '0123456789abcdef' for c in value),"SHA256")


def phase_freeze(v):
    require(type(v) is dict and v.get('schema')=='quest-persisted-weighted-phase-freeze-v1',"phase schema")
    require(v.get('dimension')==32 and v.get('max_degree')==81 and v.get('reciprocal_tolerance')==1e-4,"phase fixed premises")
    require(set(v)=={'schema','dimension','degree','alpha','scale','error_bound','coefficient_rounding_bound','physical_rescaling','spectrum_lower','spectrum_upper','spectral_evidence','source_identity','construction_identity','coefficients','coefficient_sha256','phase_sha256','phase_format','synthesis','finite_polynomial_success','reciprocal_ideal_success','reciprocal_tolerance','max_degree','constructor_retained_polynomial_bytes','certificate','construction'},'known freeze fields')
    require(v.get('phase_format')=='existing QspInput::Symmetric JSON' and type(v.get('spectral_evidence')) is str,'phase provenance')
    integer(v.get('constructor_retained_polynomial_bytes'),1,33554432)
    degree=integer(v.get('degree'),1,81);require(degree%2==1,"odd degree")
    alpha=number(v.get('alpha'),1e-300);scale=number(v.get('scale'),1e-300)
    require(0<=number(v.get('error_bound'))<=1e-4 and 'certificate' in v and v['certificate'] is None,"uncertified error premise")
    integer(v.get('source_identity'));integer(v.get('construction_identity'))
    coefs=v.get('coefficients');require(type(coefs) is list and len(coefs)==degree+1,"coefficient count")
    for c in coefs: require(type(c) in (int,float) and math.isfinite(c),"coefficient")
    sha(v.get('coefficient_sha256'));sha(v.get('phase_sha256'))
    require(hashlib.sha256(b''.join(struct.pack('<d',c) for c in coefs)).hexdigest()==v['coefficient_sha256'],"coefficient bits")
    lo=number(v.get('spectrum_lower'),1e-300);hi=number(v.get('spectrum_upper'),lo)
    require(lo<=math.sqrt(26)/8<=hi and lo<hi and hi-lo<=1e-14,"directed nominal spectrum")
    product=alpha*scale;number(product,1e-300)
    require(v.get('physical_rescaling')==1/product,"actual inverse scale")
    require(type(v.get('synthesis')) is dict,"synthesis provenance")
    s=v['synthesis'];require(s.get('algorithm')=='InverseNlftDivideConquer' and s.get('backend')=='Scalar',"synthesis method")
    require(s.get('max_completion_grid')==16384 and s.get('response_tolerance')==1e-11 and s.get('contractivity_margin')==1e-12,"synthesis cap")
    require(s.get('limits')=={'length':16384,'bytes':33554432,'work':1000000000},"synthesis numerical caps")
    integer(s.get('completion_grid'),1,16384)
    number(s.get('completion_residual'))
    require('reconstruction_residual' in s,'optional reconstruction provenance')
    if s['reconstruction_residual'] is not None:number(s['reconstruction_residual'])
    for key in ['finite_polynomial_success','reciprocal_ideal_success','coefficient_rounding_bound']:number(v.get(key))
    x=(lo+hi)/2/alpha;prior=1.;current=x;response=coefs[0]
    for j,c in enumerate(coefs[1:],1):
        if j>1:prior,current=current,2*x*current-prior
        response+=c*current
    require(abs(v['finite_polynomial_success']-response*response)<=1e-12,'independent Chebyshev success')
    ideal=(alpha*scale/((lo+hi)/2))**2
    require(abs(v['reciprocal_ideal_success']-ideal)<=1e-12,'returned-scale reciprocal success')
    return degree


def header(h):
    require(type(h) is dict,"header")
    require(h.get('rows')==32 and h.get('cols')==32 and h.get('record_count')==64 and h.get('num_colors')==2 and h.get('color_qubits')==1 and h.get('system_qubits')==5,"complete matching layout")
    require(h.get('beta')==1 and h.get('alpha')==2,"matching normalization")
    integer(h.get('source_identity'));integer(h.get('record_digest'))


def stage(v):
    require(type(v) is dict,"stage")
    number(v.get('seconds'));memory(v.get('memory'))


def memory(v):
    require(type(v) is dict and set(v)==MEMORY,"memory observations")
    for x in v.values():integer(x,1)
    require(v['rss_high_water_bytes']>=v['rss_endpoint_bytes'] and v['address_space_high_water_bytes']>=v['address_space_endpoint_bytes'],"memory high-water")


def children(stages,parts):
    require(type(stages) is dict and type(stages.get('children')) is list and len(stages['children'])==3,"three retained children")
    for index,c in enumerate(stages['children']):
        require(type(c) is dict,'child receipt object')
        require(c.get('term')==index,"child order");header(c.get('header'))
        for k in ['returned_source_bytes','source_clone_bytes','actual_target_bytes','native_preparation_scratch_allowance','whole_rank_admitted_ceiling','planned_whole_rank_peak','whole_node_admitted_ceiling','constructor_work']:integer(c.get(k),1)
        integer(c.get('application_payload_ceiling'),0 if parts==1 else 1,8388608)
        require(c['whole_rank_admitted_ceiling']<=FIXED['rank_bytes'] and c['whole_node_admitted_ceiling']<=FIXED['node_bytes'],"bridge whole-live caps")
        stage(c.get('load'));stage(c.get('bridge'))
    stage(stages.get('stage'));number(stages.get('normalization'),1e-300)
    integer(stages.get('source_identity'));integer(stages.get('construction_identity'))


def rollup(receipts):
    sums={k:0 for k in ROUTING};exact=True;queries=events=0
    for t in receipts:
        require(type(t) is dict and type(t.get('exact')) is bool and type(t.get('routing')) is dict and set(t['routing'])==ROUTING,"current telemetry")
        exact=exact and t['exact'];queries+=integer(t.get('source_queries'));events+=integer(t.get('child_events'))
        for k,x in t['routing'].items():
            integer(x);exact=exact and x!=U64_MAX
            if k in PEAKS:sums[k]=max(sums[k],x)
            else:sums[k]+=x
    if queries>U64_MAX or events>U64_MAX or any(x>U64_MAX for x in sums.values()):exact=False
    return {'source_queries':min(queries,U64_MAX),'child_events':min(events,U64_MAX),
            'routing':{k:min(v,U64_MAX) for k,v in sums.items()},'exact':exact,
            'units':'checked sum across rank receipts; peak fields use max; inexact totals are lower bounds',
            'excludes':['PREP','response/projectors','constructor/coordinator','native internal MPI','MPI protocol']}


def validate_row(row,mode,world_parts):
    finite_tree(row)
    require(type(row) is dict and set(row)=={'schema','mode','world_rank','world_parts','rank','parts','status','phase','error','stages','result','fixed','claims','memory','environment_accounted_bytes'},"row fields")
    require(row['schema']==ROW_SCHEMA and row['mode']==mode and row['world_parts']==world_parts,"row identity")
    world_rank=integer(row['world_rank'],0,world_parts-1);parts=2 if mode=='split' else world_parts
    integer(row['world_parts'],1,8);integer(row['parts'],1,8);integer(row['rank'],0,parts-1)
    require(row['parts']==parts and row['rank']==world_rank%parts,"group partition")
    exact_values(row['fixed'],FIXED);exact_values(row['claims'],CLAIMS)
    memory(row['memory']);integer(row['environment_accounted_bytes'],0,FIXED['rank_bytes'])
    require(type(row['phase']) is str and 0<len(row['phase'])<=128,"phase progress")
    require(row['status'] in ('completed','accuracy-failure','rejected'),"outcome")
    if row['status']=='rejected':
        require(type(row['error']) is str and 0<len(row['error'])<=4096,"typed rejection")
        require(row['result'] is None or (type(row['result']) is dict and row['result'].get('status')=='rejected'),"failure payload")
        s=row['stages'];require(type(s) is dict,"partial stage")
        for noun,key in [('children','completed_children'),('terms','completed_terms')]:
            if key in s:require(type(s.get(noun)) is list and integer(s[key],0,2)==len(s[noun]) and integer(s.get('attempt_term'),0,2)==s[key],"failure prefix")
        return
    require(row['error'] is None and type(row['result']) is dict,"complete payload")
    v=row['result']
    if mode=='publish':
        require(row['status']=='completed' and type(v.get('publication')) is dict,"publication")
        terms=v['publication'].get('terms');require(type(terms) is list and len(terms)==3,"publication terms")
        for index,t in enumerate(terms):
            require(type(t) is dict,'term receipt object')
            require(t.get('term')==index and t.get('local_generated_entries')==8,"genuine origin sharding")
            header(t.get('header'));stage(t.get('generation'));stage(t.get('publication'))
            buckets=t.get('buckets');require(type(buckets) is list and len(buckets)==8,"immutable buckets")
            require(all(type(b) is dict for b in buckets),'bucket receipt objects')
            require(sorted(b.get('bucket') for b in buckets)==list(range(8)) and sum(integer(b.get('records')) for b in buckets)==64,"bucket coverage")
            for b in buckets:
                integer(b.get('file_bytes'),1,1048576)
                for k in ['sha256','semantic_sha256']:require(type(b.get(k)) is list and len(b[k])==32 and all(type(x) is int and 0<=x<=255 for x in b[k]),"bucket SHA bytes")
        return
    children(row['stages'],parts);degree=phase_freeze(v.get('freeze'))
    require(v['freeze']['source_identity']==row['stages']['source_identity'] and v['freeze']['construction_identity']==row['stages']['construction_identity'],"compiled A/U IDs")
    if mode=='compile':
        require(row['status']=='completed' and v.get('status')=='completed' and v.get('compilation_envelope_bytes')==FIXED['compile_bytes'],"one-time compilation")
        stage(v.get('compilation'));return
    require(v.get('status')==row['status'] and type(v.get('calls')) is list and len(v['calls'])==3 and all(type(call) is dict for call in v['calls']),"three reusable requests")
    require(v.get('fixed_declared_three_apply_work_ceiling')==24000000000,'cumulative declared cap')
    require('rigorous_inverse_certificate' in v and v['rigorous_inverse_certificate'] is None and 'native_os_overhead_bytes' in v and v['native_os_overhead_bytes'] is None,"unknown native bound")
    for k in ['phase_import','transform_construction']:stage(v.get(k))
    integer(v.get('cumulative_max_rank_work'),1,24000000000)
    for index,c in enumerate(v['calls']):
        require(type(c) is dict,'call receipt object')
        require(integer(c.get('index'),0,2)==index and type(c.get('adjoint')) is bool and c.get('adjoint')== (index==1) and c.get('fresh_basis_rhs') is True and c.get('coherent_rhs_identity_gates')==0,"literal fresh RHS queries")
        for k in ['application','readout_stage']:stage(c.get(k))
        init=c.get('local_simulator_initialization');require(type(init) is dict and init.get('max_rank_work_ceiling')==4000000,'separate initialization allowance');stage(init.get('stage'))
        r=c.get('readout');require(type(r) is dict and r.get('system_coordinates_visited')==32,"full-coordinate readout")
        for k in ['total_probability','success_probability','relative_vector_error','relative_residual','recovered_norm_squared']:number(r.get(k))
        require(0<r['success_probability']<=r['total_probability']+1e-12,'finite projected probability relationship')
        passed=r['relative_residual']<=1e-3 and r['relative_vector_error']<=2e-3 and abs(r['total_probability']-1)<=1e-10
        require(c.get('status')==('accuracy-passed' if passed else 'accuracy-failure'),"empirical accuracy verdict")
        if row['status']=='completed':require(passed,"completed inverse accuracy")
        else:require(any(call.get('status')=='accuracy-failure' for call in v['calls']),'accuracy failure evidence')
        t=c.get('routing');rollup([t])
        require(t['source_queries']==2*degree and t['child_events']==6*degree,"actual source call receipt")
        if parts>1:require(t['routing']['sent_bytes']>0,"actual cross-rank matching sends")
        a=c.get('admission');require(type(a) is dict,"apply admission")
        for k,cap in [('max_rank_work',8000000000),('aggregate_work',64000000000),('native_dispatches',1000000),('aggregate_application_payload_ceiling',8000000000),('control_calls',2000000),('preflight_work',1000000000),('managed_rank_peak_bytes',67108864)]:integer(a.get(k),0,cap)

    require(v['cumulative_max_rank_work']==sum(call['admission']['max_rank_work'] for call in v['calls']),'three admitted work sum')


def scalar_comparisons(rows,mode):
    groups={}
    for row in rows:groups.setdefault(row['world_rank']//row['parts'],[]).append(row)
    result=[]
    for group,members in sorted(groups.items()):
        first=members[0]['result'];freeze=first['freeze']
        for index,call in enumerate(first['calls']):
            readout=call['readout'];actual=readout['success_probability']
            require(all(member['result']['calls'][index]['readout']==readout for member in members),'common reduced readout')
            item={'group':group,'index':index,'adjoint':call['adjoint'],
                  'finite_polynomial_success_difference':actual-freeze['finite_polynomial_success'],
                  'reciprocal_ideal_success_difference':actual-freeze['reciprocal_ideal_success'],
                  'direct_residual':readout['relative_residual'],'relative_vector_error':readout['relative_vector_error'],
                  'recovered_norm_squared_difference':readout['recovered_norm_squared']-32/13,
                  'scope':'empirical scalar comparisons only; unknown source/PREP/native/RHS uniform error'}
            finite_tree(item);result.append(item)
    return result


def validate_coverage(rows,mode,parts):
    require(type(rows) is list and all(type(row) is dict for row in rows),'rank row objects')
    require(len(rows)==parts and sorted(row.get('world_rank') for row in rows)==list(range(parts)),"rank coverage")
    for row in rows:validate_row(row,mode,parts)
    require(len({row['status'] for row in rows})==1,"agreed outcome")


def atomic(path,value):
    payload=json.dumps(value,sort_keys=True,allow_nan=False,separators=(',',':')).encode()
    require(len(payload)<=16*OUTPUT_BYTES,"aggregate receipt cap")
    tmp=Path(str(path)+'.tmp');tmp.write_bytes(payload);os.replace(tmp,path)


def collect(command,stem,seconds=JOB_SECONDS):
    global ACTIVE_CHILD
    if SUPERVISOR_INTERRUPTED:raise DriverTimeout('driver stopped before child spawn')
    stem=Path(stem);paths={name:Path(str(stem)+'.'+name) for name in ['stdout','stderr']}
    # Capture limits apply only to parent-owned output files, not MPI transport
    # files. No process-wide filesystem quota is claimed by this collector.
    selector=selectors.DefaultSelector()
    old_mask=signal.pthread_sigmask(signal.SIG_BLOCK,{signal.SIGTERM})
    def caps():
        os.setsid()
        resource.setrlimit(resource.RLIMIT_AS,(AS_BYTES,AS_BYTES))
        signal.pthread_sigmask(signal.SIG_SETMASK,old_mask)
    start=time.monotonic();timed_out=False;driver_interrupted=False;output_limit=False
    retained={name:0 for name in paths};observed={name:0 for name in paths}
    truncated={name:False for name in paths};capture_incomplete=False
    try:
        if SUPERVISOR_INTERRUPTED:raise DriverTimeout('driver stopped before registered child spawn')
        with paths['stdout'].open('wb') as stdout,paths['stderr'].open('wb') as stderr:
            writers={'stdout':stdout,'stderr':stderr}
            process=subprocess.Popen(command,stdout=subprocess.PIPE,stderr=subprocess.PIPE,preexec_fn=caps)
            ACTIVE_CHILD=process
            finished=False
            def capture_step(wait):
                nonlocal output_limit
                for key,_ in selector.select(wait):
                    name=key.data
                    try:chunk=os.read(key.fd,65536)
                    except BlockingIOError:continue
                    if not chunk:
                        selector.unregister(key.fileobj);key.fileobj.close();continue
                    observed[name]+=len(chunk)
                    remaining=OUTPUT_BYTES-retained[name]
                    if len(chunk)>remaining:
                        output_limit=True;truncated[name]=True
                    keep=min(len(chunk),remaining)
                    if keep:
                        writers[name].write(chunk[:keep]);retained[name]+=keep
            try:
                for name,pipe in [('stdout',process.stdout),('stderr',process.stderr)]:
                    os.set_blocking(pipe.fileno(),False);selector.register(pipe,selectors.EVENT_READ,name)
                signal.pthread_sigmask(signal.SIG_SETMASK,old_mask)
                stop_deadline=None
                while selector.get_map():
                    now=time.monotonic()
                    if stop_deadline is None and now-start>=seconds:
                        timed_out=True;kill_group(process);stop_deadline=now+3
                    if stop_deadline is not None and now>=stop_deadline:
                        capture_incomplete=True;break
                    capture_step(min(.1,max(.001,(stop_deadline or (start+seconds))-now)))
                    if output_limit and stop_deadline is None:
                        kill_group(process);stop_deadline=time.monotonic()+3
                remaining=max(.001,seconds-(time.monotonic()-start)) if stop_deadline is None else 3
                try:code=process.wait(timeout=remaining)
                except subprocess.TimeoutExpired:
                    timed_out=True;kill_group(process);code=process.wait(timeout=3)
                finished=True
            except DriverTimeout:
                timed_out=True;driver_interrupted=True
                signal.pthread_sigmask(signal.SIG_BLOCK,{signal.SIGTERM})
                kill_group(process);code=process.wait(timeout=3)
                cleanup_deadline=time.monotonic()+3
                while selector.get_map() and time.monotonic()<cleanup_deadline:capture_step(.1)
                capture_incomplete=bool(selector.get_map());finished=True
            finally:
                signal.pthread_sigmask(signal.SIG_BLOCK,{signal.SIGTERM})
                if not finished or process.poll() is None:
                    kill_group(process);process.wait(timeout=3)
                ACTIVE_CHILD=None
                for key in list(selector.get_map().values()):
                    truncated[key.data]=True;key.fileobj.close()
                for pipe in [process.stdout,process.stderr]:pipe.close()
    finally:
        selector.close()
        signal.pthread_sigmask(signal.SIG_SETMASK,old_mask)
    result={'returncode':code,'timed_out':timed_out,'driver_interrupted':driver_interrupted,
            'output_limit':output_limit,'capture_incomplete':capture_incomplete,
            'elapsed_seconds':time.monotonic()-start,
            'caps':{'address_space_bytes':AS_BYTES,'stdout_bytes':OUTPUT_BYTES,
                    'stderr_bytes':OUTPUT_BYTES,'read_chunk_bytes':65536,
                    'process_file_size_limit':None,'seconds':seconds}}
    for name,path in paths.items():
        result[name]={'path':path.name,'bytes':path.stat().st_size,'sha256':digest(path),
                      'observed_bytes':observed[name],'truncated':truncated[name]}
        require(path.stat().st_size<=OUTPUT_BYTES,'parent capture exceeds cap')
    return result


def read_rows(stdout,directory,mode,parts):
    raw=bounded_read(stdout,OUTPUT_BYTES)
    lines=raw.splitlines();require(len(lines)==parts,"one bounded rank index each")
    rows=[]
    for line in lines:
        index=decode(line,1024)
        require(type(index) is dict and index.get('schema')==INDEX_SCHEMA and index.get('mode')==mode and index.get('world_parts')==parts,"receipt index")
        rank=integer(index.get('world_rank'),0,parts-1)
        expected=f'receipt-{mode}-world{parts}-rank{rank}.json'
        require(index.get('receipt')==expected,"safe rank receipt path")
        path=Path(directory)/expected;integer(index.get('receipt_bytes'),1,RECEIPT_BYTES)
        raw=bounded_read(path,RECEIPT_BYTES)
        require(len(raw)==index['receipt_bytes'] and hashlib.sha256(raw).hexdigest()==index.get('receipt_sha256'),"rank receipt SHA/bytes")
        row=decode(raw);require(type(row) is dict,'rank receipt object');require(row.get('status')==index.get('status') and row.get('rank')==index.get('rank') and row.get('parts')==index.get('parts'),"rank index content")
        rows.append(row)
    validate_coverage(rows,mode,parts)
    return sorted(rows,key=lambda r:r['world_rank'])


def source_check(path,repo):
    raw=bounded_read(path,1048576);value=decode(raw,1048576)
    require(type(value) is dict,'source manifest object')
    require(value.get('schema')=='quest-persisted-weighted-consumer-source-v1' and type(value.get('files')) is list and 1<=len(value['files'])<=32,"scoped source manifest")
    seen=set()
    for entry in value['files']:
        require(type(entry) is dict,'source manifest entry object')
        rel=entry.get('path');require(type(rel) is str and not Path(rel).is_absolute() and '..' not in Path(rel).parts and rel not in seen,"source path")
        seen.add(rel);sha(entry.get('sha256'));file=Path(repo)/rel
        expected_bytes=integer(entry.get('bytes'),0,4194304)
        require(file.stat().st_size==expected_bytes,'source declared byte admission')
        require(digest(file)==entry['sha256'] and file.stat().st_size==expected_bytes,"source file changed")
    return hashlib.sha256(raw).hexdigest()


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary',type=Path,required=True)
    parser.add_argument('--binary-sha256',required=True)
    parser.add_argument('--source-manifest',type=Path,required=True)
    parser.add_argument('--repository',type=Path,required=True)
    parser.add_argument('--output-directory',type=Path,required=True)
    parser.add_argument('--build-attestation',type=Path,required=True)
    args=parser.parse_args();sha(args.binary_sha256)
    args.output_directory.mkdir()
    output=args.output_directory/'receipt.json'
    install_supervisor()
    report={'schema':'quest-persisted-weighted-transform-campaign-v1','status':'initial-admission',
            'requests':REQUESTS,'outcomes':[],'scope':'local capped fixed protocol; no multi-host capacity or uniform error certificate'}
    try:
        source_sha=source_check(args.source_manifest,args.repository)
        attestation_raw=bounded_read(args.build_attestation,1048576);attestation=decode(attestation_raw,1048576)
        require(type(attestation) is dict,'build attestation object')
        require(attestation.get('binary_sha256')==args.binary_sha256 and attestation.get('consumer_source_manifest_sha256')==source_sha,'separate build pin')
        require(args.binary.stat().st_size<=536870912 and digest(args.binary)==args.binary_sha256,'binary pin')
        dataset=args.output_directory/'dataset';dataset.mkdir()
        binary=args.output_directory/'binary';shutil.copyfile(args.binary,binary);binary.chmod(0o500)
        require(digest(binary)==args.binary_sha256,'immutable copied binary identity')
        report.update({'status':'running','source_manifest_sha256':source_sha,'binary_sha256':args.binary_sha256,
                       'build_attestation_sha256':hashlib.sha256(attestation_raw).hexdigest()})
    except (ValueError,OSError,KeyError,TypeError,ArithmeticError) as exc:
        report['status']='driver-timeout' if isinstance(exc,DriverTimeout) else 'initial-provenance-failure'
        report['error']=str(exc)[:4096]
        atomic(output,report);return
    output=args.output_directory/'receipt.json';started=time.monotonic();blocked=False;frozen_files=None;compiled_freeze=None
    for index,(mode,parts) in enumerate(REQUESTS):
        outcome={'index':index,'mode':mode,'world_parts':parts}
        if blocked or SUPERVISOR_INTERRUPTED or time.monotonic()-started>=DRIVER_SECONDS:
            outcome['status']='not-run';outcome['reason']='prior prerequisite failed or fixed driver timeout'
        else:
            try:
                require(digest(binary)==args.binary_sha256 and source_check(args.source_manifest,args.repository)==source_sha,"pre-child source/binary drift")
                if frozen_files:
                    for rel,want in frozen_files.items():require(digest(dataset/rel)==want,"immutable persisted/phase bytes")
                job=collect(['mpiexec','-n',str(parts),str(binary),mode,str(dataset)],args.output_directory/f'job-{index}',seconds=min(JOB_SECONDS,max(.001,DRIVER_SECONDS-(time.monotonic()-started))))
                outcome['job']=job
                if job.get('output_limit'):outcome['status']='output-limit'
                elif job['timed_out']:outcome['status']='timeout'
                elif job.get('capture_incomplete'):outcome['status']='capture-failure'
                elif job['returncode']!=0:outcome['status']='process-failure'
                else:
                    rows=read_rows(args.output_directory/f'job-{index}.stdout',dataset,mode,parts)
                    outcome['rows']=rows;outcome['status']=rows[0]['status']
                    if mode in ('replay','split') and outcome['status']!='rejected':
                        require(compiled_freeze is not None and all(row['result']['freeze']==compiled_freeze for row in rows),'same compiled phase metadata')
                    if mode in ('replay','split') and outcome['status']!='rejected':
                        groups=parts//2 if mode=='split' else 1
                        outcome['routing_rollups']=[{'group':group,'calls':[rollup([row['result']['calls'][call]['routing'] for row in rows if row['world_rank']//row['parts']==group]) for call in range(3)]} for group in range(groups)]
                        outcome['scalar_comparisons']=scalar_comparisons(rows,mode)
                require(digest(binary)==args.binary_sha256 and source_check(args.source_manifest,args.repository)==source_sha,"post-child source/binary drift")
                if mode=='publish' and outcome['status']=='completed':
                    frozen_files={p.relative_to(dataset).as_posix():digest(p) for p in dataset.glob('term-*/*') if p.is_file()}
                    require(len(frozen_files)==27,"three manifests and 24 persisted buckets")
                elif mode=='compile' and outcome['status']=='completed':
                    frozen_files.update({name:digest(dataset/name) for name in ['freeze.json','phases.json']})
                    freeze_raw=bounded_read(dataset/'freeze.json',16384);compiled_freeze=decode(freeze_raw,16384)
                    require(hashlib.sha256(freeze_raw).hexdigest()==frozen_files['freeze.json'],'freeze snapshot identity')
                    require(compiled_freeze==outcome['rows'][0]['result']['freeze'] and digest(dataset/'phases.json')==compiled_freeze['phase_sha256'],'compiled export byte binding')
                elif frozen_files:
                    for rel,want in frozen_files.items():require(digest(dataset/rel)==want,"post-child immutable inputs")
            except DriverTimeout as exc:
                outcome['status']='driver-timeout';outcome['error']=str(exc)[:4096]
            except (ValueError,OSError,KeyError,TypeError,ArithmeticError) as exc:
                outcome['status']='malformed-or-admission-failure';outcome['error']=str(exc)[:4096]
            if mode in ('publish','compile') and outcome['status']!='completed':blocked=True
        report['outcomes'].append(outcome);atomic(output,report)
    report['supervisor_interrupted']=SUPERVISOR_INTERRUPTED
    report['status']='completed' if all(r['status']=='completed' for r in report['outcomes']) else 'incomplete-or-accuracy-failure'
    if SUPERVISOR_INTERRUPTED:report['status']='driver-timeout'
    report['elapsed_seconds']=time.monotonic()-started;report['immutable_inputs']=frozen_files
    final_provenance(report,lambda:source_check(args.source_manifest,args.repository)==source_sha,lambda:digest(binary)==args.binary_sha256)
    if SUPERVISOR_INTERRUPTED:report['status']='driver-timeout'
    atomic(output,report)

if __name__=='__main__':main()
