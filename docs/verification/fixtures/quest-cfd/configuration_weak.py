#!/usr/bin/env python3
"""Seven fixed capped initial diagnostics; never retries or substitutes a rejected row."""
import hashlib
import json
import math
import os
from pathlib import Path
import resource
import signal
import struct
import subprocess
import sys
import time
from campaign import decode_report, report_finite, sha256

ROWS = ('p1-c3-e1-w1_2', 'p1-c4-e1-w1_2', 'p1-c5-e1-w1_2', 'p2-c1-e1-w1_2',
        'p2-c3-e1-w1_2', 'p1-c5-e5_3-w1_2', 'p1-c3-e1-w3_5')
CENTER = [.15, -.1, .07, .11, -.04]
CAPS = {'max_bytes':268435456, 'max_source_work':1000000000,
        'max_physical_work':100000000000, 'max_physical_calls':1000000}
AS_BYTES, SECONDS, FILE_BYTES = 512*1024**2, 180, 4*1024**2
RESOURCE_FIELDS = 'source_work caller_declared_input_preparation_work physical_work physical_calls physical_calls_attempted constructor_peak_bytes retained_source_bytes grid_bytes accessible_state_bytes external_bytes result_bytes scratch_bytes peak_bytes supported_rows row_query_work maximum_row_entries'
RATE_FIELDS = ('expectation','raw_skew_rate','normalized_rate','physical_rate','absolute_defect','scaled_defect')

def shape(value, fields):
    if type(value) is not dict or set(value) != set(fields.split()):
        raise ValueError('unexpected or missing fields')

def number(value, minimum=None, maximum=None):
    try:
        valid = type(value) in (int,float) and math.isfinite(value)
    except OverflowError:
        valid = False
    if not valid or minimum is not None and value < minimum or maximum is not None and value > maximum:
        raise ValueError('invalid finite numeric payload')
    return value

def integer(value, minimum=0, maximum=None):
    if type(value) is not int:
        raise ValueError('invalid integer counter')
    return number(value, minimum, maximum)

def boolean(value):
    if type(value) is not bool:
        raise ValueError('invalid boolean')

def vector(value, count, minimum=None):
    if type(value) is not list or len(value) != count:
        raise ValueError('wrong vector dimension')
    for word in value:
        number(word, minimum)

def digest(value):
    if type(value) is not str or len(value)!=64 or any(c not in '0123456789abcdef' for c in value):
        raise ValueError('invalid SHA256')

def close(actual, expected):
    number(actual); number(expected)
    if abs(actual-expected)>1e-11*max(1.,abs(expected)):
        raise ValueError('inconsistent numerical relation')

def bits(value):
    return struct.unpack('<Q',struct.pack('<d',value))[0]

def parameters(row_id):
    if row_id not in ROWS:
        raise ValueError('unknown fixed row')
    order,cells,extent,width = ((1,3,1.,.5),(1,4,1.,.5),(1,5,1.,.5),(2,1,1.,.5),
                               (2,3,1.,.5),(1,5,5/3,.5),(1,3,1.,3/5))[ROWS.index(row_id)]
    return dict(row_id=row_id,order=order,cells=cells,extent=extent,width=width,center=CENTER,
                viscosity=.01,time=0.,physical_dimension=5,minimum_support_samples=2,nonlinear_witness=True)

def word(hasher,value):
    hasher.update(struct.pack('<Q',value))

def tag(hasher,value):
    text=value.encode();word(hasher,len(text));hasher.update(text)

