//! Four-dimensional algebraic grid enumeration. Exact rational LLL followed by
//! sphere enumeration replaces coordinate scanning. All array indices are
//! bounded by the fixed lattice dimension, independently of caller input.
#![allow(
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::redundant_pub_crate
)]
use crate::{Budget, Result, SynthesisError};
use num_bigint::BigInt;
use num_traits::{One, Signed, Zero};
use quest_math::{Cyclotomic, DyadicBox8, ExactMatrix, Rational};
type Vector = [BigInt; 4];
type Basis = [Vector; 4];
type RVector = [Rational; 4];
struct Orthogonal {
    vectors: [RVector; 4],
    mu: [RVector; 4],
    norms: [Rational; 4],
}
#[derive(Clone, Copy)]
struct SearchWindow<'a> {
    weighted: &'a Rational,
    coefficient: &'a Rational,
    radial_error: &'a Rational,
    radial_headroom: &'a Rational,
}
struct IntegerWindow {
    middle: BigInt,
    bound: BigInt,
    lower: BigInt,
    upper: BigInt,
}
fn integer_window(
    centers: [&Rational; 2],
    radii_squared: [&Rational; 2],
    budget: &mut Budget,
) -> Result<Option<IntegerWindow>> {
    let bounds: [BigInt; 2] = [
        crate::approximation::sqrt(
            &(radii_squared[0].numer() / radii_squared[0].denom()),
            budget,
        )? + 2,
        crate::approximation::sqrt(
            &(radii_squared[1].numer() / radii_squared[1].denom()),
            budget,
        )? + 2,
    ];
    let preferred = nearest(centers[0]);
    let secondary = nearest(centers[1]);
    let lower = (&preferred - &bounds[0]).max(&secondary - &bounds[1]);
    let upper = (&preferred + &bounds[0]).min(&secondary + &bounds[1]);
    for value in [
        &bounds[0], &bounds[1], &preferred, &secondary, &lower, &upper,
    ] {
        admit(value, budget)?;
    }
    if lower > upper {
        return Ok(None);
    }
    let middle = preferred.clamp(lower.clone(), upper.clone());
    let bound = (&middle - &lower).max(&upper - &middle);
    admit(&bound, budget)?;
    Ok(Some(IntegerWindow {
        middle,
        bound,
        lower,
        upper,
    }))
}
pub(crate) struct Grid {
    #[cfg(test)]
    original_basis: Basis,
    reserved_bytes: usize,
    transform: Basis,
    orthogonal: Orthogonal,
    coefficient_metric: Orthogonal,
    radial_projection: RVector,
    radial_headroom: Rational,
    center: BigInt,
    radius: BigInt,
}

