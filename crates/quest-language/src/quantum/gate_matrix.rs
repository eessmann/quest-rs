use super::{BoundGate, Result};
use crate::matrix::allocate;
pub use crate::matrix::{MatrixPolicy, NumericalOperator};
use num_complex::Complex64;
impl BoundGate {
    /// Numeric realization of the pinned ideal gate; target 0 is the local LSB.
    #[expect(
        clippy::arithmetic_side_effects,
        reason = "Complex floating point formulas cannot panic; admission checks finite results"
    )]
    /// # Errors
    /// Rejects insufficient matrix budget or nonfinite gate entries.
    pub fn matrix(&self, policy: MatrixPolicy) -> Result<NumericalOperator> {
        use BoundGate::{H, Id, Phase, Rx, Ry, Rz, S, Sdg, Swap, Sx, Sxdg, T, Tdg, U, X, Y, Z};
        let c = |r: f64, i: f64| Complex64::new(r, i);
        let exp = |a: f64| Complex64::from_polar(1.0, a);
        let hadamard = std::f64::consts::FRAC_1_SQRT_2;
        let values = match self {
            Id => [c(1.0, 0.0), c(0.0, 0.0), c(0.0, 0.0), c(1.0, 0.0)],
            X => [c(0.0, 0.0), c(1.0, 0.0), c(1.0, 0.0), c(0.0, 0.0)],
            Y => [c(0.0, 0.0), c(0.0, -1.0), c(0.0, 1.0), c(0.0, 0.0)],
            Z => [c(1.0, 0.0), c(0.0, 0.0), c(0.0, 0.0), c(-1.0, 0.0)],
            H => [
                c(hadamard, 0.0),
                c(hadamard, 0.0),
                c(hadamard, 0.0),
                c(-hadamard, 0.0),
            ],
            S => [
                c(1.0, 0.0),
                c(0.0, 0.0),
                c(0.0, 0.0),
                exp(std::f64::consts::FRAC_PI_2),
            ],
            Sdg => [
                c(1.0, 0.0),
                c(0.0, 0.0),
                c(0.0, 0.0),
                exp(-std::f64::consts::FRAC_PI_2),
            ],
            T => [
                c(1.0, 0.0),
                c(0.0, 0.0),
                c(0.0, 0.0),
                exp(std::f64::consts::FRAC_PI_4),
            ],
            Tdg => [
                c(1.0, 0.0),
                c(0.0, 0.0),
                c(0.0, 0.0),
                exp(-std::f64::consts::FRAC_PI_4),
            ],
            Phase(angle) => [c(1.0, 0.0), c(0.0, 0.0), c(0.0, 0.0), exp(*angle)],
            Sx => [c(0.5, 0.5), c(0.5, -0.5), c(0.5, -0.5), c(0.5, 0.5)],
            Sxdg => [c(0.5, -0.5), c(0.5, 0.5), c(0.5, 0.5), c(0.5, -0.5)],
            Rx(a) => {
                let (sine, cosine) = (a / 2.0).sin_cos();
                [c(cosine, 0.0), c(0.0, -sine), c(0.0, -sine), c(cosine, 0.0)]
            }
            Ry(a) => {
                let (sine, cosine) = (a / 2.0).sin_cos();
                [c(cosine, 0.0), c(-sine, 0.0), c(sine, 0.0), c(cosine, 0.0)]
            }
            Rz(a) => [exp(-a / 2.0), c(0.0, 0.0), c(0.0, 0.0), exp(a / 2.0)],
            U { theta, phi, lambda } => {
                let (sine, cosine) = (theta / 2.0).sin_cos();
                let phase = exp(theta / 2.0);
                [
                    phase * cosine,
                    -phase * exp(*lambda) * sine,
                    phase * exp(*phi) * sine,
                    // Multiplication keeps finite phases valid even when their
                    // sum would overflow before periodic reduction.
                    phase * exp(*phi) * exp(*lambda) * cosine,
                ]
            }
            Swap => {
                policy.check(4, 1)?;
                return Ok(NumericalOperator::from_owned(allocate(
                    4,
                    policy,
                    |r, col| {
                        c(
                            if r == ((col & 1) << 1 | (col & 2) >> 1) {
                                1.0
                            } else {
                                0.0
                            },
                            0.0,
                        )
                    },
                )?)?);
            }
        };
        policy.check(2, 1)?;
        let [top_left, top_right, bottom_left, bottom_right] = values;
        Ok(NumericalOperator::from_owned(allocate(
            2,
            policy,
            |row, col| match (row, col) {
                (0, 0) => top_left,
                (0, _) => top_right,
                (_, 0) => bottom_left,
                _ => bottom_right,
            },
        )?)?)
    }
}
