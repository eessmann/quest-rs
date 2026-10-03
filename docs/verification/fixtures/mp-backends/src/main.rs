//! Dashu workloads corresponding to the archived primitive benchmark.
use dashu_float::{
    ConstCache, Context, FBig, Repr,
    round::{
        ErrorBounds, Round,
        mode::{Down, HalfEven, Up},
    },
};
use dashu_int::IBig;
use std::{hint::black_box, time::Instant};
type Binary = FBig<HalfEven, 2>;
const N: usize = 16;
trait Ops {
    type S: Clone;
    fn integer(&self, n: i64) -> Self::S;
    fn add(&self, a: &Self::S, b: &Self::S) -> Self::S;
    fn sub(&self, a: &Self::S, b: &Self::S) -> Self::S;
    fn mul(&self, a: &Self::S, b: &Self::S) -> Self::S;
    fn div(&self, a: &Self::S, b: &Self::S) -> Self::S;
    fn sqrt(&self, a: &Self::S) -> Self::S;
}
struct Dashu(Context<HalfEven>);
impl Ops for Dashu {
    type S = Binary;
    fn integer(&self, n: i64) -> Binary {
        self.0.convert_int(IBig::from(n)).value()
    }
    fn add(&self, a: &Binary, b: &Binary) -> Binary {
        self.0.add(a.repr(), b.repr()).unwrap().value()
    }
    fn sub(&self, a: &Binary, b: &Binary) -> Binary {
        self.0.sub(a.repr(), b.repr()).unwrap().value()
    }
    fn mul(&self, a: &Binary, b: &Binary) -> Binary {
        self.0.mul(a.repr(), b.repr()).unwrap().value()
    }
    fn div(&self, a: &Binary, b: &Binary) -> Binary {
        self.0.div(a.repr(), b.repr()).unwrap().value()
    }
    fn sqrt(&self, a: &Binary) -> Binary {
        self.0.sqrt(a.repr()).unwrap().value()
    }
}
fn inputs(p: usize) -> Vec<Binary> {
    (0..N * N)
        .map(|i| {
            let text = binary_input(p, i);
            let integer = IBig::from_str_radix(&text[2..], 2).unwrap();
            Binary::from_repr(Repr::new(integer, -(p as isize)), Context::new(p))
        })
        .collect()
}
fn trans<R: Round + ErrorBounds>(
    op: usize,
    c: &Context<R>,
    x: &Binary,
    cache: &mut ConstCache,
) -> Binary {
    match op {
        0 => c.exp(x.repr(), Some(cache)),
        1 => c.ln(x.repr(), Some(cache)),
        2 => c.sin(x.repr(), Some(cache)),
        _ => c.cos(x.repr(), Some(cache)),
    }
    .unwrap()
    .value()
    .with_rounding::<HalfEven>()
}
fn binary_input(bits: usize, seed: usize) -> String {
    let mut state = seed as u64 + 0x9876_5432_1234;
    let mut text = String::from("0.1");
    for _ in 1..bits {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        text.push(if state & 1 == 0 { '0' } else { '1' });
    }
    text
}

fn matrix<B: Ops>(b: &B, input: &[B::S]) -> Vec<B::S> {
    let mut result = input.to_vec();
    let diagonal = b.integer(20);
    for i in 0..N {
        result[i * N + i] = b.add(&result[i * N + i], &diagonal);
    }
    result
}

fn matmul<B: Ops>(b: &B, a: &[B::S]) -> Vec<B::S> {
    let mut c = vec![b.integer(0); N * N];
    for i in 0..N {
        for j in 0..N {
            for k in 0..N {
                c[i * N + j] = b.add(&c[i * N + j], &b.mul(&a[i * N + k], &a[k * N + j]));
            }
        }
    }
    c
}

