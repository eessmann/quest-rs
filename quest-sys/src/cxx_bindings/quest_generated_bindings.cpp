#include "quest_generated_bindings.hpp"

#include "quest-sys/src/generated_api.rs.h"

#include <complex>
#include <cstdint>
#include <memory>
#include <string>
#include <vector>

namespace quest_sys {
namespace {

[[maybe_unused]] qcomp to_qcomp(const GeneratedComplex& value) {
    return qcomp(value.re, value.im);
}

[[maybe_unused]] GeneratedComplex from_qcomp(qcomp value) {
    return GeneratedComplex{static_cast<double>(std::real(value)), static_cast<double>(std::imag(value))};
}

[[maybe_unused]] std::vector<int> to_int_vec(rust::Slice<const std::int32_t> values) {
    std::vector<int> out;
    out.reserve(values.size());
    for (const auto value : values) {
        out.push_back(static_cast<int>(value));
    }
    return out;
}

[[maybe_unused]] std::vector<unsigned> to_uint_vec(rust::Slice<const std::uint32_t> values) {
    std::vector<unsigned> out;
    out.reserve(values.size());
    for (const auto value : values) {
        out.push_back(static_cast<unsigned>(value));
    }
    return out;
}

[[maybe_unused]] std::vector<qreal> to_qreal_vec(rust::Slice<const double> values) {
    std::vector<qreal> out;
    out.reserve(values.size());
    for (const auto value : values) {
        out.push_back(static_cast<qreal>(value));
    }
    return out;
}

[[maybe_unused]] std::vector<qcomp> to_qcomp_vec(rust::Slice<const GeneratedComplex> values) {
    std::vector<qcomp> out;
    out.reserve(values.size());
    for (const auto& value : values) {
        out.push_back(to_qcomp(value));
    }
    return out;
}

[[maybe_unused]] rust::Vec<GeneratedComplex> from_qcomp_vec(const std::vector<qcomp>& values) {
    rust::Vec<GeneratedComplex> out;
    out.reserve(values.size());
    for (const auto value : values) {
        out.push_back(from_qcomp(value));
    }
    return out;
}

[[maybe_unused]] rust::Vec<double> from_qreal_vec(const std::vector<qreal>& values) {
    rust::Vec<double> out;
    out.reserve(values.size());
    for (const auto value : values) {
        out.push_back(static_cast<double>(value));
    }
    return out;
}

[[maybe_unused]] rust::Vec<std::uint32_t> from_uint_vec(const std::vector<unsigned>& values) {
    rust::Vec<std::uint32_t> out;
    out.reserve(values.size());
    for (const auto value : values) {
        out.push_back(static_cast<std::uint32_t>(value));
    }
    return out;
}

} // namespace

void apply_comp_matr1(Qureg& qureg, std::int32_t target, const CompMatr1& matrix) {
    ::applyCompMatr1(qureg.raw(), static_cast<int>(target), matrix.raw());
}

void apply_comp_matr2(Qureg& qureg, std::int32_t target1, std::int32_t target2, const CompMatr2& matrix) {
    ::applyCompMatr2(qureg.raw(), static_cast<int>(target1), static_cast<int>(target2), matrix.raw());
}

void apply_controlled_comp_matr(Qureg& qureg, std::int32_t control, rust::Slice<const std::int32_t> targets, const CompMatr& matr) {
    ::applyControlledCompMatr(qureg.raw(), static_cast<int>(control), to_int_vec(targets), matr.raw());
}

void apply_controlled_comp_matr1(Qureg& qureg, std::int32_t control, std::int32_t target, const CompMatr1& matrix) {
    ::applyControlledCompMatr1(qureg.raw(), static_cast<int>(control), static_cast<int>(target), matrix.raw());
}

void apply_controlled_comp_matr2(Qureg& qureg, std::int32_t control, std::int32_t target1, std::int32_t target2, const CompMatr2& matr) {
    ::applyControlledCompMatr2(qureg.raw(), static_cast<int>(control), static_cast<int>(target1), static_cast<int>(target2), matr.raw());
}

void apply_controlled_diag_matr(Qureg& qureg, std::int32_t control, rust::Slice<const std::int32_t> targets, const DiagMatr& matrix) {
    ::applyControlledDiagMatr(qureg.raw(), static_cast<int>(control), to_int_vec(targets), matrix.raw());
}

void apply_controlled_diag_matr1(Qureg& qureg, std::int32_t control, std::int32_t target, const DiagMatr1& matr) {
    ::applyControlledDiagMatr1(qureg.raw(), static_cast<int>(control), static_cast<int>(target), matr.raw());
}

void apply_controlled_diag_matr2(Qureg& qureg, std::int32_t control, std::int32_t target1, std::int32_t target2, const DiagMatr2& matr) {
    ::applyControlledDiagMatr2(qureg.raw(), static_cast<int>(control), static_cast<int>(target1), static_cast<int>(target2), matr.raw());
}

void apply_controlled_diag_matr_power(Qureg& qureg, std::int32_t control, rust::Slice<const std::int32_t> targets, const DiagMatr& matrix, GeneratedComplex exponent) {
    ::applyControlledDiagMatrPower(qureg.raw(), static_cast<int>(control), to_int_vec(targets), matrix.raw(), to_qcomp(exponent));
}

void apply_controlled_hadamard(Qureg& qureg, std::int32_t control, std::int32_t target) {
    ::applyControlledHadamard(qureg.raw(), static_cast<int>(control), static_cast<int>(target));
}

void apply_controlled_multi_qubit_not(Qureg& qureg, std::int32_t control, rust::Slice<const std::int32_t> targets) {
    ::applyControlledMultiQubitNot(qureg.raw(), static_cast<int>(control), to_int_vec(targets));
}

void apply_controlled_pauli_gadget(Qureg& qureg, std::int32_t control, const PauliStr& str_arg, double angle) {
    ::applyControlledPauliGadget(qureg.raw(), static_cast<int>(control), str_arg.raw(), static_cast<qreal>(angle));
}

void apply_controlled_pauli_str(Qureg& qureg, std::int32_t control, const PauliStr& str_arg) {
    ::applyControlledPauliStr(qureg.raw(), static_cast<int>(control), str_arg.raw());
}

void apply_controlled_pauli_x(Qureg& qureg, std::int32_t control, std::int32_t target) {
    ::applyControlledPauliX(qureg.raw(), static_cast<int>(control), static_cast<int>(target));
}

void apply_controlled_pauli_y(Qureg& qureg, std::int32_t control, std::int32_t target) {
    ::applyControlledPauliY(qureg.raw(), static_cast<int>(control), static_cast<int>(target));
}

void apply_controlled_pauli_z(Qureg& qureg, std::int32_t control, std::int32_t target) {
    ::applyControlledPauliZ(qureg.raw(), static_cast<int>(control), static_cast<int>(target));
}

void apply_controlled_phase_gadget(Qureg& qureg, std::int32_t control, rust::Slice<const std::int32_t> targets, double angle) {
    ::applyControlledPhaseGadget(qureg.raw(), static_cast<int>(control), to_int_vec(targets), static_cast<qreal>(angle));
}

void apply_controlled_rotate_around_axis(Qureg& qureg, std::int32_t ctrl, std::int32_t targ, double angle, double axisX, double axisY, double axisZ) {
    ::applyControlledRotateAroundAxis(qureg.raw(), static_cast<int>(ctrl), static_cast<int>(targ), static_cast<qreal>(angle), static_cast<qreal>(axisX), static_cast<qreal>(axisY), static_cast<qreal>(axisZ));
}

void apply_controlled_rotate_x(Qureg& qureg, std::int32_t control, std::int32_t target, double angle) {
    ::applyControlledRotateX(qureg.raw(), static_cast<int>(control), static_cast<int>(target), static_cast<qreal>(angle));
}

void apply_controlled_rotate_y(Qureg& qureg, std::int32_t control, std::int32_t target, double angle) {
    ::applyControlledRotateY(qureg.raw(), static_cast<int>(control), static_cast<int>(target), static_cast<qreal>(angle));
}

void apply_controlled_rotate_z(Qureg& qureg, std::int32_t control, std::int32_t target, double angle) {
    ::applyControlledRotateZ(qureg.raw(), static_cast<int>(control), static_cast<int>(target), static_cast<qreal>(angle));
}

void apply_controlled_s(Qureg& qureg, std::int32_t control, std::int32_t target) {
    ::applyControlledS(qureg.raw(), static_cast<int>(control), static_cast<int>(target));
}

void apply_controlled_sqrt_swap(Qureg& qureg, std::int32_t control, std::int32_t qubit1, std::int32_t qubit2) {
    ::applyControlledSqrtSwap(qureg.raw(), static_cast<int>(control), static_cast<int>(qubit1), static_cast<int>(qubit2));
}

void apply_controlled_swap(Qureg& qureg, std::int32_t control, std::int32_t qubit1, std::int32_t qubit2) {
    ::applyControlledSwap(qureg.raw(), static_cast<int>(control), static_cast<int>(qubit1), static_cast<int>(qubit2));
}

void apply_controlled_t(Qureg& qureg, std::int32_t control, std::int32_t target) {
    ::applyControlledT(qureg.raw(), static_cast<int>(control), static_cast<int>(target));
}

void apply_diag_matr(Qureg& qureg, rust::Slice<const std::int32_t> targets, const DiagMatr& matrix) {
    ::applyDiagMatr(qureg.raw(), to_int_vec(targets), matrix.raw());
}

void apply_diag_matr1(Qureg& qureg, std::int32_t target, const DiagMatr1& matr) {
    ::applyDiagMatr1(qureg.raw(), static_cast<int>(target), matr.raw());
}

void apply_diag_matr2(Qureg& qureg, std::int32_t target1, std::int32_t target2, const DiagMatr2& matr) {
    ::applyDiagMatr2(qureg.raw(), static_cast<int>(target1), static_cast<int>(target2), matr.raw());
}

void apply_diag_matr_power(Qureg& qureg, rust::Slice<const std::int32_t> targets, const DiagMatr& matrix, GeneratedComplex exponent) {
    ::applyDiagMatrPower(qureg.raw(), to_int_vec(targets), matrix.raw(), to_qcomp(exponent));
}

double apply_forced_multi_qubit_measurement(Qureg& qureg, rust::Slice<const std::int32_t> qubits, rust::Slice<const std::int32_t> outcomes) {
    return static_cast<double>(::applyForcedMultiQubitMeasurement(qureg.raw(), to_int_vec(qubits), to_int_vec(outcomes)));
}

double apply_forced_qubit_measurement(Qureg& qureg, std::int32_t target, std::int32_t outcome) {
    return static_cast<double>(::applyForcedQubitMeasurement(qureg.raw(), static_cast<int>(target), static_cast<int>(outcome)));
}

void apply_full_quantum_fourier_transform(Qureg& qureg, bool inverse) {
    ::applyFullQuantumFourierTransform(qureg.raw(), inverse);
}

void apply_full_state_diag_matr(Qureg& qureg, const FullStateDiagMatr& matrix) {
    ::applyFullStateDiagMatr(qureg.raw(), matrix.raw());
}

void apply_full_state_diag_matr_power(Qureg& qureg, const FullStateDiagMatr& matrix, GeneratedComplex exponent) {
    ::applyFullStateDiagMatrPower(qureg.raw(), matrix.raw(), to_qcomp(exponent));
}

void apply_multi_controlled_comp_matr(Qureg& qureg, rust::Slice<const std::int32_t> controls, rust::Slice<const std::int32_t> targets, const CompMatr& matr) {
    ::applyMultiControlledCompMatr(qureg.raw(), to_int_vec(controls), to_int_vec(targets), matr.raw());
}

void apply_multi_controlled_comp_matr1(Qureg& qureg, rust::Slice<const std::int32_t> controls, std::int32_t target, const CompMatr1& matrix) {
    ::applyMultiControlledCompMatr1(qureg.raw(), to_int_vec(controls), static_cast<int>(target), matrix.raw());
}

void apply_multi_controlled_comp_matr2(Qureg& qureg, rust::Slice<const std::int32_t> controls, std::int32_t target1, std::int32_t target2, const CompMatr2& matr) {
    ::applyMultiControlledCompMatr2(qureg.raw(), to_int_vec(controls), static_cast<int>(target1), static_cast<int>(target2), matr.raw());
}

void apply_multi_controlled_diag_matr(Qureg& qureg, rust::Slice<const std::int32_t> controls, rust::Slice<const std::int32_t> targets, const DiagMatr& matrix) {
    ::applyMultiControlledDiagMatr(qureg.raw(), to_int_vec(controls), to_int_vec(targets), matrix.raw());
}

void apply_multi_controlled_diag_matr1(Qureg& qureg, rust::Slice<const std::int32_t> controls, std::int32_t target, const DiagMatr1& matr) {
    ::applyMultiControlledDiagMatr1(qureg.raw(), to_int_vec(controls), static_cast<int>(target), matr.raw());
}

void apply_multi_controlled_diag_matr2(Qureg& qureg, rust::Slice<const std::int32_t> controls, std::int32_t target1, std::int32_t target2, const DiagMatr2& matr) {
    ::applyMultiControlledDiagMatr2(qureg.raw(), to_int_vec(controls), static_cast<int>(target1), static_cast<int>(target2), matr.raw());
}

void apply_multi_controlled_diag_matr_power(Qureg& qureg, rust::Slice<const std::int32_t> controls, rust::Slice<const std::int32_t> targets, const DiagMatr& matrix, GeneratedComplex exponent) {
    ::applyMultiControlledDiagMatrPower(qureg.raw(), to_int_vec(controls), to_int_vec(targets), matrix.raw(), to_qcomp(exponent));
}

void apply_multi_controlled_hadamard(Qureg& qureg, rust::Slice<const std::int32_t> controls, std::int32_t target) {
    ::applyMultiControlledHadamard(qureg.raw(), to_int_vec(controls), static_cast<int>(target));
}

void apply_multi_controlled_multi_qubit_not(Qureg& qureg, rust::Slice<const std::int32_t> controls, rust::Slice<const std::int32_t> targets) {
    ::applyMultiControlledMultiQubitNot(qureg.raw(), to_int_vec(controls), to_int_vec(targets));
}

void apply_multi_controlled_pauli_gadget(Qureg& qureg, rust::Slice<const std::int32_t> controls, const PauliStr& str_arg, double angle) {
    ::applyMultiControlledPauliGadget(qureg.raw(), to_int_vec(controls), str_arg.raw(), static_cast<qreal>(angle));
}

void apply_multi_controlled_pauli_str(Qureg& qureg, rust::Slice<const std::int32_t> controls, const PauliStr& str_arg) {
    ::applyMultiControlledPauliStr(qureg.raw(), to_int_vec(controls), str_arg.raw());
}

void apply_multi_controlled_pauli_x(Qureg& qureg, rust::Slice<const std::int32_t> controls, std::int32_t target) {
    ::applyMultiControlledPauliX(qureg.raw(), to_int_vec(controls), static_cast<int>(target));
}

void apply_multi_controlled_pauli_y(Qureg& qureg, rust::Slice<const std::int32_t> controls, std::int32_t target) {
    ::applyMultiControlledPauliY(qureg.raw(), to_int_vec(controls), static_cast<int>(target));
}

void apply_multi_controlled_pauli_z(Qureg& qureg, rust::Slice<const std::int32_t> controls, std::int32_t target) {
    ::applyMultiControlledPauliZ(qureg.raw(), to_int_vec(controls), static_cast<int>(target));
}

void apply_multi_controlled_phase_gadget(Qureg& qureg, rust::Slice<const std::int32_t> controls, rust::Slice<const std::int32_t> targets, double angle) {
    ::applyMultiControlledPhaseGadget(qureg.raw(), to_int_vec(controls), to_int_vec(targets), static_cast<qreal>(angle));
}

void apply_multi_controlled_rotate_around_axis(Qureg& qureg, rust::Slice<const std::int32_t> ctrls, std::int32_t targ, double angle, double axisX, double axisY, double axisZ) {
    ::applyMultiControlledRotateAroundAxis(qureg.raw(), to_int_vec(ctrls), static_cast<int>(targ), static_cast<qreal>(angle), static_cast<qreal>(axisX), static_cast<qreal>(axisY), static_cast<qreal>(axisZ));
}

void apply_multi_controlled_rotate_x(Qureg& qureg, rust::Slice<const std::int32_t> controls, std::int32_t target, double angle) {
    ::applyMultiControlledRotateX(qureg.raw(), to_int_vec(controls), static_cast<int>(target), static_cast<qreal>(angle));
}

void apply_multi_controlled_rotate_y(Qureg& qureg, rust::Slice<const std::int32_t> controls, std::int32_t target, double angle) {
    ::applyMultiControlledRotateY(qureg.raw(), to_int_vec(controls), static_cast<int>(target), static_cast<qreal>(angle));
}

void apply_multi_controlled_rotate_z(Qureg& qureg, rust::Slice<const std::int32_t> controls, std::int32_t target, double angle) {
    ::applyMultiControlledRotateZ(qureg.raw(), to_int_vec(controls), static_cast<int>(target), static_cast<qreal>(angle));
}

void apply_multi_controlled_s(Qureg& qureg, rust::Slice<const std::int32_t> controls, std::int32_t target) {
    ::applyMultiControlledS(qureg.raw(), to_int_vec(controls), static_cast<int>(target));
}

void apply_multi_controlled_sqrt_swap(Qureg& qureg, rust::Slice<const std::int32_t> controls, std::int32_t qubit1, std::int32_t qubit2) {
    ::applyMultiControlledSqrtSwap(qureg.raw(), to_int_vec(controls), static_cast<int>(qubit1), static_cast<int>(qubit2));
}

void apply_multi_controlled_swap(Qureg& qureg, rust::Slice<const std::int32_t> controls, std::int32_t qubit1, std::int32_t qubit2) {
    ::applyMultiControlledSwap(qureg.raw(), to_int_vec(controls), static_cast<int>(qubit1), static_cast<int>(qubit2));
}

void apply_multi_controlled_t(Qureg& qureg, rust::Slice<const std::int32_t> controls, std::int32_t target) {
    ::applyMultiControlledT(qureg.raw(), to_int_vec(controls), static_cast<int>(target));
}

std::int64_t apply_multi_qubit_measurement(Qureg& qureg, rust::Slice<const std::int32_t> qubits) {
    return static_cast<std::int64_t>(::applyMultiQubitMeasurement(qureg.raw(), to_int_vec(qubits)));
}

void apply_multi_qubit_not(Qureg& qureg, rust::Slice<const std::int32_t> targets) {
    ::applyMultiQubitNot(qureg.raw(), to_int_vec(targets));
}

void apply_multi_qubit_phase_flip(Qureg& qureg, rust::Slice<const std::int32_t> targets) {
    ::applyMultiQubitPhaseFlip(qureg.raw(), to_int_vec(targets));
}

void apply_multi_qubit_phase_shift(Qureg& qureg, rust::Slice<const std::int32_t> targets, double angle) {
    ::applyMultiQubitPhaseShift(qureg.raw(), to_int_vec(targets), static_cast<qreal>(angle));
}

void apply_multi_qubit_projector(Qureg& qureg, rust::Slice<const std::int32_t> qubits, rust::Slice<const std::int32_t> outcomes) {
    ::applyMultiQubitProjector(qureg.raw(), to_int_vec(qubits), to_int_vec(outcomes));
}

void apply_multi_state_controlled_comp_matr(Qureg& qureg, rust::Slice<const std::int32_t> controls, rust::Slice<const std::int32_t> states, rust::Slice<const std::int32_t> targets, const CompMatr& matr) {
    ::applyMultiStateControlledCompMatr(qureg.raw(), to_int_vec(controls), to_int_vec(states), to_int_vec(targets), matr.raw());
}

void apply_multi_state_controlled_comp_matr1(Qureg& qureg, rust::Slice<const std::int32_t> controls, rust::Slice<const std::int32_t> states, std::int32_t target, const CompMatr1& matrix) {
    ::applyMultiStateControlledCompMatr1(qureg.raw(), to_int_vec(controls), to_int_vec(states), static_cast<int>(target), matrix.raw());
}

void apply_multi_state_controlled_comp_matr2(Qureg& qureg, rust::Slice<const std::int32_t> controls, rust::Slice<const std::int32_t> states, std::int32_t target1, std::int32_t target2, const CompMatr2& matr) {
    ::applyMultiStateControlledCompMatr2(qureg.raw(), to_int_vec(controls), to_int_vec(states), static_cast<int>(target1), static_cast<int>(target2), matr.raw());
}

void apply_multi_state_controlled_diag_matr(Qureg& qureg, rust::Slice<const std::int32_t> controls, rust::Slice<const std::int32_t> states, rust::Slice<const std::int32_t> targets, const DiagMatr& matrix) {
    ::applyMultiStateControlledDiagMatr(qureg.raw(), to_int_vec(controls), to_int_vec(states), to_int_vec(targets), matrix.raw());
}

void apply_multi_state_controlled_diag_matr1(Qureg& qureg, rust::Slice<const std::int32_t> controls, rust::Slice<const std::int32_t> states, std::int32_t target, const DiagMatr1& matr) {
    ::applyMultiStateControlledDiagMatr1(qureg.raw(), to_int_vec(controls), to_int_vec(states), static_cast<int>(target), matr.raw());
}

void apply_multi_state_controlled_diag_matr2(Qureg& qureg, rust::Slice<const std::int32_t> controls, rust::Slice<const std::int32_t> states, std::int32_t target1, std::int32_t target2, const DiagMatr2& matr) {
    ::applyMultiStateControlledDiagMatr2(qureg.raw(), to_int_vec(controls), to_int_vec(states), static_cast<int>(target1), static_cast<int>(target2), matr.raw());
}

void apply_multi_state_controlled_diag_matr_power(Qureg& qureg, rust::Slice<const std::int32_t> controls, rust::Slice<const std::int32_t> states, rust::Slice<const std::int32_t> targets, const DiagMatr& matrix, GeneratedComplex exponent) {
    ::applyMultiStateControlledDiagMatrPower(qureg.raw(), to_int_vec(controls), to_int_vec(states), to_int_vec(targets), matrix.raw(), to_qcomp(exponent));
}

void apply_multi_state_controlled_hadamard(Qureg& qureg, rust::Slice<const std::int32_t> controls, rust::Slice<const std::int32_t> states, std::int32_t target) {
    ::applyMultiStateControlledHadamard(qureg.raw(), to_int_vec(controls), to_int_vec(states), static_cast<int>(target));
}

void apply_multi_state_controlled_multi_qubit_not(Qureg& qureg, rust::Slice<const std::int32_t> controls, rust::Slice<const std::int32_t> states, rust::Slice<const std::int32_t> targets) {
    ::applyMultiStateControlledMultiQubitNot(qureg.raw(), to_int_vec(controls), to_int_vec(states), to_int_vec(targets));
}

void apply_multi_state_controlled_pauli_gadget(Qureg& qureg, rust::Slice<const std::int32_t> controls, rust::Slice<const std::int32_t> states, const PauliStr& str_arg, double angle) {
    ::applyMultiStateControlledPauliGadget(qureg.raw(), to_int_vec(controls), to_int_vec(states), str_arg.raw(), static_cast<qreal>(angle));
}

void apply_multi_state_controlled_pauli_str(Qureg& qureg, rust::Slice<const std::int32_t> controls, rust::Slice<const std::int32_t> states, const PauliStr& str_arg) {
    ::applyMultiStateControlledPauliStr(qureg.raw(), to_int_vec(controls), to_int_vec(states), str_arg.raw());
}

void apply_multi_state_controlled_pauli_x(Qureg& qureg, rust::Slice<const std::int32_t> controls, rust::Slice<const std::int32_t> states, std::int32_t target) {
    ::applyMultiStateControlledPauliX(qureg.raw(), to_int_vec(controls), to_int_vec(states), static_cast<int>(target));
}

void apply_multi_state_controlled_pauli_y(Qureg& qureg, rust::Slice<const std::int32_t> controls, rust::Slice<const std::int32_t> states, std::int32_t target) {
    ::applyMultiStateControlledPauliY(qureg.raw(), to_int_vec(controls), to_int_vec(states), static_cast<int>(target));
}

void apply_multi_state_controlled_pauli_z(Qureg& qureg, rust::Slice<const std::int32_t> controls, rust::Slice<const std::int32_t> states, std::int32_t target) {
    ::applyMultiStateControlledPauliZ(qureg.raw(), to_int_vec(controls), to_int_vec(states), static_cast<int>(target));
}

void apply_multi_state_controlled_phase_gadget(Qureg& qureg, rust::Slice<const std::int32_t> controls, rust::Slice<const std::int32_t> states, rust::Slice<const std::int32_t> targets, double angle) {
    ::applyMultiStateControlledPhaseGadget(qureg.raw(), to_int_vec(controls), to_int_vec(states), to_int_vec(targets), static_cast<qreal>(angle));
}

void apply_multi_state_controlled_rotate_around_axis(Qureg& qureg, rust::Slice<const std::int32_t> ctrls, rust::Slice<const std::int32_t> states, std::int32_t targ, double angle, double axisX, double axisY, double axisZ) {
    ::applyMultiStateControlledRotateAroundAxis(qureg.raw(), to_int_vec(ctrls), to_int_vec(states), static_cast<int>(targ), static_cast<qreal>(angle), static_cast<qreal>(axisX), static_cast<qreal>(axisY), static_cast<qreal>(axisZ));
}

void apply_multi_state_controlled_rotate_x(Qureg& qureg, rust::Slice<const std::int32_t> controls, rust::Slice<const std::int32_t> states, std::int32_t target, double angle) {
    ::applyMultiStateControlledRotateX(qureg.raw(), to_int_vec(controls), to_int_vec(states), static_cast<int>(target), static_cast<qreal>(angle));
}

void apply_multi_state_controlled_rotate_y(Qureg& qureg, rust::Slice<const std::int32_t> controls, rust::Slice<const std::int32_t> states, std::int32_t target, double angle) {
    ::applyMultiStateControlledRotateY(qureg.raw(), to_int_vec(controls), to_int_vec(states), static_cast<int>(target), static_cast<qreal>(angle));
}

void apply_multi_state_controlled_rotate_z(Qureg& qureg, rust::Slice<const std::int32_t> controls, rust::Slice<const std::int32_t> states, std::int32_t target, double angle) {
    ::applyMultiStateControlledRotateZ(qureg.raw(), to_int_vec(controls), to_int_vec(states), static_cast<int>(target), static_cast<qreal>(angle));
}

void apply_multi_state_controlled_s(Qureg& qureg, rust::Slice<const std::int32_t> controls, rust::Slice<const std::int32_t> states, std::int32_t target) {
    ::applyMultiStateControlledS(qureg.raw(), to_int_vec(controls), to_int_vec(states), static_cast<int>(target));
}

void apply_multi_state_controlled_sqrt_swap(Qureg& qureg, rust::Slice<const std::int32_t> controls, rust::Slice<const std::int32_t> states, std::int32_t qubit1, std::int32_t qubit2) {
    ::applyMultiStateControlledSqrtSwap(qureg.raw(), to_int_vec(controls), to_int_vec(states), static_cast<int>(qubit1), static_cast<int>(qubit2));
}

void apply_multi_state_controlled_swap(Qureg& qureg, rust::Slice<const std::int32_t> controls, rust::Slice<const std::int32_t> states, std::int32_t qubit1, std::int32_t qubit2) {
    ::applyMultiStateControlledSwap(qureg.raw(), to_int_vec(controls), to_int_vec(states), static_cast<int>(qubit1), static_cast<int>(qubit2));
}

void apply_multi_state_controlled_t(Qureg& qureg, rust::Slice<const std::int32_t> controls, rust::Slice<const std::int32_t> states, std::int32_t target) {
    ::applyMultiStateControlledT(qureg.raw(), to_int_vec(controls), to_int_vec(states), static_cast<int>(target));
}

void apply_non_unitary_pauli_gadget(Qureg& qureg, const PauliStr& str_arg, GeneratedComplex angle) {
    ::applyNonUnitaryPauliGadget(qureg.raw(), str_arg.raw(), to_qcomp(angle));
}

void apply_pauli_gadget(Qureg& qureg, const PauliStr& str_arg, double angle) {
    ::applyPauliGadget(qureg.raw(), str_arg.raw(), static_cast<qreal>(angle));
}

void apply_pauli_str(Qureg& qureg, const PauliStr& str_arg) {
    ::applyPauliStr(qureg.raw(), str_arg.raw());
}

void apply_phase_flip(Qureg& qureg, std::int32_t target) {
    ::applyPhaseFlip(qureg.raw(), static_cast<int>(target));
}

void apply_phase_gadget(Qureg& qureg, rust::Slice<const std::int32_t> targets, double angle) {
    ::applyPhaseGadget(qureg.raw(), to_int_vec(targets), static_cast<qreal>(angle));
}

void apply_phase_shift(Qureg& qureg, std::int32_t target, double angle) {
    ::applyPhaseShift(qureg.raw(), static_cast<int>(target), static_cast<qreal>(angle));
}

void apply_quantum_fourier_transform(Qureg& qureg, rust::Slice<const std::int32_t> targets, bool inverse) {
    ::applyQuantumFourierTransform(qureg.raw(), to_int_vec(targets), inverse);
}

void apply_qubit_projector(Qureg& qureg, std::int32_t target, std::int32_t outcome) {
    ::applyQubitProjector(qureg.raw(), static_cast<int>(target), static_cast<int>(outcome));
}

void apply_rotate_around_axis(Qureg& qureg, std::int32_t target, double angle, double axisX, double axisY, double axisZ) {
    ::applyRotateAroundAxis(qureg.raw(), static_cast<int>(target), static_cast<qreal>(angle), static_cast<qreal>(axisX), static_cast<qreal>(axisY), static_cast<qreal>(axisZ));
}

void apply_rotate_x(Qureg& qureg, std::int32_t target, double angle) {
    ::applyRotateX(qureg.raw(), static_cast<int>(target), static_cast<qreal>(angle));
}

void apply_rotate_y(Qureg& qureg, std::int32_t target, double angle) {
    ::applyRotateY(qureg.raw(), static_cast<int>(target), static_cast<qreal>(angle));
}

void apply_rotate_z(Qureg& qureg, std::int32_t target, double angle) {
    ::applyRotateZ(qureg.raw(), static_cast<int>(target), static_cast<qreal>(angle));
}

void apply_s(Qureg& qureg, std::int32_t target) {
    ::applyS(qureg.raw(), static_cast<int>(target));
}

void apply_sqrt_swap(Qureg& qureg, std::int32_t qubit1, std::int32_t qubit2) {
    ::applySqrtSwap(qureg.raw(), static_cast<int>(qubit1), static_cast<int>(qubit2));
}

void apply_swap(Qureg& qureg, std::int32_t qubit1, std::int32_t qubit2) {
    ::applySwap(qureg.raw(), static_cast<int>(qubit1), static_cast<int>(qubit2));
}

void apply_t(Qureg& qureg, std::int32_t target) {
    ::applyT(qureg.raw(), static_cast<int>(target));
}

void apply_trotterized_controlled_pauli_str_sum_gadget(Qureg& qureg, std::int32_t control, const PauliStrSum& sum, double angle, std::int32_t order, std::int32_t reps, bool permuteTerms) {
    ::applyTrotterizedControlledPauliStrSumGadget(qureg.raw(), static_cast<int>(control), sum.raw(), static_cast<qreal>(angle), static_cast<int>(order), static_cast<int>(reps), permuteTerms);
}

void apply_trotterized_imaginary_time_evolution(Qureg& qureg, const PauliStrSum& hamil, double tau, std::int32_t order, std::int32_t reps, bool permuteTerms) {
    ::applyTrotterizedImaginaryTimeEvolution(qureg.raw(), hamil.raw(), static_cast<qreal>(tau), static_cast<int>(order), static_cast<int>(reps), permuteTerms);
}

void apply_trotterized_multi_controlled_pauli_str_sum_gadget(Qureg& qureg, rust::Slice<const std::int32_t> controls, const PauliStrSum& sum, double angle, std::int32_t order, std::int32_t reps, bool permuteTerms) {
    ::applyTrotterizedMultiControlledPauliStrSumGadget(qureg.raw(), to_int_vec(controls), sum.raw(), static_cast<qreal>(angle), static_cast<int>(order), static_cast<int>(reps), permuteTerms);
}

void apply_trotterized_multi_state_controlled_pauli_str_sum_gadget(Qureg& qureg, rust::Slice<const std::int32_t> controls, rust::Slice<const std::int32_t> states, const PauliStrSum& sum, double angle, std::int32_t order, std::int32_t reps, bool permuteTerms) {
    ::applyTrotterizedMultiStateControlledPauliStrSumGadget(qureg.raw(), to_int_vec(controls), to_int_vec(states), sum.raw(), static_cast<qreal>(angle), static_cast<int>(order), static_cast<int>(reps), permuteTerms);
}

void apply_trotterized_non_unitary_pauli_str_sum_gadget(Qureg& qureg, const PauliStrSum& sum, GeneratedComplex angle, std::int32_t order, std::int32_t reps, bool permuteTerms) {
    ::applyTrotterizedNonUnitaryPauliStrSumGadget(qureg.raw(), sum.raw(), to_qcomp(angle), static_cast<int>(order), static_cast<int>(reps), permuteTerms);
}

void apply_trotterized_pauli_str_sum_gadget(Qureg& qureg, const PauliStrSum& sum, double angle, std::int32_t order, std::int32_t reps, bool permuteTerms) {
    ::applyTrotterizedPauliStrSumGadget(qureg.raw(), sum.raw(), static_cast<qreal>(angle), static_cast<int>(order), static_cast<int>(reps), permuteTerms);
}

void apply_two_qubit_phase_flip(Qureg& qureg, std::int32_t target1, std::int32_t target2) {
    ::applyTwoQubitPhaseFlip(qureg.raw(), static_cast<int>(target1), static_cast<int>(target2));
}

void apply_two_qubit_phase_shift(Qureg& qureg, std::int32_t target1, std::int32_t target2, double angle) {
    ::applyTwoQubitPhaseShift(qureg.raw(), static_cast<int>(target1), static_cast<int>(target2), static_cast<qreal>(angle));
}

double calc_expec_full_state_diag_matr(const Qureg& qureg, const FullStateDiagMatr& matr) {
    return static_cast<double>(::calcExpecFullStateDiagMatr(qureg.raw(), matr.raw()));
}

double calc_expec_full_state_diag_matr_power(const Qureg& qureg, const FullStateDiagMatr& matrix, double exponent) {
    return static_cast<double>(::calcExpecFullStateDiagMatrPower(qureg.raw(), matrix.raw(), static_cast<qreal>(exponent)));
}

GeneratedComplex calc_expec_non_hermitian_full_state_diag_matr(const Qureg& qureg, const FullStateDiagMatr& matr) {
    return from_qcomp(::calcExpecNonHermitianFullStateDiagMatr(qureg.raw(), matr.raw()));
}

GeneratedComplex calc_expec_non_hermitian_full_state_diag_matr_power(const Qureg& qureg, const FullStateDiagMatr& matrix, GeneratedComplex exponent) {
    return from_qcomp(::calcExpecNonHermitianFullStateDiagMatrPower(qureg.raw(), matrix.raw(), to_qcomp(exponent)));
}

GeneratedComplex calc_expec_non_hermitian_pauli_str_sum(const Qureg& qureg, const PauliStrSum& sum) {
    return from_qcomp(::calcExpecNonHermitianPauliStrSum(qureg.raw(), sum.raw()));
}

double calc_expec_pauli_str(const Qureg& qureg, const PauliStr& str_arg) {
    return static_cast<double>(::calcExpecPauliStr(qureg.raw(), str_arg.raw()));
}

double calc_expec_pauli_str_sum(const Qureg& qureg, const PauliStrSum& sum) {
    return static_cast<double>(::calcExpecPauliStrSum(qureg.raw(), sum.raw()));
}

double calc_fidelity(const Qureg& qureg, const Qureg& other) {
    return static_cast<double>(::calcFidelity(qureg.raw(), other.raw()));
}

GeneratedComplex calc_inner_product(const Qureg& qureg, const Qureg& other) {
    return from_qcomp(::calcInnerProduct(qureg.raw(), other.raw()));
}

std::unique_ptr<Qureg> calc_partial_trace(const Qureg& qureg, rust::Slice<const std::int32_t> traceOutQubits) {
    return std::make_unique<Qureg>(::calcPartialTrace(qureg.raw(), to_int_vec(traceOutQubits)));
}

double calc_prob_of_basis_state(const Qureg& qureg, std::int64_t index) {
    return static_cast<double>(::calcProbOfBasisState(qureg.raw(), static_cast<qindex>(index)));
}

double calc_prob_of_multi_qubit_outcome(const Qureg& qureg, rust::Slice<const std::int32_t> qubits, rust::Slice<const std::int32_t> outcomes) {
    return static_cast<double>(::calcProbOfMultiQubitOutcome(qureg.raw(), to_int_vec(qubits), to_int_vec(outcomes)));
}

double calc_prob_of_qubit_outcome(const Qureg& qureg, std::int32_t qubit, std::int32_t outcome) {
    return static_cast<double>(::calcProbOfQubitOutcome(qureg.raw(), static_cast<int>(qubit), static_cast<int>(outcome)));
}

rust::Vec<double> calc_probs_of_all_multi_qubit_outcomes(const Qureg& qureg, rust::Slice<const std::int32_t> qubits) {
    return from_qreal_vec(::calcProbsOfAllMultiQubitOutcomes(qureg.raw(), to_int_vec(qubits)));
}

double calc_purity(const Qureg& qureg) {
    return static_cast<double>(::calcPurity(qureg.raw()));
}

std::unique_ptr<Qureg> calc_reduced_density_matrix(const Qureg& qureg, rust::Slice<const std::int32_t> retainQubits) {
    return std::make_unique<Qureg>(::calcReducedDensityMatrix(qureg.raw(), to_int_vec(retainQubits)));
}

void clear_qu_est_gpu_cache() {
    ::clearQuESTGpuCache();
}

std::unique_ptr<Qureg> create_clone_qureg(const Qureg& qureg) {
    return std::make_unique<Qureg>(::createCloneQureg(qureg.raw()));
}

std::unique_ptr<FullStateDiagMatr> create_custom_full_state_diag_matr(std::int32_t numQubits, std::int32_t useDistrib, std::int32_t useGpuAccel, std::int32_t useMultithread) {
    return std::make_unique<FullStateDiagMatr>(::createCustomFullStateDiagMatr(static_cast<int>(numQubits), static_cast<int>(useDistrib), static_cast<int>(useGpuAccel), static_cast<int>(useMultithread)));
}

std::unique_ptr<Qureg> create_custom_qureg(std::int32_t numQubits, std::int32_t isDensMatr, std::int32_t useDistrib, std::int32_t useGpuAccel, std::int32_t useMultithread) {
    return std::make_unique<Qureg>(::createCustomQureg(static_cast<int>(numQubits), static_cast<int>(isDensMatr), static_cast<int>(useDistrib), static_cast<int>(useGpuAccel), static_cast<int>(useMultithread)));
}

std::unique_ptr<DiagMatr> create_diag_matr(std::int32_t numQubits) {
    return std::make_unique<DiagMatr>(::createDiagMatr(static_cast<int>(numQubits)));
}

std::unique_ptr<Qureg> create_forced_density_qureg(std::int32_t numQubits) {
    return std::make_unique<Qureg>(::createForcedDensityQureg(static_cast<int>(numQubits)));
}

std::unique_ptr<Qureg> create_forced_qureg(std::int32_t numQubits) {
    return std::make_unique<Qureg>(::createForcedQureg(static_cast<int>(numQubits)));
}

std::unique_ptr<FullStateDiagMatr> create_full_state_diag_matr(std::int32_t numQubits) {
    return std::make_unique<FullStateDiagMatr>(::createFullStateDiagMatr(static_cast<int>(numQubits)));
}

std::unique_ptr<FullStateDiagMatr> create_full_state_diag_matr_from_pauli_str_sum(const PauliStrSum& in_arg) {
    return std::make_unique<FullStateDiagMatr>(::createFullStateDiagMatrFromPauliStrSum(in_arg.raw()));
}

std::unique_ptr<DiagMatr> create_inline_diag_matr(std::int32_t numQb, rust::Slice<const GeneratedComplex> elems) {
    return std::make_unique<DiagMatr>(::createInlineDiagMatr(static_cast<int>(numQb), to_qcomp_vec(elems)));
}

std::unique_ptr<PauliStrSum> create_pauli_str_sum_from_file(rust::Str fn) {
    return std::make_unique<PauliStrSum>(::createPauliStrSumFromFile(std::string(fn.data(), fn.size())));
}

std::unique_ptr<PauliStrSum> create_pauli_str_sum_from_reversed_file(rust::Str fn) {
    return std::make_unique<PauliStrSum>(::createPauliStrSumFromReversedFile(std::string(fn.data(), fn.size())));
}

std::unique_ptr<Qureg> create_qureg_from_file(rust::Str fn) {
    return std::make_unique<Qureg>(::createQuregFromFile(std::string(fn.data(), fn.size())));
}

GeneratedComplex get_density_qureg_amp(const Qureg& qureg, std::int64_t row, std::int64_t column) {
    return from_qcomp(::getDensityQuregAmp(qureg.raw(), static_cast<qindex>(row), static_cast<qindex>(column)));
}

std::unique_ptr<DiagMatr1> get_diag_matr1(rust::Slice<const GeneratedComplex> in_arg) {
    return std::make_unique<DiagMatr1>(::getDiagMatr1(to_qcomp_vec(in_arg)));
}

std::unique_ptr<DiagMatr2> get_diag_matr2(rust::Slice<const GeneratedComplex> in_arg) {
    return std::make_unique<DiagMatr2>(::getDiagMatr2(to_qcomp_vec(in_arg)));
}

std::unique_ptr<PauliStr> get_pauli_str(rust::Str paulis, rust::Slice<const std::int32_t> indices) {
    return std::make_unique<PauliStr>(::getPauliStr(std::string(paulis.data(), paulis.size()), to_int_vec(indices)));
}

std::int64_t get_qu_est_gpu_cache_size() {
    return static_cast<std::int64_t>(::getQuESTGpuCacheSize());
}

std::int32_t get_qu_est_num_gpu_threads_per_block() {
    return static_cast<std::int32_t>(::getQuESTNumGpuThreadsPerBlock());
}

std::int32_t get_qu_est_num_seeds() {
    return static_cast<std::int32_t>(::getQuESTNumSeeds());
}

double get_qu_est_validation_epsilon() {
    return static_cast<double>(::getQuESTValidationEpsilon());
}

void init_blank_state(Qureg& qureg) {
    ::initBlankState(qureg.raw());
}

void init_classical_state(Qureg& qureg, std::int64_t stateInd) {
    ::initClassicalState(qureg.raw(), static_cast<qindex>(stateInd));
}

void init_debug_state(Qureg& qureg) {
    ::initDebugState(qureg.raw());
}

void init_pure_state(Qureg& qureg, const Qureg& pure) {
    ::initPureState(qureg.raw(), pure.raw());
}

void init_random_mixed_state(Qureg& qureg, std::int64_t numPureStates) {
    ::initRandomMixedState(qureg.raw(), static_cast<qindex>(numPureStates));
}

void init_random_pure_state(Qureg& qureg) {
    ::initRandomPureState(qureg.raw());
}

void leftapply_comp_matr1(Qureg& qureg, std::int32_t target, const CompMatr1& matrix) {
    ::leftapplyCompMatr1(qureg.raw(), static_cast<int>(target), matrix.raw());
}

void leftapply_comp_matr2(Qureg& qureg, std::int32_t target1, std::int32_t target2, const CompMatr2& matr) {
    ::leftapplyCompMatr2(qureg.raw(), static_cast<int>(target1), static_cast<int>(target2), matr.raw());
}

void leftapply_diag_matr(Qureg& qureg, rust::Slice<const std::int32_t> targets, const DiagMatr& matrix) {
    ::leftapplyDiagMatr(qureg.raw(), to_int_vec(targets), matrix.raw());
}

void leftapply_diag_matr1(Qureg& qureg, std::int32_t target, const DiagMatr1& matr) {
    ::leftapplyDiagMatr1(qureg.raw(), static_cast<int>(target), matr.raw());
}

void leftapply_diag_matr2(Qureg& qureg, std::int32_t target1, std::int32_t target2, const DiagMatr2& matr) {
    ::leftapplyDiagMatr2(qureg.raw(), static_cast<int>(target1), static_cast<int>(target2), matr.raw());
}

void leftapply_diag_matr_power(Qureg& qureg, rust::Slice<const std::int32_t> targets, const DiagMatr& matrix, GeneratedComplex exponent) {
    ::leftapplyDiagMatrPower(qureg.raw(), to_int_vec(targets), matrix.raw(), to_qcomp(exponent));
}

void leftapply_full_state_diag_matr(Qureg& qureg, const FullStateDiagMatr& matrix) {
    ::leftapplyFullStateDiagMatr(qureg.raw(), matrix.raw());
}

void leftapply_full_state_diag_matr_power(Qureg& qureg, const FullStateDiagMatr& matrix, GeneratedComplex exponent) {
    ::leftapplyFullStateDiagMatrPower(qureg.raw(), matrix.raw(), to_qcomp(exponent));
}

void leftapply_multi_qubit_not(Qureg& qureg, rust::Slice<const std::int32_t> targets) {
    ::leftapplyMultiQubitNot(qureg.raw(), to_int_vec(targets));
}

void leftapply_multi_qubit_projector(Qureg& qureg, rust::Slice<const std::int32_t> qubits, rust::Slice<const std::int32_t> outcomes) {
    ::leftapplyMultiQubitProjector(qureg.raw(), to_int_vec(qubits), to_int_vec(outcomes));
}

void leftapply_pauli_gadget(Qureg& qureg, const PauliStr& str_arg, double angle) {
    ::leftapplyPauliGadget(qureg.raw(), str_arg.raw(), static_cast<qreal>(angle));
}

void leftapply_pauli_str(Qureg& qureg, const PauliStr& str_arg) {
    ::leftapplyPauliStr(qureg.raw(), str_arg.raw());
}

void leftapply_pauli_str_sum(Qureg& qureg, const PauliStrSum& sum, const Qureg& workspace) {
    ::leftapplyPauliStrSum(qureg.raw(), sum.raw(), workspace.raw());
}

void leftapply_pauli_x(Qureg& qureg, std::int32_t target) {
    ::leftapplyPauliX(qureg.raw(), static_cast<int>(target));
}

void leftapply_pauli_y(Qureg& qureg, std::int32_t target) {
    ::leftapplyPauliY(qureg.raw(), static_cast<int>(target));
}

void leftapply_pauli_z(Qureg& qureg, std::int32_t target) {
    ::leftapplyPauliZ(qureg.raw(), static_cast<int>(target));
}

void leftapply_phase_gadget(Qureg& qureg, rust::Slice<const std::int32_t> targets, double angle) {
    ::leftapplyPhaseGadget(qureg.raw(), to_int_vec(targets), static_cast<qreal>(angle));
}

void leftapply_qubit_projector(Qureg& qureg, std::int32_t qubit, std::int32_t outcome) {
    ::leftapplyQubitProjector(qureg.raw(), static_cast<int>(qubit), static_cast<int>(outcome));
}

void leftapply_swap(Qureg& qureg, std::int32_t qubit1, std::int32_t qubit2) {
    ::leftapplySwap(qureg.raw(), static_cast<int>(qubit1), static_cast<int>(qubit2));
}

void mix_damping(Qureg& qureg, std::int32_t target, double prob) {
    ::mixDamping(qureg.raw(), static_cast<int>(target), static_cast<qreal>(prob));
}

void mix_depolarising(Qureg& qureg, std::int32_t target, double prob) {
    ::mixDepolarising(qureg.raw(), static_cast<int>(target), static_cast<qreal>(prob));
}

void mix_kraus_map(Qureg& qureg, rust::Slice<const std::int32_t> targets, const KrausMap& map) {
    ::mixKrausMap(qureg.raw(), to_int_vec(targets), map.raw());
}

void mix_paulis(Qureg& qureg, std::int32_t target, double probX, double probY, double probZ) {
    ::mixPaulis(qureg.raw(), static_cast<int>(target), static_cast<qreal>(probX), static_cast<qreal>(probY), static_cast<qreal>(probZ));
}

void mix_qureg(Qureg& qureg, const Qureg& other, double prob) {
    ::mixQureg(qureg.raw(), other.raw(), static_cast<qreal>(prob));
}

void mix_super_op(Qureg& qureg, rust::Slice<const std::int32_t> targets, const SuperOp& superop) {
    ::mixSuperOp(qureg.raw(), to_int_vec(targets), superop.raw());
}

void mix_two_qubit_dephasing(Qureg& qureg, std::int32_t target1, std::int32_t target2, double prob) {
    ::mixTwoQubitDephasing(qureg.raw(), static_cast<int>(target1), static_cast<int>(target2), static_cast<qreal>(prob));
}

void mix_two_qubit_depolarising(Qureg& qureg, std::int32_t target1, std::int32_t target2, double prob) {
    ::mixTwoQubitDepolarising(qureg.raw(), static_cast<int>(target1), static_cast<int>(target2), static_cast<qreal>(prob));
}

void report_comp_matr(const CompMatr& matrix) {
    ::reportCompMatr(matrix.raw());
}

void report_comp_matr1(const CompMatr1& matrix) {
    ::reportCompMatr1(matrix.raw());
}

void report_comp_matr2(const CompMatr2& matrix) {
    ::reportCompMatr2(matrix.raw());
}

void report_diag_matr(const DiagMatr& matrix) {
    ::reportDiagMatr(matrix.raw());
}

void report_diag_matr1(const DiagMatr1& matrix) {
    ::reportDiagMatr1(matrix.raw());
}

void report_diag_matr2(const DiagMatr2& matrix) {
    ::reportDiagMatr2(matrix.raw());
}

void report_full_state_diag_matr(const FullStateDiagMatr& matr) {
    ::reportFullStateDiagMatr(matr.raw());
}

void report_kraus_map(const KrausMap& map) {
    ::reportKrausMap(map.raw());
}

void report_pauli_str(const PauliStr& str_arg) {
    ::reportPauliStr(str_arg.raw());
}

void report_pauli_str_sum(const PauliStrSum& str_arg) {
    ::reportPauliStrSum(str_arg.raw());
}

void report_qureg(const Qureg& qureg) {
    ::reportQureg(qureg.raw());
}

void report_qureg_params(const Qureg& qureg) {
    ::reportQuregParams(qureg.raw());
}

void report_scalar(rust::Str label, GeneratedComplex num) {
    ::reportScalar(std::string(label.data(), label.size()), to_qcomp(num));
}

void report_str(rust::Str str_arg) {
    ::reportStr(std::string(str_arg.data(), str_arg.size()));
}

void report_super_op(const SuperOp& op) {
    ::reportSuperOp(op.raw());
}

void rightapply_comp_matr1(Qureg& qureg, std::int32_t target, const CompMatr1& matrix) {
    ::rightapplyCompMatr1(qureg.raw(), static_cast<int>(target), matrix.raw());
}

void rightapply_comp_matr2(Qureg& qureg, std::int32_t target1, std::int32_t target2, const CompMatr2& matrix) {
    ::rightapplyCompMatr2(qureg.raw(), static_cast<int>(target1), static_cast<int>(target2), matrix.raw());
}

void rightapply_diag_matr(Qureg& qureg, rust::Slice<const std::int32_t> targets, const DiagMatr& matrix) {
    ::rightapplyDiagMatr(qureg.raw(), to_int_vec(targets), matrix.raw());
}

void rightapply_diag_matr1(Qureg& qureg, std::int32_t target, const DiagMatr1& matrix) {
    ::rightapplyDiagMatr1(qureg.raw(), static_cast<int>(target), matrix.raw());
}

void rightapply_diag_matr2(Qureg& qureg, std::int32_t target1, std::int32_t target2, const DiagMatr2& matrix) {
    ::rightapplyDiagMatr2(qureg.raw(), static_cast<int>(target1), static_cast<int>(target2), matrix.raw());
}

void rightapply_diag_matr_power(Qureg& qureg, rust::Slice<const std::int32_t> targets, const DiagMatr& matrix, GeneratedComplex exponent) {
    ::rightapplyDiagMatrPower(qureg.raw(), to_int_vec(targets), matrix.raw(), to_qcomp(exponent));
}

void rightapply_full_state_diag_matr(Qureg& qureg, const FullStateDiagMatr& matrix) {
    ::rightapplyFullStateDiagMatr(qureg.raw(), matrix.raw());
}

void rightapply_full_state_diag_matr_power(Qureg& qureg, const FullStateDiagMatr& matrix, GeneratedComplex exponent) {
    ::rightapplyFullStateDiagMatrPower(qureg.raw(), matrix.raw(), to_qcomp(exponent));
}

void rightapply_multi_qubit_not(Qureg& qureg, rust::Slice<const std::int32_t> targets) {
    ::rightapplyMultiQubitNot(qureg.raw(), to_int_vec(targets));
}

void rightapply_multi_qubit_projector(Qureg& qureg, rust::Slice<const std::int32_t> qubits, rust::Slice<const std::int32_t> outcomes) {
    ::rightapplyMultiQubitProjector(qureg.raw(), to_int_vec(qubits), to_int_vec(outcomes));
}

void rightapply_pauli_gadget(Qureg& qureg, const PauliStr& str_arg, double angle) {
    ::rightapplyPauliGadget(qureg.raw(), str_arg.raw(), static_cast<qreal>(angle));
}

void rightapply_pauli_str(Qureg& qureg, const PauliStr& str_arg) {
    ::rightapplyPauliStr(qureg.raw(), str_arg.raw());
}

void rightapply_pauli_str_sum(Qureg& qureg, const PauliStrSum& sum, const Qureg& workspace) {
    ::rightapplyPauliStrSum(qureg.raw(), sum.raw(), workspace.raw());
}

void rightapply_pauli_x(Qureg& qureg, std::int32_t target) {
    ::rightapplyPauliX(qureg.raw(), static_cast<int>(target));
}

void rightapply_pauli_y(Qureg& qureg, std::int32_t target) {
    ::rightapplyPauliY(qureg.raw(), static_cast<int>(target));
}

void rightapply_pauli_z(Qureg& qureg, std::int32_t target) {
    ::rightapplyPauliZ(qureg.raw(), static_cast<int>(target));
}

void rightapply_phase_gadget(Qureg& qureg, rust::Slice<const std::int32_t> targets, double angle) {
    ::rightapplyPhaseGadget(qureg.raw(), to_int_vec(targets), static_cast<qreal>(angle));
}

void rightapply_qubit_projector(Qureg& qureg, std::int32_t qubit, std::int32_t outcome) {
    ::rightapplyQubitProjector(qureg.raw(), static_cast<int>(qubit), static_cast<int>(outcome));
}

void rightapply_swap(Qureg& qureg, std::int32_t qubit1, std::int32_t qubit2) {
    ::rightapplySwap(qureg.raw(), static_cast<int>(qubit1), static_cast<int>(qubit2));
}

void save_qureg_to_file(Qureg& qureg, rust::Str arg1) {
    ::saveQuregToFile(qureg.raw(), std::string(arg1.data(), arg1.size()));
}

void set_density_qureg_flat_amps(Qureg& qureg, std::int64_t startInd, rust::Slice<const GeneratedComplex> amps) {
    ::setDensityQuregFlatAmps(qureg.raw(), static_cast<qindex>(startInd), to_qcomp_vec(amps));
}

void set_diag_matr(DiagMatr& out_arg, rust::Slice<const GeneratedComplex> in_arg) {
    ::setDiagMatr(out_arg.raw(), to_qcomp_vec(in_arg));
}

void set_full_state_diag_matr(FullStateDiagMatr& out_arg, std::int64_t startInd, rust::Slice<const GeneratedComplex> in_arg) {
    ::setFullStateDiagMatr(out_arg.raw(), static_cast<qindex>(startInd), to_qcomp_vec(in_arg));
}

void set_full_state_diag_matr_from_pauli_str_sum(FullStateDiagMatr& out_arg, const PauliStrSum& in_arg) {
    ::setFullStateDiagMatrFromPauliStrSum(out_arg.raw(), in_arg.raw());
}

void set_inline_diag_matr(DiagMatr& matr, std::int32_t numQb, rust::Slice<const GeneratedComplex> in_arg) {
    ::setInlineDiagMatr(matr.raw(), static_cast<int>(numQb), to_qcomp_vec(in_arg));
}

void set_inline_full_state_diag_matr(FullStateDiagMatr& matr, std::int64_t startInd, std::int64_t numElems, rust::Slice<const GeneratedComplex> in_arg) {
    ::setInlineFullStateDiagMatr(matr.raw(), static_cast<qindex>(startInd), static_cast<qindex>(numElems), to_qcomp_vec(in_arg));
}

void set_qu_est_max_num_reported_items(std::int64_t numRows, std::int64_t numCols) {
    ::setQuESTMaxNumReportedItems(static_cast<qindex>(numRows), static_cast<qindex>(numCols));
}

void set_qu_est_max_num_reported_sig_figs(std::int32_t numSigFigs) {
    ::setQuESTMaxNumReportedSigFigs(static_cast<int>(numSigFigs));
}

void set_qu_est_num_gpu_threads_per_block(std::int32_t numThreadsPerBlock) {
    ::setQuESTNumGpuThreadsPerBlock(static_cast<int>(numThreadsPerBlock));
}

void set_qu_est_num_reported_newlines(std::int32_t numNewlines) {
    ::setQuESTNumReportedNewlines(static_cast<int>(numNewlines));
}

void set_qu_est_reported_pauli_chars(rust::Str paulis) {
    const std::string paulis_string(paulis.data(), paulis.size());
    ::setQuESTReportedPauliChars(paulis_string.c_str());
}

void set_qu_est_reported_pauli_str_style(std::int32_t style) {
    ::setQuESTReportedPauliStrStyle(static_cast<int>(style));
}

void set_qu_est_seeds_to_default() {
    ::setQuESTSeedsToDefault();
}

void set_qu_est_validation_epsilon(double eps) {
    ::setQuESTValidationEpsilon(static_cast<qreal>(eps));
}

void set_qu_est_validation_epsilon_to_default() {
    ::setQuESTValidationEpsilonToDefault();
}

void set_qu_est_validation_off() {
    ::setQuESTValidationOff();
}

void set_qu_est_validation_on() {
    ::setQuESTValidationOn();
}

void set_qureg_amps(Qureg& qureg, std::int64_t startInd, rust::Slice<const GeneratedComplex> amps) {
    ::setQuregAmps(qureg.raw(), static_cast<qindex>(startInd), to_qcomp_vec(amps));
}

void set_qureg_to_clone(Qureg& outQureg, const Qureg& inQureg) {
    ::setQuregToClone(outQureg.raw(), inQureg.raw());
}

void set_qureg_to_partial_trace(Qureg& out_arg, const Qureg& in_arg, rust::Slice<const std::int32_t> traceOutQubits) {
    ::setQuregToPartialTrace(out_arg.raw(), in_arg.raw(), to_int_vec(traceOutQubits));
}

void set_qureg_to_pauli_str_sum(Qureg& qureg, const PauliStrSum& sum) {
    ::setQuregToPauliStrSum(qureg.raw(), sum.raw());
}

void set_qureg_to_reduced_density_matrix(Qureg& out_arg, const Qureg& in_arg, rust::Slice<const std::int32_t> retainQubits) {
    ::setQuregToReducedDensityMatrix(out_arg.raw(), in_arg.raw(), to_int_vec(retainQubits));
}

double set_qureg_to_renormalized(Qureg& qureg) {
    return static_cast<double>(::setQuregToRenormalized(qureg.raw()));
}

void sort_pauli_str_sum_lexicographic(PauliStrSum& sum) {
    ::sortPauliStrSumLexicographic(sum.raw());
}

void sort_pauli_str_sum_magnitude(PauliStrSum& sum) {
    ::sortPauliStrSumMagnitude(sum.raw());
}

void sync_comp_matr(CompMatr& matr) {
    ::syncCompMatr(matr.raw());
}

void sync_diag_matr(DiagMatr& matr) {
    ::syncDiagMatr(matr.raw());
}

void sync_full_state_diag_matr(FullStateDiagMatr& matr) {
    ::syncFullStateDiagMatr(matr.raw());
}

void sync_kraus_map(KrausMap& map) {
    ::syncKrausMap(map.raw());
}

void sync_qureg_from_gpu(Qureg& qureg) {
    ::syncQuregFromGpu(qureg.raw());
}

void sync_qureg_to_gpu(Qureg& qureg) {
    ::syncQuregToGpu(qureg.raw());
}

void sync_sub_qureg_from_gpu(Qureg& qureg, std::int64_t localStartInd, std::int64_t numLocalAmps) {
    ::syncSubQuregFromGpu(qureg.raw(), static_cast<qindex>(localStartInd), static_cast<qindex>(numLocalAmps));
}

void sync_sub_qureg_to_gpu(Qureg& qureg, std::int64_t localStartInd, std::int64_t numLocalAmps) {
    ::syncSubQuregToGpu(qureg.raw(), static_cast<qindex>(localStartInd), static_cast<qindex>(numLocalAmps));
}

void sync_super_op(SuperOp& op) {
    ::syncSuperOp(op.raw());
}

} // namespace quest_sys
