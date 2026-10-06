#!/usr/bin/env python3
"""Frozen paired-history parser, admission and subprocess regressions."""
import copy
import contextlib
import io
import json
from pathlib import Path
import resource
import tempfile
import unittest
from unittest import mock
import paired_history as runner


def completed(request):
    history = request.get("History")
    lift = history["lift"] if history else None
    n = 243 if lift == "Kvn" else ({2: 20, 3: 55, 4: 125}[lift["Carleman"]["order"]] if history else None)
    row = {"schema": "quest-cfd-paired-history-row-v1", "request": request,
           "physical_dimension": 5,
           "history_dimension": n * history["time_cells"] * (history["time_order"] + 1) if history else None,
           "initial": {"sample_count": 243, "support_count": 243,
                       "identity": "fnv1a64:0123456789abcdef", "coordinate_mean": [0.] * 5,
                       "covariance_trace": .1, "energy": .05, "probability": 1.,
                       "outer_occupation": .2, "scale": 1., "moment_reconstruction_error": 1e-15 if history and lift != "Kvn" else None},
           "observations": [{"time": t, "slab": (0 if t < .01 else history["time_cells"] - 1) if history else None,
                             "side": ("Interior" if t < .01 else "SlabRight") if history else "classical physical time",
                             "coordinate_mean": [0.] * 5, "energy": .05,
                             "probability": 1. if history and lift == "Kvn" else None,
                             "imaginary_residue": 0., "interpolation_norm_upper": 1. if history else None}
                            for t in (.0025, .01)],
           "history_relative_residual": 1e-14 if history else None,
           "modeled_peak_bytes": 8388608, "history_reference_work": 1000 if history else 0,
           "source_query_work": 1000 if history else 0,
           "physical_reference_work": (4 * 243 * request["EnsembleReference"]["steps"] * 100000 + 243 * request["EnsembleReference"]["steps"] * 1024) if not history else 100000000,
           "physical_drift_calls": 4 * 243 * request["EnsembleReference"]["steps"] if not history else 1000,
           "constructor_work_allowance": 100000000,
           "extraction_probe_error": 1e-15 if history else None,
           "nonlinear_initial_action": .001 if lift == "Kvn" else None,
           "limits": copy.deepcopy(runner.LIMITS), "elapsed_seconds": 1.,
           "quantum_execution": False, "convergence_certified": False,
           "truncation_evidence": "ensemble hierarchy truncation unverified; compare independent order differences, not a single-trajectory certificate"}
    return {"exit_code": 0, "timed_out": False, "elapsed_seconds": 1.,
            "peak_rss_kib": 1000, "address_space_cap_bytes": 512 * 1024**2,
            "stdout_bytes": 1000, "valid_report": True,
            "report": {"status": "completed", "row": row}}