// Modified Gram-Schmidt is an algorithmic workload, not a claim that it matches
// quest-polynomial's pivoted Householder implementation or its allocation policy.
fn qr<B: Ops>(b: &B, a: &[B::S]) -> (Vec<B::S>, Vec<B::S>) {
    let mut q = a.to_vec();
    let mut r = vec![b.integer(0); N * N];
    for k in 0..N {
        let mut norm = b.integer(0);
        for i in 0..N {
            norm = b.add(&norm, &b.mul(&q[i * N + k], &q[i * N + k]));
        }
        r[k * N + k] = b.sqrt(&norm);
        for i in 0..N {
            q[i * N + k] = b.div(&q[i * N + k], &r[k * N + k]);
        }
        for j in k + 1..N {
            let mut projection = b.integer(0);
            for i in 0..N {
                projection = b.add(&projection, &b.mul(&q[i * N + k], &q[i * N + j]));
            }
            r[k * N + j] = projection.clone();
            for i in 0..N {
                q[i * N + j] = b.sub(&q[i * N + j], &b.mul(&q[i * N + k], &projection));
            }
        }
    }
    (q, r)
}

fn cpu_ns() -> u64 {
    let mut t = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: a writable timespec is passed and the status is checked.
    let status = unsafe { libc::clock_gettime(libc::CLOCK_PROCESS_CPUTIME_ID, &mut t) };
    assert_eq!(status, 0);
    t.tv_sec as u64 * 1_000_000_000 + t.tv_nsec as u64
}

fn measure(
    mut f: impl FnMut(),
    trial: usize,
    p: usize,
    backend: &str,
    mode: &str,
    workload: &str,
    count: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    for _ in 0..8 {
        f();
    }
    let start_cpu = cpu_ns();
    let start_wall = Instant::now();
    for _ in 0..count {
        f();
    }
    let wall = start_wall.elapsed().as_nanos();
    let cpu = cpu_ns() - start_cpu;
    let mut writer = csv::WriterBuilder::new()
        .terminator(csv::Terminator::Any(b'\n'))
        .from_writer(std::io::stdout().lock());
    writer.serialize((
        trial,
        p,
        backend,
        mode,
        workload,
        count,
        cpu,
        wall,
        format!("{:.3}", cpu as f64 / count as f64),
        format!("{:.3}", wall as f64 / count as f64),
    ))?;
    writer.flush()?;
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut writer = csv::WriterBuilder::new()
        .terminator(csv::Terminator::Any(b'\n'))
        .from_writer(std::io::stdout().lock());
    writer.write_record([
        "trial",
        "bits",
        "backend",
        "allocation",
        "workload",
        "iterations",
        "cpu_ns",
        "wall_ns",
        "cpu_ns_per_iteration",
        "wall_ns_per_iteration",
    ])?;
    writer.flush()?;
    drop(writer);
    for trial in 0..3 {
        for p in [128, 256, 512] {
            let input = inputs(p);
            let b = Dashu(Context::new(p));
            for (op, name) in ["add", "mul", "div"].into_iter().enumerate() {
                let mut i = 0;
                measure(
                    || {
                        i = (i + 1) % N;
                        let j = (i + 1) % N;
                        black_box(match op {
                            0 => b.add(black_box(&input[i]), black_box(&input[j])),
                            1 => b.mul(black_box(&input[i]), black_box(&input[j])),
                            _ => b.div(black_box(&input[i]), black_box(&input[j])),
                        });
                    },
                    trial,
                    p,
                    "dashu",
                    "fresh",
                    name,
                    200_000,
                )?;
            }
            let down = Context::<Down>::new(p);
            let up = Context::<Up>::new(p);
            let mut cache = ConstCache::new();
            for (op, name) in [
                "exp_endpoints",
                "ln_endpoints",
                "sin_endpoints",
                "cos_endpoints",
            ]
            .into_iter()
            .enumerate()
            {
                let mut i = 0;
                measure(
                    || {
                        i = (i + 1) % N;
                        black_box(trans(op, &down, black_box(&input[i]), &mut cache));
                        black_box(trans(op, &up, black_box(&input[i]), &mut cache));
                    },
                    trial,
                    p,
                    "dashu",
                    "fresh",
                    name,
                    2_000,
                )?;
            }
            let a = matrix(&b, &input);
            measure(
                || {
                    black_box(matmul(&b, black_box(&a)));
                },
                trial,
                p,
                "dashu",
                "fresh",
                "matmul16",
                50,
            )?;
            measure(
                || {
                    black_box(qr(&b, black_box(&a)));
                },
                trial,
                p,
                "dashu",
                "fresh",
                "qr16",
                50,
            )?;
        }
    }
    Ok(())
}
