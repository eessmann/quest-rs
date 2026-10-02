#!/usr/bin/env python3
"""Regenerate the external libmpdec witness data (not part of Rust test dependencies)."""
from decimal import Decimal, localcontext
from pathlib import Path
import json
import sys

cases = []
exact = {("exp", "0"): "1", ("ln", "1"): "0", ("sqrt", "0"): "0",
         ("sqrt", "1e-100"): "1e-50", ("sqrt", "1e100"): "1e50"}
with localcontext() as ctx:
    ctx.prec = 210
    for operation, inputs in [
        ("exp", ["-10", "-0.5", "0", "0.1", "1", "10"]),
        ("ln", ["1e-100", "0.1", "0.5", "1", "1.00000000000000000000001", "10"]),
        ("sqrt", ["0", "1e-100", "0.1", "2", "10", "1e100"]),
    ]:
        for text in inputs:
            value = getattr(Decimal(text), operation)()
            singleton = exact.get((operation, text))
            cases.append(dict(operation=operation, input=text,
                              lower=singleton or str(value.next_minus()),
                              upper=singleton or str(value.next_plus())))
output = Path(sys.argv[1])
output.write_text(json.dumps(dict(
    generator="CPython decimal / libmpdec; 210 decimal digits; outward adjacent values around correctly rounded result",
    python=sys.version, cases=cases), indent=2) + "\n")