fn nearest(value: &Rational) -> BigInt {
    let numerator = value.numer() * 2 + value.denom();
    floor(&numerator, &(value.denom() * 2))
}
fn floor(n: &BigInt, d: &BigInt) -> BigInt {
    let quotient = n / d;
    if (n % d).is_negative() {
        quotient - 1
    } else {
        quotient
    }
}
fn admit(value: &BigInt, budget: &Budget) -> Result<()> {
    if value.bits() > budget.options.limits.coefficient_bits {
        return Err(SynthesisError::Budget {
            resource: "lattice coefficient bits",
        });
    }
    Ok(())
}
fn admit_rational(value: &Rational, budget: &Budget) -> Result<()> {
    admit(value.numer(), budget)?;
    admit(value.denom(), budget)
}
fn orthogonal(basis: &Basis, budget: &mut Budget) -> Result<Orthogonal> {
    budget.charge(256)?;
    for value in basis.iter().flatten() {
        admit(value, budget)?;
    }
    let mut vectors: [RVector; 4] =
        std::array::from_fn(|_| std::array::from_fn(|_| Rational::zero()));
    let mut mu: [RVector; 4] = std::array::from_fn(|_| std::array::from_fn(|_| Rational::zero()));
    let mut norms = std::array::from_fn(|_| Rational::zero());
    for i in 0..4 {
        for j in 0..i {
            let mut dot = Rational::from_integer(
                (0..4).fold(BigInt::zero(), |sum, k| sum + &basis[i][k] * &basis[j][k]),
            );
            admit_rational(&dot, budget)?;
            for k in 0..j {
                dot -= &mu[i][k] * &mu[j][k] * &norms[k];
                admit_rational(&dot, budget)?;
            }
            mu[i][j] = dot / &norms[j];
            admit_rational(&mu[i][j], budget)?;
        }
        norms[i] =
            Rational::from_integer(basis[i].iter().fold(BigInt::zero(), |sum, x| sum + x * x));
        vectors[i][0] = Rational::from_integer(basis[i][0].clone());
        admit_rational(&norms[i], budget)?;
        for j in 0..i {
            norms[i] = &norms[i] - &mu[i][j] * &mu[i][j] * &norms[j];
            vectors[i][0] = &vectors[i][0] - &mu[i][j] * &vectors[j][0];
            admit_rational(&norms[i], budget)?;
            admit_rational(&vectors[i][0], budget)?;
        }
        if norms[i].is_zero() {
            return Err(SynthesisError::Invalid(
                "singular lattice at request precision",
            ));
        }
        for x in &vectors[i] {
            admit(x.numer(), budget)?;
            admit(x.denom(), budget)?;
        }
    }
    Ok(Orthogonal { vectors, mu, norms })
}
fn reduce(mut basis: Basis, budget: &mut Budget) -> Result<(Basis, Orthogonal)> {
    let mut transform =
        std::array::from_fn(|i| std::array::from_fn(|j| BigInt::from(u8::from(i == j))));
    let mut gs = orthogonal(&basis, budget)?;
    let mut k = 1;
    while k < 4 {
        for j in (0..k).rev() {
            let q = nearest(&gs.mu[k][j]);
            admit(&q, budget)?;
            if !q.is_zero() {
                budget.charge(16)?;
                for i in 0..4 {
                    basis[k][i] = &basis[k][i] - &q * &basis[j][i];
                    transform[k][i] = &transform[k][i] - &q * &transform[j][i];
                    admit(&basis[k][i], budget)?;
                    admit(&transform[k][i], budget)?;
                }
                let rational_q = Rational::from_integer(q);
                for l in 0..j {
                    gs.mu[k][l] = &gs.mu[k][l] - &rational_q * &gs.mu[j][l];
                    admit_rational(&gs.mu[k][l], budget)?;
                }
                gs.mu[k][j] -= rational_q;
                admit_rational(&gs.mu[k][j], budget)?;
            }
        }
        if gs.norms[k]
            >= (Rational::new(3.into(), 4.into()) - &gs.mu[k][k - 1] * &gs.mu[k][k - 1])
                * &gs.norms[k - 1]
        {
            k += 1;
        } else {
            basis.swap(k, k - 1);
            transform.swap(k, k - 1);
            gs = orthogonal(&basis, budget)?;
            k = k.saturating_sub(1).max(1);
        }
    }
    Ok((transform, gs))
}
impl Grid {
    pub(crate) fn new(
        target: &DyadicBox8,
        epsilon: &Rational,
        budget: &mut Budget,
    ) -> Result<Self> {
        budget.charge(1)?;
        // Conservative scratch admission for 4x4 rational Gram--Schmidt buffers.
        let scratch = budget
            .options
            .limits
            .coefficient_bits
            .checked_mul(512)
            .ok_or(SynthesisError::Budget {
                resource: "lattice bytes",
            })?;
        if scratch > u64::try_from(budget.options.limits.bytes).unwrap_or(u64::MAX) {
            return Err(SynthesisError::Budget {
                resource: "lattice bytes",
            });
        }
        let scale = BigInt::one() << target.bits();
        admit(&scale, budget)?;
        let square = &scale * &scale;
        admit(&square, budget)?;
        let en = epsilon.numer();
        let ed = epsilon.denom();
        if &scale * en * en < ed * ed * 4096 {
            return Err(SynthesisError::PrecisionUnresolved {
                precision_bits: target.bits(),
            });
        }
        let (xlo, xhi) = target.coordinate(0)?;
        let (ylo, yhi) = target.coordinate(1)?;
        let target_width = (xhi - xlo) + (yhi - ylo);
        if ((xhi - xlo) + (yhi - ylo)) * ed * ed * 4096 > en * en * &scale {
            return Err(SynthesisError::PrecisionUnresolved {
                precision_bits: target.bits(),
            });
        }
        let (lo, hi) = target.coordinate(0)?;
        let x = (lo + hi) >> 1;
        let (lo, hi) = target.coordinate(1)?;
        let y = (lo + hi) >> 1;
        let h = Cyclotomic::new(
            [0.into(), 1.into(), 0.into(), (-1).into()],
            1,
            budget.options.limits,
        )?;
        let matrix = ExactMatrix::from_entries(
            1,
            vec![h, Cyclotomic::zero(), Cyclotomic::zero(), Cyclotomic::one()],
            budget.options.limits,
        )?;
        let root = DyadicBox8::from_exact(&matrix, target.bits(), budget.options.limits)?;
        let (lo, hi) = root.coordinate(0)?;
        let root_width = hi - lo;
        let r = (lo + hi) >> 1;
        let radial = [&x * &scale, &r * (&x + &y), &y * &scale, &r * (-&x + &y)];
        let tangent = [-&y * &scale, &r * (-&y + &x), &x * &scale, &r * (&y + &x)];
        let bullet_real = [square.clone(), -&r * &scale, BigInt::zero(), &r * &scale];
        let bullet_imag = [BigInt::zero(), -&r * &scale, square.clone(), -&r * &scale];
        let factors = [ed * ed * 8, ed * en * 4, en * en * 2, en * en * 2];
        let rows = [radial, tangent, bullet_real, bullet_imag];
        let basis = std::array::from_fn(|i| std::array::from_fn(|j| &rows[j][i] * &factors[j]));
        #[cfg(test)]
        let original_basis = basis.clone();
        let (transform, orthogonal) = reduce(basis, budget)?;
        let coefficient_metric = self::orthogonal(&transform, budget)?;
        let center = (ed * ed * 8 - en * en) * &square;
        // If both algebraic embeddings lie in the unit disk, the coefficient
        // norm is at most one. Replacing sqrt(1/2) by its enclosure midpoint
        // changes the physical point's L1 norm by at most 2*root_width/scale.
        // The target midpoint's norm is at most 1+target_width/scale. Thus this
        // exact rational radial upper bound includes every feasible point.
        let radial_headroom = Rational::from_integer(
            ed * ed * 8 * (&scale + target_width) * (&scale + root_width * 2) - &center,
        );
        budget.charge(64)?;
        let mut radial_projection = std::array::from_fn(|_| Rational::zero());
        let mut projection = Rational::zero();
        for i in 0..4 {
            projection +=
                &orthogonal.vectors[i][0] * &orthogonal.vectors[i][0] / &orthogonal.norms[i];
            admit_rational(&projection, budget)?;
            radial_projection[i] = projection.clone();
        }
        admit_rational(&radial_headroom, budget)?;
        let radius = en * en * &square * 6;
        admit(&center, budget)?;
        admit(&radius, budget)?;
        Ok(Self {
            #[cfg(test)]
            original_basis,
            reserved_bytes: usize::try_from(scratch).map_err(|_| SynthesisError::Budget {
                resource: "lattice bytes",
            })?,
            transform,
            orthogonal,
            coefficient_metric,
            radial_projection,
            radial_headroom,
            center,
            radius,
        })
    }
    pub(crate) const fn reserved_bytes(&self) -> usize {
        self.reserved_bytes
    }
    pub(crate) fn enumerate<T>(
        &self,
        exponent: u32,
        budget: &mut Budget,
        mut visit: impl FnMut(Vector, &mut Budget) -> Result<Option<T>>,
    ) -> Result<Option<T>> {
        budget.charge(16)?;
        let factor = BigInt::one() << exponent;
        admit(&factor, budget)?;
        let center = &self.center * &factor;
        admit(&center, budget)?;
        let y = std::array::from_fn(|i| {
            Rational::from_integer(center.clone()) * &self.orthogonal.vectors[i][0]
                / &self.orthogonal.norms[i]
        });
        let radius = &self.radius * &factor;
        admit(&radius, budget)?;
        let remaining = Rational::from_integer(&radius * &radius);
        let coefficient_radius = Rational::from_integer(&factor * &factor);
        let radial_headroom = &self.radial_headroom * Rational::from_integer(factor);
        for value in &y {
            admit_rational(value, budget)?;
        }
        admit_rational(&remaining, budget)?;
        admit_rational(&coefficient_radius, budget)?;
        admit_rational(&radial_headroom, budget)?;
        self.enumerate_level(
            3,
            &y,
            &mut std::array::from_fn(|_| BigInt::zero()),
            SearchWindow {
                weighted: &remaining,
                coefficient: &coefficient_radius,
                radial_error: &Rational::zero(),
                radial_headroom: &radial_headroom,
            },
            budget,
            &mut visit,
        )
    }
    fn outside_radial_bound(
        &self,
        level: usize,
        remaining: SearchWindow<'_>,
        budget: &Budget,
    ) -> Result<bool> {
        let radial_excess = remaining.radial_error - remaining.radial_headroom;
        admit_rational(&radial_excess, budget)?;
        if radial_excess.is_positive() {
            let squared_excess = &radial_excess * &radial_excess;
            let reachable = remaining.weighted * &self.radial_projection[level];
            admit_rational(&squared_excess, budget)?;
            admit_rational(&reachable, budget)?;
            if squared_excess > reachable {
                return Ok(true);
            }
        }
        Ok(false)
    }
    fn enumerate_level<T>(
        &self,
        level: usize,
        y: &RVector,
        coefficients: &mut Vector,
        remaining: SearchWindow<'_>,
        budget: &mut Budget,
        visit: &mut impl FnMut(Vector, &mut Budget) -> Result<Option<T>>,
    ) -> Result<Option<T>> {
        budget.charge(16)?;
        if self.outside_radial_bound(level, remaining, budget)? {
            return Ok(None);
        }
        let mut center = y[level].clone();
        let mut coefficient_center = Rational::zero();
        for j in (level + 1)..4 {
            center -=
                &self.orthogonal.mu[j][level] * Rational::from_integer(coefficients[j].clone());
            admit_rational(&center, budget)?;
            coefficient_center -= &self.coefficient_metric.mu[j][level]
                * Rational::from_integer(coefficients[j].clone());
            admit_rational(&coefficient_center, budget)?;
        }
        let radius_squared = remaining.weighted / &self.orthogonal.norms[level];
        let coefficient_radius = remaining.coefficient / &self.coefficient_metric.norms[level];
        admit_rational(&coefficient_radius, budget)?;
        admit_rational(&radius_squared, budget)?;
        let Some(IntegerWindow {
            middle,
            bound,
            lower,
            upper,
        }) = integer_window(
            [&center, &coefficient_center],
            [&radius_squared, &coefficient_radius],
            budget,
        )?
        else {
            return Ok(None);
        };
        let mut offset = BigInt::zero();
        while offset <= bound {
            for sign in [1, -1] {
                if offset.is_zero() && sign < 0 {
                    continue;
                }
                budget.charge(8)?;
                let value: BigInt = &middle + &offset * sign;
                admit(&value, budget)?;
                if value < lower || value > upper {
                    continue;
                }
                let error = Rational::from_integer(value.clone()) - &center;
                admit_rational(&error, budget)?;
                let cost = &error * &error * &self.orthogonal.norms[level];
                admit_rational(&cost, budget)?;
                let coefficient_error = Rational::from_integer(value.clone()) - &coefficient_center;
                let coefficient_cost =
                    &coefficient_error * &coefficient_error * &self.coefficient_metric.norms[level];
                admit_rational(&coefficient_error, budget)?;
                admit_rational(&coefficient_cost, budget)?;
                if &cost > remaining.weighted || &coefficient_cost > remaining.coefficient {
                    continue;
                }
                coefficients[level] = value;
                if level == 0 {
                    let point = std::array::from_fn(|i| {
                        (0..4).fold(BigInt::zero(), |sum, j| {
                            sum + &coefficients[j] * &self.transform[j][i]
                        })
                    });
                    for value in &point {
                        admit(value, budget)?;
                    }
                    if let Some(result) = visit(point, budget)? {
                        return Ok(Some(result));
                    }
                } else {
                    let next = [
                        remaining.weighted - cost,
                        remaining.coefficient - coefficient_cost,
                        remaining.radial_error + &error * &self.orthogonal.vectors[level][0],
                    ];
                    for value in &next {
                        admit_rational(value, budget)?;
                    }
                    if let Some(result) = self.enumerate_level(
                        level - 1,
                        y,
                        coefficients,
                        SearchWindow {
                            weighted: &next[0],
                            coefficient: &next[1],
                            radial_error: &next[2],
                            radial_headroom: remaining.radial_headroom,
                        },
                        budget,
                        visit,
                    )? {
                        return Ok(Some(result));
                    }
                }
            }
            offset += 1;
        }
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SynthesisOptions;
    use quest_math::{AngleTarget, Axis, Rational, Target, rotation_enclosure};
    #[expect(
        clippy::many_single_char_names,
        reason = "Exact algebraic coefficients and relative norm components"
    )]
    fn feasible(point: &Vector, radius: &BigInt) -> bool {
        let [a, b, c, d] = point;
        let n = radius * radius - a * a - b * b - c * c - d * d;
        let m = a * (b - d) + c * (b + d);
        !n.is_negative() && &n * &n >= &m * &m * 2
    }
    fn in_original_sphere(grid: &Grid, point: &Vector, factor: &BigInt) -> bool {
        let embedded: Vector = std::array::from_fn(|j| {
            (0..4).fold(BigInt::zero(), |sum, i| {
                sum + &point[i] * &grid.original_basis[i][j]
            })
        });
        let mut difference = embedded;
        difference[0] -= &grid.center * factor;
        let cost = difference.iter().fold(BigInt::zero(), |sum, x| sum + x * x);
        let radius = &grid.radius * factor;
        cost <= &radius * &radius
    }
    #[test]
    fn exact_epsilon_cap_is_contained_in_the_search_sphere() {
        // Independent exact Q(sqrt(2)) comparison for the target z=1.
        // a + (b-d)/sqrt(2) >= radius*(1-epsilon²/4).
        fn in_cap(point: &Vector, radius: &BigInt, epsilon: &Rational) -> bool {
            let right = Rational::from_integer(radius.clone())
                * (Rational::one() - epsilon * epsilon / BigInt::from(4))
                - Rational::from_integer(point[0].clone());
            let radical = &point[1] - &point[3];
            if radical.is_negative() != right.is_negative() {
                return !radical.is_negative();
            }
            let left_square = &radical * &radical * right.denom() * right.denom();
            let right_square = right.numer() * right.numer() * 2;
            if radical.is_negative() {
                left_square <= right_square
            } else {
                left_square >= right_square
            }
        }
        let scale: BigInt = BigInt::one() << 20;
        let target = DyadicBox8::point(
            20,
            [
                scale.clone(),
                0.into(),
                0.into(),
                0.into(),
                0.into(),
                0.into(),
                scale,
                0.into(),
            ],
        )
        .unwrap();
        for denominator in [1, 2, 8] {
            let epsilon = Rational::new(1.into(), denominator.into());
            let mut budget = Budget {
                options: SynthesisOptions::default(),
                used: 0,
            };
            let grid = Grid::new(&target, &epsilon, &mut budget).unwrap();
            let mut witnesses = 0;
            for exponent in 0..=2 {
                let bound = 1_i32 << exponent;
                let radius = BigInt::from(bound);
                for a in -bound..=bound {
                    for b in -bound..=bound {
                        for c in -bound..=bound {
                            for d in -bound..=bound {
                                let point = [a.into(), b.into(), c.into(), d.into()];
                                if feasible(&point, &radius) && in_cap(&point, &radius, &epsilon) {
                                    witnesses += 1;
                                    assert!(
                                        in_original_sphere(&grid, &point, &radius),
                                        "epsilon={epsilon}, exponent={exponent}, point={point:?}"
                                    );
                                }
                            }
                        }
                    }
                }
            }
            assert!(witnesses >= 3, "every level contains the exact target");
        }
    }
    #[test]
    fn intersection_pruning_preserves_every_feasible_small_grid_point() {
        for (numerator, denominator) in [(0, 1), (1, 1), (1, 2), (1, 4), (1, 7), (-1, 4), (5, 4)] {
            let options = SynthesisOptions::default();
            let mut budget = Budget {
                options: options.clone(),
                used: 0,
            };
            let target = Target {
                axis: Axis::Z,
                angle: AngleTarget::RationalPi {
                    numerator: numerator.into(),
                    denominator: denominator.into(),
                },
            };
            let enclosure =
                rotation_enclosure(&target, options.limits.precision_bits, options.limits).unwrap();
            let grid =
                Grid::new(&enclosure, &Rational::new(1.into(), 2.into()), &mut budget).unwrap();
            for exponent in 0..=2 {
                let radius = 1i32 << exponent;
                let factor = BigInt::from(radius);
                let mut expected = std::collections::BTreeSet::new();
                for a in -radius..=radius {
                    for b in -radius..=radius {
                        for c in -radius..=radius {
                            for d in -radius..=radius {
                                let point = [a.into(), b.into(), c.into(), d.into()];
                                if feasible(&point, &factor)
                                    && in_original_sphere(&grid, &point, &factor)
                                {
                                    expected.insert(point);
                                }
                            }
                        }
                    }
                }
                let mut actual = std::collections::BTreeSet::new();
                grid.enumerate(exponent, &mut budget, |point, _| {
                    if feasible(&point, &factor) {
                        actual.insert(point);
                    }
                    Ok(None::<()>)
                })
                .unwrap();
                assert_eq!(
                    actual, expected,
                    "numerator={numerator} denominator={denominator} exponent={exponent}"
                );
            }
        }
    }
    #[test]
    fn orthogonal_rational_storage_obeys_coefficient_bounds() {
        let mut budget = Budget {
            options: SynthesisOptions {
                limits: quest_math::Limits {
                    coefficient_bits: 8,
                    ..quest_math::Limits::default()
                },
                ..SynthesisOptions::default()
            },
            used: 0,
        };
        let basis = [
            [127.into(), 0.into(), 0.into(), 0.into()],
            [0.into(), 127.into(), 0.into(), 0.into()],
            [0.into(), 0.into(), 127.into(), 0.into()],
            [0.into(), 0.into(), 0.into(), 127.into()],
        ];
        assert!(matches!(
            orthogonal(&basis, &mut budget),
            Err(SynthesisError::Budget {
                resource: "lattice coefficient bits"
            })
        ));
    }
    #[test]
    fn reduced_lattice_finds_exact_diagonal_at_twelve_digits() {
        let options = SynthesisOptions::default();
        let mut budget = Budget {
            options: options.clone(),
            used: 0,
        };
        let target = Target {
            axis: Axis::Z,
            angle: AngleTarget::RationalPi {
                numerator: 1.into(),
                denominator: 2.into(),
            },
        };
        let enclosure =
            rotation_enclosure(&target, options.limits.precision_bits, options.limits).unwrap();
        let epsilon = Rational::new(1.into(), 1_000_000_000_000_u64.into());
        let lattice = Grid::new(&enclosure, &epsilon, &mut budget).unwrap();
        let found = lattice
            .enumerate(0, &mut budget, |coefficients, _| {
                let u = quest_math::Cyclotomic::new(coefficients, 0, options.limits)?;
                Ok((u == quest_math::Cyclotomic::omega(7)).then_some(u))
            })
            .unwrap();
        assert!(found.is_some());
    }
}
