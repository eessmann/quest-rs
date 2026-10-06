#!/usr/bin/env python3
"""Fixed represented-coordinate P2 cylinder constraint rank, independently assembled.

Standard-library Cartesian Fraction arithmetic; no production CFD or MathCore
imports. This proves rank over the exact dyadic source coordinates, NOT rank
or error of the numerically rounded Rust matrix or physical convergence.
"""
from fractions import Fraction as F
from hashlib import sha256
from itertools import combinations
import json
import math
from pathlib import Path
import platform
import sys
from time import perf_counter

import affine_exact as exact

POWERS = ((0, 0), (0, 1), (1, 0), (0, 2), (1, 1), (2, 0))
SOURCE_SHA256 = "e4e5d45032e4d7b10241b91b4514f539c5b3e3698daca40251ece587db5cfb3e"
SOURCE_FINGERPRINT = 17853273085390264558
SOURCE = Path(__file__).resolve().parents[2] / "data/2026-10-06-cylinder-p2/geometry.json"


def assemble(vertices, cells, exterior):
    """Complete Cartesian P2 vector constraints on the finite 2D source."""
    columns = 12 * len(cells)
    owners = {}
    for cell_id, cell in enumerate(cells):
        exact.require(len(cell) == 3 and len(set(cell)) == 3, "invalid triangle")
        for face in combinations(cell, 2):
            owners.setdefault(tuple(sorted(face)), []).append(cell_id)
    labels = {}
    for item in exterior:
        face = tuple(sorted(item["vertices"]))
        exact.require(len(face) == 2 and face not in labels, "duplicate exterior")
        exact.require(item["condition"] in ("dirichlet", "natural_mechanical_traction"),
                      "unknown condition")
        exact.require(face in owners and len(owners[face]) == 1, "non-exterior label")
        labels[face] = item
    exact.require(set(labels) == {face for face, ids in owners.items() if len(ids) == 1},
                  "incomplete exterior")
    rows = []
    counts = {"interior": 0, "dirichlet": 0, "natural_mechanical_traction": 0}
    for face, ids in sorted(owners.items()):
        exact.require(1 <= len(ids) <= 2, "invalid facet incidence")
        condition = "interior" if len(ids) == 2 else labels[face]["condition"]
        counts[condition] += 1
        if condition == "natural_mechanical_traction":
            continue
        left, right = (vertices[i] for i in face)
        normal = (right[1] - left[1], left[0] - right[0])
        exact.require(normal != (0, 0), "zero facet")
        nodes = (left, right, tuple((a + b) / 2 for a, b in zip(left, right)))
        for node in nodes:
            row = [F(0)] * columns
            for owner, sign in zip(ids, (1, -1)):
                for component in range(2):
                    for k, powers in enumerate(POWERS):
                        row[12 * owner + 6 * component + k] = (
                            sign * normal[component] * exact.monomial(node, powers))
            rows.append(row)
    trace_rows = len(rows)
    for cell_id, cell in enumerate(cells):
        points = [vertices[i] for i in cell]
        determinant = exact.determinant([
            [points[k][j] - points[0][j] for j in range(2)] for k in (1, 2)])
        volume = abs(determinant) / 2
        exact.require(volume > 0, "zero exact area")
        for test in ((0, 0), (1, 0), (0, 1)):
            row = [F(0)] * columns
            for component in range(2):
                for k, powers in enumerate(POWERS):
                    if powers[component]:
                        derivative = tuple(powers[j] + test[j] - int(j == component)
                                           for j in range(2))
                        row[12 * cell_id + 6 * component + k] = (
                            powers[component] * exact.cartesian_moment(points, volume, derivative))
            rows.append(row)
    return rows, counts, trace_rows


def certificate(vertices, cells, exterior):
    rows, counts, trace_rows = assemble(vertices, cells, exterior)
    matrix = exact.modular_matrix(rows)
    rank, pivot_rows, pivot_columns = exact.modular_rank(matrix)
    exact.require(rank == len(rows), "not full row rank")
    minor = exact.verify_pivot_minor(matrix, pivot_rows, pivot_columns)
    identity = sha256()
    for count in (len(rows), len(rows[0])):
        identity.update(count.to_bytes(8, "little"))
    for row in rows:
        for value in row:
            encoded = str(value).encode("ascii")
            identity.update(len(encoded).to_bytes(8, "little"))
            identity.update(encoded)
    return {
        "broken_velocity_columns": len(rows[0]),
        "constraint_rows": len(rows),
        "exact_rational_rank": rank,
        "full_velocity_dimension": len(rows[0]) - rank,
        "trace_rows": trace_rows,
        "divergence_rows": len(rows) - trace_rows,
        "facets": counts,
        "pivot_minor_rows": pivot_rows,
        "pivot_minor_columns": pivot_columns,
        "pivot_minor_determinant_mod_prime": minor,
        "constraint_matrix_sha256": identity.hexdigest(),
    }