def validate_grid(grid,request):
    shape(grid,'axis_dimension dimension nodes weights node_bits weight_bits lower_bits upper_bits spacing_bits grid_sha256 derivative_sha256')
    n=request['cells']*(request['order']+1);N=n**5
    for key,expected in [('axis_dimension',n),('dimension',N)]:
        integer(grid[key],1)
        if grid[key]!=expected: raise ValueError('changed tensor dimension')
    vector(grid['nodes'],n);vector(grid['weights'],n,0.)
    for key in ('node_bits','weight_bits'):
        if type(grid[key]) is not list or len(grid[key])!=n:raise ValueError('wrong bit array')
        for x in grid[key]:integer(x,0,2**64-1)
    for key in ('lower_bits','upper_bits','spacing_bits'):integer(grid[key],0,2**64-1)
    digest(grid['grid_sha256']);digest(grid['derivative_sha256'])
    h=2*request['extent']/request['cells'];q=request['order']+1
    ref=[-1.,1.] if q==2 else [-1.,0.,1.]
    weights=[1.,1.] if q==2 else [1/3,4/3,1/3]
    deriv=[[-.5,.5],[-.5,.5]] if q==2 else [[-1.5,2.,-.5],[-.5,0.,.5],[.5,-2.,1.5]]
    axis=[-request['extent']+h*(c+.5*(x+1)) for c in range(request['cells']) for x in ref]
    mass=[.5*h*w for _ in range(request['cells']) for w in weights]
    if grid['node_bits']!=[bits(x) for x in axis] or grid['weight_bits']!=[bits(x) for x in mass]:
        raise ValueError('changed represented nodes or weights')
    if [bits(x) for x in grid['nodes']]!=grid['node_bits'] or [bits(x) for x in grid['weights']]!=grid['weight_bits']:
        raise ValueError('inconsistent node/weight bits')
    if [grid[x] for x in ('lower_bits','upper_bits','spacing_bits')]!=[bits(-request['extent']),bits(request['extent']),bits(h)]:
        raise ValueError('changed represented bounds')
    dh=hashlib.sha256();tag(dh,'quest-weak-derivative-v1');word(dh,n)
    for c in range(request['cells']):
        for a in range(q):
            row=[(c*q+b,2/h*deriv[a][b]) for b in range(q)]
            if a==0:row += [(c*q,1/(h*weights[a])),(((c+request['cells']-1)%request['cells'])*q+q-1,-1/(h*weights[a]))]
            if a==q-1:row += [(c*q+a,-1/(h*weights[a])),(((c+1)%request['cells'])*q,1/(h*weights[a]))]
            word(dh,len(row))
            for j,d in row:word(dh,j);word(dh,bits(d))
    if dh.hexdigest()!=grid['derivative_sha256']:raise ValueError('changed derivative recipe')
    gh=hashlib.sha256();tag(gh,'quest-weak-grid-v1')
    for x in (5,request['order'],request['cells'],n,N):word(gh,x)
    for x in (-request['extent'],request['extent']):word(gh,bits(x))
    tag(gh,'axis');word(gh,n)
    for x,w in zip(axis,mass):word(gh,bits(x));word(gh,bits(w))
    tag(gh,'derivative-sha256');gh.update(dh.digest())
    if gh.hexdigest()!=grid['grid_sha256']:raise ValueError('changed grid digest')
    return axis

def validate_resource(r, preparation=None):
    shape(r,RESOURCE_FIELDS)
    for key,value in r.items():
        if key=='caller_declared_input_preparation_work' and value is None:continue
        integer(value)
    if r['caller_declared_input_preparation_work']!=preparation:raise ValueError('unbound prior input charge')
    if r['physical_work']!=r['physical_calls']*100000 or r['physical_calls_attempted']>r['physical_calls']:
        raise ValueError('invalid original physical work/calls')
    if min(r['peak_bytes'],r['constructor_peak_bytes'],r['retained_source_bytes'],r['source_work'])<=0:
        raise ValueError('missing source storage/work')
    if r['peak_bytes']<r['constructor_peak_bytes']:raise ValueError('lost construction peak')

def validate_initial(initial,N,request):
    shape(initial,'probability coordinate_means coordinate_variances mean_minus_continuum_center standard_deviations_per_spacing maximum_coefficient_probability effective_coefficients outer_cell_occupation zero_exterior_trace supported_coefficients ensemble_sha256')
    number(initial['probability'],0.);close(initial['probability'],1.)
    for name in ('coordinate_means','mean_minus_continuum_center'):vector(initial[name],5)
    for name in ('coordinate_variances','standard_deviations_per_spacing'):vector(initial[name],5,0.)
    number(initial['maximum_coefficient_probability'],(1.-1e-11)/N,1.000000000001)
    number(initial['effective_coefficients'],1.-1e-11,N*(1.+1e-11));number(initial['outer_cell_occupation'],0.,1.000000000001)
    integer(initial['supported_coefficients'],1,N);boolean(initial['zero_exterior_trace']);digest(initial['ensemble_sha256'])
    if initial['zero_exterior_trace'] is not True:raise ValueError('compact interior fixture has exterior trace')
    for mean,center,bias in zip(initial['coordinate_means'],request['center'],initial['mean_minus_continuum_center']):close(bias,mean-center)

