use super::{
    Context, OfflineError as Error, OfflineResult as Result, fft,
    number::{Number, add, div, mul, neg, sqrt, sub, validate},
};
use crate::precision::{checked, exact_from_f64};
use astro_float::{BigFloat, RoundingMode};
pub(super) type Matrix = [Number; 4];
fn at(values: &[Number], index: usize, precision: u32) -> Number {
    values
        .get(index)
        .cloned()
        .unwrap_or_else(|| Number::zero(precision))
}
fn reverse(values: &[Number]) -> Vec<Number> {
    values.iter().rev().map(Number::conj).collect()
}
pub(super) fn controls(gamma: &[Number], context: &mut Context) -> Result<Vec<Matrix>> {
    context.charge(
        gamma
            .len()
            .checked_mul(64)
            .ok_or(Error::Budget("offline control work"))?,
    )?;
    let p = context.precision;
    let one = BigFloat::from_i64(1, crate::offline::number::precision_bits(p));
    let zero = BigFloat::from_i64(0, crate::offline::number::precision_bits(p));
    let mut result = Vec::with_capacity(gamma.len());
    for value in gamma {
        value.validate()?;
        let mut scale = one.clone();
        for component in [&value.re, &value.im] {
            let magnitude = component.clone().abs();
            if magnitude > scale {
                scale = magnitude;
            }
        }
        let re = div(&value.re, &scale);
        let im = div(&value.im, &scale);
        let scaled_one = div(&one, &scale);
        let norm = sqrt(&add(
            &add(&mul(&re, &re), &mul(&im, &im)),
            &mul(&scaled_one, &scaled_one),
        ));
        validate(&norm)?;
        let diagonal = Number {
            re: div(&scaled_one, &norm),
            im: zero.clone(),
        };
        let off = Number {
            re: div(&re, &norm),
            im: div(&im, &norm),
        };
        result.push([diagonal.clone(), off.clone(), off.conj().neg(), diagonal]);
    }
    Ok(result)
}
pub(super) fn completion(
    target: &[Number],
    context: &mut Context,
) -> Result<(Vec<Number>, Vec<Number>, BigFloat, usize)> {
    let p = context.precision;
    let one = BigFloat::from_i64(1, crate::offline::number::precision_bits(p));
    let half = exact_from_f64(0.5, p)?;
    let two = BigFloat::from_i64(2, crate::offline::number::precision_bits(p));
    let tolerance = div(
        &exact_from_f64(context.policy.certification.completion_tolerance, p)?,
        &BigFloat::from_i64(16, crate::offline::number::precision_bits(p)),
    );
    let mut grid = target
        .len()
        .checked_mul(4)
        .and_then(usize::checked_next_power_of_two)
        .ok_or(Error::Budget("offline completion grid"))?
        .max(32);
    while grid <= context.policy.max_grid {
        context.admit_grid(grid)?;
        let mut values = vec![Number::zero(p); grid];
        for (out, value) in values.iter_mut().zip(target) {
            out.clone_from(value);
        }
        fft::transform(&mut values, true, false, context)?;
        let mut ratio_samples = values.clone();
        for value in &mut values {
            let remainder = sub(
                &one,
                &add(&mul(&value.re, &value.re), &mul(&value.im, &value.im)),
            );
            validate(&remainder)?;
            if !remainder.is_positive() || remainder.is_zero() {
                return Err(Error::Numerical("offline Weiss logarithm domain"));
            }
            *value = Number::real(mul(
                &half,
                &checked(remainder.ln(
                    crate::offline::number::precision_bits(p),
                    RoundingMode::ToEven,
                    &mut context.constants,
                ))?,
            ));
        }
        fft::transform(&mut values, false, true, context)?;
        for (index, value) in values.iter_mut().enumerate().skip(1) {
            if index <= grid / 2 {
                *value = Number::zero(p);
            } else {
                *value = value.scale(&two);
            }
        }
        fft::transform(&mut values, true, false, context)?;
        for (sample, exponent) in ratio_samples.iter_mut().zip(&values) {
            *sample = sample.mul(&exponent.neg().exp(&mut context.constants)?);
            sample.validate()?;
        }
        fft::transform(&mut ratio_samples, false, true, context)?;
        let ratio = ratio_samples
            .get(..target.len())
            .ok_or(Error::Budget("offline ratio support"))?
            .to_vec();
        for value in &mut values {
            *value = value.exp(&mut context.constants)?;
            if !value.is_finite() {
                return Err(Error::Numerical("offline Weiss exponential"));
            }
        }
        fft::transform(&mut values, false, true, context)?;
        let mut astar = Vec::with_capacity(target.len());
        for index in 0..target.len() {
            let slot = if index == 0 {
                0
            } else {
                grid.checked_sub(index)
                    .ok_or(Error::Budget("offline completion support"))?
            };
            astar.push(at(&values, slot, p).conj());
        }
        let first = astar
            .first_mut()
            .ok_or(Error::Numerical("empty complement"))?;
        first.im = BigFloat::from_i64(0, crate::offline::number::precision_bits(p));
        let residual = completion_residual(&astar, target, context)?;
        if residual <= tolerance {
            return Ok((astar, ratio, residual, grid));
        }
        grid = grid
            .checked_mul(2)
            .ok_or(Error::Budget("offline grid refinement"))?;
    }
    Err(Error::Numerical("offline completion grid exhausted"))
}
fn completion_residual(
    astar: &[Number],
    target: &[Number],
    context: &mut Context,
) -> Result<BigFloat> {
    let a = fft::convolve(astar, &reverse(astar), context)?;
    let b = fft::convolve(target, &reverse(target), context)?;
    let center = target.len().saturating_sub(1);
    let mut total =
        BigFloat::from_i64(0, crate::offline::number::precision_bits(context.precision));
    for (index, (a, b)) in a.iter().zip(&b).enumerate() {
        let mut value = a.add(b);
        if index == center {
            value = value.sub(&Number::one(context.precision));
        }
        total = add(&total, &value.abs());
    }
    validate(&total)?;
    Ok(total)
}
struct InverseNode {
    xi: Vec<Number>,
    eta: Vec<Number>,
}
pub(super) fn inverse(
    astar: &[Number],
    target: &[Number],
    context: &mut Context,
) -> Result<Vec<Number>> {
    let mut gamma = vec![Number::zero(context.precision); target.len()];
    inverse_node(astar, target, &mut gamma, context)?;
    Ok(gamma)
}
fn inverse_node(
    astar: &[Number],
    target: &[Number],
    gamma: &mut [Number],
    context: &mut Context,
) -> Result<InverseNode> {
    let p = context.precision;
    let count = target.len();
    if count == 0 || astar.len() != count || gamma.len() != count {
        return Err(Error::Numerical("offline inverse support"));
    }
    if count == 1 {
        let pivot = at(astar, 0, p);
        if pivot.is_zero() {
            return Err(Error::Numerical("offline singular inverse pivot"));
        }
        let reflection = at(target, 0, p).div(&pivot);
        let matrix = controls(std::slice::from_ref(&reflection), context)?
            .pop()
            .ok_or(Error::Numerical("offline leaf control"))?;
        *gamma
            .first_mut()
            .ok_or(Error::Numerical("offline leaf support"))? = reflection;
        let [eta, xi, _, _] = matrix;
        return Ok(InverseNode {
            xi: vec![xi],
            eta: vec![eta],
        });
    }
    let lower_len = count / 2;
    let upper_len = count
        .checked_sub(lower_len)
        .ok_or(Error::Budget("offline inverse split"))?;
    let (upper_gamma, lower_gamma) = gamma.split_at_mut(upper_len);
    let upper = inverse_node(
        astar
            .get(..upper_len)
            .ok_or(Error::Numerical("offline inverse prefix"))?,
        target
            .get(..upper_len)
            .ok_or(Error::Numerical("offline inverse prefix"))?,
        upper_gamma,
        context,
    )?;
    let eta_conj = reverse(&upper.eta);
    let xi_conj = reverse(&upper.xi);
    let ea = fft::convolve(&eta_conj, astar, context)?;
    let xb = fft::convolve(&xi_conj, target, context)?;
    let eb = fft::convolve(&upper.eta, target, context)?;
    let xa = fft::convolve(&upper.xi, astar, context)?;
    let offset = upper_len.saturating_sub(1);
    let mut midpoint_a = Vec::with_capacity(lower_len);
    let mut midpoint_b = Vec::with_capacity(lower_len);
    for index in 0..lower_len {
        let a_index = offset
            .checked_add(index)
            .ok_or(Error::Budget("offline midpoint support"))?;
        let b_index = upper_len
            .checked_add(index)
            .ok_or(Error::Budget("offline midpoint support"))?;
        midpoint_a.push(at(&ea, a_index, p).add(&at(&xb, a_index, p)));
        midpoint_b.push(at(&eb, b_index, p).sub(&at(&xa, b_index, p)));
    }
    // The second inverse is invoked only after the completed first-half update.
    let lower = inverse_node(&midpoint_a, &midpoint_b, lower_gamma, context)?;
    let ex = fft::convolve(&eta_conj, &lower.xi, context)?;
    let xe = fft::convolve(&upper.xi, &lower.eta, context)?;
    let ee = fft::convolve(&upper.eta, &lower.eta, context)?;
    let xx = fft::convolve(&xi_conj, &lower.xi, context)?;
    let mut xi = Vec::with_capacity(count);
    let mut eta = Vec::with_capacity(count);
    for index in 0..count {
        let first = index
            .checked_sub(1)
            .map_or_else(|| Number::zero(p), |i| at(&ex, i, p));
        let second = index
            .checked_sub(1)
            .map_or_else(|| Number::zero(p), |i| at(&xx, i, p));
        xi.push(first.add(&at(&xe, index, p)));
        eta.push(at(&ee, index, p).sub(&second));
    }
    Ok(InverseNode { xi, eta })
}
pub(super) fn real_parity_wx_phases(
    gamma: &[Number],
    context: &mut Context,
) -> Result<(Vec<BigFloat>, Vec<Matrix>)> {
    context.charge(
        gamma
            .len()
            .checked_mul(32)
            .ok_or(Error::Budget("offline phase work"))?,
    )?;
    let p = context.precision;
    let two = BigFloat::from_i64(2, crate::offline::number::precision_bits(p));
    let zero = BigFloat::from_i64(0, crate::offline::number::precision_bits(p));
    let mut phases: Vec<_> = gamma
        .iter()
        .map(|value| {
            checked(value.re.atan(
                crate::offline::number::precision_bits(p),
                RoundingMode::ToEven,
                &mut context.constants,
            ))
        })
        .collect::<std::result::Result<_, _>>()?;
    for index in 0..phases.len() / 2 {
        let mirror = phases
            .len()
            .checked_sub(index)
            .and_then(|v| v.checked_sub(1))
            .ok_or(Error::Budget("offline phase support"))?;
        let middle = div(
            &add(
                phases.get(index).ok_or(Error::Numerical("offline phase"))?,
                phases
                    .get(mirror)
                    .ok_or(Error::Numerical("offline phase"))?,
            ),
            &two,
        );
        phases
            .get_mut(index)
            .ok_or(Error::Numerical("offline phase"))?
            .clone_from(&middle);
        *phases
            .get_mut(mirror)
            .ok_or(Error::Numerical("offline phase"))? = middle;
    }
    let matrices = phases
        .iter()
        .map(|phase| {
            let sin = checked(phase.sin(
                crate::offline::number::precision_bits(p),
                RoundingMode::ToEven,
                &mut context.constants,
            ))?;
            let cos = checked(phase.cos(
                crate::offline::number::precision_bits(p),
                RoundingMode::ToEven,
                &mut context.constants,
            ))?;
            Ok([
                Number {
                    re: cos.clone(),
                    im: zero.clone(),
                },
                Number {
                    re: sin.clone(),
                    im: zero.clone(),
                },
                Number {
                    re: neg(&sin),
                    im: zero.clone(),
                },
                Number {
                    re: cos,
                    im: zero.clone(),
                },
            ])
        })
        .collect::<Result<_>>()?;
    Ok((phases, matrices))
}

