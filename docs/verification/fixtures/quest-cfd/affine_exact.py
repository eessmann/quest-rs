#!/usr/bin/env python3
"""Fixed ideal-rational affine BDM constraint and normalized P2 mass certificates.

Run with Python's standard library only. No input geometry, polynomial degree,
prime, production module or symbolic engine is accepted. These exact rational
fixtures are NOT the represented-binary64 matrices of a compiled CFD binary.
"""
from fractions import Fraction
from hashlib import sha256
from itertools import combinations, permutations, product
import json
from math import factorial, isqrt
from pathlib import Path
import platform
import sys
from time import perf_counter

PRIME = 1_000_000_007
EXPECTED = {
    ("closed_trapezoid", 1): (12, 12, 11, 1),
    ("closed_trapezoid", 2): (24, 21, 20, 4),
    ("closed_bipyramid", 1): (24, 23, 22, 2),
    ("closed_bipyramid", 2): (60, 50, 49, 11),
    ("periodic_square_fan", 1): (24, 16, 15, 9),
    ("periodic_square_fan", 2): (48, 30, 29, 19),
    ("periodic_two_strip_cube", 1): (144, 84, 83, 61),
    ("periodic_two_strip_cube", 2): (360, 192, 191, 169),
}


def require(condition, message):
    """Proof checks deliberately survive python -O."""
    if not condition:
        raise ValueError(message)


def verify_fixed_prime():
    require(PRIME == 1_000_000_007 and PRIME % 2 != 0, "unexpected prime")
    trials = 0
    for divisor in range(3, isqrt(PRIME) + 1, 2):
        trials += 1
        require(PRIME % divisor != 0, "fixed modulus is not prime")
    return trials


def determinant(matrix):
    """Only 1x1, 2x2 or 3x3 Cartesian determinants occur."""
    size = len(matrix)
    require(1 <= size <= 3 and all(len(row) == size for row in matrix),
            "invalid determinant shape")
    if size == 1:
        return matrix[0][0]
    return sum((-1) ** column * matrix[0][column] * determinant(
        [row[:column] + row[column + 1:] for row in matrix[1:]])
        for column in range(size))


def fixed_mesh(name):
    if name == "closed_trapezoid":
        vertices = [(0, 0), (2, 0), (1, 1), (0, 1)]
        cells = [(0, 1, 2), (0, 2, 3)]
        dimension, periodic = 2, False
    elif name == "closed_bipyramid":
        vertices = [(0, 0, 0), (1, 0, 0), (0, 1, 0), (0, 0, 1), (0, 0, -2)]
        cells = [(0, 1, 2, 3), (0, 1, 2, 4)]
        dimension, periodic = 3, False
    elif name == "periodic_square_fan":
        vertices = [(0, 0), (1, 0), (1, 1), (0, 1),
                    (Fraction(3, 10), Fraction(2, 5))]
        cells = [(0, 1, 4), (1, 2, 4), (2, 3, 4), (3, 0, 4)]
        dimension, periodic = 2, True
    elif name == "periodic_two_strip_cube":
        dimension, periodic = 3, True
        cuts = [(Fraction(0), Fraction(1, 3), Fraction(1)),
                (Fraction(0), Fraction(1)), (Fraction(0), Fraction(1))]
        vertices = list(product(*cuts))
        ids = {point: index for index, point in enumerate(vertices)}
        cells = []
        for strip in range(2):
            for axes in permutations(range(3)):
                index = [strip, 0, 0]
                cell = [ids[tuple(cuts[axis][index[axis]] for axis in range(3))]]
                for axis in axes:
                    index[axis] += 1
                    cell.append(ids[tuple(cuts[j][index[j]] for j in range(3))])
                cells.append(tuple(cell))
    else:
        raise ValueError("fixture is outside the finite supported set")
    return dimension, periodic, [tuple(map(Fraction, point)) for point in vertices], cells


