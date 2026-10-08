#!/usr/bin/env python3
"""Check the real nextest discovery protocol without reading any fixture."""
import os
import subprocess
import sys

binary = sys.argv[1]
env = dict(os.environ, QUEST_BENCH_INPUT="/nonexistent/fixture-must-not-be-read")
for ignored, expected in ((False, 5), (True, 0)):
    arguments = [binary, "--list", "--format", "terse"] + (["--ignored"] if ignored else [])
    result = subprocess.run(arguments, env=env, capture_output=True, text=True, check=True, timeout=10)
    names = [line for line in result.stdout.splitlines() if line.endswith(": benchmark")]
    assert len(names) == expected, (arguments, result.stdout, result.stderr)
print("metadata-only normal and ignored discovery passed")