def validate_source(src,ir):
    shape(src,'physical_recipe_sha256 chart_mass_residual constraint_residual extraction_probe_error extraction_scaled_error coefficient_roundoff_estimate residual_evaluations resources')
    digest(src['physical_recipe_sha256']);integer(src['residual_evaluations'],1)
    for key in ('chart_mass_residual','constraint_residual','extraction_probe_error','extraction_scaled_error','coefficient_roundoff_estimate'):number(src[key],0.)
    validate_resource(src['resources'])
    if src['resources']['external_bytes']!=ir['declared_source_external_bytes'] or src['resources']['source_work']!=100000000 or src['resources']['physical_calls']!=58:
        raise ValueError('changed construction/live owner receipt')
    if src['residual_evaluations']!=58 or src['resources']['physical_calls_attempted']!=58:raise ValueError('wrong extraction calls')
    sr=src['resources']
    if sr['constructor_peak_bytes']!=8388608+ir['declared_source_external_bytes'] or sr['peak_bytes']!=sr['constructor_peak_bytes']:
        raise ValueError('lost fixed source construction envelope')
    return src['resources']


def validate_sampling(sample,axis,request):
    n=request['cells']*(request['order']+1)
    shape(sample,'distinct_samples minimum_required maximum_distinct_spacing width_per_spacing support_margin support_intersects_boundary policy_passed convergence_certified')
    unique=list(dict.fromkeys(axis));counts=[sum(abs((x-c)/request['width'])<1 for x in unique) for c in CENTER]
    if type(sample['distinct_samples']) is not list or len(sample['distinct_samples'])!=5:raise ValueError('wrong support shape')
    for x in sample['distinct_samples']:integer(x,0,n)
    integer(sample['minimum_required'],1)
    if sample['distinct_samples']!=counts or sample['minimum_required']!=2:raise ValueError('changed support policy')
    for key in ('support_intersects_boundary','policy_passed','convergence_certified'):boolean(sample[key])
    close(sample['maximum_distinct_spacing'],max(b-a for a,b in zip(unique,unique[1:])))
    close(sample['width_per_spacing'],request['width']/sample['maximum_distinct_spacing'])
    close(sample['support_margin'],min(request['extent']-abs(c)-request['width'] for c in CENTER))
    if sample['support_intersects_boundary'] is not False or sample['convergence_certified'] is not False:raise ValueError('wrong support scope')
    return counts


