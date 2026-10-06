#!/usr/bin/env python3
"""One additive initial diagnostic; common producer schema/caps and no physical retry."""
from pathlib import Path
import hashlib
import sys
import configuration_weak as shared

ROW = 'p2-c2-e1-w1_2'
SCHEMA = 'quest-configuration-weak-energy-shell-v2'
REFERENCE = Path(__file__).resolve().parents[2]/'data/2026-10-06-configuration-weak-campaign/p1-c4-e1-w1_2.json'
REFERENCE_SHA = '14664ebc2b353933ef9dabd3e9768f64e4ba5edfa9b4a5d6731e1fc8351d7d76'


def request():
    row=shared.parameters(shared.ROWS[0])
    row.update(row_id=ROW,order=2,cells=2)
    return row


def validate(row,row_id):
    if row_id!=ROW:raise ValueError('only the additive DG2/two-cell row is supported')
    return shared.validate_against(row,request())


def reference_snapshot(path):
    with path.open('rb') as stream:
        raw=stream.read(shared.FILE_BYTES+1)
    if len(raw)>shared.FILE_BYTES:
        raise ValueError('published c4 reference exceeds bounded size')
    return raw,hashlib.sha256(raw).hexdigest()


def load_reference(path=REFERENCE):
    raw,identity=reference_snapshot(path)
    if identity!=REFERENCE_SHA:
        raise ValueError('published c4 reference changed')
    row=shared.decode_report(raw.decode('utf-8'))
    shared.validate(row,'p1-c4-e1-w1_2')
    return row


def comparison(reference,row):
    result=shared.compare_pair('DG1-c4-vs-DG2-c2-quadrature-and-stencil',reference,row,
                               shared.parameters('p1-c4-e1-w1_2'),request())
    result.update(reference_sha256=REFERENCE_SHA,
                  quadrature_weights_changed=True,
                  pure_h_or_p_refinement=False,
                  interpretation='Different configuration weights and stencil; sampled physical expectation must be compared separately from generator rate.')
    return result


def main():
    if len(sys.argv)!=3:raise SystemExit('usage: configuration_weak_energy_shell_v2.py PINNED_BINARY NEW_OUTPUT_DIRECTORY')
    reference=load_reference()  # Validate immutable baseline before launching any child.
    binary=Path(sys.argv[1]).resolve(strict=True);identity=shared.sha256(binary)
    output=Path(sys.argv[2]);output.mkdir(exist_ok=False)
    receipt={'schema':SCHEMA,'fixed_row_id':ROW,'binary_sha256':identity,
             'bounds':{'address_space_bytes':shared.AS_BYTES,'child_seconds':shared.SECONDS,'captured_file_bytes':shared.FILE_BYTES},
             'reference_sha256':REFERENCE_SHA,'rows':[],'comparison':None,
             'history_execution':False,'quantum_execution':False,'convergence_certified':False}
    shared.save_receipt(output/'receipt.json',receipt)
    process,parsed=shared.collect(binary,ROW,output,validator=validate)
    receipt['rows'].append(process)
    receipt['comparison']={'status':'unavailable','reason':'post-run provenance pending','convergence_certified':False}
    receipt['provenance_status']='pending'
    receipt['provenance_unchanged']=None
    shared.save_receipt(output/'receipt.json',receipt)
    receipt['provenance_errors']=[]
    for name,path in [('binary',binary),('reference',REFERENCE)]:
        try:
            identity_after=reference_snapshot(path)[1] if name=='reference' else shared.sha256(path)
        except (OSError,ValueError) as error:
            identity_after=None
            receipt['provenance_errors'].append({'source':name,'error_kind':type(error).__name__})
        receipt[name+'_sha256_after']=identity_after
    receipt['provenance_unchanged']=(receipt['binary_sha256_after']==identity and receipt['reference_sha256_after']==REFERENCE_SHA)
    receipt['provenance_status']='verified' if receipt['provenance_unchanged'] else 'failed'
    if receipt['provenance_unchanged']:
        receipt['comparison']=comparison(reference,parsed)
    else:
        receipt['comparison']={'status':'unavailable','reason':'binary or reference identity changed or unavailable after execution','convergence_certified':False}
    shared.save_receipt(output/'receipt.json',receipt)

if __name__=='__main__':main()