def constrained_facets(dimension, periodic, vertices, cells):
    owners = {}
    for cell_id, cell in enumerate(cells):
        for face in combinations(cell, dimension):
            owners.setdefault(tuple(sorted(face)), []).append(cell_id)
    interior, exterior = [], []
    for face, cell_ids in owners.items():
        require(1 <= len(cell_ids) <= 2, "invalid fixed facet incidence")
        if len(cell_ids) == 2:
            interior.append((face, cell_ids[0], face, cell_ids[1]))
        else:
            exterior.append((face, cell_ids[0]))
    if not periodic:
        return interior + [(face, cell_id, None, None) for face, cell_id in exterior]
    used = set()
    for index, (face, cell_id) in enumerate(exterior):
        if index in used:
            continue
        axes = [axis for axis in range(dimension)
                if all(vertices[node][axis] == vertices[face[0]][axis] for node in face)
                and vertices[face[0]][axis] in (0, 1)]
        require(len(axes) == 1, "fixed exterior facet is not a box side")
        axis = axes[0]
        shift = 1 if vertices[face[0]][axis] == 0 else -1
        shifted = [tuple(value + (shift if j == axis else 0)
                         for j, value in enumerate(vertices[node])) for node in face]
        partners = [(other_index, other_face, other_cell)
                    for other_index, (other_face, other_cell) in enumerate(exterior)
                    if set(vertices[node] for node in other_face) == set(shifted)]
        require(len(partners) == 1, "periodic correspondence is not unique")
        other_index, other_face, other_cell = partners[0]
        require(other_index not in used and other_index != index, "duplicate periodic pair")
        mapped = tuple(next(node for node in other_face if vertices[node] == point)
                       for point in shifted)
        interior.append((face, cell_id, mapped, other_cell))
        used.update((index, other_index))
    require(len(used) == len(exterior), "periodic exterior not fully paired")
    return interior


def monomial(point, powers):
    result = Fraction(1)
    for value, exponent in zip(point, powers):
        result *= value ** exponent
    return result


def cartesian_moment(vertices, volume, powers):
    """Integrate degree<=2 from barycentric first/second moments, independently."""
    dimension = len(vertices[0])
    axes = [axis for axis, exponent in enumerate(powers) for _ in range(exponent)]
    require(len(axes) <= 2, "moment degree exceeds the fixed P1/P2 proof")
    if not axes:
        return volume
    if len(axes) == 1:
        return volume * sum(point[axes[0]] for point in vertices) / (dimension + 1)
    i, j = axes
    return volume * (sum(point[i] for point in vertices) * sum(point[j] for point in vertices)
                     + sum(point[i] * point[j] for point in vertices)) / (
                         (dimension + 1) * (dimension + 2))


