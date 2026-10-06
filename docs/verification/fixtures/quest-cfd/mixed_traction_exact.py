#!/usr/bin/env python3
"""Exact fixed mixed-boundary rank and energy fixtures; standard library only.

These Cartesian rational matrices are independent of the Rust finite-element
assembly. They certify only the named ideal-rational fixtures, not arbitrary
geometry or the represented binary64 runtime matrices. The shared verifier
primitives in affine_exact.py are included in the provenance hashes.
"""
from fractions import Fraction as F
from hashlib import sha256
from itertools import combinations, product
import json
from math import factorial
from pathlib import Path
import platform
import sys
from time import perf_counter

import affine_exact as exact


EXPECTED = {(2, 1): (6, 5, 1), (2, 2): (12, 9, 3),
            (3, 1): (12, 10, 2), (3, 2): (30, 22, 8)}


def fixed_system(dimension, order):
    exact.require((dimension, order) in EXPECTED, "outside finite fixture set")
    vertices = [tuple(F(0) for _ in range(dimension))]
    for axis in range(dimension):
        vertices.append(tuple(F(2 if dimension == 2 and axis == 0 else 1)
                              if j == axis else F(0) for j in range(dimension)))
    powers = [a for total in range(order + 1)
              for a in product(range(order + 1), repeat=dimension) if sum(a) == total]
    columns = dimension * len(powers)
    # Coordinate planes are Dirichlet. The opposite facet has prescribed total
    # viscous-pressure traction and contributes NO normal-velocity constraint.
    dirichlet = [face for face in combinations(range(dimension + 1), dimension)
                 if 0 in face]
    natural = tuple(range(1, dimension + 1))
    rows = []
    for face in dirichlet:
        edges = [[vertices[face[k]][j] - vertices[face[0]][j]
                  for j in range(dimension)] for k in range(1, dimension)]
        normal = [(-1) ** j * exact.determinant([e[:j] + e[j + 1:] for e in edges])
                  for j in range(dimension)]
        nodes = [vertices[i] for i in face]
        if order == 2:
            nodes.extend(tuple((vertices[i][j] + vertices[k][j]) / 2
                               for j in range(dimension)) for i, k in combinations(face, 2))
        for node in nodes:
            rows.append([normal[component] * exact.monomial(node, alpha)
                         for component in range(dimension) for alpha in powers])
    facet_rows = len(rows)
    volume = abs(exact.determinant([[vertices[k][j] - vertices[0][j]
                                     for j in range(dimension)]
                                    for k in range(1, dimension + 1)])) / factorial(dimension)
    tests = [(0,) * dimension]
    if order == 2:
        tests.extend(tuple(int(j == axis) for j in range(dimension))
                     for axis in range(dimension))
    for test in tests:
        row = []
        for component in range(dimension):
            for alpha in powers:
                value = F(0)
                if alpha[component]:
                    derivative = tuple(alpha[j] + test[j] - int(j == component)
                                       for j in range(dimension))
                    value = alpha[component] * exact.cartesian_moment(vertices, volume, derivative)
                row.append(value)
        rows.append(row)
    exact.require(all(len(row) == columns for row in rows), "constraint width")
    return vertices, dirichlet, natural, powers, rows, facet_rows, volume


