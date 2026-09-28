#include "quest_generated_bindings.hpp"

#include "quest-sys/src/generated_api.rs.h"

#include <algorithm>
#include <cmath>
#include <complex>
#include <cstddef>
#include <cstdint>
#include <limits>
#include <memory>
#include <stdexcept>
#include <string>
#include <vector>

namespace quest_sys {
namespace {

[[maybe_unused]] qcomp to_qcomp(const QuestComplex& value) {
  return qcomp(value.re, value.im);
}

[[maybe_unused]] QuestComplex from_qcomp(qcomp value) {
  return QuestComplex{static_cast<double>(std::real(value)),
                      static_cast<double>(std::imag(value))};
}

[[maybe_unused]] std::vector<int> to_int_vec(
    rust::Slice<const std::int32_t> values) {
  std::vector<int> out;
  out.reserve(values.size());
  for (const auto value : values) {
    out.push_back(static_cast<int>(value));
  }
  return out;
}

[[maybe_unused]] std::vector<unsigned> to_uint_vec(
    rust::Slice<const std::uint32_t> values) {
  std::vector<unsigned> out;
  out.reserve(values.size());
  for (const auto value : values) {
    out.push_back(static_cast<unsigned>(value));
  }
  return out;
}

[[maybe_unused]] std::vector<qreal> to_qreal_vec(
    rust::Slice<const double> values) {
  std::vector<qreal> out;
  out.reserve(values.size());
  for (const auto value : values) {
    out.push_back(static_cast<qreal>(value));
  }
  return out;
}

[[maybe_unused]] std::vector<qcomp> to_qcomp_vec(
    rust::Slice<const QuestComplex> values) {
  std::vector<qcomp> out;
  out.reserve(values.size());
  for (const auto& value : values) {
    out.push_back(to_qcomp(value));
  }
  return out;
}

[[maybe_unused]] rust::Vec<double> from_qreal_vec(
    const std::vector<qreal>& values) {
  rust::Vec<double> out;
  out.reserve(values.size());
  for (const auto value : values) {
    out.push_back(static_cast<double>(value));
  }
  return out;
}

[[maybe_unused]] rust::Vec<std::uint32_t> from_uint_vec(
    const std::vector<unsigned>& values) {
  rust::Vec<std::uint32_t> out;
  out.reserve(values.size());
  for (const auto value : values) {
    out.push_back(static_cast<std::uint32_t>(value));
  }
  return out;
}

}  // namespace

void apply_comp_matr1(Qureg& qureg,
                      std::int32_t target,
                      const CompMatr1& matrix) {
  const auto admission = admit_native_call();
  ::applyCompMatr1(qureg.raw(), static_cast<int>(target), matrix.raw());
}

void apply_comp_matr2(Qureg& qureg,
                      std::int32_t target1,
                      std::int32_t target2,
                      const CompMatr2& matrix) {
  const auto admission = admit_native_call();
  ::applyCompMatr2(qureg.raw(), static_cast<int>(target1),
                   static_cast<int>(target2), matrix.raw());
}

void apply_controlled_comp_matr(Qureg& qureg,
                                std::int32_t control,
                                rust::Slice<const std::int32_t> targets,
                                const CompMatr& matr) {
  const auto admission = admit_native_call();
  ::applyControlledCompMatr(qureg.raw(), static_cast<int>(control),
                            to_int_vec(targets), matr.raw());
}

void apply_controlled_comp_matr1(Qureg& qureg,
                                 std::int32_t control,
                                 std::int32_t target,
                                 const CompMatr1& matrix) {
  const auto admission = admit_native_call();
  ::applyControlledCompMatr1(qureg.raw(), static_cast<int>(control),
                             static_cast<int>(target), matrix.raw());
}

void apply_controlled_comp_matr2(Qureg& qureg,
                                 std::int32_t control,
                                 std::int32_t target1,
                                 std::int32_t target2,
                                 const CompMatr2& matr) {
  const auto admission = admit_native_call();
  ::applyControlledCompMatr2(qureg.raw(), static_cast<int>(control),
                             static_cast<int>(target1),
                             static_cast<int>(target2), matr.raw());
}

void apply_controlled_diag_matr(Qureg& qureg,
                                std::int32_t control,
                                rust::Slice<const std::int32_t> targets,
                                const DiagMatr& matrix) {
  const auto admission = admit_native_call();
  ::applyControlledDiagMatr(qureg.raw(), static_cast<int>(control),
                            to_int_vec(targets), matrix.raw());
}

void apply_controlled_diag_matr1(Qureg& qureg,
                                 std::int32_t control,
                                 std::int32_t target,
                                 const DiagMatr1& matr) {
  const auto admission = admit_native_call();
  ::applyControlledDiagMatr1(qureg.raw(), static_cast<int>(control),
                             static_cast<int>(target), matr.raw());
}

void apply_controlled_diag_matr2(Qureg& qureg,
                                 std::int32_t control,
                                 std::int32_t target1,
                                 std::int32_t target2,
                                 const DiagMatr2& matr) {
  const auto admission = admit_native_call();
  ::applyControlledDiagMatr2(qureg.raw(), static_cast<int>(control),
                             static_cast<int>(target1),
                             static_cast<int>(target2), matr.raw());
}

void apply_controlled_diag_matr_power(Qureg& qureg,
                                      std::int32_t control,
                                      rust::Slice<const std::int32_t> targets,
                                      const DiagMatr& matrix,
                                      QuestComplex exponent) {
  const auto admission = admit_native_call();
  ::applyControlledDiagMatrPower(qureg.raw(), static_cast<int>(control),
                                 to_int_vec(targets), matrix.raw(),
                                 to_qcomp(exponent));
}

void apply_controlled_hadamard(Qureg& qureg,
                               std::int32_t control,
                               std::int32_t target) {
  const auto admission = admit_native_call();
  ::applyControlledHadamard(qureg.raw(), static_cast<int>(control),
                            static_cast<int>(target));
}

void apply_controlled_multi_qubit_not(Qureg& qureg,
                                      std::int32_t control,
                                      rust::Slice<const std::int32_t> targets) {
  const auto admission = admit_native_call();
  ::applyControlledMultiQubitNot(qureg.raw(), static_cast<int>(control),
                                 to_int_vec(targets));
}

void apply_controlled_pauli_gadget(Qureg& qureg,
                                   std::int32_t control,
                                   const PauliStr& str_arg,
                                   double angle) {
  const auto admission = admit_native_call();
  ::applyControlledPauliGadget(qureg.raw(), static_cast<int>(control),
                               str_arg.raw(), static_cast<qreal>(angle));
}

void apply_controlled_pauli_str(Qureg& qureg,
                                std::int32_t control,
                                const PauliStr& str_arg) {
  const auto admission = admit_native_call();
  ::applyControlledPauliStr(qureg.raw(), static_cast<int>(control),
                            str_arg.raw());
}

void apply_controlled_pauli_x(Qureg& qureg,
                              std::int32_t control,
                              std::int32_t target) {
  const auto admission = admit_native_call();
  ::applyControlledPauliX(qureg.raw(), static_cast<int>(control),
                          static_cast<int>(target));
}

void apply_controlled_pauli_y(Qureg& qureg,
                              std::int32_t control,
                              std::int32_t target) {
  const auto admission = admit_native_call();
  ::applyControlledPauliY(qureg.raw(), static_cast<int>(control),
                          static_cast<int>(target));
}

void apply_controlled_pauli_z(Qureg& qureg,
                              std::int32_t control,
                              std::int32_t target) {
  const auto admission = admit_native_call();
  ::applyControlledPauliZ(qureg.raw(), static_cast<int>(control),
                          static_cast<int>(target));
}

void apply_controlled_phase_gadget(Qureg& qureg,
                                   std::int32_t control,
                                   rust::Slice<const std::int32_t> targets,
                                   double angle) {
  const auto admission = admit_native_call();
  ::applyControlledPhaseGadget(qureg.raw(), static_cast<int>(control),
                               to_int_vec(targets), static_cast<qreal>(angle));
}

void apply_controlled_rotate_around_axis(Qureg& qureg,
                                         std::int32_t ctrl,
                                         std::int32_t targ,
                                         double angle,
                                         double axisX,
                                         double axisY,
                                         double axisZ) {
  const auto admission = admit_native_call();
  ::applyControlledRotateAroundAxis(
      qureg.raw(), static_cast<int>(ctrl), static_cast<int>(targ),
      static_cast<qreal>(angle), static_cast<qreal>(axisX),
      static_cast<qreal>(axisY), static_cast<qreal>(axisZ));
}

void apply_controlled_rotate_x(Qureg& qureg,
                               std::int32_t control,
                               std::int32_t target,
                               double angle) {
  const auto admission = admit_native_call();
  ::applyControlledRotateX(qureg.raw(), static_cast<int>(control),
                           static_cast<int>(target), static_cast<qreal>(angle));
}

void apply_controlled_rotate_y(Qureg& qureg,
                               std::int32_t control,
                               std::int32_t target,
                               double angle) {
  const auto admission = admit_native_call();
  ::applyControlledRotateY(qureg.raw(), static_cast<int>(control),
                           static_cast<int>(target), static_cast<qreal>(angle));
}

void apply_controlled_rotate_z(Qureg& qureg,
                               std::int32_t control,
                               std::int32_t target,
                               double angle) {
  const auto admission = admit_native_call();
  ::applyControlledRotateZ(qureg.raw(), static_cast<int>(control),
                           static_cast<int>(target), static_cast<qreal>(angle));
}

void apply_controlled_s(Qureg& qureg,
                        std::int32_t control,
                        std::int32_t target) {
  const auto admission = admit_native_call();
  ::applyControlledS(qureg.raw(), static_cast<int>(control),
                     static_cast<int>(target));
}

void apply_controlled_sqrt_swap(Qureg& qureg,
                                std::int32_t control,
                                std::int32_t qubit1,
                                std::int32_t qubit2) {
  const auto admission = admit_native_call();
  ::applyControlledSqrtSwap(qureg.raw(), static_cast<int>(control),
                            static_cast<int>(qubit1), static_cast<int>(qubit2));
}

void apply_controlled_swap(Qureg& qureg,
                           std::int32_t control,
                           std::int32_t qubit1,
                           std::int32_t qubit2) {
  const auto admission = admit_native_call();
  ::applyControlledSwap(qureg.raw(), static_cast<int>(control),
                        static_cast<int>(qubit1), static_cast<int>(qubit2));
}

void apply_controlled_t(Qureg& qureg,
                        std::int32_t control,
                        std::int32_t target) {
  const auto admission = admit_native_call();
  ::applyControlledT(qureg.raw(), static_cast<int>(control),
                     static_cast<int>(target));
}

void apply_diag_matr(Qureg& qureg,
                     rust::Slice<const std::int32_t> targets,
                     const DiagMatr& matrix) {
  const auto admission = admit_native_call();
  ::applyDiagMatr(qureg.raw(), to_int_vec(targets), matrix.raw());
}

void apply_diag_matr1(Qureg& qureg,
                      std::int32_t target,
                      const DiagMatr1& matr) {
  const auto admission = admit_native_call();
  ::applyDiagMatr1(qureg.raw(), static_cast<int>(target), matr.raw());
}

void apply_diag_matr2(Qureg& qureg,
                      std::int32_t target1,
                      std::int32_t target2,
                      const DiagMatr2& matr) {
  const auto admission = admit_native_call();
  ::applyDiagMatr2(qureg.raw(), static_cast<int>(target1),
                   static_cast<int>(target2), matr.raw());
}

void apply_diag_matr_power(Qureg& qureg,
                           rust::Slice<const std::int32_t> targets,
                           const DiagMatr& matrix,
                           QuestComplex exponent) {
  const auto admission = admit_native_call();
  ::applyDiagMatrPower(qureg.raw(), to_int_vec(targets), matrix.raw(),
                       to_qcomp(exponent));
}

double apply_forced_multi_qubit_measurement(
    Qureg& qureg,
    rust::Slice<const std::int32_t> qubits,
    rust::Slice<const std::int32_t> outcomes) {
  const auto admission = admit_native_call();
  return static_cast<double>(::applyForcedMultiQubitMeasurement(
      qureg.raw(), to_int_vec(qubits), to_int_vec(outcomes)));
}

double apply_forced_qubit_measurement(Qureg& qureg,
                                      std::int32_t target,
                                      std::int32_t outcome) {
  const auto admission = admit_native_call();
  return static_cast<double>(::applyForcedQubitMeasurement(
      qureg.raw(), static_cast<int>(target), static_cast<int>(outcome)));
}

void apply_full_quantum_fourier_transform(Qureg& qureg, bool inverse) {
  const auto admission = admit_native_call();
  ::applyFullQuantumFourierTransform(qureg.raw(), inverse);
}

void apply_full_state_diag_matr(Qureg& qureg, const FullStateDiagMatr& matrix) {
  const auto admission = admit_native_call();
  ::applyFullStateDiagMatr(qureg.raw(), matrix.raw());
}

void apply_full_state_diag_matr_power(Qureg& qureg,
                                      const FullStateDiagMatr& matrix,
                                      QuestComplex exponent) {
  const auto admission = admit_native_call();
  ::applyFullStateDiagMatrPower(qureg.raw(), matrix.raw(), to_qcomp(exponent));
}

void apply_multi_controlled_comp_matr(Qureg& qureg,
                                      rust::Slice<const std::int32_t> controls,
                                      rust::Slice<const std::int32_t> targets,
                                      const CompMatr& matr) {
  const auto admission = admit_native_call();
  ::applyMultiControlledCompMatr(qureg.raw(), to_int_vec(controls),
                                 to_int_vec(targets), matr.raw());
}

void apply_multi_controlled_comp_matr1(Qureg& qureg,
                                       rust::Slice<const std::int32_t> controls,
                                       std::int32_t target,
                                       const CompMatr1& matrix) {
  const auto admission = admit_native_call();
  ::applyMultiControlledCompMatr1(qureg.raw(), to_int_vec(controls),
                                  static_cast<int>(target), matrix.raw());
}

void apply_multi_controlled_comp_matr2(Qureg& qureg,
                                       rust::Slice<const std::int32_t> controls,
                                       std::int32_t target1,
                                       std::int32_t target2,
                                       const CompMatr2& matr) {
  const auto admission = admit_native_call();
  ::applyMultiControlledCompMatr2(qureg.raw(), to_int_vec(controls),
                                  static_cast<int>(target1),
                                  static_cast<int>(target2), matr.raw());
}

void apply_multi_controlled_diag_matr(Qureg& qureg,
                                      rust::Slice<const std::int32_t> controls,
                                      rust::Slice<const std::int32_t> targets,
                                      const DiagMatr& matrix) {
  const auto admission = admit_native_call();
  ::applyMultiControlledDiagMatr(qureg.raw(), to_int_vec(controls),
                                 to_int_vec(targets), matrix.raw());
}

void apply_multi_controlled_diag_matr1(Qureg& qureg,
                                       rust::Slice<const std::int32_t> controls,
                                       std::int32_t target,
                                       const DiagMatr1& matr) {
  const auto admission = admit_native_call();
  ::applyMultiControlledDiagMatr1(qureg.raw(), to_int_vec(controls),
                                  static_cast<int>(target), matr.raw());
}

void apply_multi_controlled_diag_matr2(Qureg& qureg,
                                       rust::Slice<const std::int32_t> controls,
                                       std::int32_t target1,
                                       std::int32_t target2,
                                       const DiagMatr2& matr) {
  const auto admission = admit_native_call();
  ::applyMultiControlledDiagMatr2(qureg.raw(), to_int_vec(controls),
                                  static_cast<int>(target1),
                                  static_cast<int>(target2), matr.raw());
}

void apply_multi_controlled_diag_matr_power(
    Qureg& qureg,
    rust::Slice<const std::int32_t> controls,
    rust::Slice<const std::int32_t> targets,
    const DiagMatr& matrix,
    QuestComplex exponent) {
  const auto admission = admit_native_call();
  ::applyMultiControlledDiagMatrPower(qureg.raw(), to_int_vec(controls),
                                      to_int_vec(targets), matrix.raw(),
                                      to_qcomp(exponent));
}

void apply_multi_controlled_hadamard(Qureg& qureg,
                                     rust::Slice<const std::int32_t> controls,
                                     std::int32_t target) {
  const auto admission = admit_native_call();
  ::applyMultiControlledHadamard(qureg.raw(), to_int_vec(controls),
                                 static_cast<int>(target));
}

void apply_multi_controlled_multi_qubit_not(
    Qureg& qureg,
    rust::Slice<const std::int32_t> controls,
    rust::Slice<const std::int32_t> targets) {
  const auto admission = admit_native_call();
  ::applyMultiControlledMultiQubitNot(qureg.raw(), to_int_vec(controls),
                                      to_int_vec(targets));
}

void apply_multi_controlled_pauli_gadget(
    Qureg& qureg,
    rust::Slice<const std::int32_t> controls,
    const PauliStr& str_arg,
    double angle) {
  const auto admission = admit_native_call();
  ::applyMultiControlledPauliGadget(qureg.raw(), to_int_vec(controls),
                                    str_arg.raw(), static_cast<qreal>(angle));
}

void apply_multi_controlled_pauli_str(Qureg& qureg,
                                      rust::Slice<const std::int32_t> controls,
                                      const PauliStr& str_arg) {
  const auto admission = admit_native_call();
  ::applyMultiControlledPauliStr(qureg.raw(), to_int_vec(controls),
                                 str_arg.raw());
}

// @pauli-multi-cpp@

void apply_multi_controlled_phase_gadget(
    Qureg& qureg,
    rust::Slice<const std::int32_t> controls,
    rust::Slice<const std::int32_t> targets,
    double angle) {
  const auto admission = admit_native_call();
  ::applyMultiControlledPhaseGadget(qureg.raw(), to_int_vec(controls),
                                    to_int_vec(targets),
                                    static_cast<qreal>(angle));
}

void apply_multi_controlled_rotate_around_axis(
    Qureg& qureg,
    rust::Slice<const std::int32_t> ctrls,
    std::int32_t targ,
    double angle,
    double axisX,
    double axisY,
    double axisZ) {
  const auto admission = admit_native_call();
  ::applyMultiControlledRotateAroundAxis(
      qureg.raw(), to_int_vec(ctrls), static_cast<int>(targ),
      static_cast<qreal>(angle), static_cast<qreal>(axisX),
      static_cast<qreal>(axisY), static_cast<qreal>(axisZ));
}

void apply_multi_controlled_rotate_x(Qureg& qureg,
                                     rust::Slice<const std::int32_t> controls,
                                     std::int32_t target,
                                     double angle) {
  const auto admission = admit_native_call();
  ::applyMultiControlledRotateX(qureg.raw(), to_int_vec(controls),
                                static_cast<int>(target),
                                static_cast<qreal>(angle));
}

void apply_multi_controlled_rotate_y(Qureg& qureg,
                                     rust::Slice<const std::int32_t> controls,
                                     std::int32_t target,
                                     double angle) {
  const auto admission = admit_native_call();
  ::applyMultiControlledRotateY(qureg.raw(), to_int_vec(controls),
                                static_cast<int>(target),
                                static_cast<qreal>(angle));
}

void apply_multi_controlled_rotate_z(Qureg& qureg,
                                     rust::Slice<const std::int32_t> controls,
                                     std::int32_t target,
                                     double angle) {
  const auto admission = admit_native_call();
  ::applyMultiControlledRotateZ(qureg.raw(), to_int_vec(controls),
                                static_cast<int>(target),
                                static_cast<qreal>(angle));
}

void apply_multi_controlled_s(Qureg& qureg,
                              rust::Slice<const std::int32_t> controls,
                              std::int32_t target) {
  const auto admission = admit_native_call();
  ::applyMultiControlledS(qureg.raw(), to_int_vec(controls),
                          static_cast<int>(target));
}

void apply_multi_controlled_sqrt_swap(Qureg& qureg,
                                      rust::Slice<const std::int32_t> controls,
                                      std::int32_t qubit1,
                                      std::int32_t qubit2) {
  const auto admission = admit_native_call();
  ::applyMultiControlledSqrtSwap(qureg.raw(), to_int_vec(controls),
                                 static_cast<int>(qubit1),
                                 static_cast<int>(qubit2));
}

void apply_multi_controlled_swap(Qureg& qureg,
                                 rust::Slice<const std::int32_t> controls,
                                 std::int32_t qubit1,
                                 std::int32_t qubit2) {
  const auto admission = admit_native_call();
  ::applyMultiControlledSwap(qureg.raw(), to_int_vec(controls),
                             static_cast<int>(qubit1),
                             static_cast<int>(qubit2));
}

void apply_multi_controlled_t(Qureg& qureg,
                              rust::Slice<const std::int32_t> controls,
                              std::int32_t target) {
  const auto admission = admit_native_call();
  ::applyMultiControlledT(qureg.raw(), to_int_vec(controls),
                          static_cast<int>(target));
}

std::int64_t apply_multi_qubit_measurement(
    Qureg& qureg,
    rust::Slice<const std::int32_t> qubits) {
  const auto admission = admit_native_call();
  return static_cast<std::int64_t>(
      ::applyMultiQubitMeasurement(qureg.raw(), to_int_vec(qubits)));
}

void apply_multi_qubit_not(Qureg& qureg,
                           rust::Slice<const std::int32_t> targets) {
  const auto admission = admit_native_call();
  ::applyMultiQubitNot(qureg.raw(), to_int_vec(targets));
}

void apply_multi_qubit_phase_flip(Qureg& qureg,
                                  rust::Slice<const std::int32_t> targets) {
  const auto admission = admit_native_call();
  ::applyMultiQubitPhaseFlip(qureg.raw(), to_int_vec(targets));
}

void apply_multi_qubit_phase_shift(Qureg& qureg,
                                   rust::Slice<const std::int32_t> targets,
                                   double angle) {
  const auto admission = admit_native_call();
  ::applyMultiQubitPhaseShift(qureg.raw(), to_int_vec(targets),
                              static_cast<qreal>(angle));
}

void apply_multi_qubit_projector(Qureg& qureg,
                                 rust::Slice<const std::int32_t> qubits,
                                 rust::Slice<const std::int32_t> outcomes) {
  const auto admission = admit_native_call();
  ::applyMultiQubitProjector(qureg.raw(), to_int_vec(qubits),
                             to_int_vec(outcomes));
}

void apply_multi_state_controlled_comp_matr(
    Qureg& qureg,
    rust::Slice<const std::int32_t> controls,
    rust::Slice<const std::int32_t> states,
    rust::Slice<const std::int32_t> targets,
    const CompMatr& matr) {
  const auto admission = admit_native_call();
  ::applyMultiStateControlledCompMatr(qureg.raw(), to_int_vec(controls),
                                      to_int_vec(states), to_int_vec(targets),
                                      matr.raw());
}

void apply_multi_state_controlled_comp_matr1(
    Qureg& qureg,
    rust::Slice<const std::int32_t> controls,
    rust::Slice<const std::int32_t> states,
    std::int32_t target,
    const CompMatr1& matrix) {
  const auto admission = admit_native_call();
  ::applyMultiStateControlledCompMatr1(qureg.raw(), to_int_vec(controls),
                                       to_int_vec(states),
                                       static_cast<int>(target), matrix.raw());
}

void apply_multi_state_controlled_comp_matr2(
    Qureg& qureg,
    rust::Slice<const std::int32_t> controls,
    rust::Slice<const std::int32_t> states,
    std::int32_t target1,
    std::int32_t target2,
    const CompMatr2& matr) {
  const auto admission = admit_native_call();
  ::applyMultiStateControlledCompMatr2(
      qureg.raw(), to_int_vec(controls), to_int_vec(states),
      static_cast<int>(target1), static_cast<int>(target2), matr.raw());
}

void apply_multi_state_controlled_diag_matr(
    Qureg& qureg,
    rust::Slice<const std::int32_t> controls,
    rust::Slice<const std::int32_t> states,
    rust::Slice<const std::int32_t> targets,
    const DiagMatr& matrix) {
  const auto admission = admit_native_call();
  ::applyMultiStateControlledDiagMatr(qureg.raw(), to_int_vec(controls),
                                      to_int_vec(states), to_int_vec(targets),
                                      matrix.raw());
}

void apply_multi_state_controlled_diag_matr1(
    Qureg& qureg,
    rust::Slice<const std::int32_t> controls,
    rust::Slice<const std::int32_t> states,
    std::int32_t target,
    const DiagMatr1& matr) {
  const auto admission = admit_native_call();
  ::applyMultiStateControlledDiagMatr1(qureg.raw(), to_int_vec(controls),
                                       to_int_vec(states),
                                       static_cast<int>(target), matr.raw());
}

void apply_multi_state_controlled_diag_matr2(
    Qureg& qureg,
    rust::Slice<const std::int32_t> controls,
    rust::Slice<const std::int32_t> states,
    std::int32_t target1,
    std::int32_t target2,
    const DiagMatr2& matr) {
  const auto admission = admit_native_call();
  ::applyMultiStateControlledDiagMatr2(
      qureg.raw(), to_int_vec(controls), to_int_vec(states),
      static_cast<int>(target1), static_cast<int>(target2), matr.raw());
}

void apply_multi_state_controlled_diag_matr_power(
    Qureg& qureg,
    rust::Slice<const std::int32_t> controls,
    rust::Slice<const std::int32_t> states,
    rust::Slice<const std::int32_t> targets,
    const DiagMatr& matrix,
    QuestComplex exponent) {
  const auto admission = admit_native_call();
  ::applyMultiStateControlledDiagMatrPower(
      qureg.raw(), to_int_vec(controls), to_int_vec(states),
      to_int_vec(targets), matrix.raw(), to_qcomp(exponent));
}

void apply_multi_state_controlled_hadamard(
    Qureg& qureg,
    rust::Slice<const std::int32_t> controls,
    rust::Slice<const std::int32_t> states,
    std::int32_t target) {
  const auto admission = admit_native_call();
  ::applyMultiStateControlledHadamard(qureg.raw(), to_int_vec(controls),
                                      to_int_vec(states),
                                      static_cast<int>(target));
}

void apply_multi_state_controlled_multi_qubit_not(
    Qureg& qureg,
    rust::Slice<const std::int32_t> controls,
    rust::Slice<const std::int32_t> states,
    rust::Slice<const std::int32_t> targets) {
  const auto admission = admit_native_call();
  ::applyMultiStateControlledMultiQubitNot(qureg.raw(), to_int_vec(controls),
                                           to_int_vec(states),
                                           to_int_vec(targets));
}

void apply_multi_state_controlled_pauli_gadget(
    Qureg& qureg,
    rust::Slice<const std::int32_t> controls,
    rust::Slice<const std::int32_t> states,
    const PauliStr& str_arg,
    double angle) {
  const auto admission = admit_native_call();
  ::applyMultiStateControlledPauliGadget(qureg.raw(), to_int_vec(controls),
                                         to_int_vec(states), str_arg.raw(),
                                         static_cast<qreal>(angle));
}

void apply_multi_state_controlled_pauli_str(
    Qureg& qureg,
    rust::Slice<const std::int32_t> controls,
    rust::Slice<const std::int32_t> states,
    const PauliStr& str_arg) {
  const auto admission = admit_native_call();
  ::applyMultiStateControlledPauliStr(qureg.raw(), to_int_vec(controls),
                                      to_int_vec(states), str_arg.raw());
}

// @pauli-state-cpp@

void apply_multi_state_controlled_phase_gadget(
    Qureg& qureg,
    rust::Slice<const std::int32_t> controls,
    rust::Slice<const std::int32_t> states,
    rust::Slice<const std::int32_t> targets,
    double angle) {
  const auto admission = admit_native_call();
  ::applyMultiStateControlledPhaseGadget(
      qureg.raw(), to_int_vec(controls), to_int_vec(states),
      to_int_vec(targets), static_cast<qreal>(angle));
}

void apply_multi_state_controlled_rotate_around_axis(
    Qureg& qureg,
    rust::Slice<const std::int32_t> ctrls,
    rust::Slice<const std::int32_t> states,
    std::int32_t targ,
    double angle,
    double axisX,
    double axisY,
    double axisZ) {
  const auto admission = admit_native_call();
  ::applyMultiStateControlledRotateAroundAxis(
      qureg.raw(), to_int_vec(ctrls), to_int_vec(states),
      static_cast<int>(targ), static_cast<qreal>(angle),
      static_cast<qreal>(axisX), static_cast<qreal>(axisY),
      static_cast<qreal>(axisZ));
}

void apply_multi_state_controlled_rotate_x(
    Qureg& qureg,
    rust::Slice<const std::int32_t> controls,
    rust::Slice<const std::int32_t> states,
    std::int32_t target,
    double angle) {
  const auto admission = admit_native_call();
  ::applyMultiStateControlledRotateX(
      qureg.raw(), to_int_vec(controls), to_int_vec(states),
      static_cast<int>(target), static_cast<qreal>(angle));
}

void apply_multi_state_controlled_rotate_y(
    Qureg& qureg,
    rust::Slice<const std::int32_t> controls,
    rust::Slice<const std::int32_t> states,
    std::int32_t target,
    double angle) {
  const auto admission = admit_native_call();
  ::applyMultiStateControlledRotateY(
      qureg.raw(), to_int_vec(controls), to_int_vec(states),
      static_cast<int>(target), static_cast<qreal>(angle));
}

void apply_multi_state_controlled_rotate_z(
    Qureg& qureg,
    rust::Slice<const std::int32_t> controls,
    rust::Slice<const std::int32_t> states,
    std::int32_t target,
    double angle) {
  const auto admission = admit_native_call();
  ::applyMultiStateControlledRotateZ(
      qureg.raw(), to_int_vec(controls), to_int_vec(states),
      static_cast<int>(target), static_cast<qreal>(angle));
}

void apply_multi_state_controlled_s(Qureg& qureg,
                                    rust::Slice<const std::int32_t> controls,
                                    rust::Slice<const std::int32_t> states,
                                    std::int32_t target) {
  const auto admission = admit_native_call();
  ::applyMultiStateControlledS(qureg.raw(), to_int_vec(controls),
                               to_int_vec(states), static_cast<int>(target));
}

void apply_multi_state_controlled_sqrt_swap(
    Qureg& qureg,
    rust::Slice<const std::int32_t> controls,
    rust::Slice<const std::int32_t> states,
    std::int32_t qubit1,
    std::int32_t qubit2) {
  const auto admission = admit_native_call();
  ::applyMultiStateControlledSqrtSwap(
      qureg.raw(), to_int_vec(controls), to_int_vec(states),
      static_cast<int>(qubit1), static_cast<int>(qubit2));
}

void apply_multi_state_controlled_swap(Qureg& qureg,
                                       rust::Slice<const std::int32_t> controls,
                                       rust::Slice<const std::int32_t> states,
                                       std::int32_t qubit1,
                                       std::int32_t qubit2) {
  const auto admission = admit_native_call();
  ::applyMultiStateControlledSwap(qureg.raw(), to_int_vec(controls),
                                  to_int_vec(states), static_cast<int>(qubit1),
                                  static_cast<int>(qubit2));
}

void apply_multi_state_controlled_t(Qureg& qureg,
                                    rust::Slice<const std::int32_t> controls,
                                    rust::Slice<const std::int32_t> states,
                                    std::int32_t target) {
  const auto admission = admit_native_call();
  ::applyMultiStateControlledT(qureg.raw(), to_int_vec(controls),
                               to_int_vec(states), static_cast<int>(target));
}

void apply_non_unitary_pauli_gadget(Qureg& qureg,
                                    const PauliStr& str_arg,
                                    QuestComplex angle) {
  const auto admission = admit_native_call();
  ::applyNonUnitaryPauliGadget(qureg.raw(), str_arg.raw(), to_qcomp(angle));
}

void apply_pauli_gadget(Qureg& qureg, const PauliStr& str_arg, double angle) {
  const auto admission = admit_native_call();
  ::applyPauliGadget(qureg.raw(), str_arg.raw(), static_cast<qreal>(angle));
}

void apply_pauli_str(Qureg& qureg, const PauliStr& str_arg) {
  const auto admission = admit_native_call();
  ::applyPauliStr(qureg.raw(), str_arg.raw());
}

void apply_phase_flip(Qureg& qureg, std::int32_t target) {
  const auto admission = admit_native_call();
  ::applyPhaseFlip(qureg.raw(), static_cast<int>(target));
}

void apply_phase_gadget(Qureg& qureg,
                        rust::Slice<const std::int32_t> targets,
                        double angle) {
  const auto admission = admit_native_call();
  ::applyPhaseGadget(qureg.raw(), to_int_vec(targets),
                     static_cast<qreal>(angle));
}

void apply_phase_shift(Qureg& qureg, std::int32_t target, double angle) {
  const auto admission = admit_native_call();
  ::applyPhaseShift(qureg.raw(), static_cast<int>(target),
                    static_cast<qreal>(angle));
}

void apply_quantum_fourier_transform(Qureg& qureg,
                                     rust::Slice<const std::int32_t> targets,
                                     bool inverse) {
  const auto admission = admit_native_call();
  ::applyQuantumFourierTransform(qureg.raw(), to_int_vec(targets), inverse);
}

void apply_qubit_projector(Qureg& qureg,
                           std::int32_t target,
                           std::int32_t outcome) {
  const auto admission = admit_native_call();
  ::applyQubitProjector(qureg.raw(), static_cast<int>(target),
                        static_cast<int>(outcome));
}

void apply_rotate_around_axis(Qureg& qureg,
                              std::int32_t target,
                              double angle,
                              double axisX,
                              double axisY,
                              double axisZ) {
  const auto admission = admit_native_call();
  ::applyRotateAroundAxis(qureg.raw(), static_cast<int>(target),
                          static_cast<qreal>(angle), static_cast<qreal>(axisX),
                          static_cast<qreal>(axisY), static_cast<qreal>(axisZ));
}

void apply_rotate_x(Qureg& qureg, std::int32_t target, double angle) {
  const auto admission = admit_native_call();
  ::applyRotateX(qureg.raw(), static_cast<int>(target),
                 static_cast<qreal>(angle));
}

void apply_rotate_y(Qureg& qureg, std::int32_t target, double angle) {
  const auto admission = admit_native_call();
  ::applyRotateY(qureg.raw(), static_cast<int>(target),
                 static_cast<qreal>(angle));
}

void apply_rotate_z(Qureg& qureg, std::int32_t target, double angle) {
  const auto admission = admit_native_call();
  ::applyRotateZ(qureg.raw(), static_cast<int>(target),
                 static_cast<qreal>(angle));
}

void apply_s(Qureg& qureg, std::int32_t target) {
  const auto admission = admit_native_call();
  ::applyS(qureg.raw(), static_cast<int>(target));
}

void apply_sqrt_swap(Qureg& qureg, std::int32_t qubit1, std::int32_t qubit2) {
  const auto admission = admit_native_call();
  ::applySqrtSwap(qureg.raw(), static_cast<int>(qubit1),
                  static_cast<int>(qubit2));
}

void apply_swap(Qureg& qureg, std::int32_t qubit1, std::int32_t qubit2) {
  const auto admission = admit_native_call();
  ::applySwap(qureg.raw(), static_cast<int>(qubit1), static_cast<int>(qubit2));
}

void apply_t(Qureg& qureg, std::int32_t target) {
  const auto admission = admit_native_call();
  ::applyT(qureg.raw(), static_cast<int>(target));
}

void apply_trotterized_controlled_pauli_str_sum_gadget(Qureg& qureg,
                                                       std::int32_t control,
                                                       const PauliStrSum& sum,
                                                       double angle,
                                                       std::int32_t order,
                                                       std::int32_t reps,
                                                       bool permuteTerms) {
  const auto admission = admit_native_call();
  ::applyTrotterizedControlledPauliStrSumGadget(
      qureg.raw(), static_cast<int>(control), sum.raw(),
      static_cast<qreal>(angle), static_cast<int>(order),
      static_cast<int>(reps), permuteTerms);
}

void apply_trotterized_imaginary_time_evolution(Qureg& qureg,
                                                const PauliStrSum& hamil,
                                                double tau,
                                                std::int32_t order,
                                                std::int32_t reps,
                                                bool permuteTerms) {
  const auto admission = admit_native_call();
  ::applyTrotterizedImaginaryTimeEvolution(
      qureg.raw(), hamil.raw(), static_cast<qreal>(tau),
      static_cast<int>(order), static_cast<int>(reps), permuteTerms);
}

void apply_trotterized_multi_controlled_pauli_str_sum_gadget(
    Qureg& qureg,
    rust::Slice<const std::int32_t> controls,
    const PauliStrSum& sum,
    double angle,
    std::int32_t order,
    std::int32_t reps,
    bool permuteTerms) {
  const auto admission = admit_native_call();
  ::applyTrotterizedMultiControlledPauliStrSumGadget(
      qureg.raw(), to_int_vec(controls), sum.raw(), static_cast<qreal>(angle),
      static_cast<int>(order), static_cast<int>(reps), permuteTerms);
}

void apply_trotterized_multi_state_controlled_pauli_str_sum_gadget(
    Qureg& qureg,
    rust::Slice<const std::int32_t> controls,
    rust::Slice<const std::int32_t> states,
    const PauliStrSum& sum,
    double angle,
    std::int32_t order,
    std::int32_t reps,
    bool permuteTerms) {
  const auto admission = admit_native_call();
  ::applyTrotterizedMultiStateControlledPauliStrSumGadget(
      qureg.raw(), to_int_vec(controls), to_int_vec(states), sum.raw(),
      static_cast<qreal>(angle), static_cast<int>(order),
      static_cast<int>(reps), permuteTerms);
}

void apply_trotterized_non_unitary_pauli_str_sum_gadget(Qureg& qureg,
                                                        const PauliStrSum& sum,
                                                        QuestComplex angle,
                                                        std::int32_t order,
                                                        std::int32_t reps,
                                                        bool permuteTerms) {
  const auto admission = admit_native_call();
  ::applyTrotterizedNonUnitaryPauliStrSumGadget(
      qureg.raw(), sum.raw(), to_qcomp(angle), static_cast<int>(order),
      static_cast<int>(reps), permuteTerms);
}

void apply_trotterized_pauli_str_sum_gadget(Qureg& qureg,
                                            const PauliStrSum& sum,
                                            double angle,
                                            std::int32_t order,
                                            std::int32_t reps,
                                            bool permuteTerms) {
  const auto admission = admit_native_call();
  ::applyTrotterizedPauliStrSumGadget(
      qureg.raw(), sum.raw(), static_cast<qreal>(angle),
      static_cast<int>(order), static_cast<int>(reps), permuteTerms);
}

void apply_two_qubit_phase_flip(Qureg& qureg,
                                std::int32_t target1,
                                std::int32_t target2) {
  const auto admission = admit_native_call();
  ::applyTwoQubitPhaseFlip(qureg.raw(), static_cast<int>(target1),
                           static_cast<int>(target2));
}

void apply_two_qubit_phase_shift(Qureg& qureg,
                                 std::int32_t target1,
                                 std::int32_t target2,
                                 double angle) {
  const auto admission = admit_native_call();
  ::applyTwoQubitPhaseShift(qureg.raw(), static_cast<int>(target1),
                            static_cast<int>(target2),
                            static_cast<qreal>(angle));
}

double calc_expec_full_state_diag_matr(const Qureg& qureg,
                                       const FullStateDiagMatr& matr) {
  const auto admission = admit_native_call();
  return static_cast<double>(
      ::calcExpecFullStateDiagMatr(qureg.raw(), matr.raw()));
}

double calc_expec_full_state_diag_matr_power(const Qureg& qureg,
                                             const FullStateDiagMatr& matrix,
                                             double exponent) {
  const auto admission = admit_native_call();
  return static_cast<double>(::calcExpecFullStateDiagMatrPower(
      qureg.raw(), matrix.raw(), static_cast<qreal>(exponent)));
}

QuestComplex calc_expec_non_hermitian_full_state_diag_matr(
    const Qureg& qureg,
    const FullStateDiagMatr& matr) {
  const auto admission = admit_native_call();
  return from_qcomp(
      ::calcExpecNonHermitianFullStateDiagMatr(qureg.raw(), matr.raw()));
}

QuestComplex calc_expec_non_hermitian_full_state_diag_matr_power(
    const Qureg& qureg,
    const FullStateDiagMatr& matrix,
    QuestComplex exponent) {
  const auto admission = admit_native_call();
  return from_qcomp(::calcExpecNonHermitianFullStateDiagMatrPower(
      qureg.raw(), matrix.raw(), to_qcomp(exponent)));
}

QuestComplex calc_expec_non_hermitian_pauli_str_sum(const Qureg& qureg,
                                                    const PauliStrSum& sum) {
  const auto admission = admit_native_call();
  return from_qcomp(::calcExpecNonHermitianPauliStrSum(qureg.raw(), sum.raw()));
}

double calc_expec_pauli_str(const Qureg& qureg, const PauliStr& str_arg) {
  const auto admission = admit_native_call();
  return static_cast<double>(::calcExpecPauliStr(qureg.raw(), str_arg.raw()));
}

double calc_expec_pauli_str_sum(const Qureg& qureg, const PauliStrSum& sum) {
  const auto admission = admit_native_call();
  return static_cast<double>(::calcExpecPauliStrSum(qureg.raw(), sum.raw()));
}

double calc_fidelity(const Qureg& qureg, const Qureg& other) {
  const auto admission = admit_native_call();
  return static_cast<double>(::calcFidelity(qureg.raw(), other.raw()));
}

QuestComplex calc_inner_product(const Qureg& qureg, const Qureg& other) {
  const auto admission = admit_native_call();
  return from_qcomp(::calcInnerProduct(qureg.raw(), other.raw()));
}

std::unique_ptr<Qureg> calc_partial_trace(
    const Qureg& qureg,
    rust::Slice<const std::int32_t> traceOutQubits) {
  const auto admission = admit_native_call();
  return std::unique_ptr<Qureg>(
      new Qureg(::calcPartialTrace(qureg.raw(), to_int_vec(traceOutQubits))));
}

double calc_prob_of_basis_state(const Qureg& qureg, std::int64_t index) {
  const auto admission = admit_native_call();
  return static_cast<double>(
      ::calcProbOfBasisState(qureg.raw(), static_cast<qindex>(index)));
}

double calc_prob_of_multi_qubit_outcome(
    const Qureg& qureg,
    rust::Slice<const std::int32_t> qubits,
    rust::Slice<const std::int32_t> outcomes) {
  const auto admission = admit_native_call();
  return static_cast<double>(::calcProbOfMultiQubitOutcome(
      qureg.raw(), to_int_vec(qubits), to_int_vec(outcomes)));
}

double calc_prob_of_qubit_outcome(const Qureg& qureg,
                                  std::int32_t qubit,
                                  std::int32_t outcome) {
  const auto admission = admit_native_call();
  return static_cast<double>(::calcProbOfQubitOutcome(
      qureg.raw(), static_cast<int>(qubit), static_cast<int>(outcome)));
}

rust::Vec<double> calc_probs_of_all_multi_qubit_outcomes(
    const Qureg& qureg,
    rust::Slice<const std::int32_t> qubits) {
  const auto admission = admit_native_call();
  const auto count = qubits.size();
  if (count == 0 || count > static_cast<std::size_t>(qureg.raw().numQubits) ||
      count >= std::numeric_limits<std::size_t>::digits ||
      count >= std::numeric_limits<qindex>::digits ||
      count > static_cast<std::size_t>(std::numeric_limits<int>::max())) {
    throw std::invalid_argument("probability target count is invalid");
  }

  std::vector<int> targets;
  targets.reserve(count);
  for (const auto qubit : qubits) {
    if (qubit < 0 || qubit >= qureg.raw().numQubits) {
      throw std::invalid_argument("probability target index is out of bounds");
    }
    if (std::find(targets.begin(), targets.end(), qubit) != targets.end()) {
      throw std::invalid_argument(
          "duplicate target qubit in probability query");
    }
    targets.push_back(qubit);
  }

  const auto outcome_count = std::size_t{1} << count;
  if (outcome_count > std::vector<qreal>().max_size() ||
      outcome_count >
          static_cast<std::size_t>(std::numeric_limits<std::ptrdiff_t>::max()) /
              sizeof(qreal)) {
    throw std::length_error("probability output size is not representable");
  }
  std::vector<qreal> probabilities(outcome_count);
  ::calcProbsOfAllMultiQubitOutcomes(probabilities.data(), qureg.raw(),
                                     targets.data(), static_cast<int>(count));
  return from_qreal_vec(probabilities);
}

double calc_purity(const Qureg& qureg) {
  const auto admission = admit_native_call();
  return static_cast<double>(::calcPurity(qureg.raw()));
}

std::unique_ptr<Qureg> calc_reduced_density_matrix(
    const Qureg& qureg,
    rust::Slice<const std::int32_t> retainQubits) {
  const auto admission = admit_native_call();
  return std::unique_ptr<Qureg>(new Qureg(
      ::calcReducedDensityMatrix(qureg.raw(), to_int_vec(retainQubits))));
}

void clear_qu_est_gpu_cache() {
  const auto admission = admit_native_call();
  ::clearQuESTGpuCache();
}

std::unique_ptr<Qureg> create_clone_qureg(const Qureg& qureg) {
  const auto admission = admit_native_call();
  return std::unique_ptr<Qureg>(new Qureg(::createCloneQureg(qureg.raw())));
}

std::unique_ptr<FullStateDiagMatr> create_custom_full_state_diag_matr(
    std::int32_t numQubits,
    std::int32_t useDistrib,
    std::int32_t useGpuAccel,
    std::int32_t useMultithread) {
  const auto admission = admit_native_call();
  return std::unique_ptr<FullStateDiagMatr>(
      new FullStateDiagMatr(::createCustomFullStateDiagMatr(
          static_cast<int>(numQubits), static_cast<int>(useDistrib),
          static_cast<int>(useGpuAccel), static_cast<int>(useMultithread))));
}

std::unique_ptr<Qureg> create_custom_qureg(std::int32_t numQubits,
                                           std::int32_t isDensMatr,
                                           std::int32_t useDistrib,
                                           std::int32_t useGpuAccel,
                                           std::int32_t useMultithread) {
  const auto admission = admit_native_call();
  return std::unique_ptr<Qureg>(new Qureg(::createCustomQureg(
      static_cast<int>(numQubits), static_cast<int>(isDensMatr),
      static_cast<int>(useDistrib), static_cast<int>(useGpuAccel),
      static_cast<int>(useMultithread))));
}

std::unique_ptr<DiagMatr> create_diag_matr(std::int32_t numQubits) {
  const auto admission = admit_native_call();
  return std::unique_ptr<DiagMatr>(
      new DiagMatr(::createDiagMatr(static_cast<int>(numQubits))));
}

std::unique_ptr<Qureg> create_forced_density_qureg(std::int32_t numQubits) {
  const auto admission = admit_native_call();
  return std::unique_ptr<Qureg>(
      new Qureg(::createForcedDensityQureg(static_cast<int>(numQubits))));
}

std::unique_ptr<Qureg> create_forced_qureg(std::int32_t numQubits) {
  const auto admission = admit_native_call();
  return std::unique_ptr<Qureg>(
      new Qureg(::createForcedQureg(static_cast<int>(numQubits))));
}

std::unique_ptr<FullStateDiagMatr> create_full_state_diag_matr(
    std::int32_t numQubits) {
  const auto admission = admit_native_call();
  return std::unique_ptr<FullStateDiagMatr>(new FullStateDiagMatr(
      ::createFullStateDiagMatr(static_cast<int>(numQubits))));
}

std::unique_ptr<FullStateDiagMatr>
create_full_state_diag_matr_from_pauli_str_sum(const PauliStrSum& in_arg) {
  const auto admission = admit_native_call();
  return std::unique_ptr<FullStateDiagMatr>(new FullStateDiagMatr(
      ::createFullStateDiagMatrFromPauliStrSum(in_arg.raw())));
}

std::unique_ptr<DiagMatr> create_inline_diag_matr(
    std::int32_t numQb,
    rust::Slice<const QuestComplex> elems) {
  const auto admission = admit_native_call();
  return std::unique_ptr<DiagMatr>(new DiagMatr(
      ::createInlineDiagMatr(static_cast<int>(numQb), to_qcomp_vec(elems))));
}

std::unique_ptr<PauliStrSum> create_pauli_str_sum_from_file(rust::Str fn) {
  const auto admission = admit_native_call();
  return std::unique_ptr<PauliStrSum>(new PauliStrSum(
      ::createPauliStrSumFromFile(std::string(fn.data(), fn.size()))));
}

std::unique_ptr<PauliStrSum> create_pauli_str_sum_from_reversed_file(
    rust::Str fn) {
  const auto admission = admit_native_call();
  return std::unique_ptr<PauliStrSum>(new PauliStrSum(
      ::createPauliStrSumFromReversedFile(std::string(fn.data(), fn.size()))));
}

std::unique_ptr<Qureg> create_qureg_from_file(rust::Str fn) {
  const auto admission = admit_native_call();
  return std::unique_ptr<Qureg>(
      new Qureg(::createQuregFromFile(std::string(fn.data(), fn.size()))));
}

QuestComplex get_density_qureg_amp(const Qureg& qureg,
                                   std::int64_t row,
                                   std::int64_t column) {
  const auto admission = admit_native_call();
  return from_qcomp(::getDensityQuregAmp(qureg.raw(), static_cast<qindex>(row),
                                         static_cast<qindex>(column)));
}

std::unique_ptr<DiagMatr1> get_diag_matr1(
    rust::Slice<const QuestComplex> in_arg) {
  const auto admission = admit_native_call();
  return std::unique_ptr<DiagMatr1>(
      new DiagMatr1(::getDiagMatr1(to_qcomp_vec(in_arg))));
}

std::unique_ptr<DiagMatr2> get_diag_matr2(
    rust::Slice<const QuestComplex> in_arg) {
  const auto admission = admit_native_call();
  return std::unique_ptr<DiagMatr2>(
      new DiagMatr2(::getDiagMatr2(to_qcomp_vec(in_arg))));
}

std::unique_ptr<PauliStr> get_pauli_str_from_string(rust::Str paulis) {
  const auto admission = admit_native_call();
  return std::unique_ptr<PauliStr>(
      new PauliStr(::getPauliStr(std::string(paulis.data(), paulis.size()))));
}

std::unique_ptr<PauliStr> get_pauli_str(
    rust::Str paulis,
    rust::Slice<const std::int32_t> indices) {
  const auto admission = admit_native_call();
  return std::unique_ptr<PauliStr>(new PauliStr(::getPauliStr(
      std::string(paulis.data(), paulis.size()), to_int_vec(indices))));
}

std::int64_t get_qu_est_gpu_cache_size() {
  const auto admission = admit_native_call();
  return static_cast<std::int64_t>(::getQuESTGpuCacheSize());
}

std::int32_t get_qu_est_num_gpu_threads_per_block() {
  const auto admission = admit_native_call();
  return static_cast<std::int32_t>(::getQuESTNumGpuThreadsPerBlock());
}

std::int32_t get_qu_est_num_seeds() {
  const auto admission = admit_native_call();
  return static_cast<std::int32_t>(::getQuESTNumSeeds());
}

double get_qu_est_validation_epsilon() {
  const auto admission = admit_native_call();
  return static_cast<double>(::getQuESTValidationEpsilon());
}

void init_blank_state(Qureg& qureg) {
  const auto admission = admit_native_call();
  ::initBlankState(qureg.raw());
}

void init_classical_state(Qureg& qureg, std::int64_t stateInd) {
  const auto admission = admit_native_call();
  ::initClassicalState(qureg.raw(), static_cast<qindex>(stateInd));
}

void init_debug_state(Qureg& qureg) {
  const auto admission = admit_native_call();
  ::initDebugState(qureg.raw());
}

void init_pure_state(Qureg& qureg, const Qureg& pure) {
  const auto admission = admit_native_call();
  ::initPureState(qureg.raw(), pure.raw());
}

void init_random_mixed_state(Qureg& qureg, std::int64_t numPureStates) {
  const auto admission = admit_native_call();
  ::initRandomMixedState(qureg.raw(), static_cast<qindex>(numPureStates));
}

void init_random_pure_state(Qureg& qureg) {
  const auto admission = admit_native_call();
  ::initRandomPureState(qureg.raw());
}

void leftapply_comp_matr1(Qureg& qureg,
                          std::int32_t target,
                          const CompMatr1& matrix) {
  const auto admission = admit_native_call();
  ::leftapplyCompMatr1(qureg.raw(), static_cast<int>(target), matrix.raw());
}

void leftapply_comp_matr2(Qureg& qureg,
                          std::int32_t target1,
                          std::int32_t target2,
                          const CompMatr2& matr) {
  const auto admission = admit_native_call();
  ::leftapplyCompMatr2(qureg.raw(), static_cast<int>(target1),
                       static_cast<int>(target2), matr.raw());
}

void leftapply_diag_matr(Qureg& qureg,
                         rust::Slice<const std::int32_t> targets,
                         const DiagMatr& matrix) {
  const auto admission = admit_native_call();
  ::leftapplyDiagMatr(qureg.raw(), to_int_vec(targets), matrix.raw());
}

void leftapply_diag_matr1(Qureg& qureg,
                          std::int32_t target,
                          const DiagMatr1& matr) {
  const auto admission = admit_native_call();
  ::leftapplyDiagMatr1(qureg.raw(), static_cast<int>(target), matr.raw());
}

void leftapply_diag_matr2(Qureg& qureg,
                          std::int32_t target1,
                          std::int32_t target2,
                          const DiagMatr2& matr) {
  const auto admission = admit_native_call();
  ::leftapplyDiagMatr2(qureg.raw(), static_cast<int>(target1),
                       static_cast<int>(target2), matr.raw());
}

void leftapply_diag_matr_power(Qureg& qureg,
                               rust::Slice<const std::int32_t> targets,
                               const DiagMatr& matrix,
                               QuestComplex exponent) {
  const auto admission = admit_native_call();
  ::leftapplyDiagMatrPower(qureg.raw(), to_int_vec(targets), matrix.raw(),
                           to_qcomp(exponent));
}

void leftapply_full_state_diag_matr(Qureg& qureg,
                                    const FullStateDiagMatr& matrix) {
  const auto admission = admit_native_call();
  ::leftapplyFullStateDiagMatr(qureg.raw(), matrix.raw());
}

void leftapply_full_state_diag_matr_power(Qureg& qureg,
                                          const FullStateDiagMatr& matrix,
                                          QuestComplex exponent) {
  const auto admission = admit_native_call();
  ::leftapplyFullStateDiagMatrPower(qureg.raw(), matrix.raw(),
                                    to_qcomp(exponent));
}

void leftapply_multi_qubit_not(Qureg& qureg,
                               rust::Slice<const std::int32_t> targets) {
  const auto admission = admit_native_call();
  ::leftapplyMultiQubitNot(qureg.raw(), to_int_vec(targets));
}

void leftapply_multi_qubit_projector(Qureg& qureg,
                                     rust::Slice<const std::int32_t> qubits,
                                     rust::Slice<const std::int32_t> outcomes) {
  const auto admission = admit_native_call();
  ::leftapplyMultiQubitProjector(qureg.raw(), to_int_vec(qubits),
                                 to_int_vec(outcomes));
}

void leftapply_pauli_gadget(Qureg& qureg,
                            const PauliStr& str_arg,
                            double angle) {
  const auto admission = admit_native_call();
  ::leftapplyPauliGadget(qureg.raw(), str_arg.raw(), static_cast<qreal>(angle));
}

void leftapply_pauli_str(Qureg& qureg, const PauliStr& str_arg) {
  const auto admission = admit_native_call();
  ::leftapplyPauliStr(qureg.raw(), str_arg.raw());
}

void leftapply_pauli_str_sum(Qureg& qureg,
                             const PauliStrSum& sum,
                             const Qureg& workspace) {
  const auto admission = admit_native_call();
  ::leftapplyPauliStrSum(qureg.raw(), sum.raw(), workspace.raw());
}

void leftapply_pauli_x(Qureg& qureg, std::int32_t target) {
  const auto admission = admit_native_call();
  ::leftapplyPauliX(qureg.raw(), static_cast<int>(target));
}

void leftapply_pauli_y(Qureg& qureg, std::int32_t target) {
  const auto admission = admit_native_call();
  ::leftapplyPauliY(qureg.raw(), static_cast<int>(target));
}

void leftapply_pauli_z(Qureg& qureg, std::int32_t target) {
  const auto admission = admit_native_call();
  ::leftapplyPauliZ(qureg.raw(), static_cast<int>(target));
}

void leftapply_phase_gadget(Qureg& qureg,
                            rust::Slice<const std::int32_t> targets,
                            double angle) {
  const auto admission = admit_native_call();
  ::leftapplyPhaseGadget(qureg.raw(), to_int_vec(targets),
                         static_cast<qreal>(angle));
}

void leftapply_qubit_projector(Qureg& qureg,
                               std::int32_t qubit,
                               std::int32_t outcome) {
  const auto admission = admit_native_call();
  ::leftapplyQubitProjector(qureg.raw(), static_cast<int>(qubit),
                            static_cast<int>(outcome));
}

void leftapply_swap(Qureg& qureg, std::int32_t qubit1, std::int32_t qubit2) {
  const auto admission = admit_native_call();
  ::leftapplySwap(qureg.raw(), static_cast<int>(qubit1),
                  static_cast<int>(qubit2));
}

void mix_damping(Qureg& qureg, std::int32_t target, double prob) {
  const auto admission = admit_native_call();
  ::mixDamping(qureg.raw(), static_cast<int>(target), static_cast<qreal>(prob));
}

void mix_depolarising(Qureg& qureg, std::int32_t target, double prob) {
  const auto admission = admit_native_call();
  ::mixDepolarising(qureg.raw(), static_cast<int>(target),
                    static_cast<qreal>(prob));
}

void mix_kraus_map(Qureg& qureg,
                   rust::Slice<const std::int32_t> targets,
                   const KrausMap& map) {
  const auto admission = admit_native_call();
  ::mixKrausMap(qureg.raw(), to_int_vec(targets), map.raw());
}

void mix_paulis(Qureg& qureg,
                std::int32_t target,
                double probX,
                double probY,
                double probZ) {
  const auto admission = admit_native_call();
  ::mixPaulis(qureg.raw(), static_cast<int>(target), static_cast<qreal>(probX),
              static_cast<qreal>(probY), static_cast<qreal>(probZ));
}

void mix_qureg(Qureg& qureg, const Qureg& other, double prob) {
  const auto admission = admit_native_call();
  ::mixQureg(qureg.raw(), other.raw(), static_cast<qreal>(prob));
}

void mix_super_op(Qureg& qureg,
                  rust::Slice<const std::int32_t> targets,
                  const SuperOp& superop) {
  const auto admission = admit_native_call();
  ::mixSuperOp(qureg.raw(), to_int_vec(targets), superop.raw());
}

void mix_two_qubit_dephasing(Qureg& qureg,
                             std::int32_t target1,
                             std::int32_t target2,
                             double prob) {
  const auto admission = admit_native_call();
  ::mixTwoQubitDephasing(qureg.raw(), static_cast<int>(target1),
                         static_cast<int>(target2), static_cast<qreal>(prob));
}

void mix_two_qubit_depolarising(Qureg& qureg,
                                std::int32_t target1,
                                std::int32_t target2,
                                double prob) {
  const auto admission = admit_native_call();
  ::mixTwoQubitDepolarising(qureg.raw(), static_cast<int>(target1),
                            static_cast<int>(target2),
                            static_cast<qreal>(prob));
}

void report_comp_matr(const CompMatr& matrix) {
  const auto admission = admit_native_call();
  ::reportCompMatr(matrix.raw());
}

void report_comp_matr1(const CompMatr1& matrix) {
  const auto admission = admit_native_call();
  ::reportCompMatr1(matrix.raw());
}

void report_comp_matr2(const CompMatr2& matrix) {
  const auto admission = admit_native_call();
  ::reportCompMatr2(matrix.raw());
}

void report_diag_matr(const DiagMatr& matrix) {
  const auto admission = admit_native_call();
  ::reportDiagMatr(matrix.raw());
}

void report_diag_matr1(const DiagMatr1& matrix) {
  const auto admission = admit_native_call();
  ::reportDiagMatr1(matrix.raw());
}

void report_diag_matr2(const DiagMatr2& matrix) {
  const auto admission = admit_native_call();
  ::reportDiagMatr2(matrix.raw());
}

void report_full_state_diag_matr(const FullStateDiagMatr& matr) {
  const auto admission = admit_native_call();
  ::reportFullStateDiagMatr(matr.raw());
}

void report_kraus_map(const KrausMap& map) {
  const auto admission = admit_native_call();
  ::reportKrausMap(map.raw());
}

void report_pauli_str(const PauliStr& str_arg) {
  const auto admission = admit_native_call();
  ::reportPauliStr(str_arg.raw());
}

void report_pauli_str_sum(const PauliStrSum& str_arg) {
  const auto admission = admit_native_call();
  ::reportPauliStrSum(str_arg.raw());
}

void report_qureg(const Qureg& qureg) {
  const auto admission = admit_native_call();
  ::reportQureg(qureg.raw());
}

void report_qureg_params(const Qureg& qureg) {
  const auto admission = admit_native_call();
  ::reportQuregParams(qureg.raw());
}

void report_scalar_real(rust::Str label, double num) {
  const auto admission = admit_native_call();
  ::reportScalar(std::string(label.data(), label.size()),
                 static_cast<qreal>(num));
}

void report_scalar(rust::Str label, QuestComplex num) {
  const auto admission = admit_native_call();
  ::reportScalar(std::string(label.data(), label.size()), to_qcomp(num));
}

void report_str(rust::Str str_arg) {
  const auto admission = admit_native_call();
  ::reportStr(std::string(str_arg.data(), str_arg.size()));
}

void report_super_op(const SuperOp& op) {
  const auto admission = admit_native_call();
  ::reportSuperOp(op.raw());
}

void rightapply_comp_matr1(Qureg& qureg,
                           std::int32_t target,
                           const CompMatr1& matrix) {
  const auto admission = admit_native_call();
  ::rightapplyCompMatr1(qureg.raw(), static_cast<int>(target), matrix.raw());
}

void rightapply_comp_matr2(Qureg& qureg,
                           std::int32_t target1,
                           std::int32_t target2,
                           const CompMatr2& matrix) {
  const auto admission = admit_native_call();
  ::rightapplyCompMatr2(qureg.raw(), static_cast<int>(target1),
                        static_cast<int>(target2), matrix.raw());
}

void rightapply_diag_matr(Qureg& qureg,
                          rust::Slice<const std::int32_t> targets,
                          const DiagMatr& matrix) {
  const auto admission = admit_native_call();
  ::rightapplyDiagMatr(qureg.raw(), to_int_vec(targets), matrix.raw());
}

void rightapply_diag_matr1(Qureg& qureg,
                           std::int32_t target,
                           const DiagMatr1& matrix) {
  const auto admission = admit_native_call();
  ::rightapplyDiagMatr1(qureg.raw(), static_cast<int>(target), matrix.raw());
}

void rightapply_diag_matr2(Qureg& qureg,
                           std::int32_t target1,
                           std::int32_t target2,
                           const DiagMatr2& matrix) {
  const auto admission = admit_native_call();
  ::rightapplyDiagMatr2(qureg.raw(), static_cast<int>(target1),
                        static_cast<int>(target2), matrix.raw());
}

void rightapply_diag_matr_power(Qureg& qureg,
                                rust::Slice<const std::int32_t> targets,
                                const DiagMatr& matrix,
                                QuestComplex exponent) {
  const auto admission = admit_native_call();
  ::rightapplyDiagMatrPower(qureg.raw(), to_int_vec(targets), matrix.raw(),
                            to_qcomp(exponent));
}

void rightapply_full_state_diag_matr(Qureg& qureg,
                                     const FullStateDiagMatr& matrix) {
  const auto admission = admit_native_call();
  ::rightapplyFullStateDiagMatr(qureg.raw(), matrix.raw());
}

void rightapply_full_state_diag_matr_power(Qureg& qureg,
                                           const FullStateDiagMatr& matrix,
                                           QuestComplex exponent) {
  const auto admission = admit_native_call();
  ::rightapplyFullStateDiagMatrPower(qureg.raw(), matrix.raw(),
                                     to_qcomp(exponent));
}

void rightapply_multi_qubit_not(Qureg& qureg,
                                rust::Slice<const std::int32_t> targets) {
  const auto admission = admit_native_call();
  ::rightapplyMultiQubitNot(qureg.raw(), to_int_vec(targets));
}

void rightapply_multi_qubit_projector(
    Qureg& qureg,
    rust::Slice<const std::int32_t> qubits,
    rust::Slice<const std::int32_t> outcomes) {
  const auto admission = admit_native_call();
  ::rightapplyMultiQubitProjector(qureg.raw(), to_int_vec(qubits),
                                  to_int_vec(outcomes));
}

void rightapply_pauli_gadget(Qureg& qureg,
                             const PauliStr& str_arg,
                             double angle) {
  const auto admission = admit_native_call();
  ::rightapplyPauliGadget(qureg.raw(), str_arg.raw(),
                          static_cast<qreal>(angle));
}

void rightapply_pauli_str(Qureg& qureg, const PauliStr& str_arg) {
  const auto admission = admit_native_call();
  ::rightapplyPauliStr(qureg.raw(), str_arg.raw());
}

void rightapply_pauli_str_sum(Qureg& qureg,
                              const PauliStrSum& sum,
                              const Qureg& workspace) {
  const auto admission = admit_native_call();
  ::rightapplyPauliStrSum(qureg.raw(), sum.raw(), workspace.raw());
}

void rightapply_pauli_x(Qureg& qureg, std::int32_t target) {
  const auto admission = admit_native_call();
  ::rightapplyPauliX(qureg.raw(), static_cast<int>(target));
}

void rightapply_pauli_y(Qureg& qureg, std::int32_t target) {
  const auto admission = admit_native_call();
  ::rightapplyPauliY(qureg.raw(), static_cast<int>(target));
}

void rightapply_pauli_z(Qureg& qureg, std::int32_t target) {
  const auto admission = admit_native_call();
  ::rightapplyPauliZ(qureg.raw(), static_cast<int>(target));
}

void rightapply_phase_gadget(Qureg& qureg,
                             rust::Slice<const std::int32_t> targets,
                             double angle) {
  const auto admission = admit_native_call();
  ::rightapplyPhaseGadget(qureg.raw(), to_int_vec(targets),
                          static_cast<qreal>(angle));
}

void rightapply_qubit_projector(Qureg& qureg,
                                std::int32_t qubit,
                                std::int32_t outcome) {
  const auto admission = admit_native_call();
  ::rightapplyQubitProjector(qureg.raw(), static_cast<int>(qubit),
                             static_cast<int>(outcome));
}

void rightapply_swap(Qureg& qureg, std::int32_t qubit1, std::int32_t qubit2) {
  const auto admission = admit_native_call();
  ::rightapplySwap(qureg.raw(), static_cast<int>(qubit1),
                   static_cast<int>(qubit2));
}

void save_qureg_to_file(Qureg& qureg, rust::Str arg1) {
  const auto admission = admit_native_call();
  ::saveQuregToFile(qureg.raw(), std::string(arg1.data(), arg1.size()));
}

void set_density_qureg_flat_amps(Qureg& qureg,
                                 std::int64_t startInd,
                                 rust::Slice<const QuestComplex> amps) {
  const auto admission = admit_native_call();
  ::setDensityQuregFlatAmps(qureg.raw(), static_cast<qindex>(startInd),
                            to_qcomp_vec(amps));
}

void set_diag_matr(DiagMatr& out_arg, rust::Slice<const QuestComplex> in_arg) {
  const auto admission = admit_native_call();
  ::setDiagMatr(out_arg.raw(), to_qcomp_vec(in_arg));
}

void set_full_state_diag_matr(FullStateDiagMatr& out_arg,
                              std::int64_t startInd,
                              rust::Slice<const QuestComplex> in_arg) {
  const auto admission = admit_native_call();
  ::setFullStateDiagMatr(out_arg.raw(), static_cast<qindex>(startInd),
                         to_qcomp_vec(in_arg));
}

void set_full_state_diag_matr_from_pauli_str_sum(FullStateDiagMatr& out_arg,
                                                 const PauliStrSum& in_arg) {
  const auto admission = admit_native_call();
  ::setFullStateDiagMatrFromPauliStrSum(out_arg.raw(), in_arg.raw());
}

void set_inline_diag_matr(DiagMatr& matr,
                          std::int32_t numQb,
                          rust::Slice<const QuestComplex> in_arg) {
  const auto admission = admit_native_call();
  ::setInlineDiagMatr(matr.raw(), static_cast<int>(numQb),
                      to_qcomp_vec(in_arg));
}

void set_inline_full_state_diag_matr(FullStateDiagMatr& matr,
                                     std::int64_t startInd,
                                     std::int64_t numElems,
                                     rust::Slice<const QuestComplex> in_arg) {
  const auto admission = admit_native_call();
  ::setInlineFullStateDiagMatr(matr.raw(), static_cast<qindex>(startInd),
                               static_cast<qindex>(numElems),
                               to_qcomp_vec(in_arg));
}

void set_qu_est_max_num_reported_items(std::int64_t numRows,
                                       std::int64_t numCols) {
  const auto admission = admit_native_call();
  ::setQuESTMaxNumReportedItems(static_cast<qindex>(numRows),
                                static_cast<qindex>(numCols));
}

void set_qu_est_max_num_reported_sig_figs(std::int32_t numSigFigs) {
  const auto admission = admit_native_call();
  ::setQuESTMaxNumReportedSigFigs(static_cast<int>(numSigFigs));
}

void set_qu_est_num_gpu_threads_per_block(std::int32_t numThreadsPerBlock) {
  const auto admission = admit_native_call();
  ::setQuESTNumGpuThreadsPerBlock(static_cast<int>(numThreadsPerBlock));
}

void set_qu_est_num_reported_newlines(std::int32_t numNewlines) {
  const auto admission = admit_native_call();
  ::setQuESTNumReportedNewlines(static_cast<int>(numNewlines));
}

void set_qu_est_reported_pauli_chars(rust::Str paulis) {
  const auto admission = admit_native_call();
  const std::string paulis_string(paulis.data(), paulis.size());
  ::setQuESTReportedPauliChars(paulis_string.c_str());
}

void set_qu_est_reported_pauli_str_style(std::int32_t style) {
  const auto admission = admit_native_call();
  ::setQuESTReportedPauliStrStyle(static_cast<int>(style));
}

void set_qu_est_seeds_to_default() {
  const auto admission = admit_native_call();
  ::setQuESTSeedsToDefault();
}

void set_qu_est_validation_epsilon(double eps) {
  const auto admission = admit_native_call();
  if (!std::isfinite(eps) || eps <= 0) {
    throw std::invalid_argument(
        "validation epsilon must be finite and positive");
  }
  ::setQuESTValidationEpsilon(static_cast<qreal>(eps));
}

void set_qu_est_validation_epsilon_to_default() {
  const auto admission = admit_native_call();
  ::setQuESTValidationEpsilonToDefault();
}

void set_qu_est_validation_on() {
  const auto admission = admit_native_call();
  ::setQuESTValidationOn();
}

void set_qureg_amps(Qureg& qureg,
                    std::int64_t startInd,
                    rust::Slice<const QuestComplex> amps) {
  const auto admission = admit_native_call();
  ::setQuregAmps(qureg.raw(), static_cast<qindex>(startInd),
                 to_qcomp_vec(amps));
}

void set_qureg_to_clone(Qureg& outQureg, const Qureg& inQureg) {
  const auto admission = admit_native_call();
  ::setQuregToClone(outQureg.raw(), inQureg.raw());
}

void set_qureg_to_partial_trace(
    Qureg& out_arg,
    const Qureg& in_arg,
    rust::Slice<const std::int32_t> traceOutQubits) {
  const auto admission = admit_native_call();
  ::setQuregToPartialTrace(out_arg.raw(), in_arg.raw(),
                           to_int_vec(traceOutQubits));
}

void set_qureg_to_pauli_str_sum(Qureg& qureg, const PauliStrSum& sum) {
  const auto admission = admit_native_call();
  ::setQuregToPauliStrSum(qureg.raw(), sum.raw());
}

void set_qureg_to_reduced_density_matrix(
    Qureg& out_arg,
    const Qureg& in_arg,
    rust::Slice<const std::int32_t> retainQubits) {
  const auto admission = admit_native_call();
  ::setQuregToReducedDensityMatrix(out_arg.raw(), in_arg.raw(),
                                   to_int_vec(retainQubits));
}

double set_qureg_to_renormalized(Qureg& qureg) {
  const auto admission = admit_native_call();
  return static_cast<double>(::setQuregToRenormalized(qureg.raw()));
}

void sort_pauli_str_sum_lexicographic(PauliStrSum& sum) {
  const auto admission = admit_native_call();
  ::sortPauliStrSumLexicographic(sum.raw());
}

void sort_pauli_str_sum_magnitude(PauliStrSum& sum) {
  const auto admission = admit_native_call();
  ::sortPauliStrSumMagnitude(sum.raw());
}

void sync_comp_matr(CompMatr& matr) {
  const auto admission = admit_native_call();
  ::syncCompMatr(matr.raw());
}

void sync_diag_matr(DiagMatr& matr) {
  const auto admission = admit_native_call();
  ::syncDiagMatr(matr.raw());
}

void sync_full_state_diag_matr(FullStateDiagMatr& matr) {
  const auto admission = admit_native_call();
  ::syncFullStateDiagMatr(matr.raw());
}

void sync_kraus_map(KrausMap& map) {
  const auto admission = admit_native_call();
  ::syncKrausMap(map.raw());
}

void sync_qureg_from_gpu(Qureg& qureg) {
  const auto admission = admit_native_call();
  ::syncQuregFromGpu(qureg.raw());
}

void sync_qureg_to_gpu(Qureg& qureg) {
  const auto admission = admit_native_call();
  ::syncQuregToGpu(qureg.raw());
}

void sync_sub_qureg_from_gpu(Qureg& qureg,
                             std::int64_t localStartInd,
                             std::int64_t numLocalAmps) {
  const auto admission = admit_native_call();
  ::syncSubQuregFromGpu(qureg.raw(), static_cast<qindex>(localStartInd),
                        static_cast<qindex>(numLocalAmps));
}

void sync_sub_qureg_to_gpu(Qureg& qureg,
                           std::int64_t localStartInd,
                           std::int64_t numLocalAmps) {
  const auto admission = admit_native_call();
  ::syncSubQuregToGpu(qureg.raw(), static_cast<qindex>(localStartInd),
                      static_cast<qindex>(numLocalAmps));
}

void sync_super_op(SuperOp& op) {
  const auto admission = admit_native_call();
  ::syncSuperOp(op.raw());
}

}  // namespace quest_sys
