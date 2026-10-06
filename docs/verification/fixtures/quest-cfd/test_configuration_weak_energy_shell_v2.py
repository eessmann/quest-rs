"""Additive profile tests; no physical follow-up execution."""
import copy
import json
import io
from unittest.mock import MagicMock
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
import configuration_weak as shared
import configuration_weak_energy_shell_v2 as runner

class EnergyShellProtocol(unittest.TestCase):
    def test_original_matrix_and_additive_request_are_separate(self):
        self.assertEqual(len(shared.ROWS),7)
        self.assertNotIn(runner.ROW,shared.ROWS)
        self.assertEqual(runner.request()['order'],2)
        self.assertEqual(runner.request()['cells'],2)
        with self.assertRaises(ValueError):shared.parameters(runner.ROW)

    def test_saved_reference_is_exact_c4_not_a_new_execution(self):
        old=runner.load_reference()
        self.assertEqual(old['request']['row_id'],'p1-c4-e1-w1_2')
        self.assertEqual(shared.sha256(runner.REFERENCE),runner.REFERENCE_SHA)
        bad=copy.deepcopy(old);bad['request']['width']=.6
        with self.assertRaises(ValueError):runner.validate(bad,runner.ROW)

    def test_genuine_shared_rust_injected_prefix_and_strict_request(self):
        path=Path(__file__).with_name('configuration_weak_energy_shell_v2_fixtures')/'grid_failure.json'
        row=shared.decode_report(path.read_text())
        self.assertEqual(runner.validate(row,runner.ROW)['status'],'construction-rejected')
        for keys,value in [(('request','width'),.6),(('request','order'),True),(('limits','max_source_work'),2000000000),(('visited_rows',),1)]:
            bad=copy.deepcopy(row);owner=bad
            for key in keys[:-1]:owner=owner[key]
            owner[keys[-1]]=value
            with self.subTest(keys=keys),self.assertRaises(ValueError):runner.validate(bad,runner.ROW)
        result=runner.comparison(runner.load_reference(),row)
        self.assertEqual(result['status'],'unavailable')
        self.assertTrue(result['quadrature_weights_changed'])
        self.assertFalse(result['pure_h_or_p_refinement'])

    def test_reference_hash_and_decode_use_one_bounded_byte_snapshot(self):
        raw=runner.REFERENCE.read_bytes()
        path=MagicMock();stream=MagicMock(wraps=io.BytesIO(raw))
        path.open.return_value.__enter__.return_value=stream
        loaded=runner.load_reference(path)
        self.assertEqual(loaded['request']['row_id'],'p1-c4-e1-w1_2')
        path.open.assert_called_once_with('rb')
        stream.read.assert_called_once_with(shared.FILE_BYTES+1)
        path.stat.assert_not_called();path.read_text.assert_not_called()

    def test_reference_hash_rejects_modified_saved_values(self):
        with tempfile.TemporaryDirectory() as folder:
            path=Path(folder)/'altered.json'
            row=runner.load_reference();row['diagnostic']['physical_rate'][5]+=1
            path.write_text(json.dumps(row))
            with self.assertRaises(ValueError):runner.load_reference(path)

    def test_one_fake_failed_child_is_retained_without_expanding_original_matrix(self):
        with tempfile.TemporaryDirectory() as folder:
            folder=Path(folder);binary=folder/'fake';counter=folder/'calls';out=folder/'result'
            program = "\n".join(["#!/usr/bin/python3", "from pathlib import Path", "import sys",
                "with Path("+repr(str(counter))+').open("a") as f: f.write(sys.argv[1]+"\\n")',
                "print("+repr('{"status":1e999}')+")", ""])
            binary.write_text(program)
            binary.chmod(0o700)
            result=subprocess.run([sys.executable,'-B',runner.__file__,str(binary),str(out)],capture_output=True,timeout=10)
            self.assertEqual(result.returncode,0,result.stderr.decode())
            receipt=json.loads((out/'receipt.json').read_text())
            self.assertEqual(receipt['schema'],runner.SCHEMA)
            self.assertEqual(len(receipt['rows']),1)
            self.assertEqual(counter.read_text().splitlines(),[runner.ROW])
            self.assertFalse(receipt['rows'][0]['validated'])
            self.assertEqual(receipt['comparison']['status'],'unavailable')
            self.assertTrue(receipt['provenance_unchanged'])

    def test_self_removing_fake_child_retains_capture_and_failed_provenance(self):
        with tempfile.TemporaryDirectory() as folder:
            folder=Path(folder);binary=folder/'fake';counter=folder/'calls';out=folder/'result'
            program = "\n".join(["#!/usr/bin/python3", "from pathlib import Path", "import sys",
                "with Path("+repr(str(counter))+').open("a") as f: f.write(sys.argv[1]+"\\n")',
                "Path(sys.argv[0]).unlink()", "print("+repr((Path(__file__).with_name('configuration_weak_energy_shell_v2_fixtures')/'grid_failure.json').read_text())+")", ""])
            binary.write_text(program);binary.chmod(0o700)
            result=subprocess.run([sys.executable,'-B',runner.__file__,str(binary),str(out)],capture_output=True,timeout=10)
            receipt=json.loads((out/'receipt.json').read_text())
            self.assertEqual(len(receipt['rows']),1)
            self.assertEqual(counter.read_text().splitlines(),[runner.ROW])
            self.assertEqual(result.returncode,0,result.stderr.decode())
            self.assertFalse(receipt['provenance_unchanged'])
            self.assertTrue(receipt['rows'][0]['validated'])
            self.assertEqual(receipt['provenance_status'],'failed')
            self.assertIsNone(receipt['binary_sha256_after'])
            self.assertEqual(receipt['reference_sha256_after'],runner.REFERENCE_SHA)
            self.assertEqual(receipt['comparison']['status'],'unavailable')
            self.assertTrue(receipt['provenance_errors'])
            self.assertTrue((out/(runner.ROW+'.json')).exists())

    def test_saved_seven_comparisons_remain_identical_after_helper_extraction(self):
        folder=runner.REFERENCE.parent
        rows={row:shared.decode_report((folder/(row+'.json')).read_text()) for row in shared.ROWS}
        receipt=json.loads((folder/'receipt.json').read_text())
        self.assertEqual(shared.comparisons(rows),receipt['comparisons'])

if __name__=='__main__':unittest.main()