def constraint_system(name, degree):
    dimension, periodic, vertices, cells = fixed_mesh(name)
    powers = [entry for total in range(degree + 1)
              for entry in product(range(degree + 1), repeat=dimension) if sum(entry) == total]
    local = dimension * len(powers)
    columns = len(cells) * local
    facets = constrained_facets(dimension, periodic, vertices, cells)
    rows, dependency = [], []
    for face, cell_id, right, right_id in facets:
        edges = [[vertices[face[k]][j] - vertices[face[0]][j] for j in range(dimension)]
                 for k in range(1, dimension)]
        normal = [(-1) ** j * determinant([edge[:j] + edge[j + 1:] for edge in edges])
                  for j in range(dimension)]
        center = [sum(vertices[node][j] for node in cells[cell_id]) / (dimension + 1)
                  for j in range(dimension)]
        orientation = sum(normal[j] * (vertices[face[0]][j] - center[j])
                          for j in range(dimension))
        require(orientation != 0, "zero oriented fixed facet")
        sign = 1 if orientation > 0 else -1
        # Facet measure / magnitude of our unnormalized rational normal.
        gamma = Fraction(1, factorial(dimension - 1))
        nodes = [(Fraction(1), i, i) for i in range(dimension)]
        if degree == 2:
            nodes.extend((Fraction(1, 2), i, j) for i, j in combinations(range(dimension), 2))
        for weight, i, j in nodes:
            left_point = tuple(weight * vertices[face[i]][axis]
                               + (1 - weight) * vertices[face[j]][axis]
                               for axis in range(dimension))
            right_point = (tuple(weight * vertices[right[i]][axis]
                                 + (1 - weight) * vertices[right[j]][axis]
                                 for axis in range(dimension)) if right is not None else None)
            row = [Fraction(0)] * columns
            for component in range(dimension):
                for monomial_id, exponents in enumerate(powers):
                    column = cell_id * local + component * len(powers) + monomial_id
                    row[column] += normal[component] * monomial(left_point, exponents)
                    if right_id is not None:
                        column = right_id * local + component * len(powers) + monomial_id
                        row[column] -= normal[component] * monomial(right_point, exponents)
            rows.append(row)
            integral_weight = (gamma / dimension if degree == 1 else gamma * Fraction(
                3 - dimension if i == j else 4, dimension * (dimension + 1)))
            dependency.append(-sign * integral_weight)
    facet_rows = len(rows)
    volumes = []
    for cell_id, cell in enumerate(cells):
        points = [vertices[node] for node in cell]
        volume = abs(determinant([[points[k][j] - points[0][j] for j in range(dimension)]
                                  for k in range(1, dimension + 1)])) / factorial(dimension)
        require(volume > 0, "degenerate ideal rational cell")
        volumes.append(volume)
        tests = [(0,) * dimension]
        if degree == 2:
            tests.extend(tuple(int(i == j) for i in range(dimension)) for j in range(dimension))
        for test in tests:
            row = [Fraction(0)] * columns
            for component in range(dimension):
                for monomial_id, exponents in enumerate(powers):
                    if exponents[component]:
                        derivative = tuple(exponents[j] + test[j] - int(j == component)
                                           for j in range(dimension))
                        row[cell_id * local + component * len(powers) + monomial_id] = (
                            exponents[component] * cartesian_moment(points, volume, derivative))
            rows.append(row)
            dependency.append(Fraction(1) if not any(test) else Fraction(0))
    return dimension, periodic, vertices, cells, powers, rows, dependency, volumes, len(facets), facet_rows


def modular_matrix(matrix):
    width = len(matrix[0])
    require(all(len(row) == width for row in matrix), "inconsistent fixed row width")
    inverse = {}
    for row in matrix:
        for value in row:
            denominator = value.denominator % PRIME
            require(denominator != 0, "rational denominator not invertible modulo the prime")
            if denominator not in inverse:
                inverse[denominator] = pow(denominator, PRIME - 2, PRIME)
    return [[value.numerator * inverse[value.denominator % PRIME] % PRIME for value in row]
            for row in matrix]


def modular_rank(matrix):
    work = [row.copy() for row in matrix]
    original_rows = list(range(len(work)))
    pivot_rows, pivot_columns = [], []
    rank = 0
    for column in range(len(work[0])):
        pivot = next((row for row in range(rank, len(work)) if work[row][column]), None)
        if pivot is None:
            continue
        work[rank], work[pivot] = work[pivot], work[rank]
        original_rows[rank], original_rows[pivot] = original_rows[pivot], original_rows[rank]
        pivot_rows.append(original_rows[rank])
        pivot_columns.append(column)
        inverse = pow(work[rank][column], PRIME - 2, PRIME)
        work[rank] = [value * inverse % PRIME for value in work[rank]]
        for row in range(rank + 1, len(work)):
            factor = work[row][column]
            if factor:
                work[row] = [(value - factor * pivot_value) % PRIME
                             for value, pivot_value in zip(work[row], work[rank])]
        rank += 1
        if rank == len(work):
            break
    return rank, pivot_rows, pivot_columns


