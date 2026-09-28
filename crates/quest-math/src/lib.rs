#![doc = include_str!("../README.md")]
#![forbid(unsafe_code)]
mod types;
pub use types::{
    AngleTarget, Axis, Control, Error, Gate, Limits, Operation, Rational, Result, Sequence, Target,
};
mod ring;
pub use ring::Cyclotomic;
mod matrix;
pub use matrix::{
    ExactCertificate, ExactMatrix, MatrixKey, PhaseRecovery, reconstruct,
    recover_eighth_root_phase, verify_exact,
};
mod approx;
pub use approx::{
    ApproxCertificate, DyadicBox8, adjoint_times_rotation_enclosure, admit_rotation_target,
    certify_rotation, dyadic_from_bits, rotation_enclosure,
};
mod controlled;
pub use controlled::{ControlledApproxCertificate, lift_controlled_rotation};
mod interval;
