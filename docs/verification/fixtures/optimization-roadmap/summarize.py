#!/usr/bin/env python3
"""Summarize preserved runs without dropping failed or exhausted records."""
import argparse
from collections import defaultdict
from fractions import Fraction
import json
import math
from pathlib import Path
from statistics import median


def rows(path):
    return [json.loads(line) for line in path.read_text().splitlines() if line.strip()]


def complete_samples(group, expected):
    return (len(group) == expected
            and sorted(row['sample'] for row in group) == list(range(expected))
            and all(row.get('status') == 'complete' for row in group))


def component_ns(samples, component):
    return Fraction(median(row['elapsed_ns'] for row in samples),
                    10 if component == 'warm_execution' else 1)


def compiler(directory):
    completion=json.loads((directory/'completion.json').read_text())
    groups=defaultdict(list)
    results=directory/'results.jsonl'
    for row in rows(results) if results.exists() else []:
        if 'sample' in row:
            groups[(row['case'],row['stage'])].append(row)
    summary=[]
    for (case,stage),group in sorted(groups.items()):
        item={'case':case,'stage':stage,'samples':len(group),
              'failed':[row for row in group if row['status']!='complete'],
              'completion_reasons':[row.get('detail',{}).get('completion') for row in group if row.get('detail')]}
        comparable=(completion.get('status') == 'complete' and complete_samples(group, 5))
        item['measurement_status']='complete' if comparable else 'partial'
        if comparable:
            for key in ['elapsed_ns','allocations','requested_bytes','retained_bytes','peak_bytes']:
                item['median_'+key]=median(row[key] for row in group)
        else:
            item['raw_rows']=group
        summary.append(item)
    return {'manifest':completion,'groups':summary}


def native(directory):
    completion=json.loads((directory/'completion.json').read_text())
    results=[]
    for configuration in completion['cases']:
        mode,kind,nodes=configuration['mode'],configuration['kind'],configuration['nodes']
        path=directory/f'{mode}-{kind}-{nodes}.jsonl'
        records=rows(path) if path.exists() else []
        group=defaultdict(list)
        if mode=='mpi':
            max_path=directory/f'{mode}-{kind}-{nodes}-max-rank.jsonl'
            measurements=rows(max_path) if max_path.exists() else []
        else:
            measurements=records
        for row in measurements:
            if 'sample' not in row: continue
            stage,component=row['stage'].rsplit('/',1)
            group[(row['case'],stage,component)].append(row)
        stage_rows=[]
        for case,stage in sorted({(case,stage) for case,stage,_ in group}):
            components=['search','preparation','warm_execution']
            expected={'search':1,'preparation':5,'warm_execution':5}
            stage_records=[row for component in components for row in group.get((case,stage,component),[])]
            stage_complete=(completion.get('status') == 'complete'
                            and configuration.get('status') == 'complete'
                            and all(complete_samples(group.get((case,stage,component),[]), expected[component])
                                    for component in components))
            item={'case':case,'stage':stage,'components':{},
                  'measurement_status':'complete' if stage_complete else 'partial',
                  'optimizer_completion':sorted({row.get('detail',{}).get('completion') for row in stage_records
                                                 if row.get('detail',{}).get('completion')})}
            if not stage_complete:
                item['raw_rows']=stage_records
            else:
                for component in components:
                    samples=group[(case,stage,component)]
                    item['components'][component]=float(component_ns(samples, component))
            base_complete=(completion.get('status') == 'complete'
                           and configuration.get('status') == 'complete'
                           and all(complete_samples(group.get((case,'unchanged',component),[]), expected[component])
                                   for component in components))
            if stage_complete and base_complete:
                base={component:component_ns(group[(case,'unchanged',component)],component) for component in components}
                observed={component:component_ns(group[(case,stage,component)],component) for component in components}
                saved=base['warm_execution']-observed['warm_execution']
                extra=observed['search']-base['search']+observed['preparation']-base['preparation']
                item['execution_time_ratio']=float(observed['warm_execution']/base['warm_execution']) if base['warm_execution']>0 else None
                item['incremental_setup_ns']=float(extra)
                item['reuse_break_even_including_search']=math.ceil(max(Fraction(0),extra)/saved) if saved>0 else None
            stage_rows.append(item)
        results.append({'configuration':configuration,'stages':stage_rows,
                        'failed_records':[row for row in records if row.get('status')=='failed'],
                        'completion_reasons':sorted({row['detail']['completion'] for row in records if row.get('detail',{}).get('completion')})})
    return {'manifest':completion,'configurations':results}


def main():
    parser=argparse.ArgumentParser()
    parser.add_argument('--compiler',type=Path,action='append',default=[])
    parser.add_argument('--native',type=Path,action='append',default=[])
    parser.add_argument('--output',type=Path,required=True)
    args=parser.parse_args()
    if args.output.exists(): raise SystemExit('choose a fresh summary path')
    result={'schema':1,'timing':'median wall nanoseconds; native execution per completed run; MPI uses maximum rank per sample',
            'break_even':'ceil(max(0, incremental search plus preparation)/positive per-run saving); null means no observed saving, not a proof of slowdown',
            'compiler':{str(path):compiler(path) for path in args.compiler},
            'native':{str(path):native(path) for path in args.native}}
    args.output.write_text(json.dumps(result,indent=2)+'\n')


if __name__=='__main__': main()