class PairedValidation(unittest.TestCase):
    def test_optional_separate_assembly_allowance_preserves_historical_rows(self):
        for req in runner.requests():
            legacy = completed(req)
            runner.validate(req, legacy)
            row = legacy['report']['row']
            expected = 1000000000 if 'History' in req else 0
            row['history_assembly_work_allowance'] = expected
            runner.validate(req, legacy)
            for wrong in (True, -1, expected + 1, float(expected)):
                row['history_assembly_work_allowance'] = wrong
                with self.subTest(req=req, wrong=wrong), self.assertRaises(ValueError):
                    runner.validate(req, legacy)

    def test_impossible_completed_zero_resources_and_norms_reject(self):
        req = list(runner.requests())[2]
        for path, value in [(('history_reference_work',), 0), (('source_query_work',), 0),
                            (('physical_drift_calls',), 0), (('physical_reference_work',), 0),
                            (('physical_reference_work',), 100000001),
                            (('observations', 0, 'probability'), 0),
                            (('observations', 0, 'interpolation_norm_upper'), 0),
                            (('nonlinear_initial_action',), 1e-12)]:
            record = completed(req)
            obj = record['report']['row']
            for key in path[:-1]:
                obj = obj[key]
            obj[path[-1]] = value
            with self.subTest(path=path), self.assertRaises(ValueError):
                runner.validate(req, record)

    def test_fixed_requests_and_dimensions(self):
        reqs = list(runner.requests())
        self.assertEqual(len(reqs), 14)
        self.assertEqual(len({json.dumps(x, sort_keys=True) for x in reqs}), 14)
        dims = [runner.validate(r, completed(r))["history_dimension"] for r in reqs]
        self.assertEqual(dims, [None, None, 486, 972, 729, 40, 80, 60, 110, 220, 165, 250, 500, 375])

    def test_malformed_required_fields_and_claims(self):
        req = list(runner.requests())[2]
        for path, value in [(('schema',), 'unknown'), (('request',), {}),
                            (('physical_dimension',), 4), (('history_dimension',), 487),
                            (('modeled_peak_bytes',), 268435457), (('history_reference_work',), 10000000001),
                            (('source_query_work',), True), (('physical_reference_work',), -1),
                            (('physical_drift_calls',), 1000001), (('quantum_execution',), True),
                            (('convergence_certified',), True), (('history_relative_residual',), 1e-9),
                            (('initial', 'identity'), ''), (('initial', 'support_count'), 244),
                            (('initial', 'probability'), .1), (('initial', 'scale'), 0.),
                            (('initial', 'coordinate_mean'), [0.] * 4),
                            (('limits', 'max_bytes'), 1), (('elapsed_seconds',), float('nan'))]:
            record = completed(req)
            obj = record['report']['row']
            for key in path[:-1]:
                obj = obj[key]
            obj[path[-1]] = value
            with self.subTest(path=path, value=value), self.assertRaises(ValueError):
                runner.validate(req, record)
        for key in completed(req)['report']['row']:
            bad = completed(req)
            del bad['report']['row'][key]
            with self.subTest(missing=key), self.assertRaises(ValueError):
                runner.validate(req, bad)

    def test_observation_layout_and_finite_values(self):
        for req in list(runner.requests()):
            for field, value in [('time', .003), ('slab', 99), ('side', 'SlabLeft'),
                                 ('coordinate_mean', [float('inf')] * 5), ('energy', -1),
                                 ('imaginary_residue', -1), ('probability', -1),
                                 ('interpolation_norm_upper', -1)]:
                record = completed(req)
                record['report']['row']['observations'][0][field] = value
                if field == 'energy' and 'History' in req and req['History']['lift'] != 'Kvn':
                    self.assertEqual(runner.validate(req, record)['observations'][0]['energy'], -1)
                    continue
                with self.subTest(req=req, field=field), self.assertRaises(ValueError):
                    runner.validate(req, record)

    def test_process_failure_and_output_budget(self):
        req = list(runner.requests())[0]
        for field, value in [('exit_code', 1), ('timed_out', True), ('valid_report', False),
                             ('stdout_bytes', 4 * 1024**2 + 1), ('peak_rss_kib', -1),
                             ('address_space_cap_bytes', 1), ('elapsed_seconds', float('inf'))]:
            record = completed(req)
            record[field] = value
            with self.subTest(field=field), self.assertRaises(ValueError):
                runner.validate(req, record)

    def test_request_types_rejections_and_common_identity(self):
        for request in [{"EnsembleReference": {"steps": 256.}},
                        {"History": {"lift": "Kvn", "time_cells": True, "time_order": 1}}]:
            with self.assertRaises(ValueError):
                runner.dimension(request)
        req = list(runner.requests())[2]
        record = completed(req)
        record['report'] = {'status': 'rejected', 'request': req, 'limits': runner.LIMITS, 'error': 'work admission'}
        runner.validate_rejected(req, record)
        for key, value in [('error', ''), ('request', {}), ('limits', {}), ('status', 'completed')]:
            bad = copy.deepcopy(record)
            bad['report'][key] = value
            with self.assertRaises(ValueError):
                runner.validate_rejected(req, bad)
        records = []
        for request in runner.requests():
            r = completed(request)
            r.update(request=request, outcome='completed')
            records.append(r)
        self.assertEqual(len(runner.comparisons(records)['pairs']), 36)
        records[2]['report']['row']['initial']['identity'] = 'fnv1a64:fedcba9876543210'
        result = runner.comparisons(records)
        self.assertTrue(result['withheld'])
        self.assertTrue(all(2 not in (x['left_row'], x['right_row']) for x in result['pairs']))
        records[3]['outcome'] = 'timeout'
        self.assertTrue(all(3 not in (x['left_row'], x['right_row']) for x in runner.comparisons(records)['pairs']))
        records[0]['report']['row']['observations'][0]['coordinate_mean'] = [1e308] * 5
        records[1]['report']['row']['observations'][0]['coordinate_mean'] = [-1e308] * 5
        self.assertTrue(any(x['reason'] == 'paired arithmetic is not finite'
                            for x in runner.comparisons(records)['withheld']))

    def test_actual_collector_retains_timeout_and_rejects_malformed_output(self):
        req = list(runner.requests())[0]
        payload = json.dumps(completed(req)['report'])
        with tempfile.TemporaryDirectory() as directory:
            binary = Path(directory) / 'child'
            def execute(source, timeout=5):
                binary.write_text('#!/usr/bin/python3\n' + source)
                binary.chmod(0o700)
                limits = resource.getrlimit(resource.RLIMIT_FSIZE)
                result = runner.collect_row(binary, req, timeout)
                self.assertEqual(resource.getrlimit(resource.RLIMIT_FSIZE), limits)
                return result
            good = execute('print(' + repr(payload) + ')\n')
            self.assertEqual(good['outcome'], 'completed')
            rejected = execute('print(' + repr(json.dumps({'status': 'rejected', 'request': req, 'limits': runner.LIMITS, 'error': 'bytes'})) + ')\n')
            self.assertEqual(rejected['outcome'], 'rejected')
            for bad in ['{"status":"completed","status":"rejected"}', '{"x":NaN}', '{"x":1e999}',
                        '[' * 1000 + '0' + ']' * 1000, 'null', '[]', 'not JSON']:
                with self.subTest(payload=bad[:30]):
                    self.assertEqual(execute('print(' + repr(bad) + ')\n')['outcome'], 'invalid')
            timed = execute('import time\nprint(' + repr(payload) + ',flush=True)\ntime.sleep(10)\n', .1)
            self.assertEqual(timed['outcome'], 'timeout')
            self.assertTrue(timed['timed_out'])
            self.assertIsNotNone(timed['report'])

    def test_actual_stdout_and_stderr_file_caps(self):
        req = list(runner.requests())[0]
        with tempfile.TemporaryDirectory() as directory:
            binary = Path(directory) / 'child'
            for descriptor in (1, 2):
                binary.write_text('#!/usr/bin/python3\nimport os\nos.write(' + str(descriptor) + ', b"x" * (8 * 1024**2))\nos.write(' + str(descriptor) + ', b"x")\n')
                binary.chmod(0o700)
                record = runner.collect_row(binary, req, 5)
                self.assertEqual(record['outcome'], 'invalid')
                self.assertLessEqual(record['stdout_bytes'], runner.OUTPUT_BYTES)
                self.assertNotEqual(record['exit_code'], 0)

    def test_incremental_receipt_survives_interruption(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            binary = root / 'child'
            binary.write_bytes(b'fixture bytes')
            output = root / 'receipt.json'
            req = list(runner.requests())[0]
            first = completed(req)
            first.update(outcome='completed', validation_error=None)
            with mock.patch.object(runner, 'source_snapshot', return_value={'sha256': '0' * 64}), \
                 mock.patch.object(runner, 'collect_row', side_effect=[first, KeyboardInterrupt()]):
                with self.assertRaises(KeyboardInterrupt):
                    runner.run(binary, output, root)
            receipt = runner.decode_report(output.read_bytes())
            self.assertFalse(receipt['complete'])
            self.assertEqual(len(receipt['records']), 1)
            self.assertEqual(receipt['records'][0]['outcome'], 'completed')

    def test_fourteen_actual_children_use_pinned_binary_and_retain_rejection(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            binary = root / 'child'
            output = root / 'receipt.json'
            reports = {runner.request_key(r): completed(r)['report'] for r in runner.requests()}
            req = list(runner.requests())[0]
            reports[runner.request_key(req)] = {'status': 'rejected', 'request': req,
                                               'limits': runner.LIMITS, 'error': 'work admission'}
            binary.write_text('#!/usr/bin/python3\nimport json,sys\nreports=' + repr(reports) +
                              '\nr=json.loads(sys.argv[2])\nprint(json.dumps(reports[json.dumps(r,sort_keys=True)]))\n')
            binary.chmod(0o700)
            original = runner.collect_row
            def replace_supplied_binary(pinned, request):
                result = original(pinned, request, 5)
                binary.write_text('#!/usr/bin/python3\nraise SystemExit(99)\n')
                return result
            with mock.patch.object(runner, 'source_snapshot', return_value={'sha256': '0' * 64}), \
                 mock.patch.object(runner, 'collect_row', side_effect=replace_supplied_binary), \
                 contextlib.redirect_stdout(io.StringIO()):
                receipt = runner.run(binary, output, root)
            self.assertTrue(receipt['complete'])
            self.assertTrue(receipt['binary_unchanged'])
            self.assertNotEqual(receipt['binary_sha256'], runner.sha256(binary))
            self.assertEqual(receipt['outcome_counts'], {'completed': 13, 'rejected': 1, 'timeout': 0, 'invalid': 0})
            self.assertEqual(len(receipt['comparisons']['pairs']), 35)
            self.assertEqual(runner.decode_report(output.read_bytes()), receipt)

    def test_final_optional_evidence_and_build_attestation(self):
        for req in runner.requests():
            record = completed(req)
            history = req.get('History')
            carleman = history and history['lift'] != 'Kvn'
            record['report']['row']['initial']['moment_reconstruction_error'] = None if carleman else 0.
            with self.assertRaises(ValueError):
                runner.validate(req, record)
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            binary = root / 'binary'
            source = root / 'input.rs'
            binary.write_bytes(b'frozen')
            source.write_bytes(b'build input')
            attestation = {'schema': 'quest-cfd-paired-build-v1',
                           'command': ['cargo', 'build', '--release', '-p', 'quest-cfd', '--example', 'paired_history'],
                           'source_manifest_sha256': '0' * 64, 'source_unchanged_during_build': True,
                           'source_manifest': {'scope': 'fixture', 'sha256': '0' * 64,
                                               'files': [{'path': 'input.rs', 'sha256': runner.sha256(source)}]},
                           'binary_sha256': runner.sha256(binary), 'profile': 'release', 'features': 'default'}
            path = root / 'build.json'
            path.write_text(json.dumps(attestation))
            self.assertTrue(runner.admit_build_attestation(path, binary, root)['current_source_matches_build_manifest'])
            source.write_bytes(b'later source')
            self.assertEqual(runner.admit_build_attestation(path, binary, root)['changed_since_build'], ['input.rs'])
            for mutate in [lambda x: x.update(binary_sha256='f' * 64),
                           lambda x: x.update(source_unchanged_during_build=False),
                           lambda x: x['source_manifest']['files'][0].update(path='../private'),
                           lambda x: x.update(schema='unknown'), lambda x: x.update(profile='debug')]:
                bad = copy.deepcopy(attestation)
                mutate(bad)
                path.write_text(json.dumps(bad))
                with self.assertRaises(ValueError):
                    runner.admit_build_attestation(path, binary, root)


if __name__ == '__main__':
    unittest.main()