def validate_against(row,request):
    if not report_finite(row):raise ValueError('nonfinite or excessive report')
    shape(row,'schema status phase error request limits plan input_resources sampling represented_grid initial source diagnostic_resources validated_rows visited_rows diagnostic quantum_execution history_execution convergence_certified')
    if row['schema']!='quest-configuration-weak-row-v1' or row['request']!=request or row['limits']!=CAPS:
        raise ValueError('changed fixed request/caps/schema')
    shape(row['request'],'row_id order cells extent width center viscosity time physical_dimension minimum_support_samples nonlinear_witness')
    for key in ('order','cells','physical_dimension','minimum_support_samples'):integer(row['request'][key],1)
    boolean(row['request']['nonlinear_witness']);vector(row['request']['center'],5)
    for key in ('extent','width','viscosity','time'):number(row['request'][key],0.)
    for value in row['limits'].values():integer(value,1)
    if any(row[key] is not False for key in ('quantum_execution','history_execution','convergence_certified')):raise ValueError('unexecuted scientific claim')
    if type(row['status']) is not str or type(row['phase']) is not str:raise ValueError('invalid status')
    if row['error'] is not None and (type(row['error']) is not str or not 1<=len(row['error'])<=1024):raise ValueError('invalid error')
    n=request['cells']*(request['order']+1);N=n**5;work=1048576+16384*n+4096*N
    if row['plan'] is None:
        if row['status']!='construction-rejected' or row['phase']!='input admission' or row['error'] is None:
            raise ValueError('missing planned input')
        shape(row['input_resources'],'input_work peak_bytes grid_bytes state_capacity_bytes support_capacity_bytes metadata_capacity_bytes statistics_capacity_bytes declared_source_external_bytes declared_query_extra_bytes serializer_bytes helper_bytes')
        for key,value in row['input_resources'].items():
            integer(value)
            if value!=(65536 if key in ('serializer_bytes','helper_bytes') else 0):raise ValueError('invented preflight owner')
        if any(row[k] is not None for k in ('sampling','represented_grid','initial','source','diagnostic_resources','diagnostic')) or row['validated_rows']!=0 or row['visited_rows']!=0:
            raise ValueError('invented preflight progress')
        integer(row['validated_rows']);integer(row['visited_rows'])
        return row
    shape(row['plan'],'axis_dimension dimension input_work state_payload_bytes')
    if row['plan']!=dict(axis_dimension=n,dimension=N,input_work=work,state_payload_bytes=16*N):raise ValueError('changed producer plan')
    for x in row['plan'].values():integer(x,1)
    ir=row['input_resources'];shape(ir,'input_work peak_bytes grid_bytes state_capacity_bytes support_capacity_bytes metadata_capacity_bytes statistics_capacity_bytes declared_source_external_bytes declared_query_extra_bytes serializer_bytes helper_bytes')
    for x in ir.values():integer(x)
    if ir['input_work']!=work or ir['serializer_bytes']!=65536 or ir['helper_bytes']!=65536 or not 0<ir['peak_bytes']<=CAPS['max_bytes']:raise ValueError('invalid producer receipt')
    integer(row['validated_rows'],0,N);integer(row['visited_rows'],0,N)
    if row['status']=='construction-rejected' and row['phase']=='grid':
        if row['error'] is None or row['validated_rows'] or row['visited_rows'] or any(row[k] is not None for k in ('initial','source','diagnostic_resources','diagnostic')):
            raise ValueError('invented grid-failure suffix')
        if any(ir[k] for k in ('state_capacity_bytes','support_capacity_bytes','statistics_capacity_bytes','declared_source_external_bytes','declared_query_extra_bytes')) or ir['peak_bytes']<196608:
            raise ValueError('invented grid-failure allocation')
        if row['represented_grid'] is None:
            if row['sampling'] is not None:raise ValueError('sampling without grid')
        else:
            axis=validate_grid(row['represented_grid'],request)
            validate_sampling(row['sampling'],axis,request)
            if row['sampling']['policy_passed'] is not False:raise ValueError('premature policy success')
        return row
    axis=validate_grid(row['represented_grid'],request)
    sample=row['sampling'];counts=validate_sampling(sample,axis,request)
    integer(row['validated_rows'],0,N);integer(row['visited_rows'],0,N)
    if row['status']=='sampling-rejected':
        if row['phase']!='sampling policy' or min(counts)>=2 or sample['policy_passed'] or row['error'] is None:raise ValueError('invalid sampling rejection')
        if any(row[x] is not None for x in ('initial','source','diagnostic_resources','diagnostic')) or row['validated_rows'] or row['visited_rows'] or ir['state_capacity_bytes']:raise ValueError('fabricated later phase')
        return row
    if sample['policy_passed'] is not True or min(counts)<2:raise ValueError('failed sampling entered later phase')
    if row['status']=='construction-rejected':
        if row['phase'] not in ('source construction','state construction','input diagnostics') or row['error'] is None or row['initial'] is not None or row['diagnostic'] is not None or row['validated_rows'] or row['visited_rows']:
            raise ValueError('invalid construction prefix')
        if ir['declared_source_external_bytes']!=131072+ir['grid_bytes']+ir['metadata_capacity_bytes']+ir['support_capacity_bytes']:
            raise ValueError('lost construction external owner')
        if ir['support_capacity_bytes']<40 or ir['metadata_capacity_bytes']<32*n+128:
            raise ValueError('missing construction metadata')
        if row['phase']=='source construction':
            if row['source'] is not None or any(ir[k] for k in ('state_capacity_bytes','statistics_capacity_bytes','declared_query_extra_bytes')):
                raise ValueError('invented prepared source/state')
            r=row['diagnostic_resources']
            if r is not None:
                shape(r,RESOURCE_FIELDS)
                for key,value in r.items():
                    if key=='caller_declared_input_preparation_work':
                        if value is not None:raise ValueError('early input charge')
                    else:integer(value)
                if r['source_work']!=100000000 or r['physical_calls']!=58 or r['physical_work']!=5800000 or r['physical_calls_attempted']>58 or r['external_bytes']!=ir['declared_source_external_bytes'] or r['constructor_peak_bytes']!=8388608+r['external_bytes'] or r['peak_bytes']!=r['constructor_peak_bytes'] or ir['peak_bytes']<r['peak_bytes']:
                    raise ValueError('invalid failed extraction receipt')
                if any(r[k] for k in ('grid_bytes','accessible_state_bytes','result_bytes','scratch_bytes','supported_rows','row_query_work','maximum_row_entries')):
                    raise ValueError('invented query before source')
        else:
            sr=validate_source(row['source'],ir)
            if row['diagnostic_resources']!=sr or ir['peak_bytes']<sr['peak_bytes']:
                raise ValueError('lost prepared source on failure')
            if row['phase']=='state construction':
                if ir['statistics_capacity_bytes']:raise ValueError('invented later statistics')
                if ir['state_capacity_bytes'] not in (0,) and ir['state_capacity_bytes']<16*N:raise ValueError('incomplete returned state capacity')
            elif ir['state_capacity_bytes']<16*N:raise ValueError('statistics without full state')
            if ir['declared_query_extra_bytes']!=max(0,ir['state_capacity_bytes']-16*N):raise ValueError('lost failed state capacity')
        return row
    validate_initial(row['initial'],N,request)
    initial=row['initial']
    for variance,width in zip(initial['coordinate_variances'],initial['standard_deviations_per_spacing']):
        close(width,math.sqrt(variance)/sample['maximum_distinct_spacing'])
    pmax=initial['maximum_coefficient_probability'];effective=initial['effective_coefficients']
    if effective<(1.-1e-10)/pmax or effective>(1.+1e-10)/(pmax*pmax):raise ValueError('impossible concentration')
    if ir['state_capacity_bytes']<16*N or ir['declared_query_extra_bytes']!=ir['state_capacity_bytes']-16*N:raise ValueError('lost input capacity')
    if ir['declared_source_external_bytes']!=131072+ir['grid_bytes']+ir['metadata_capacity_bytes']+ir['support_capacity_bytes']:raise ValueError('lost retained producer owner')
    src=row['source'];sr=validate_source(src,ir)
    if ir['statistics_capacity_bytes']<120 or ir['support_capacity_bytes']<40 or ir['metadata_capacity_bytes']<32*n+128:
        raise ValueError('missing actual producer output capacity')
    resources=row['diagnostic_resources'];validate_resource(resources,work)
    if row['status']=='diagnostic-rejected' and row['phase'] in ('input admission','state validation'):
        if row['error'] is None or row['diagnostic'] is not None or row['visited_rows'] or resources['physical_calls']!=58 or resources['physical_calls_attempted']!=58 or resources['supported_rows']:
            raise ValueError('invented pre-contraction work')
        if row['phase']=='input admission' and row['validated_rows']:raise ValueError('invented validation progress')
        if resources['retained_source_bytes']!=sr['retained_source_bytes'] or resources['constructor_peak_bytes']!=sr['constructor_peak_bytes'] or ir['peak_bytes']<resources['peak_bytes'] or resources['external_bytes']!=ir['declared_source_external_bytes']+ir['declared_query_extra_bytes'] or resources['accessible_state_bytes']!=16*N:
            raise ValueError('lost failed query input/source')
        if resources['grid_bytes'] not in (0,ir['grid_bytes']):raise ValueError('invalid failed grid charge')
        if row['phase']=='state validation' and (resources['grid_bytes']!=ir['grid_bytes'] or resources['row_query_work']<=0 or resources['maximum_row_entries']!=5*(request['order']+3)):
            raise ValueError('validation without prepared recipe')
        return row
    if resources['result_bytes']<=0 or resources['scratch_bytes']<=0:raise ValueError('missing diagnostic output/scratch')
    if resources['retained_source_bytes']!=sr['retained_source_bytes'] or resources['constructor_peak_bytes']!=sr['constructor_peak_bytes']:
        raise ValueError('lost retained source identity/capacity')
    live=sum(resources[key] for key in ('retained_source_bytes','external_bytes','accessible_state_bytes','grid_bytes','result_bytes','scratch_bytes'))
    if resources['peak_bytes']<live or ir['peak_bytes']<resources['peak_bytes'] or ir['peak_bytes']<sr['peak_bytes']:
        raise ValueError('lost whole live managed peak')
    S=row['initial']['supported_coefficients']
    if resources['supported_rows']!=S or resources['physical_calls']!=58+2*S+1 or resources['accessible_state_bytes']!=16*N:raise ValueError('wrong full state/query charge')
    if resources['external_bytes']!=ir['declared_source_external_bytes']+ir['declared_query_extra_bytes'] or resources['grid_bytes']!=ir['grid_bytes']:raise ValueError('lost live overlap')
    if resources['maximum_row_entries']!=5*(request['order']+3) or resources['row_query_work']<=0:raise ValueError('invalid row allowance')
    if resources['source_work']<100000000+work+4096*(n+N)+S*resources['row_query_work']:raise ValueError('undercounted source work')
    if row['validated_rows']!=N:raise ValueError('state not fully validated')
    if row['status'] in ('diagnostic-rejected','numerical-failure'):
        if row['error'] is None or row['diagnostic'] is not None:raise ValueError('failure fabricated result')
        if row['phase']=='contraction admission':
            if row['status']!='diagnostic-rejected' or row['visited_rows'] or resources['physical_calls_attempted']!=58:
                raise ValueError('invalid contraction preflight failure')
            if all(resources[k]<=CAPS[c] for k,c in [('source_work','max_source_work'),('physical_work','max_physical_work'),('physical_calls','max_physical_calls'),('peak_bytes','max_bytes')]):
                raise ValueError('unexplained contraction admission failure')
        elif row['phase']=='contraction':
            v=row['visited_rows'];actual=resources['physical_calls_attempted']
            if row['status']!='numerical-failure' or v>S:
                raise ValueError('invalid failed row progress')
            lower=59+2*max(0,v-1);upper=59+2*v
            if not lower<=actual<=upper:raise ValueError('inconsistent attempted callbacks')
            for key,cap in [('source_work','max_source_work'),('physical_work','max_physical_work'),('physical_calls','max_physical_calls'),('peak_bytes','max_bytes')]:
                if resources[key]>CAPS[cap]:raise ValueError('failure entered unadmitted contraction')
        else:raise ValueError('unknown failed contraction phase')
        return row
    if row['status']!='completed' or row['phase']!='complete' or row['error'] is not None:raise ValueError('unvalidated non-completed phase')
    if row['visited_rows']!=S or resources['physical_calls_attempted']!=resources['physical_calls']:raise ValueError('wrong completed query progress')
    for key,cap in [('source_work','max_source_work'),('physical_work','max_physical_work'),('physical_calls','max_physical_calls'),('peak_bytes','max_bytes')]:
        if resources[key]>CAPS[cap]:raise ValueError('completed beyond budget')
    d=row['diagnostic'];shape(d,'source_identity time dimension probability probability_rate expectation raw_skew_rate normalized_rate physical_rate absolute_defect scaled_defect zero_exterior_trace outer_cell_occupation coordinate_standard_deviations maximum_coefficient_probability effective_coefficients nonlinear_mean_square cartesian_energy_discrepancy chart_mass_residual extraction_probe_error convergence_certified')
    integer(d['source_identity'],1,2**64-1);integer(d['dimension'],1)
    number(d['time']);number(d['probability'],0.);number(d['probability_rate'])
    if d['time']!=0 or d['dimension']!=N or d['convergence_certified'] is not False or d['zero_exterior_trace'] is not True:raise ValueError('wrong diagnostic scope')
    for key in RATE_FIELDS:vector(d[key],6,0. if key in ('absolute_defect','scaled_defect') else None)
    vector(d['coordinate_standard_deviations'],5,0.)
    for key in ('nonlinear_mean_square','cartesian_energy_discrepancy','chart_mass_residual','extraction_probe_error'):number(d[key],0.)
    number(d['outer_cell_occupation'],0.,1.000000000001);number(d['maximum_coefficient_probability'],(1.-1e-11)/N,1.000000000001);number(d['effective_coefficients'],1.-1e-11,N*(1+1e-11))
    close(d['probability'],row['initial']['probability'])
    for i in range(6):
        close(d['normalized_rate'][i],d['raw_skew_rate'][i]-d['expectation'][i]*d['probability_rate']/d['probability'])
        close(d['absolute_defect'][i],abs(d['normalized_rate'][i]-d['physical_rate'][i]))
        close(d['scaled_defect'][i],d['absolute_defect'][i]/max(1.,abs(d['physical_rate'][i])))
    for a,b in zip(d['expectation'][:5],row['initial']['coordinate_means']):close(a,b)
    number(d['expectation'][5],0.)
    for std,variance in zip(d['coordinate_standard_deviations'],row['initial']['coordinate_variances']):close(std,math.sqrt(variance))
    for key in ('maximum_coefficient_probability','effective_coefficients','outer_cell_occupation'):close(d[key],row['initial'][key])
    return row


