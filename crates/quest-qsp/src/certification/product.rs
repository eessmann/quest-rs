use super::{
    CertificationError as Error, CertificationMode, CertificationResult as Result, Context,
    ConvolutionMethod, MpComplex, MpInterval,
};
use crate::FrozenCandidate;
use std::sync::Arc;
pub(super) type MatrixPolynomial = [Vec<MpComplex>; 4];
pub(super) type Matrix = [MpComplex; 4];
pub(super) fn controls<M: CertificationMode>(
    candidate: &FrozenCandidate<M>,
    precision: u32,
    constants: &mut astro_float::Consts,
) -> Result<Vec<Matrix>> {
    if M::CANONICAL {
        let mut matrices = Vec::with_capacity(candidate.phases.len());
        for phase in candidate.phases.iter() {
            let (sin, cos) = MpInterval::sin_cos_exact(*phase, precision, constants)?;
            let zero = MpInterval::integer(0, precision);
            matrices.push([
                MpComplex::new(cos.clone(), zero.clone()),
                MpComplex::new(sin.clone(), zero.clone()),
                MpComplex::new(sin.neg(), zero.clone()),
                MpComplex::new(cos, zero),
            ]);
        }
        let last = matrices.last_mut().ok_or(Error::Export("empty phases"))?;
        let [a, b, c, d] = last.clone();
        *last = [b, a.neg(), d, c.neg()];
        Ok(matrices)
    } else {
        candidate
            .controls()
            .iter()
            .map(|control| {
                let [[a, b], [c, d]] = control;
                Ok([
                    MpComplex::exact(*a, precision)?,
                    MpComplex::exact(*b, precision)?,
                    MpComplex::exact(*c, precision)?,
                    MpComplex::exact(*d, precision)?,
                ])
            })
            .collect()
    }
}
pub(super) fn reconstruct(controls: &[Matrix], context: &mut Context) -> Result<MatrixPolynomial> {
    match context.policy.method {
        ConvolutionMethod::Direct => sequential(controls, context),
        ConvolutionMethod::IntervalFft => tree(controls, true, context),
    }
}
#[expect(
    clippy::many_single_char_names,
    reason = "Named 2x2 matrix entries expose the complete multiplication formula"
)]
fn sequential(controls: &[Matrix], context: &mut Context) -> Result<MatrixPolynomial> {
    let first = controls.first().ok_or(Error::Export("empty controls"))?;
    let mut product = first.clone().map(|value| vec![value]);
    for control in controls.iter().skip(1) {
        let count = product
            .first()
            .ok_or(Error::Export("matrix"))?
            .len()
            .checked_add(1)
            .ok_or(Error::Budget("product support"))?;
        context.charge(
            count
                .checked_mul(128)
                .ok_or(Error::Budget("direct product work"))?,
        )?;
        let mut next = std::array::from_fn(|_| vec![MpComplex::zero(context.precision); count]);
        let [a, b, c, d] = &product;
        let [u, v, w, x] = control;
        let [oa, ob, oc, od] = &mut next;
        let zero = MpComplex::zero(context.precision);
        for index in 0..count {
            let previous = index.checked_sub(1);
            let av = previous.and_then(|k| a.get(k)).unwrap_or(&zero);
            let cv = previous.and_then(|k| c.get(k)).unwrap_or(&zero);
            let bv = b.get(index).unwrap_or(&zero);
            let dv = d.get(index).unwrap_or(&zero);
            *oa.get_mut(index).ok_or(Error::Export("matrix support"))? =
                av.mul(u)?.add(&bv.mul(w)?)?;
            *ob.get_mut(index).ok_or(Error::Export("matrix support"))? =
                av.mul(v)?.add(&bv.mul(x)?)?;
            *oc.get_mut(index).ok_or(Error::Export("matrix support"))? =
                cv.mul(u)?.add(&dv.mul(w)?)?;
            *od.get_mut(index).ok_or(Error::Export("matrix support"))? =
                cv.mul(v)?.add(&dv.mul(x)?)?;
        }
        product = next;
    }
    Ok(product)
}
fn tree(controls: &[Matrix], last: bool, context: &mut Context) -> Result<MatrixPolynomial> {
    if controls.len() <= 8 {
        let mut result = sequential(controls, context)?;
        if !last {
            let [a, b, c, d] = &mut result;
            a.insert(0, MpComplex::zero(context.precision));
            c.insert(0, MpComplex::zero(context.precision));
            b.push(MpComplex::zero(context.precision));
            d.push(MpComplex::zero(context.precision));
        }
        return Ok(result);
    }
    let midpoint = controls.len() / 2;
    let (left, right) = controls.split_at(midpoint);
    let left = tree(left, false, context)?;
    let right = tree(right, last, context)?;
    multiply(&left, &right, context)
}
#[expect(
    clippy::many_single_char_names,
    reason = "Named 2x2 matrix entries expose the complete multiplication formula"
)]
fn multiply(
    left: &MatrixPolynomial,
    right: &MatrixPolynomial,
    context: &mut Context,
) -> Result<MatrixPolynomial> {
    let [a, b, c, d] = left;
    let [u, v, w, x] = right;
    Ok([
        sum(convolve(a, u, context)?, convolve(b, w, context)?)?,
        sum(convolve(a, v, context)?, convolve(b, x, context)?)?,
        sum(convolve(c, u, context)?, convolve(d, w, context)?)?,
        sum(convolve(c, v, context)?, convolve(d, x, context)?)?,
    ])
}
fn sum(mut left: Vec<MpComplex>, right: Vec<MpComplex>) -> Result<Vec<MpComplex>> {
    if left.len() != right.len() {
        return Err(Error::Export("convolution sum support"));
    }
    for (left, right) in left.iter_mut().zip(right) {
        *left = left.add(&right)?;
    }
    Ok(left)
}
pub(super) fn gram_entry(
    left0: &[MpComplex],
    left1: &[MpComplex],
    right0: &[MpComplex],
    right1: &[MpComplex],
    identity: bool,
    context: &mut Context,
) -> Result<Vec<MpComplex>> {
    if left0.len() != left1.len() || right0.len() != right1.len() || left0.len() != right0.len() {
        return Err(Error::Export("Gram support"));
    }
    let conjugate0: Vec<_> = left0.iter().rev().map(MpComplex::conj).collect();
    let conjugate1: Vec<_> = left1.iter().rev().map(MpComplex::conj).collect();
    let mut result = sum(
        convolve(&conjugate0, right0, context)?,
        convolve(&conjugate1, right1, context)?,
    )?;
    if identity {
        let constant = result
            .get_mut(left0.len().saturating_sub(1))
            .ok_or(Error::Export("Gram constant"))?;
        *constant = constant.sub(&MpComplex::one(context.precision))?;
    }
    Ok(result)
}
pub(super) fn convolve(
    left: &[MpComplex],
    right: &[MpComplex],
    context: &mut Context,
) -> Result<Vec<MpComplex>> {
    let count = left
        .len()
        .checked_add(right.len())
        .and_then(|n| n.checked_sub(1))
        .ok_or(Error::Budget("convolution support"))?;
    context.charge(
        left.len()
            .checked_add(right.len())
            .ok_or(Error::Budget("convolution work"))?,
    )?;
    // An exact-zero polynomial must not conceal a backend failure retained
    // by a value in the other operand, including at recursive tree levels.
    for value in left.iter().chain(right) {
        value.validate()?;
    }
    if left.iter().all(MpComplex::is_zero) || right.iter().all(MpComplex::is_zero) {
        return Ok(vec![MpComplex::zero(context.precision); count]);
    }
    if context.policy.method == ConvolutionMethod::Direct || count <= 16 {
        return direct(left, right, context);
    }
    let length = count
        .checked_next_power_of_two()
        .ok_or(Error::Budget("FFT convolution support"))?;
    let roots = roots(length, context)?;
    let mut a = vec![MpComplex::zero(context.precision); length];
    let mut b = a.clone();
    for (out, value) in a.iter_mut().zip(left) {
        *out = value.clone();
    }
    for (out, value) in b.iter_mut().zip(right) {
        *out = value.clone();
    }
    fft(&mut a, &roots, false, context)?;
    fft(&mut b, &roots, false, context)?;
    for (a, b) in a.iter_mut().zip(b) {
        *a = a.mul(&b)?;
    }
    fft(&mut a, &roots, true, context)?;
    a.truncate(count);
    Ok(a)
}
fn direct(
    left: &[MpComplex],
    right: &[MpComplex],
    context: &mut Context,
) -> Result<Vec<MpComplex>> {
    let count = left
        .len()
        .checked_add(right.len())
        .and_then(|n| n.checked_sub(1))
        .ok_or(Error::Budget("direct convolution support"))?;
    context.charge(
        left.len()
            .checked_mul(right.len())
            .and_then(|n| n.checked_mul(32))
            .ok_or(Error::Budget("direct convolution work"))?,
    )?;
    let mut result = vec![MpComplex::zero(context.precision); count];
    for (i, a) in left.iter().enumerate() {
        if a.is_zero() {
            continue;
        }
        for (j, b) in right.iter().enumerate() {
            if b.is_zero() {
                continue;
            }
            let out = result
                .get_mut(i.checked_add(j).ok_or(Error::Budget("convolution index"))?)
                .ok_or(Error::Export("convolution support"))?;
            *out = out.add(&a.mul(b)?)?;
        }
    }
    Ok(result)
}
fn roots(length: usize, context: &mut Context) -> Result<Arc<Vec<MpComplex>>> {
    if let Some(roots) = context.roots.get(&length) {
        return Ok(Arc::clone(roots));
    }
    context.charge(
        length
            .checked_mul(64)
            .ok_or(Error::Budget("twiddle work"))?,
    )?;
    let mut roots = Vec::with_capacity(length / 2);
    for index in 0..length / 2 {
        let (sin, cos) =
            MpInterval::twiddle(index, length, context.precision, &mut context.constants)?;
        roots.push(MpComplex::new(cos, sin.neg()));
    }
    let roots = Arc::new(roots);
    context.roots.insert(length, Arc::clone(&roots));
    Ok(roots)
}
fn fft(
    values: &mut [MpComplex],
    roots: &[MpComplex],
    inverse: bool,
    context: &mut Context,
) -> Result<()> {
    let length = values.len();
    if !length.is_power_of_two() {
        return Err(Error::Arithmetic("non-power-of-two FFT"));
    }
    let stages = usize::try_from(length.ilog2().max(1)).map_err(|_| Error::Budget("FFT stages"))?;
    context.charge(
        length
            .checked_mul(stages)
            .and_then(|n| n.checked_mul(64))
            .ok_or(Error::Budget("FFT work"))?,
    )?;
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
            .ok_or(Error::Arithmetic("FFT stage"))?;
        for block in values.chunks_exact_mut(stage) {
            let (first, second) = block.split_at_mut(stage / 2);
            for (index, (a, b)) in first.iter_mut().zip(second).enumerate() {
                let root = roots
                    .get(
                        index
                            .checked_mul(stride)
                            .ok_or(Error::Budget("FFT root index"))?,
                    )
                    .ok_or(Error::Arithmetic("FFT root"))?;
                let multiplied = b.mul(&if inverse { root.conj() } else { root.clone() })?;
                let original = a.clone();
                *a = original.add(&multiplied)?;
                *b = original.sub(&multiplied)?;
            }
        }
        if stage == length {
            break;
        }
        stage = stage.checked_mul(2).ok_or(Error::Budget("FFT stage"))?;
    }
    if inverse {
        for value in values {
            *value = value.divide_usize(length)?;
        }
    }
    Ok(())
}

#[cfg(feature = "offline-synthesis")]
pub fn circle_values(
    coefficients: &[MpComplex],
    length: usize,
    precision: u32,
    policy: super::CertificationPolicy,
) -> Result<(Vec<MpComplex>, usize, usize)> {
    if coefficients.len() > length || !length.is_power_of_two() {
        return Err(Error::Budget("circle FFT support"));
    }
    let mut context = Context::new(length, precision, policy)?;
    let roots = roots(length, &mut context)?;
    let mut values = vec![MpComplex::zero(precision); length];
    for (out, value) in values.iter_mut().zip(coefficients) {
        out.clone_from(value);
    }
    fft(&mut values, &roots, true, &mut context)?;
    let scale = MpComplex::exact(
        crate::Complex64::new(
            f64::from(u32::try_from(length).map_err(|_| Error::Budget("circle FFT length"))?),
            0.0,
        ),
        precision,
    )?;
    for value in &mut values {
        *value = value.mul(&scale)?;
    }
    Ok((values, context.work, context.bytes))
}
