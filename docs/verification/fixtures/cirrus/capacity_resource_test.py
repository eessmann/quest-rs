"""Resource-only receipts must not claim portable replay or total memory measurement."""
import copy
import hashlib
import importlib.util
import json
import pathlib
import struct
import tempfile
import unittest

HERE = pathlib.Path(__file__).resolve().parent

def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module

RUN = load("resource_capacity", HERE / "capacity.py")
OLD = load("legacy_capacity_tests", HERE.parent / "sparse-capacity" / "test_run.py")


class ResourceReceiptTests(unittest.TestCase):
    def fixtures(self, directory, version=4):
        rows = []
        for rank in range(2):
            row = OLD.ReceiptTests().receipt(rank)
            data = b"".join(struct.pack("<QQdd", column if slot == 0 else column ^ 1,
                column, .7 if slot == 0 else -.2, .1 if slot == 0 else .05)
                for column in range(rank, 64, 2) for slot in range(2))
            (directory / f"input-rank-{rank}.coo32").write_bytes(data)
            row.update(schema_version=version, canonical_input_bytes=4096,
                local_canonical_input_bytes=2048, local_input_sha256=hashlib.sha256(data).hexdigest(),
                input_storage_seconds=.01, persisted_encoding_file_bytes=0,
                node=dict(leader_rank=0, local_rank=rank, local_size=2, processor_name="local"),
                node_memory_sampling=(dict(samples=3, maximum_sum_rss_bytes=16384,
                    maximum_sum_address_space_bytes=65536, max_sample_span_seconds=.001,
                    interval_milliseconds=20, pids=2) if rank == 0 else None))
            if version in (4, 5):
                row.update(loading_route="resource-only-native", portable_replay_admission="unrun",
                    persisted_recipe_single_replay_communication_upper_bound_bytes=None,
                    persistence_load_measured_communication="logical broadcast calls only; wire bytes and MPI-internal communication unmeasured",
                    native_live_reserved_bytes=16384,
                    load_statistics=dict(phase_seconds=dict(manifest_and_admission=.01,
                        read_validate=.02, reverse_directory=.03, replay_admission=0, total=.08),
                        local_records=64, local_reverse_records=64, local_record_capacity=64,
                        local_reverse_capacity=64, reverse_broadcasts=128, replay_admitted=False,
                        admission=dict(next_record_calls=0, forward_calls=0, reverse_calls=0, broadcasts=0)),
                    native_array_payload=dict(scope="native array payload only; excludes allocator, MPI, OpenMP stacks and other temporaries",
                        input=dict(host_bytes=8192, device_bytes=0),
                        scratch=dict(host_bytes=8192, device_bytes=0),
                        total_host_bytes=16384, total_device_bytes=0))
                if version == 5:
                    row["native_array_payload"].update(scratch=None,
                        scratch_mode="borrowed-input-communication-buffer", total_host_bytes=8192)
            rows.append(row)
        return rows

    def summarize(self, directory, rows):
        for row in rows:
            (directory / f"rank-{row['rank']}.json").write_text(json.dumps(row))
        return RUN.summarize_case(directory, 2, 64, 2, [65536, 16384, 32768], 1, 2)

    def test_v4_accepts_resource_load_without_inventing_replay_bound(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = pathlib.Path(temporary)
            result = self.summarize(directory, self.fixtures(directory))
            self.assertFalse(result["capacity_closed"])
            row = result["rank_receipts"][0]
            self.assertIsNone(row["persisted_recipe_single_replay_communication_upper_bound_bytes"])
            self.assertEqual(row["portable_replay_admission"], "unrun")
            self.assertEqual(row["native_array_payload"]["total_host_bytes"], 16384)

    def test_v4_rejects_false_admission_malformed_timings_and_payload(self):
        mutations = [
            lambda r: r.update(portable_replay_admission="admitted"),
            lambda r: r.update(loading_route="portable-replay"),
            lambda r: r.update(persisted_recipe_single_replay_communication_upper_bound_bytes=0),
            lambda r: r["load_statistics"].update(replay_admitted=True),
            lambda r: r["load_statistics"]["admission"].update(forward_calls=1),
            lambda r: r["load_statistics"]["phase_seconds"].update(replay_admission=.001),
            lambda r: r["load_statistics"]["phase_seconds"].update(total=.2),
            lambda r: r["load_statistics"]["phase_seconds"].update(read_validate=float("nan")),
            lambda r: r["load_statistics"].update(local_record_capacity=63),
            lambda r: r["load_statistics"].update(local_records=63),
            lambda r: r["load_statistics"].update(reverse_broadcasts=True),
            lambda r: r["native_array_payload"]["scratch"].update(host_bytes=0),
            lambda r: r["native_array_payload"].update(total_host_bytes=8192),
            lambda r: r["native_array_payload"].update(total_device_bytes=True),
            lambda r: r["native_array_payload"].update(scope="total native memory"),
            lambda r: r.update(schema_version=3),
        ]
        with tempfile.TemporaryDirectory() as temporary:
            directory = pathlib.Path(temporary)
            baseline = self.fixtures(directory)
            for index, mutate in enumerate(mutations):
                rows = copy.deepcopy(baseline)
                mutate(rows[1])
                with self.subTest(index=index), self.assertRaises(ValueError):
                    self.summarize(directory, rows)

    def test_v5_counts_borrowed_communication_buffer_only_in_input_arrays(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = pathlib.Path(temporary)
            result = self.summarize(directory, self.fixtures(directory, version=5))
            self.assertFalse(result["capacity_closed"])
            payload = result["rank_receipts"][0]["native_array_payload"]
            self.assertIsNone(payload["scratch"])
            self.assertEqual(payload["scratch_mode"], "borrowed-input-communication-buffer")
            self.assertEqual(payload["total_host_bytes"], 8192)

    def test_v5_rejects_double_counted_or_wrong_scratch_ownership(self):
        mutations = [
            lambda r: r["native_array_payload"].update(total_host_bytes=16384),
            lambda r: r["native_array_payload"].update(scratch=dict(host_bytes=8192, device_bytes=0)),
            lambda r: r["native_array_payload"].update(scratch_mode="owned-register"),
            lambda r: r["native_array_payload"].update(scratch_mode=None),
            lambda r: r.update(schema_version=4),
        ]
        with tempfile.TemporaryDirectory() as temporary:
            directory = pathlib.Path(temporary)
            baseline = self.fixtures(directory, version=5)
            for index, mutate in enumerate(mutations):
                rows = copy.deepcopy(baseline)
                mutate(rows[1])
                with self.subTest(index=index), self.assertRaises(ValueError):
                    self.summarize(directory, rows)

    def test_v5_local_native_fallback_requires_an_owned_scratch_register(self):
        with tempfile.TemporaryDirectory() as temporary:
            row = self.fixtures(pathlib.Path(temporary), version=5)[0]
            row.update(ranks=1, local_native_amplitudes=512)
            payload = row["native_array_payload"]
            payload.update(scratch=dict(host_bytes=8192, device_bytes=0),
                scratch_mode="owned-register", total_host_bytes=16384)
            RUN.validate_resource_load(row)
            payload.update(scratch=None, scratch_mode="borrowed-input-communication-buffer", total_host_bytes=8192)
            with self.assertRaises(ValueError):
                RUN.validate_resource_load(row)

    def test_historical_v2_still_requires_numeric_replay_bound(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = pathlib.Path(temporary)
            rows = self.fixtures(directory, version=2)
            self.assertFalse(self.summarize(directory, rows)["capacity_closed"])
            rows[0]["persisted_recipe_single_replay_communication_upper_bound_bytes"] = None
            with self.assertRaises(ValueError):
                self.summarize(directory, rows)


if __name__ == "__main__":
    unittest.main()
