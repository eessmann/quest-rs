"""Scratch Cargo manifests must retain TOML semantics for arbitrary UTF-8 paths."""
import importlib.util
from pathlib import Path
import tomllib
import unittest


def launcher(relative):
    path = Path(__file__).parent / relative / "run.py"
    spec = importlib.util.spec_from_file_location(relative.replace("/", "_"), path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class ManifestTests(unittest.TestCase):
    def test_paths_and_features_round_trip(self):
        source = Path('/tmp/fixture 😀 with "quotes" and \\backslash')
        qsp = tomllib.loads(launcher("qsp-optimization").render_manifest(source))
        self.assertEqual(qsp["dependencies"]["quest-qsp"], {
            "path": str(source / "crates/quest-qsp"), "features": ["offline-synthesis"]})
        self.assertEqual(qsp["dependencies"]["serde_json"], "1")
        project = tomllib.loads(launcher("static-architecture/project").render_manifest(
            source, "project-performance-current", {"quest": "quest", "quest-compile": "quest-circuit"}))
        self.assertEqual(project["dependencies"]["quest"]["package"], "quest-rs")
        self.assertEqual(project["dependencies"]["quest-compile"]["package"], "quest-circuit")
        self.assertEqual(project["build-dependencies"]["quest-build"]["path"],
                         str(source / "crates/quest-build"))
        numerical = tomllib.loads(launcher("static-architecture/numerical").render_manifest(
            source, "current", "numeric-performance-mp"))
        self.assertEqual(numerical["bin"], [{"name": "numeric-performance-mp", "path": "src/extra.rs"}])
        solvers = tomllib.loads(launcher("mp-solvers").render_manifest(source))
        self.assertEqual(solvers["dependencies"]["quest-qsp"]["features"], ["offline-synthesis"])
        for manifest in [qsp, project, numerical, solvers]:
            self.assertEqual(manifest["workspace"], {})
            self.assertEqual(manifest["package"]["edition"], "2024")
            self.assertEqual(manifest["profile"]["release"]["lto"], "thin")
            if manifest is not qsp:
                self.assertEqual(manifest["dependencies"]["csv"], "1.4")
            for dependency in manifest["dependencies"].values():
                if isinstance(dependency, dict) and "path" in dependency:
                    self.assertTrue(dependency["path"].startswith(str(source)))


if __name__ == "__main__":
    unittest.main()