def rank_certificate(dimension, order):
    vertices, dirichlet, natural, powers, rows, facet_rows, volume = fixed_system(dimension, order)
    modular = exact.modular_matrix(rows)
    lower, pivot_rows, pivot_columns = exact.modular_rank(modular)
    # The row count itself is an upper bound. A full-row square minor proves
    # equality over the rationals, without imposing a closed-domain dependency.
    exact.require(lower == len(rows), "natural-boundary matrix is not full row rank")
    minor = exact.verify_pivot_minor(modular, pivot_rows, pivot_columns)
    columns = len(rows[0])
    exact.require((columns, lower, columns - lower) == EXPECTED[dimension, order],
                  "fixed rank/dimension expectation changed")
    identity = sha256()
    for n in (len(rows), columns):
        identity.update(n.to_bytes(8, "little"))
    for row in rows:
        for value in row:
            encoded = str(value).encode("ascii")
            identity.update(len(encoded).to_bytes(8, "little"))
            identity.update(encoded)
    return {
        "dimension": dimension, "velocity_degree": order, "pressure_degree": order - 1,
        "vertices": [[str(x) for x in point] for point in vertices],
        "dirichlet_facets": [list(face) for face in dirichlet],
        "natural_traction_facet": list(natural), "volume": str(volume),
        "cartesian_velocity_monomials": [list(a) for a in powers],
        "broken_velocity_columns": columns, "facet_trace_rows": facet_rows,
        "divergence_rows": len(rows) - facet_rows, "constraint_rows": len(rows),
        "rational_rank_upper_bound": len(rows), "modular_rank_lower_bound": lower,
        "exact_rational_rank": lower, "full_velocity_dimension": columns - lower,
        "prime": exact.PRIME, "denominator_entries_verified_invertible": len(rows) * columns,
        "pivot_minor_rows": pivot_rows, "pivot_minor_columns": pivot_columns,
        "pivot_minor_determinant_mod_prime": minor,
        "pressure_normalization": "fixed by prescribed mechanical traction; no mean subtraction",
        "constraint_matrix_sha256": identity.hexdigest(),
        "matrix_hash_framing": "u64LE rows,columns; row-major u64LE ASCII fraction length then bytes",
    }


def exact_energy():
    # x=2s,y=1-s; outward n ds=(1,2)ds. For u=a(x,-y),
    # (u.n)|u|^2 ds/a^3 = (4s-2)(5s^2-2s+1).
    normal_flux = [F(-2), F(4)]
    speed_squared = [F(1), F(-2), F(5)]
    coefficients = [F(0)] * 4
    for i, a in enumerate(normal_flux):
        for j, b in enumerate(speed_squared):
            coefficients[i + j] += a * b
    integral = sum(value / (degree + 1) for degree, value in enumerate(coefficients))
    exact.require(coefficients == list(map(F, (-2, 8, -18, 20))) and integral == 1,
                  "independent natural energy integral")
    return {"triangle": "(0,0),(2,0),(0,1)", "field": "u=a(x,-y)",
            "facet_parameter": "x=2s, y=1-s, 0<=s<=1; outward n ds=(1,2)ds",
            "integrand_over_a_cubed_ascending": list(map(str, coefficients)),
            "outward_energy_flux_over_a_cubed": str(integral),
            "convective_power_over_a_cubed": str(-integral / 2),
            "positive_a": "outward energy transport", "negative_a": "energy influx; no backflow stability claim"}


def main():
    exact.require(len(sys.argv) == 1, "fixed reproducer accepts no parameters")
    start = perf_counter()
    trials = exact.verify_fixed_prime()
    certificates = [rank_certificate(d, p) for d in (2, 3) for p in (1, 2)]
    report = {
        "schema": "quest-mixed-traction-exact-fixtures-v1", "status": "completed",
        "scope": "fixed ideal-rational fixtures; NOT binary64 runtime rank or convergence certificates",
        "proof": "verified prime and invertible denominators; nonzero full-row modular minor meets row-count upper bound",
        "independent_assembly": "Cartesian Fraction monomials and exact moments; no production imports",
        "script_sha256": sha256(Path(__file__).read_bytes()).hexdigest(),
        "helper_sha256": sha256(Path(exact.__file__).read_bytes()).hexdigest(),
        "python_version": platform.python_version(), "odd_trial_divisions": trials,
        "rank_certificates": certificates, "boundary_energy": exact_energy(),
        "elapsed_seconds": perf_counter() - start,
    }
    json.dump(report, sys.stdout, indent=2, allow_nan=False)
    sys.stdout.write("\n")


if __name__ == "__main__":
    main()
