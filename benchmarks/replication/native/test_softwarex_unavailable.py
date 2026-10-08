import importlib.util
from pathlib import Path
import unittest

PATH = Path(__file__).with_name("softwarex_unavailable.py")
SPEC = importlib.util.spec_from_file_location("softwarex_unavailable", PATH)
capability = importlib.util.module_from_spec(SPEC)
if PATH.exists():
    SPEC.loader.exec_module(capability)


class CapabilityContracts(unittest.TestCase):
    def test_lfs_evidence_requires_exact_payload_digest_and_size(self):
        self.assertTrue(hasattr(capability, "matches_committed"), "LFS payload verification missing")
        pointer = (b'version https://git-lfs.github.com/spec/v1\n'
                   b'oid sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad\nsize 3\n')
        self.assertTrue(capability.matches_committed(b'abc', pointer))
        self.assertFalse(capability.matches_committed(b'abd', pointer))
        self.assertFalse(capability.matches_committed(b'abc', pointer.replace(b'size 3', b'size 4')))

    def test_compatible_cli_cannot_be_diagnosed_as_unavailable(self):
        self.assertTrue(hasattr(capability, "verify_boundary"), "boundary guard missing")
        legacy = '"--corpus" "--case-id" "--pipeline" "--repeat"'
        current = '"--corpus" "--case-id" "--scope" "--input-root"'
        capability.verify_boundary(current, legacy)
        with self.assertRaises(ValueError):
            capability.verify_boundary(current + ' "--pipeline"', legacy)
        with self.assertRaises(ValueError):
            capability.verify_boundary(current, 'unrelated source')


if __name__ == "__main__":
    unittest.main()
