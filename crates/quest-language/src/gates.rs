//! Project-owned gate signatures and phase-exact algebraic definitions.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Effect {
	Unitary,
	Measurement,
	Reset,
	Classical,
	ControlFlow,
}

/// Adjoint parameter mapping in output-parameter order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParameterTransform {
	pub input: usize,
	pub negate: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Adjoint {
	SelfInverse,
	Gate(GateKind),
	Parameters(&'static [ParameterTransform]),
}

/// An exact rational multiple of an input gate parameter. Gate parameters are
/// unwrapped real angles; no modular classical-angle arithmetic is implied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParameterTerm {
	pub input: usize,
	pub numerator: i32,
	pub denominator: u32,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AngleExpression {
	pub terms: &'static [ParameterTerm],
	pub pi_numerator: i32,
	pub pi_denominator: u32,
}
impl AngleExpression {
	const fn pi(numerator: i32, denominator: u32) -> Self {
		Self {
			terms: &[],
			pi_numerator: numerator,
			pi_denominator: denominator,
		}
	}
	const fn parameter(terms: &'static [ParameterTerm]) -> Self {
		Self {
			terms,
			pi_numerator: 0,
			pi_denominator: 1,
		}
	}
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
	X,
	Y,
	Z,
}
/// Exact primitive definitions: rotations are exp(-i angle Pauli / 2), phase
/// is diag(1, exp(i angle)), and global phase multiplies by exp(i angle).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Primitive {
	Identity,
	Pauli(Axis),
	Hadamard,
	Swap,
	Rotation(Axis),
	Phase,
	GlobalPhase,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GateStep {
	pub gate: GateKind,
	pub parameters: &'static [AngleExpression],
}
/// Sequence order is execution order (the last matrix multiplies on the left).
///
/// Controlled gates apply the base only when all intrinsic controls are one;
/// the base's global phase therefore becomes a relative phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decomposition {
	Primitive(Primitive),
	Sequence(&'static [GateStep]),
	Controlled { base: GateKind, controls: usize },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GateDefinition {
	pub kind: GateKind,
	pub name: &'static str,
	pub rust_variant: &'static str,
	pub target_count: usize,
	pub parameter_count: usize,
	pub intrinsic_controls: usize,
	pub effect: Effect,
	pub adjoint: Adjoint,
	pub decomposition: Decomposition,
}

const NEGATE: &[ParameterTransform] = &[ParameterTransform {
	input: 0,
	negate: true,
}];
const U_ADJOINT: &[ParameterTransform] = &[
	ParameterTransform {
		input: 0,
		negate: true,
	},
	ParameterTransform {
		input: 2,
		negate: true,
	},
	ParameterTransform {
		input: 1,
		negate: true,
	},
];
const U_STEPS: &[GateStep] = &[
	GateStep {
		gate: GateKind::GlobalPhase,
		parameters: &[AngleExpression::parameter(&[
			ParameterTerm {
				input: 0,
				numerator: 1,
				denominator: 2,
			},
			ParameterTerm {
				input: 1,
				numerator: 1,
				denominator: 2,
			},
			ParameterTerm {
				input: 2,
				numerator: 1,
				denominator: 2,
			},
		])],
	},
	GateStep {
		gate: GateKind::Rz,
		parameters: &[AngleExpression::parameter(&[ParameterTerm {
			input: 2,
			numerator: 1,
			denominator: 1,
		}])],
	},
	GateStep {
		gate: GateKind::Ry,
		parameters: &[AngleExpression::parameter(&[ParameterTerm {
			input: 0,
			numerator: 1,
			denominator: 1,
		}])],
	},
	GateStep {
		gate: GateKind::Rz,
		parameters: &[AngleExpression::parameter(&[ParameterTerm {
			input: 1,
			numerator: 1,
			denominator: 1,
		}])],
	},
];

macro_rules! registry {
    ($($kind:ident, $name:literal, $variant:literal, $targets:literal, $parameters:literal, $controls:literal, $adjoint:expr, $decomposition:expr;)*) => {
        /// Semantic identities shared by text, token, and circuit adapters.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
        pub enum GateKind { $($kind),* }
        impl GateKind {
            pub const ALL: &'static [Self] = &[$(Self::$kind),*];
            #[must_use]
            pub const fn definition(self) -> &'static GateDefinition {
                match self { $(Self::$kind => { const DEFINITION: GateDefinition = GateDefinition {
                    kind: GateKind::$kind, name: $name, rust_variant: $variant,
                    target_count: $targets, parameter_count: $parameters,
                    intrinsic_controls: $controls, effect: Effect::Unitary,
                    adjoint: $adjoint, decomposition: $decomposition,
                }; &DEFINITION }),* }
            }
            #[must_use]
            pub fn lookup(name: &str) -> Option<Self> {
                match name { $($name => Some(Self::$kind),)* "u" => Some(Self::U), _ => None }
            }
        }
    };
}
// This table also generates the mechanical circuit adapters. Numerical
// formulas and independent verification remain outside this callback registry.
macro_rules! circuit_registry {
    ($callback:ident) => { $callback! {
        fixed {
            Id => ("id", "Id", 1, 0, 0, Adjoint::SelfInverse, Decomposition::Primitive(Primitive::Identity));
            X => ("x", "X", 1, 0, 0, Adjoint::SelfInverse, Decomposition::Primitive(Primitive::Pauli(Axis::X)));
            Y => ("y", "Y", 1, 0, 0, Adjoint::SelfInverse, Decomposition::Primitive(Primitive::Pauli(Axis::Y)));
            Z => ("z", "Z", 1, 0, 0, Adjoint::SelfInverse, Decomposition::Primitive(Primitive::Pauli(Axis::Z)));
            H => ("h", "H", 1, 0, 0, Adjoint::SelfInverse, Decomposition::Primitive(Primitive::Hadamard));
            S => ("s", "S", 1, 0, 0, Adjoint::Gate(GateKind::Sdg), Decomposition::Sequence(&[GateStep { gate: GateKind::Phase, parameters: &[AngleExpression::pi(1, 2)] }]));
            Sdg => ("sdg", "Sdg", 1, 0, 0, Adjoint::Gate(GateKind::S), Decomposition::Sequence(&[GateStep { gate: GateKind::Phase, parameters: &[AngleExpression::pi(-1, 2)] }]));
            T => ("t", "T", 1, 0, 0, Adjoint::Gate(GateKind::Tdg), Decomposition::Sequence(&[GateStep { gate: GateKind::Phase, parameters: &[AngleExpression::pi(1, 4)] }]));
            Tdg => ("tdg", "Tdg", 1, 0, 0, Adjoint::Gate(GateKind::T), Decomposition::Sequence(&[GateStep { gate: GateKind::Phase, parameters: &[AngleExpression::pi(-1, 4)] }]));
            Sx => ("sx", "Sx", 1, 0, 0, Adjoint::Gate(GateKind::Sxdg), Decomposition::Sequence(&[
                GateStep { gate: GateKind::GlobalPhase, parameters: &[AngleExpression::pi(1, 4)] },
                GateStep { gate: GateKind::Rx, parameters: &[AngleExpression::pi(1, 2)] },
            ]));
            Sxdg => ("sxdg", "Sxdg", 1, 0, 0, Adjoint::Gate(GateKind::Sx), Decomposition::Sequence(&[
                GateStep { gate: GateKind::GlobalPhase, parameters: &[AngleExpression::pi(-1, 4)] },
                GateStep { gate: GateKind::Rx, parameters: &[AngleExpression::pi(-1, 2)] },
            ]));
            Swap => ("swap", "Swap", 2, 0, 0, Adjoint::SelfInverse, Decomposition::Primitive(Primitive::Swap));
        }
        angle {
            Rx => ("rx", "Rx", 1, 1, 0, Adjoint::Parameters(NEGATE), Decomposition::Primitive(Primitive::Rotation(Axis::X)));
            Ry => ("ry", "Ry", 1, 1, 0, Adjoint::Parameters(NEGATE), Decomposition::Primitive(Primitive::Rotation(Axis::Y)));
            Rz => ("rz", "Rz", 1, 1, 0, Adjoint::Parameters(NEGATE), Decomposition::Primitive(Primitive::Rotation(Axis::Z)));
            Phase => ("p", "Phase", 1, 1, 0, Adjoint::Parameters(NEGATE), Decomposition::Primitive(Primitive::Phase));
        }
        euler {
            U => ("U", "U", 1, 3, 0, Adjoint::Parameters(U_ADJOINT), Decomposition::Sequence(U_STEPS));
        }
    }};
        }
pub(crate) use circuit_registry;
macro_rules! define_registry {
    (fixed { $($fixed:ident => ($($fm:tt)*);)* }
     angle { $($angle:ident => ($($am:tt)*);)* }
     euler { $euler:ident => ($($em:tt)*); }) => {
        registry! {
            $($fixed, $($fm)*;)*
            $($angle, $($am)*;)*
            $euler, $($em)*;

            Cx, "cx", "X", 1, 0, 1, Adjoint::SelfInverse, Decomposition::Controlled { base: GateKind::X, controls: 1 };
            Cy, "cy", "Y", 1, 0, 1, Adjoint::SelfInverse, Decomposition::Controlled { base: GateKind::Y, controls: 1 };
            Cz, "cz", "Z", 1, 0, 1, Adjoint::SelfInverse, Decomposition::Controlled { base: GateKind::Z, controls: 1 };
            Ccx, "ccx", "X", 1, 0, 2, Adjoint::SelfInverse, Decomposition::Controlled { base: GateKind::X, controls: 2 };
            GlobalPhase, "gphase", "GlobalPhase", 0, 1, 0, Adjoint::Parameters(NEGATE), Decomposition::Primitive(Primitive::GlobalPhase);
        }
    };
}
circuit_registry!(define_registry);