// Same complex displacement identity, in explicitly requested offline precision.
#[expect(
    clippy::many_single_char_names,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    reason = "Complex rank-two recurrence uses equal-length vectors with 0 <= k < j < n"
)]
pub(super) fn half_cholesky(c: &[Number], context: &mut Context) -> Result<Vec<Number>> {
    let n = c.len();
    if n == 0 {
        return Err(Error::Numerical("empty offline Weiss ratio"));
    }
    context.charge(
        n.checked_mul(n)
            .and_then(|v| v.checked_mul(64))
            .ok_or(Error::Budget("offline Half-Cholesky work"))?,
    )?;
    let p = context.precision;
    let mut first = vec![Number::zero(p); n];
    first[0] = Number::one(p);
    let mut second = reverse(c);
    let mut solution = second.clone();
    for k in 0..n {
        let x = first[k].clone();
        let y = second[k].clone();
        let scale = sqrt(&add(&mul(&x.abs(), &x.abs()), &mul(&y.abs(), &y.abs())));
        validate(&scale)?;
        if scale.is_zero() {
            return Err(Error::Numerical("offline Half-Cholesky pivot"));
        }
        let inverse = div(&Number::one(p).re, &scale);
        let alpha = x.scale(&inverse);
        let beta = y.scale(&inverse);
        let rhs = solution[k].clone();
        let mut previous = Number::real(scale);
        for j in k + 1..n {
            let u = first[j]
                .mul(&alpha.conj())
                .add(&second[j].mul(&beta.conj()));
            let v = second[j].mul(&alpha).sub(&first[j].mul(&beta));
            u.validate()?;
            v.validate()?;
            solution[j] = solution[j].sub(&u.scale(&inverse).mul(&rhs));
            solution[j].validate()?;
            first[j] = previous;
            second[j] = v;
            previous = u;
        }
    }
    Ok(reverse(&solution))
}
