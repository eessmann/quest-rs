"""Named native manifests must preserve the upstream process boundaries."""
import importlib.util
from pathlib import Path
import unittest


PATH = Path(__file__).with_name("run_named.py")
SPEC = importlib.util.spec_from_file_location("run_named", PATH)
named = importlib.util.module_from_spec(SPEC)
if PATH.exists():
    SPEC.loader.exec_module(named)


class NamedContracts(unittest.TestCase):
    def test_every_workload_routes_to_its_unchanged_catch_case(self):
        self.assertTrue(hasattr(named, "named_manifest"), "named manifest missing")
        groups, rows = named.named_manifest()
        self.assertEqual(len(rows), 26)
        self.assertEqual(len({row["id"] for row in rows}), 26)
        self.assertEqual({group["name"]: sum(row["group"] == group["name"] for row in rows)
                          for group in groups},
                         {"inverse_quick": 12, "inverse_full": 5,
                          "solver_random": 6, "solver_industrial": 3})
        by_case = {row["case_id"]: row for row in rows}
        self.assertEqual(by_case["named_inverse/order1000"]["benchmark_name"], "Order: 1000")
        self.assertEqual(by_case["named_inverse/order2000"]["benchmark_name"], "Forward-prepared order: 2000")
        self.assertEqual(by_case["named_inverse/order1000000"]["group"], "inverse_full")
        self.assertEqual(by_case["named_solver/random_basis_laurent_d200"]["group"], "solver_random")
        self.assertTrue(all(row["protocol"] == "catch2_10" for row in rows))
        self.assertTrue(all(row["input_sha256"] is None for row in rows),
                        "a seed or source hash is not an observed coefficient hash")


if __name__ == "__main__":
    unittest.main()
