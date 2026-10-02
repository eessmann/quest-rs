"""Synthetic summary acceptance/math tests; never runs observers or Cargo."""
import importlib.util
import json
import pathlib
import tempfile
import unittest

FIXTURE = pathlib.Path(__file__).parent
WORKLOADS = [('binary64', 256, 53), ('binary64', 1024, 53), ('offline', 16, 128),
             ('offline', 16, 256), ('offline', 256, 128), ('offline', 256, 256)]
LABELS = ['baseline', 'a', 'ab', 'abc']


def campaign(root):
    (root/'measurement-completion.json').write_text(json.dumps({
        'all_trials_succeeded': True, 'sources_unchanged': True, 'workloads_valid': True,
        'validated_workload_records': 72, 'labels': LABELS, 'trials': 3, 'errors': []}))
    for label in LABELS:
        folder=root/label
        folder.mkdir()
        for trial, value in enumerate([30, 10, 20], 1):
            rows=[]
            for kind, degree, bits in WORKLOADS:
                iterations=4 if kind=='binary64' else 1
                rows.append(dict(kind=kind,degree=degree,bits=bits,status='ok',iterations=iterations,
                    nanoseconds=value*1000000*iterations,allocations=value*iterations,
                    peak_extra_live_bytes=value*100,completion_nanoseconds=value*100000*iterations,
                    completion_allocations=value*2*iterations,completion_peak_extra_live_bytes=value*10,
                    completion_work_units=100,work_units=900,grid=2048,export_fingerprint='0123456789abcdef'))
            (folder/f'trial-{trial}.stdout').write_text('\n'.join(map(json.dumps,rows))+'\n')
            (folder/f'trial-{trial}.json').write_text(json.dumps({'exit_code':0}))


class SummaryTests(unittest.TestCase):
    def setUp(self):
        # Load by path so these checks also work through unittest discovery.
        self.assertTrue((FIXTURE/'summarize.py').exists(), 'summarizer is not implemented')
        spec=importlib.util.spec_from_file_location('summary',FIXTURE/'summarize.py')
        self.module=importlib.util.module_from_spec(spec)
        spec.loader.exec_module(self.module)

    def test_per_invocation_math_preserves_peak_and_work(self):
        with tempfile.TemporaryDirectory() as d:
            root=pathlib.Path(d)
            campaign(root)
            before={str(p.relative_to(root)):p.read_bytes() for p in root.rglob('*') if p.is_file()}
            summary=self.module.summarize(root)
            for workload in summary['workloads']:
                for variant in workload['variants'].values():
                    self.assertEqual(variant['completion']['nanoseconds'],{'median':2000000.0,'min':1000000.0,'max':3000000.0})
                    self.assertEqual(variant['end_to_end']['nanoseconds']['median'],20000000.0)
                    self.assertEqual(variant['end_to_end']['allocations']['median'],20.0)
                    self.assertEqual(variant['completion']['allocations']['median'],40.0)
                    self.assertEqual(variant['end_to_end']['peak_extra_live_bytes']['median'],2000)
                    self.assertEqual(variant['completion']['peak_extra_live_bytes']['median'],200)
                    self.assertEqual(variant['completion']['work_units']['median'],100)
                    self.assertEqual(variant['end_to_end']['work_units']['median'],900)
            self.assertIn('Baseline',self.module.render_markdown(summary))
            self.assertIn('min/max',self.module.render_markdown(summary))
            self.assertEqual(before,{str(p.relative_to(root)):p.read_bytes() for p in root.rglob('*') if p.is_file()})

    def test_rejects_incomplete_campaign_and_changed_records(self):
        for defect in ['flag','row-count','labels','missing','duplicate','fingerprint','grid','error','exit','nan','iterations']:
            with self.subTest(defect=defect), tempfile.TemporaryDirectory() as d:
                root=pathlib.Path(d)
                campaign(root)
                receipt=root/'measurement-completion.json'
                data=json.loads(receipt.read_text())
                if defect=='flag': data['workloads_valid']=False
                elif defect=='row-count': data['validated_workload_records']=71
                elif defect=='labels': data['labels']=['baseline','a','ab','other']
                receipt.write_text(json.dumps(data))
                path=root/'abc/trial-3.stdout'
                rows=[json.loads(line) for line in path.read_text().splitlines()]
                if defect=='missing': rows.pop()
                elif defect=='duplicate': rows[-1]=rows[0]
                elif defect=='fingerprint': rows[0]['export_fingerprint']='0000000000000000'
                elif defect=='grid': rows[0]['grid']=4096
                elif defect=='error': rows[0]['status']='error'
                elif defect=='nan': rows[0]['nanoseconds']=float('nan')
                elif defect=='iterations': rows[0]['iterations']=0
                elif defect=='exit': (root/'abc/trial-3.json').write_text('{"exit_code":1}')
                path.write_text('\n'.join(map(json.dumps,rows))+'\n')
                with self.assertRaises((ValueError, OSError)):
                    self.module.summarize(root)


if __name__=='__main__': unittest.main()