def verify_pivot_minor(matrix, pivot_rows, pivot_columns):
    """Separate square determinant elimination confirms the selected minor."""
    minor = [[matrix[row][column] for column in pivot_columns] for row in pivot_rows]
    result = 1
    for column in range(len(minor)):
        pivot = next((row for row in range(column, len(minor)) if minor[row][column]), None)
        require(pivot is not None, "selected modular minor is singular")
        if pivot != column:
            minor[column], minor[pivot] = minor[pivot], minor[column]
            result = -result
        diagonal = minor[column][column]
        result = result * diagonal % PRIME
        inverse = pow(diagonal, PRIME - 2, PRIME)
        for row in range(column + 1, len(minor)):
            factor = minor[row][column] * inverse % PRIME
            if factor:
                minor[row] = [(value - factor * pivot_value) % PRIME
                              for value, pivot_value in zip(minor[row], minor[column])]
    require(result != 0, "zero modular minor determinant")
    return result


def rank_certificate(name, degree):
    dimension, periodic, vertices, cells, powers, matrix, dependency, volumes, facets, facet_rows = (
        constraint_system(name, degree))
    require(any(dependency), "zero vector cannot certify an upper rank bound")
    columns = len(matrix[0])
    require(len(dependency) == len(matrix), "dependency length mismatch")
    for column in range(columns):
        require(sum(weight * row[column] for weight, row in zip(dependency, matrix)) == 0,
                "exact oriented divergence-theorem row dependency failed")
    modular = modular_matrix(matrix)
    lower, pivot_rows, pivot_columns = modular_rank(modular)
    upper = len(matrix) - 1
    require(lower == upper, "modular lower bound does not meet the exact dependency upper bound")
    minor_determinant = verify_pivot_minor(modular, pivot_rows, pivot_columns)
    result_tuple = (columns, len(matrix), lower, columns - lower)
    require(result_tuple == EXPECTED[name, degree], "documented fixed fixture dimensions changed")
    identity = sha256()
    for count in (len(matrix), columns):
        identity.update(count.to_bytes(8, "little"))
    for row in matrix:
        for value in row:
            word = str(value).encode("ascii")
            identity.update(len(word).to_bytes(8, "little"))
            identity.update(word)
    return {
        "fixture": name, "dimension": dimension, "periodic": periodic, "velocity_degree": degree,
        "pressure_degree": degree - 1, "vertices": [[str(x) for x in p] for p in vertices],
        "cells": [list(cell) for cell in cells], "cell_volumes": list(map(str, volumes)),
        "constrained_facets": facets, "cartesian_velocity_monomials": [list(a) for a in powers],
        "pressure_basis": "constant; plus Cartesian linear coordinates for P1",
        "broken_velocity_columns": columns, "constraint_rows": len(matrix),
        "facet_trace_rows": facet_rows, "divergence_rows": len(matrix) - facet_rows,
        "prime": PRIME, "denominator_entries_verified_invertible": len(matrix) * columns,
        "exact_oriented_dependency_nonzero": True, "exact_dependency_columns_verified": columns,
        "exact_row_dependency": list(map(str, dependency)), "rational_rank_upper_bound": upper,
        "modular_rank_lower_bound": lower, "pivot_minor_rows": pivot_rows,
        "pivot_minor_columns": pivot_columns, "pivot_minor_determinant_mod_prime": minor_determinant,
        "exact_rational_rank": lower, "full_velocity_dimension": columns - lower,
        "documented_dimensions_match": True, "constraint_matrix_sha256": identity.hexdigest(),
        "matrix_hash_framing": "u64LE rows,columns; row-major u64LE ASCII fraction byte count then bytes",
    }


