#!/usr/bin/env python3
"""Exercise nextest discovery with deliberately invalid runtime configuration."""
import os
from pathlib import Path
import subprocess
import sys
import tempfile

binary = Path(sys.argv[1]).resolve()
with tempfile.TemporaryDirectory() as directory:
    phase = Path(directory) / "phase"
    env = dict(os.environ, QUEST_BENCH_DIMENSION="not-a-dimension", QUEST_BENCH_PHASE=str(phase))
    for ignored, expected in ((False, 4), (True, 0)):
        arguments = [str(binary), "--list", "--format", "terse"] + (["--ignored"] if ignored else [])
        result = subprocess.run(arguments, env=env, capture_output=True, text=True,
                                check=True, timeout=10)
        names = [line for line in result.stdout.splitlines() if line.endswith(": benchmark")]
        assert len(names) == expected, (arguments, result.stdout, result.stderr)
        assert not phase.exists(), "discovery entered fixture preparation"
print("unitary discovery is metadata-only for both normal and ignored queries")
