use crate::{
    AngleTarget, ApproxCertificate, Control, Error, Limits, Operation, Rational, Result, Sequence,
};

/// A controlled embedding of an independently certified one-qubit rotation.
///
/// The target and signed controls cover the whole output register. Therefore
/// the controlled candidate-minus-target matrix has one copy of the base
/// difference and zero everywhere else, preserving the squared Frobenius bound.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct ControlledApproxCertificate {
    base: ApproxCertificate,
    sequence: Sequence,
    target_qubit: usize,
    controls: Vec<Control>,
}

impl ControlledApproxCertificate {
    #[must_use]
    pub const fn base(&self) -> &ApproxCertificate {
        &self.base
    }

    #[must_use]
    pub const fn sequence(&self) -> &Sequence {
        &self.sequence
    }

    #[must_use]
    pub const fn target_qubit(&self) -> usize {
        self.target_qubit
    }

    #[must_use]
    pub fn controls(&self) -> &[Control] {
        &self.controls
    }

    #[must_use]
    pub const fn bound_squared(&self) -> &Rational {
        self.base.bound_squared()
    }
}

/// Lift a certified one-qubit rotation into a fully controlled register.
///
/// Every base operation, including scalar `W`, receives the added signed
/// controls. Base operands on qubit zero are remapped to `target_qubit`.
/// Requiring the target and controls to cover all qubits avoids unaccounted
/// spectator copies of the base Frobenius error.
///
/// # Errors
/// Rejects incomplete, duplicate, overlapping, out-of-range, or over-budget
/// interfaces before cloning certificate-owned sequences.
pub fn lift_controlled_rotation(
    base: &ApproxCertificate,
    qubits: usize,
    target_qubit: usize,
    controls: &[Control],
    limits: Limits,
) -> Result<ControlledApproxCertificate> {
    admit_certificate(base, limits)?;
    admit_interface(qubits, target_qubit, controls, limits)?;
    admit_base_shape(base, controls.len(), qubits)?;
    let gate_count = base.candidate().operations.len();
    if gate_count > limits.gates {
        return Err(crate::types::budget(
            "gates",
            crate::types::size(gate_count)?,
            crate::types::size(limits.gates)?,
        ));
    }
    let dimension = 1usize
        .checked_shl(
            u32::try_from(qubits).map_err(|_| Error::Resource("controlled dimension".into()))?,
        )
        .ok_or_else(|| Error::Resource("controlled dimension".into()))?;
    crate::matrix::memory(gate_count, dimension, limits)?;

    let mut operations = Vec::new();
    operations
        .try_reserve_exact(gate_count)
        .map_err(|_| Error::Resource("controlled operations allocation".into()))?;
    for operation in &base.candidate().operations {
        operations.push(remap_operation(operation, target_qubit, controls, qubits)?);
    }
    let sequence = Sequence { qubits, operations };
    crate::matrix::admit(&sequence, limits)?;
    let mut owned_controls = Vec::new();
    owned_controls
        .try_reserve_exact(controls.len())
        .map_err(|_| Error::Resource("controlled interface allocation".into()))?;
    owned_controls.extend_from_slice(controls);
    Ok(ControlledApproxCertificate {
        base: base.clone(),
        sequence,
        target_qubit,
        controls: owned_controls,
    })
}

fn admit_certificate(base: &ApproxCertificate, limits: Limits) -> Result<()> {
    crate::matrix::admit(base.candidate(), limits)?;
    let mut bits = base
        .bound_squared()
        .numer()
        .bits()
        .max(base.bound_squared().denom().bits());
    if let AngleTarget::RationalPi {
        numerator,
        denominator,
    } = &base.target().angle
    {
        bits = bits.max(numerator.bits()).max(denominator.bits());
    }
    if bits > limits.coefficient_bits {
        return Err(crate::types::budget(
            "coefficient bits",
            bits,
            limits.coefficient_bits,
        ));
    }
    crate::types::allocation(bits, 16, limits)
}

fn admit_interface(
    qubits: usize,
    target_qubit: usize,
    controls: &[Control],
    limits: Limits,
) -> Result<()> {
    let width_limit = limits.qubits.min(4);
    if qubits == 0 || qubits > width_limit {
        return Err(crate::types::budget(
            "qubits",
            crate::types::size(qubits)?,
            crate::types::size(width_limit)?,
        ));
    }
    if target_qubit >= qubits {
        return Err(Error::Invalid("controlled target outside register".into()));
    }
    if controls
        .len()
        .checked_add(1)
        .ok_or_else(|| Error::Resource("controlled interface width".into()))?
        != qubits
    {
        return Err(Error::Invalid(
            "controlled interface must cover the register".into(),
        ));
    }
    let mut occupied = bit(target_qubit)?;
    for control in controls {
        if control.qubit >= qubits {
            return Err(Error::Invalid("controlled operand outside register".into()));
        }
        let control_bit = bit(control.qubit)?;
        if occupied & control_bit != 0 {
            return Err(Error::Invalid(
                "duplicate or overlapping controlled operands".into(),
            ));
        }
        occupied |= control_bit;
    }
    Ok(())
}

fn admit_base_shape(base: &ApproxCertificate, controls: usize, qubits: usize) -> Result<()> {
    for operation in &base.candidate().operations {
        if operation.targets.iter().any(|target| *target != 0)
            || operation.controls.iter().any(|control| control.qubit != 0)
        {
            return Err(Error::Invalid(
                "certificate contains a nonlocal base operand".into(),
            ));
        }
        let mapped_controls = controls
            .checked_add(operation.controls.len())
            .ok_or_else(|| Error::Resource("controlled operation width".into()))?;
        if mapped_controls > qubits {
            return Err(Error::Invalid("too many mapped controls".into()));
        }
    }
    Ok(())
}

fn remap_operation(
    operation: &Operation,
    target_qubit: usize,
    controls: &[Control],
    qubits: usize,
) -> Result<Operation> {
    let mut targets = Vec::new();
    targets
        .try_reserve_exact(operation.targets.len())
        .map_err(|_| Error::Resource("controlled targets allocation".into()))?;
    targets.extend(operation.targets.iter().map(|_| target_qubit));

    let control_count = controls
        .len()
        .checked_add(operation.controls.len())
        .ok_or_else(|| Error::Resource("controlled operation width".into()))?;
    if control_count > qubits {
        return Err(Error::Invalid("too many mapped controls".into()));
    }
    let mut mapped_controls = Vec::new();
    mapped_controls
        .try_reserve_exact(control_count)
        .map_err(|_| Error::Resource("controlled controls allocation".into()))?;
    mapped_controls.extend_from_slice(controls);
    mapped_controls.extend(operation.controls.iter().map(|control| Control {
        qubit: target_qubit,
        positive: control.positive,
    }));
    Ok(Operation {
        gate: operation.gate,
        targets,
        controls: mapped_controls,
    })
}

fn bit(qubit: usize) -> Result<usize> {
    1usize
        .checked_shl(
            u32::try_from(qubit).map_err(|_| Error::Resource("controlled operand mask".into()))?,
        )
        .ok_or_else(|| Error::Resource("controlled operand mask".into()))
}