def p2_mass_certificate(dimension):
    count = dimension + 1
    basis, names = [], []
    for vertex in range(count):
        linear = tuple(int(i == vertex) for i in range(count))
        quadratic = tuple(2 * exponent for exponent in linear)
        basis.append({quadratic: Fraction(2), linear: Fraction(-1)})
        names.append(str(vertex))
    for i, j in combinations(range(count), 2):
        basis.append({tuple(int(k in (i, j)) for k in range(count)): Fraction(4)})
        names.append(f"{i}{j}")
    matrix = []
    for left in basis:
        row = []
        for right in basis:
            value = Fraction(0)
            for a, x in left.items():
                for b, y in right.items():
                    powers = tuple(u + v for u, v in zip(a, b))
                    numerator = factorial(dimension)
                    for exponent in powers:
                        numerator *= factorial(exponent)
                    value += x * y * Fraction(numerator, factorial(dimension + sum(powers)))
            row.append(value)
        matrix.append(row)
    categories = {"vertex_diagonal": ("0", "0"), "distinct_vertices": ("0", "1"),
                  "vertex_incident_edge": ("0", "01"), "vertex_nonincident_edge": ("0", "12"),
                  "edge_diagonal": ("01", "01"), "edges_sharing_vertex": ("01", "02")}
    if dimension == 3:
        categories["disjoint_edges"] = ("01", "23")
    entries = {name: str(matrix[names.index(left)][names.index(right)])
               for name, (left, right) in categories.items()}
    expected = (["1/30", "-1/180", "0", "-1/45", "8/45", "4/45"] if dimension == 2 else
                ["1/70", "1/420", "-1/105", "-1/70", "8/105", "4/105", "2/105"])
    require(list(entries.values()) == expected, "independent documented unit-mass fractions changed")
    require(sum(map(sum, matrix)) == 1, "integrated P2 partition of unity failed")
    size = len(matrix)
    factors = [[Fraction(0)] * size for _ in range(size)]
    diagonal = []
    for i in range(size):
        require(all(matrix[i][j] == matrix[j][i] for j in range(size)), "mass is not symmetric")
        factors[i][i] = Fraction(1)
        for j in range(i):
            factors[i][j] = (matrix[i][j] - sum(factors[i][k] * diagonal[k] * factors[j][k]
                                               for k in range(j))) / diagonal[j]
        pivot = matrix[i][i] - sum(factors[i][k] ** 2 * diagonal[k] for k in range(i))
        require(pivot > 0, "normalized exact mass is not positive definite")
        diagonal.append(pivot)
    return {"dimension": dimension, "degree": 2, "basis_vertex_then_lexicographic_edges": names,
            "moment_identity": "integral(lambda^a)/volume = d!*product(a_i!)/(d+sum(a_i))!",
            "normalized_mass_matrix": [[str(x) for x in row] for row in matrix],
            "entry_categories": entries, "documented_fractions_match": True,
            "integrated_partition_of_unity": "1", "positive_ldlt_diagonal": list(map(str, diagonal))}


def main():
    if len(sys.argv) != 1:
        raise ValueError("this fixed reproducer accepts no fixture parameters; redirect stdout to save JSON")
    start = perf_counter()
    trials = verify_fixed_prime()
    certificates = [rank_certificate(name, degree)
                    for name in ("closed_trapezoid", "closed_bipyramid", "periodic_square_fan",
                                 "periodic_two_strip_cube") for degree in (1, 2)]
    mass = [p2_mass_certificate(dimension) for dimension in (2, 3)]
    report = {"schema": "quest-affine-exact-fixtures-v1", "status": "completed",
              "scope": "fixed ideal-rational fixtures; NOT represented-binary64 production/runtime rank certificates",
              "independent_assembly": "stdlib Fraction Cartesian monomials, exact moments and rational oriented normals; no production imports",
              "proof": "prime verified, every denominator invertible, nonzero exact oriented dependency plus nonzero modular pivot minor; lower=upper=rows-1",
              "finite_fixture_set": "four named meshes, both orders1/2; no nine-tet/refinement/unbounded-input path",
              "script_sha256": sha256(Path(__file__).read_bytes()).hexdigest(),
              "python_version": platform.python_version(), "elapsed_seconds": perf_counter() - start,
              "prime": PRIME, "odd_trial_divisions": trials, "rank_certificates": certificates,
              "normalized_p2_mass": mass}
    json.dump(report, sys.stdout, indent=2, allow_nan=False)
    sys.stdout.write("\n")


if __name__ == "__main__":
    main()
