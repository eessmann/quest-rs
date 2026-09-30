//! Differential fixture consumer; certificates retain their original scope.
#![cfg_attr(
    feature = "certification",
    expect(
        clippy::arithmetic_side_effects,
        reason = "Bounded 2x2 fixture arithmetic with independently certified Rust outputs"
    )
)]
#[cfg(feature = "certification")]
mod enabled {
    use quest_polynomial::{Chebyshev, Laurent, Limits, Polynomial};
    #[cfg(feature = "certification")]
    use quest_qsp::certification::{CertificationBuilder, CertificationPolicy};
    use quest_qsp::{Complex64, Control, Policy, SynthesisAlgorithm, SynthesisBuilder};
    use std::{error::Error, fs};
    type Result<T> = std::result::Result<T, Box<dyn Error>>;
    #[derive(Default)]
    struct Fixture {
        name: String,
        mode: String,
        offset: i32,
        expected: usize,
        target: Vec<Complex64>,
        phases: Vec<f64>,
        controls: Vec<Control>,
        certificate: String,
        bound: Option<f64>,
        normalization: f64,
        failed: bool,
    }
    fn number(word: Option<&str>) -> Result<f64> {
        let v: f64 = word.ok_or("missing number")?.parse()?;
        if !v.is_finite() {
            return Err("nonfinite fixture".into());
        }
        Ok(v)
    }
    #[expect(
        clippy::many_single_char_names,
        reason = "Explicit 2x2 matrix multiplication formula"
    )]
    fn multiply(a: Control, b: Control) -> Control {
        let [[a, b0], [c, d]] = a;
        let [[e, f], [g, h]] = b;
        [
            [a * e + b0 * g, a * f + b0 * h],
            [c * e + d * g, c * f + d * h],
        ]
    }
    const fn identity() -> Control {
        let one = Complex64::new(1.0, 0.0);
        let zero = Complex64::new(0.0, 0.0);
        [[one, zero], [zero, one]]
    }
    fn wx(phases: &[f64], x: f64) -> Control {
        let mut product = identity();
        let signal = [
            [
                Complex64::new(x, 0.0),
                Complex64::new(0.0, x.mul_add(-x, 1.0).max(0.0).sqrt()),
            ],
            [
                Complex64::new(0.0, x.mul_add(-x, 1.0).max(0.0).sqrt()),
                Complex64::new(x, 0.0),
            ],
        ];
        for (i, &phase) in phases.iter().enumerate() {
            if i > 0 {
                product = multiply(product, signal);
            }
            let v = Complex64::from_polar(1.0, phase);
            let zero = Complex64::new(0.0, 0.0);
            product = multiply(product, [[v, zero], [zero, v.conj()]]);
        }
        product
    }
    fn circle(controls: &[Control], signal: Complex64) -> Control {
        let mut product = identity();
        for (i, &control) in controls.iter().enumerate() {
            if i > 0 {
                let [[a, b], [c, d]] = product;
                product = [[a * signal, b], [c * signal, d]];
            }
            product = multiply(product, control);
        }
        product
    }
    fn distance(a: Control, b: Control) -> f64 {
        a.iter()
            .flatten()
            .zip(b.iter().flatten())
            .map(|(a, b)| (*a - *b).norm_sqr())
            .sum::<f64>()
            .sqrt()
    }
    #[cfg(feature = "certification")]
    fn compare(f: &Fixture) -> Result<()> {
        if f.failed {
            println!("UNAVAILABLE {} {}", f.name, f.mode);
            return Ok(());
        }
        if f.target.len() != f.expected
            || f.expected > 128
            || f.normalization.to_bits() != 1.0_f64.to_bits()
        {
            return Err("fixture target/normalization mismatch".into());
        }
        for algorithm in [
            SynthesisAlgorithm::RhwHalfCholesky,
            SynthesisAlgorithm::InverseNlftDivideConquer,
        ] {
            let policy = Policy {
                algorithm,
                ..Policy::default()
            };
            let (difference, cpp_response, response, reconstruction) = if f.mode == "wx" {
                let target = Polynomial::new(Chebyshev, f.target.clone(), Limits::default())?;
                let frozen = SynthesisBuilder::new()
                    .policy(policy)
                    .real_parity_wx(&target)?
                    .admit()?
                    .complete()?
                    .synthesize()?;
                let mut maximum = 0.0_f64;
                let mut cpp_response = 0.0_f64;
                for k in 0..129 {
                    let x = f64::from(k) / 64.0 - 1.0;
                    let foreign = wx(&f.phases, x);
                    let [[response, _], _] = foreign;
                    cpp_response = cpp_response.max((response.im - target.evaluate_real(x)?).abs());
                    maximum = maximum.max(distance(foreign, wx(frozen.phases(), x)));
                }
                let certified = CertificationBuilder::new()
                    .candidate(frozen)
                    .policy(CertificationPolicy::default())?
                    .certify()?;
                (
                    maximum,
                    cpp_response,
                    certified.report().response().upper_f64(),
                    certified.report().reconstruction().upper_f64(),
                )
            } else if f.mode == "circle" {
                let target =
                    Polynomial::new(Laurent::new(f.offset), f.target.clone(), Limits::default())?;
                let frozen = SynthesisBuilder::new()
                    .policy(policy)
                    .unit_circle_response(&target)?
                    .admit()?
                    .complete()?
                    .synthesize()?;
                let mut maximum = 0.0_f64;
                let mut cpp_response = 0.0_f64;
                for k in 0..129 {
                    let z =
                        Complex64::from_polar(1.0, f64::from(k) * std::f64::consts::TAU / 128.0);
                    let foreign = circle(&f.controls, z);
                    let [[response, _], _] = foreign;
                    cpp_response = cpp_response.max((response - target.evaluate(z)?).norm());
                    maximum = maximum.max(distance(foreign, frozen.evaluate(z)?));
                }
                let certified = CertificationBuilder::new()
                    .candidate(frozen)
                    .policy(CertificationPolicy::default())?
                    .certify()?;
                (
                    maximum,
                    cpp_response,
                    certified.report().response().upper_f64(),
                    certified.report().reconstruction().upper_f64(),
                )
            } else {
                return Err("unsupported fixture convention".into());
            };
            let cpp_bound = f
                .bound
                .map_or_else(|| "unavailable".to_owned(), |bound| format!("{bound:.17e}"));
            println!(
                "COMPARE {} {:?} full_matrix_sample_max={difference:.17e} cpp_response_sample_max={cpp_response:.17e} rust_response_bound={response:.17e} rust_all_entries_bound={reconstruction:.17e} cpp_certificate={} cpp_bound={cpp_bound} normalization={}",
                f.name, algorithm, f.certificate, f.normalization
            );
            if difference > 1e-9 || cpp_response > 1e-9 {
                return Err(
                    format!("{} full complex operator differs: {difference}", f.name).into(),
                );
            }
        }
        Ok(())
    }
    #[cfg(feature = "certification")]
    pub fn main() -> Result<()> {
        let path = std::env::args()
            .nth(1)
            .ok_or("supply branch fixture path")?;
        let text = fs::read_to_string(path)?;
        let mut fixture = Fixture::default();
        let mut ended = true;
        for line in text.lines() {
            let mut words = line.split_whitespace();
            match words.next().ok_or("empty fixture row")? {
                "REFERENCE" => println!("{line}"),
                "PROJECTION" => {
                    let target = Polynomial::new(
                        Chebyshev,
                        vec![
                            Complex64::new(0.2, 1e-14),
                            Complex64::new(1e-14, 0.0),
                            Complex64::new(0.1, 0.0),
                        ],
                        Limits::default(),
                    )?;
                    if SynthesisBuilder::new().real_parity_wx(&target).is_ok() {
                        return Err("exact real/parity admission regression".into());
                    }
                    println!("{line} rust=exact_rejection");
                }
                "CASE" => {
                    if !ended {
                        return Err("unterminated fixture".into());
                    }
                    fixture = Fixture {
                        name: words.next().ok_or("name")?.into(),
                        mode: words.next().ok_or("mode")?.into(),
                        offset: words.next().ok_or("offset")?.parse()?,
                        expected: words.next().ok_or("count")?.parse()?,
                        ..Fixture::default()
                    };
                    ended = false;
                }
                "COEFF" => fixture
                    .target
                    .push(Complex64::new(number(words.next())?, number(words.next())?)),
                "PHASE" => fixture.phases.push(number(words.next())?),
                "CONTROL" => {
                    let mut gate = identity();
                    for v in gate.iter_mut().flatten() {
                        *v = Complex64::new(number(words.next())?, number(words.next())?);
                    }
                    fixture.controls.push(gate);
                }
                "CERT" => {
                    fixture.certificate = words.next().ok_or("certificate")?.into();
                    fixture.bound = if fixture.certificate == "none" {
                        None
                    } else {
                        Some(number(words.next())?)
                    };
                }
                "NORMALIZATION" => fixture.normalization = number(words.next())?,
                "FAIL" => fixture.failed = true,
                "END" => {
                    compare(&fixture)?;
                    ended = true;
                }
                _ => return Err("unknown fixture row".into()),
            }
        }
        if !ended {
            return Err("unterminated fixture".into());
        }
        Ok(())
    }
}
#[cfg(feature = "certification")]
fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    enabled::main()
}
#[cfg(not(feature = "certification"))]
fn main() {
    eprintln!("Enable certification to compare branch fixtures");
}