def validate(row,row_id):
    return validate_against(row,parameters(row_id))


def compare_pair(kind,a,b,left_request,right_request):
    """Validate both declared requests before reusing the common dependent-rate calculation."""
    left=left_request['row_id'];right=right_request['row_id']
    record={'kind':kind,'left':left,'right':right,'status':'unavailable','convergence_certified':False}
    if a is None or b is None:
        record['reason']='missing validated row';return record
    try:validate_against(a,left_request);validate_against(b,right_request)
    except (ValueError,TypeError,KeyError,OverflowError,RecursionError):
        record['reason']='row validation failed';return record
    if a['initial'] is not None and b['initial'] is not None:
        record['sampled_inputs']={'ensemble_sha256':[a['initial']['ensemble_sha256'],b['initial']['ensemble_sha256']],
            'mean_bias_left':a['initial']['mean_minus_continuum_center'],'mean_bias_right':b['initial']['mean_minus_continuum_center']}
    if a['status']!='completed' or b['status']!='completed':
        record['reason']='rates require both completed requests';return record
    if a['source']['physical_recipe_sha256']!=b['source']['physical_recipe_sha256'] or a['diagnostic']['source_identity']!=b['diagnostic']['source_identity']:
        record['reason']='physical source identity differs';return record
    metrics=[]
    for i in range(6):
        da,db=a['diagnostic'],b['diagnostic'];pa,pb=da['physical_rate'][i],db['physical_rate'][i]
        ga,gb=da['normalized_rate'][i],db['normalized_rate'][i];ea,eb=ga-pa,gb-pb
        scale=max(1.,abs(pa),abs(pb));resolved=abs(pa)>1e-10*max(1.,abs(da['expectation'][i])) and abs(pb)>1e-10*max(1.,abs(db['expectation'][i]))
        metrics.append({'coordinate':i if i<5 else 'integrated_energy','signed_defects':[ea,eb],
            'generator_delta':gb-ga,'physical_delta':pb-pa,'defect_delta':eb-ea,
            'decomposition_roundoff':(gb-ga)-((pb-pa)+(eb-ea)),
            'reference_rate_resolved':resolved,'defect_ratio':abs(eb/ea) if resolved and abs(ea)>1e-12*scale else None})
    record.update(status='finite-sensitivity-only',metrics=metrics,
        coordinate_reference_norms=[math.hypot(*r['diagnostic']['physical_rate'][:5]) for r in (a,b)],
        coordinate_reference_resolved=[math.hypot(*r['diagnostic']['physical_rate'][:5])>1e-10*max(1.,math.hypot(*r['diagnostic']['expectation'][:5])) for r in (a,b)],
        energy_reference_resolved=[abs(r['diagnostic']['physical_rate'][5])>1e-10*max(1.,abs(r['diagnostic']['expectation'][5])) for r in (a,b)],
        nonlinear_witness_resolved=[math.sqrt(r['diagnostic']['nonlinear_mean_square'])>1e-10*max(1.,math.hypot(*r['diagnostic']['physical_rate'][:5])) for r in (a,b)])
    if not report_finite(record):record={'kind':kind,'left':left,'right':right,'status':'arithmetic-failure','convergence_certified':False}
    return record


