"""Whole-campaign markers require the specified lanes, not merely the row total."""
import json
from pathlib import Path
import tempfile
import unittest
import accounting


class CombinedContracts(unittest.TestCase):
    def test_correct_counts_with_repeated_original_roles_do_not_replace_current(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            lanes = []
            for index, count in enumerate((183, 310, 310, 4, 4, 33)):
                lane = root / str(index)
                lane.mkdir()
                rows = [{"id": f"wrong{index}/{i}", "implementation": "rust_original"} for i in range(count)]
                (lane / "manifest.json").write_text(json.dumps({"rows": rows}))
                (lane / "results.jsonl").write_text("".join(json.dumps(dict(row, status="native_error")) + "\n" for row in rows))
                (lane / "completion.json").write_text('{"accounting_complete": true}')
                lanes.append(lane)
            with self.assertRaises(ValueError):
                accounting.combined(lanes[:5], lanes[5], root / "combined")
            self.assertFalse((root / "combined" / "completion.json").exists())

    def test_811_rows_in_one_lane_do_not_replace_the_required_five_lanes(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            lane = root / "wrong-lane"
            lane.mkdir()
            rows = [{"id": f"wrong/{i}"} for i in range(811)]
            (lane / "manifest.json").write_text(json.dumps({"rows": rows}))
            (lane / "results.jsonl").write_text("".join(json.dumps(dict(row, status="native_error")) + "\n" for row in rows))
            (lane / "completion.json").write_text('{"accounting_complete": true}')
            diagnostics = root / "diagnostics"
            diagnostics.mkdir()
            (diagnostics / "manifest.json").write_text('{}')
            (diagnostics / "results.jsonl").write_text('')
            with self.assertRaises(ValueError):
                accounting.combined([lane], diagnostics, root / "combined")
            self.assertFalse((root / "combined" / "completion.json").exists())


if __name__ == "__main__":
    unittest.main()