def decode_export(data):
    """Validate the finite exported source and interpret its coordinates exactly."""
    required = {"source_policy", "fingerprint", "vertices", "cells", "exterior"}
    exact.require(type(data) is dict and set(data) == required, "geometry export schema")
    exact.require(data["source_policy"] == "explicit-rectangle-corner-priority-v1", "source policy")
    vertices, cells, boundary = data["vertices"], data["cells"], data["exterior"]
    exact.require(type(vertices) is list and len(vertices) == 16, "fixed vertex count")
    exact.require(type(cells) is list and len(cells) == 16, "fixed cell count")
    exact.require(type(boundary) is list and len(boundary) == 16, "fixed exterior count")
    rational = []
    for point in vertices:
        exact.require(type(point) is list and len(point) == 3, "vertex shape")
        exact.require(all(type(v) in (int, float) and math.isfinite(v) and abs(v) <= 3
                          for v in point), "vertex values")
        exact.require(point[2] == 0, "two dimensional source")
        rational.append(tuple(F.from_float(float(v)) for v in point[:2]))
    normalized_cells = []
    for cell in cells:
        exact.require(type(cell) is list and len(cell) == 3 and
                      all(type(i) is int and 0 <= i < 16 for i in cell), "cell indices")
        normalized_cells.append(tuple(cell))
    normalized_boundary = []
    names = {"Dirichlet": "dirichlet", "NaturalMechanicalTraction": "natural_mechanical_traction"}
    counts = {}
    for item in boundary:
        exact.require(type(item) is dict and set(item) == {"vertices", "label", "condition"},
                      "exterior shape")
        face, label, condition = item["vertices"], item["label"], item["condition"]
        exact.require(type(face) is list and len(face) == 2 and
                      all(type(i) is int and 0 <= i < 16 for i in face), "exterior indices")
        exact.require(type(label) is str and label in ("cylinder", "inlet", "outlet", "far-wall"),
                      "exterior label")
        exact.require(type(condition) is str and condition in names, "exterior condition")
        exact.require((label == "outlet") == (condition == "NaturalMechanicalTraction"),
                      "outlet traction convention")
        counts[label] = counts.get(label, 0) + 1
        if label != "cylinder":
            left, right = (rational[i] for i in face)
            xmin, xmax, ymin, ymax = F(0), F.from_float(2.2), F(0), F.from_float(0.41)
            common = [k for k, truth in enumerate((
                left[0] == right[0] == xmin, left[0] == right[0] == xmax,
                left[1] == right[1] == ymin, left[1] == right[1] == ymax)) if truth]
            exact.require(len(common) == 1, "not an exact represented rectangle edge")
            exact.require(label == ("inlet", "outlet", "far-wall", "far-wall")[common[0]],
                          "edge label does not match geometry")
        normalized_boundary.append({"vertices": face, "label": label, "condition": names[condition]})
    exact.require(counts == {"cylinder": 8, "inlet": 2, "outlet": 2, "far-wall": 4},
                  "fixed label counts")
    return rational, normalized_cells, normalized_boundary, counts


def main():
    exact.require(len(sys.argv) == 1, "fixed reproducer accepts no parameters")
    started = perf_counter()
    with SOURCE.open("rb") as source:
        data = source.read(65_537)
    exact.require(len(data) <= 65_536, "fixed source exceeds 64 KiB")
    exact.require(sha256(data).hexdigest() == SOURCE_SHA256, "fixed source hash mismatch")
    export = json.loads(data)
    exact.require(type(export.get("fingerprint")) is int and
                  export["fingerprint"] == SOURCE_FINGERPRINT, "source fingerprint")
    vertices, cells, exterior, counts = decode_export(export)
    trials = exact.verify_fixed_prime()
    result = certificate(vertices, cells, exterior)
    exact.require((result["broken_velocity_columns"], result["constraint_rows"],
                   result["exact_rational_rank"], result["full_velocity_dimension"]) ==
                  (192, 138, 138, 54), "fixed rank/count expectation")
    exact.require(result["facets"] == {"interior": 16, "dirichlet": 14,
                                      "natural_mechanical_traction": 2}, "fixed facet counts")
    report = {
        "schema": "quest-cylinder-p2-exact-rank-v1", "status": "completed",
        "scope": ("exact rational constraint rank on fixed represented dyadic coordinates; "
                  "NOT rounded runtime matrix, physical convergence or quantum execution"),
        "source_policy": export["source_policy"], "source_fingerprint": SOURCE_FINGERPRINT,
        "source_json_sha256": SOURCE_SHA256, "source_json_bytes": len(data),
        "script_sha256": sha256(Path(__file__).read_bytes()).hexdigest(),
        "helper_sha256": sha256(Path(exact.__file__).read_bytes()).hexdigest(),
        "python_version": platform.python_version(), "prime": exact.PRIME,
        "odd_trial_divisions": trials,
        "proof": ("denominators invertible modulo verified prime; independently re-eliminated "
                  "full-row nonzero minor meets row-count upper bound"),
        "coordinate_interpretation": ("Fraction.from_float: exact represented binary64, "
                                      "no decimal-rational substitution"),
        "exterior_label_counts": counts,
        "rectangle_edges": ("every non-cylinder exterior edge belongs to the exact represented "
                            "side specified by its label"),
        "pressure_normalization": ("prescribed mechanical traction; "
                                   "every pressure divergence row retained"),
        "cartesian_velocity_monomials": [list(p) for p in POWERS],
        "denominator_entries_verified_invertible": 192 * 138,
        "matrix_hash_framing": "u64LE rows,columns; row-major u64LE ASCII fraction length then bytes",
        "certificate": result, "elapsed_seconds": perf_counter() - started,
    }
    json.dump(report, sys.stdout, indent=2, allow_nan=False)
    sys.stdout.write("\n")


if __name__ == "__main__":
    main()