def comparisons(rows):
    pairs=[('h-3-4',ROWS[0],ROWS[1]),('h-4-5',ROWS[1],ROWS[2]),('intended-fixed-spacing-domain',ROWS[0],ROWS[5]),('width-sensitivity',ROWS[0],ROWS[6])]
    return [compare_pair(kind,rows.get(left),rows.get(right),parameters(left),parameters(right)) for kind,left,right in pairs]


def child_limits():
    resource.setrlimit(resource.RLIMIT_AS,(AS_BYTES,AS_BYTES));resource.setrlimit(resource.RLIMIT_FSIZE,(FILE_BYTES,FILE_BYTES));resource.setrlimit(resource.RLIMIT_CORE,(0,0))

def collect(binary,row_id,output,timeout=SECONDS,validator=validate):
    stdout=output/(row_id+'.json');stderr=output/(row_id+'.stderr');timing=output/(row_id+'.time')
    started=time.monotonic();status='exited'
    with stdout.open('xb') as out,stderr.open('xb') as err:
        child=subprocess.Popen(['/usr/bin/time','-f','%e %M','-o',str(timing),str(binary),row_id],stdout=out,stderr=err,preexec_fn=child_limits,start_new_session=True)
        try:code=child.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            status='timeout';os.killpg(child.pid,signal.SIGKILL);code=child.wait()
    rss=None;child_elapsed=None
    if timing.exists():
        fields=timing.read_text().splitlines()
        if fields and len(fields[-1].split())==2:
            try:
                elapsed_token,rss_token=fields[-1].split()
                elapsed_value=float(elapsed_token);rss_value=int(rss_token)
                if math.isfinite(elapsed_value) and elapsed_value>=0 and rss_value>0:
                    child_elapsed=elapsed_value;rss=rss_value
            except (ValueError,OverflowError):pass
    result={'row_id':row_id,'process_status':status,'exit_code':code,'elapsed_seconds':time.monotonic()-started,
        'peak_rss_kib':rss,'child_elapsed_seconds':child_elapsed,'timing':timing.name if timing.exists() else None,
        'timing_sha256':sha256(timing) if timing.exists() else None,'stdout':stdout.name,'stderr':stderr.name,'stdout_sha256':sha256(stdout),'stderr_sha256':sha256(stderr),
        'validated':False,'protocol_status':None}
    parsed=None
    if status=='exited' and code==0:
        try:
            if stdout.stat().st_size>FILE_BYTES:raise ValueError('oversized output')
            parsed=decode_report(stdout.read_text());validator(parsed,row_id)
            result['validated']=True;result['protocol_status']=parsed['status']
        except (ValueError,TypeError,KeyError,OSError,OverflowError,RecursionError) as error:
            result['validation_error']=str(error);parsed=None
    return result,parsed


