use super::{Context, OfflineError as Error, OfflineResult as Result, number::Number};
use crate::precision::{checked, integer, nearest_pi, nearest_sin_cos};
use std::sync::Arc;
fn roots(length: usize, context: &mut Context) -> Result<Arc<Vec<Number>>> {
    if let Some(values) = context.roots.get(&length) {
        return Ok(Arc::clone(values));
    }
    context.charge(
        length
            .checked_mul(32)
            .ok_or(Error::Budget("offline twiddles"))?,
    )?;
    let mut values = Vec::with_capacity(length / 2);
    // The one-point transform has no twiddles and previously allocated no constant cache.
    if length == 1 {
        let values = Arc::new(values);
        context.roots.insert(length, Arc::clone(&values));
        return Ok(values);
    }
    let p = context.precision;
    let pi = checked(nearest_pi(p, &mut context.cache)?)?;
    for index in 0..length / 2 {
        let numerator = index
            .checked_mul(2)
            .ok_or(Error::Budget("offline FFT angle"))?;
        let rational = super::number::div(
            &integer(
                p,
                u64::try_from(numerator)
                    .map_err(|_| super::OfflineError::Budget("integer interchange"))?,
            ),
            &integer(
                p,
                u64::try_from(length)
                    .map_err(|_| super::OfflineError::Budget("integer interchange"))?,
            ),
        )?;
        let angle = checked(super::number::mul(&rational, &pi)?)?;
        let (sin, cos) = nearest_sin_cos(p, &angle, &mut context.cache)?;
        values.push(Number {
            re: cos,
            im: super::number::neg(&sin),
        });
    }
    let values = Arc::new(values);
    context.roots.insert(length, Arc::clone(&values));
    Ok(values)
}
pub(super) fn transform(
    values: &mut [Number],
    inverse: bool,
    normalize: bool,
    context: &mut Context,
) -> Result<()> {
    let length = values.len();
    if !length.is_power_of_two() {
        return Err(Error::Numerical("non-power-of-two offline FFT"));
    }
    let stages =
        usize::try_from(length.ilog2().max(1)).map_err(|_| Error::Budget("offline FFT stages"))?;
    context.charge(
        length
            .checked_mul(stages)
            .and_then(|n| n.checked_mul(32))
            .ok_or(Error::Budget("offline FFT work"))?,
    )?;
    let roots = roots(length, context)?;
    let mut reversed = 0_usize;
    for index in 1..length {
        let mut bit = length >> 1;
        while reversed & bit != 0 {
            reversed ^= bit;
            bit >>= 1;
        }
        reversed ^= bit;
        if index < reversed {
            values.swap(index, reversed);
        }
    }
    let mut stage = 2_usize;
    while stage <= length {
        let stride = length
            .checked_div(stage)
            .ok_or(Error::Numerical("offline FFT stage"))?;
        for block in values.chunks_exact_mut(stage) {
            let (first, second) = block.split_at_mut(stage / 2);
            for (index, (a, b)) in first.iter_mut().zip(second).enumerate() {
                let root = roots
                    .get(
                        index
                            .checked_mul(stride)
                            .ok_or(Error::Budget("offline twiddle index"))?,
                    )
                    .ok_or(Error::Numerical("offline FFT twiddle"))?;
                let value = b.mul(&if inverse { root.conj() } else { root.clone() })?;
                let original = a.clone();
                *a = original.add(&value)?;
                *b = original.sub(&value)?;
            }
        }
        if stage == length {
            break;
        }
        stage = stage
            .checked_mul(2)
            .ok_or(Error::Budget("offline FFT stage"))?;
    }
    if normalize {
        let inverse_length = super::number::div(
            &integer(context.precision, 1),
            &integer(
                context.precision,
                u64::try_from(length)
                    .map_err(|_| super::OfflineError::Budget("integer interchange"))?,
            ),
        )?;
        for value in values.iter_mut() {
            *value = value.scale(&inverse_length)?;
        }
    }
    for value in values {
        value.validate()?;
    }
    Ok(())
}
pub(super) fn convolve(
    left: &[Number],
    right: &[Number],
    context: &mut Context,
) -> Result<Vec<Number>> {
    let count = left
        .len()
        .checked_add(right.len())
        .and_then(|n| n.checked_sub(1))
        .ok_or(Error::Budget("offline convolution"))?;
    if left.iter().all(Number::is_zero) || right.iter().all(Number::is_zero) {
        return Ok(vec![Number::zero(context.precision); count]);
    }
    if count <= 16 {
        context.charge(
            left.len()
                .checked_mul(right.len())
                .and_then(|n| n.checked_mul(16))
                .ok_or(Error::Budget("offline direct convolution"))?,
        )?;
        let mut result = vec![Number::zero(context.precision); count];
        for (i, a) in left.iter().enumerate() {
            for (j, b) in right.iter().enumerate() {
                let out = result
                    .get_mut(
                        i.checked_add(j)
                            .ok_or(Error::Budget("offline convolution index"))?,
                    )
                    .ok_or(Error::Numerical("offline support"))?;
                *out = out.add(&a.mul(b)?)?;
            }
        }
        for value in &result {
            value.validate()?;
        }
        return Ok(result);
    }
    let length = count
        .checked_next_power_of_two()
        .ok_or(Error::Budget("offline FFT support"))?;
    context.admit_grid(length)?;
    let mut a = vec![Number::zero(context.precision); length];
    let mut b = a.clone();
    for (out, value) in a.iter_mut().zip(left) {
        out.clone_from(value);
    }
    for (out, value) in b.iter_mut().zip(right) {
        out.clone_from(value);
    }
    transform(&mut a, false, false, context)?;
    transform(&mut b, false, false, context)?;
    for (a, b) in a.iter_mut().zip(b) {
        *a = a.mul(&b)?;
    }
    transform(&mut a, true, true, context)?;
    a.truncate(count);
    Ok(a)
}

#[cfg(test)]
mod tests {
    use super::super::OfflinePolicy;
    use super::*;
    use crate::precision::{nearest_cos, nearest_sin};
    use googletest::prelude::*;
    #[gtest]
    fn roots_preserve_exact_values_cache_reuse_and_length_one_resources() -> googletest::Result<()>
    {
        for length in [1, 2, 8, 32] {
            let mut context = Context::new(length, 65, OfflinePolicy::default())?;
            let actual = roots(length, &mut context)?;
            let work = context.work;
            let reused = roots(length, &mut context)?;
            expect_true!(Arc::ptr_eq(&actual, &reused));
            expect_eq!(context.work, work);
            if length == 1 {
                expect_true!(actual.is_empty());
                expect_eq!(context.cache.total_words(), 0);
            }
            for (index, root) in actual.iter().enumerate() {
                let numerator = index.checked_mul(2).ok_or(Error::Budget("fixture angle"))?;
                let ratio = super::super::number::div(
                    &integer(65, u64::try_from(numerator)?),
                    &integer(65, u64::try_from(length)?),
                )?;
                let mut scalar_cache = crate::offline::ConstCache::default();
                let pi = nearest_pi(65, &mut scalar_cache)?;
                let angle = super::super::number::mul(&ratio, &pi)?;
                let re = nearest_cos(65, &angle, &mut scalar_cache)?;
                let im = super::super::number::neg(&nearest_sin(65, &angle, &mut scalar_cache)?);
                expect_that!(&root.re, eq(&re));
                expect_that!(&root.im, eq(&im));
                expect_eq!(root.im.repr().is_neg_zero(), im.repr().is_neg_zero());
            }
        }
        Ok(())
    }
}