def save_receipt(path,receipt):
    temporary=path.with_suffix('.tmp')
    temporary.write_text(json.dumps(receipt,indent=2,allow_nan=False)+'\n')
    temporary.replace(path)


def main():
    if len(sys.argv)!=3:raise SystemExit('usage: configuration_weak.py PINNED_BINARY NEW_OUTPUT_DIRECTORY')
    binary=Path(sys.argv[1]).resolve(strict=True);output=Path(sys.argv[2]);output.mkdir(exist_ok=False)
    identity=sha256(binary);receipt={'schema':'quest-configuration-weak-campaign-v1','binary_sha256':identity,
        'bounds':{'address_space_bytes':AS_BYTES,'child_seconds':SECONDS,'captured_file_bytes':FILE_BYTES},'rows':[],'comparisons':[]}
    valid={}
    for row_id in ROWS:
        if sha256(binary)!=identity:raise RuntimeError('binary changed; refusing more children')
        process,parsed=collect(binary,row_id,output);receipt['rows'].append(process)
        if parsed is not None:valid[row_id]=parsed
        receipt['comparisons']=comparisons(valid)
        save_receipt(output/'receipt.json',receipt)
    receipt['binary_sha256_after']=sha256(binary)
    save_receipt(output/'receipt.json',receipt)

if __name__=='__main__':main()
