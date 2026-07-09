#pragma once

#include "rust/cxx.h"
#include "quest.h"

#include <cstdint>
#include <memory>

namespace quest_sys {

struct QuestComplex;
struct QuestEnvironment;
struct QubitMeasurement;

static_assert(QUEST_VERSION_MAJOR == 4 && QUEST_VERSION_MINOR == 2 && QUEST_VERSION_PATCH == 0,
              "quest-sys generated bindings target QuEST 4.2.0");
static_assert(QUEST_INCLUDE_DEPRECATED_FUNCTIONS == 0,
              "quest-sys generated bindings exclude deprecated QuEST APIs");
static_assert(QUEST_FLOAT_PRECISION == 2,
              "quest-sys generated bindings require double precision qreal");
static_assert(sizeof(qreal) == sizeof(double),
              "quest-sys generated bindings map qreal to Rust f64");
static_assert(sizeof(qindex) == sizeof(std::int64_t),
              "quest-sys generated bindings map qindex to Rust i64");

class Qureg final {
public:
    explicit Qureg(::Qureg qureg) noexcept;
    ~Qureg() noexcept;

    Qureg(Qureg&& other) noexcept;
    Qureg& operator=(Qureg&& other) noexcept;

    Qureg(const Qureg&) = delete;
    Qureg& operator=(const Qureg&) = delete;

    ::Qureg raw() const noexcept;

private:
    void reset() noexcept;

    ::Qureg qureg_{};
    bool owns_{false};
};

class CompMatr1 final {
public:
    explicit CompMatr1(::CompMatr1 matrix) noexcept;
    ::CompMatr1 raw() const noexcept;

private:
    ::CompMatr1 matrix_{};
};

class CompMatr2 final {
public:
    explicit CompMatr2(::CompMatr2 matrix) noexcept;
    ::CompMatr2 raw() const noexcept;

private:
    ::CompMatr2 matrix_{};
};

class CompMatr final {
public:
    explicit CompMatr(::CompMatr matrix) noexcept;
    ~CompMatr() noexcept;

    CompMatr(CompMatr&& other) noexcept;
    CompMatr& operator=(CompMatr&& other) noexcept;

    CompMatr(const CompMatr&) = delete;
    CompMatr& operator=(const CompMatr&) = delete;

    ::CompMatr raw() const noexcept;
    std::int64_t num_rows() const noexcept;

private:
    void reset() noexcept;

    ::CompMatr matrix_{};
    bool owns_{false};
};

class DiagMatr1 final {
public:
    explicit DiagMatr1(::DiagMatr1 matrix) noexcept;
    ::DiagMatr1 raw() const noexcept;

private:
    ::DiagMatr1 matrix_{};
};

class DiagMatr2 final {
public:
    explicit DiagMatr2(::DiagMatr2 matrix) noexcept;
    ::DiagMatr2 raw() const noexcept;

private:
    ::DiagMatr2 matrix_{};
};

class DiagMatr final {
public:
    explicit DiagMatr(::DiagMatr matrix) noexcept;
    ~DiagMatr() noexcept;

    DiagMatr(DiagMatr&& other) noexcept;
    DiagMatr& operator=(DiagMatr&& other) noexcept;

    DiagMatr(const DiagMatr&) = delete;
    DiagMatr& operator=(const DiagMatr&) = delete;

    ::DiagMatr raw() const noexcept;
    std::int64_t num_elems() const noexcept;

private:
    void reset() noexcept;

    ::DiagMatr matrix_{};
    bool owns_{false};
};

class FullStateDiagMatr final {
public:
    explicit FullStateDiagMatr(::FullStateDiagMatr matrix) noexcept;
    ~FullStateDiagMatr() noexcept;

    FullStateDiagMatr(FullStateDiagMatr&& other) noexcept;
    FullStateDiagMatr& operator=(FullStateDiagMatr&& other) noexcept;

    FullStateDiagMatr(const FullStateDiagMatr&) = delete;
    FullStateDiagMatr& operator=(const FullStateDiagMatr&) = delete;

    ::FullStateDiagMatr raw() const noexcept;
    std::int64_t num_elems() const noexcept;

private:
    void reset() noexcept;

    ::FullStateDiagMatr matrix_{};
    bool owns_{false};
};

class SuperOp final {
public:
    explicit SuperOp(::SuperOp op) noexcept;
    ~SuperOp() noexcept;

    SuperOp(SuperOp&& other) noexcept;
    SuperOp& operator=(SuperOp&& other) noexcept;

    SuperOp(const SuperOp&) = delete;
    SuperOp& operator=(const SuperOp&) = delete;

    ::SuperOp raw() const noexcept;

private:
    void reset() noexcept;

    ::SuperOp op_{};
    bool owns_{false};
};

class KrausMap final {
public:
    explicit KrausMap(::KrausMap map) noexcept;
    ~KrausMap() noexcept;

    KrausMap(KrausMap&& other) noexcept;
    KrausMap& operator=(KrausMap&& other) noexcept;

    KrausMap(const KrausMap&) = delete;
    KrausMap& operator=(const KrausMap&) = delete;

    ::KrausMap raw() const noexcept;

private:
    void reset() noexcept;

    ::KrausMap map_{};
    bool owns_{false};
};

class PauliStr final {
public:
    explicit PauliStr(::PauliStr str) noexcept;
    ::PauliStr raw() const noexcept;

private:
    ::PauliStr str_{};
};

class PauliStrSum final {
public:
    explicit PauliStrSum(::PauliStrSum sum) noexcept;
    ~PauliStrSum() noexcept;

    PauliStrSum(PauliStrSum&& other) noexcept;
    PauliStrSum& operator=(PauliStrSum&& other) noexcept;

    PauliStrSum(const PauliStrSum&) = delete;
    PauliStrSum& operator=(const PauliStrSum&) = delete;

    ::PauliStrSum raw() const noexcept;

private:
    void reset() noexcept;

    ::PauliStrSum sum_{};
    bool owns_{false};
};

void init_quest_env();
void init_custom_quest_env(bool use_distrib, bool use_gpu_accel, bool use_multithread);
void finalize_quest_env();
void sync_quest_env();
bool is_quest_env_init();
QuestEnvironment get_quest_env();
rust::String get_environment_string();

std::unique_ptr<Qureg> create_qureg(std::int32_t num_qubits);
std::unique_ptr<Qureg> create_density_qureg(std::int32_t num_qubits);
std::unique_ptr<CompMatr1> unique_ptr_marker_comp_matr1();
std::unique_ptr<CompMatr2> unique_ptr_marker_comp_matr2();
std::unique_ptr<DiagMatr1> unique_ptr_marker_diag_matr1();
std::unique_ptr<DiagMatr2> unique_ptr_marker_diag_matr2();
std::unique_ptr<DiagMatr> unique_ptr_marker_diag_matr();
std::unique_ptr<FullStateDiagMatr> unique_ptr_marker_full_state_diag_matr();
std::unique_ptr<PauliStr> unique_ptr_marker_pauli_str();

void init_zero_state(Qureg& qureg);
void init_plus_state(Qureg& qureg);
void init_arbitrary_pure_state(Qureg& qureg, rust::Slice<const QuestComplex> amps);

QuestComplex get_qureg_amp(const Qureg& qureg, std::int64_t index);
rust::Vec<QuestComplex> get_qureg_amps(const Qureg& qureg, std::int64_t start_index, std::int64_t num_amps);
double calc_total_prob(const Qureg& qureg);

std::int32_t apply_qubit_measurement(Qureg& qureg, std::int32_t target);
QubitMeasurement apply_qubit_measurement_and_get_prob(Qureg& qureg, std::int32_t target);

std::unique_ptr<CompMatr> create_comp_matr(std::int32_t num_qubits);
void set_comp_matr_flat(CompMatr& matrix, rust::Slice<const QuestComplex> values, std::int64_t num_rows);
void apply_comp_matr(Qureg& qureg, rust::Slice<const std::int32_t> targets, const CompMatr& matrix);
void leftapply_comp_matr(Qureg& qureg, rust::Slice<const std::int32_t> targets, const CompMatr& matrix);
void rightapply_comp_matr(Qureg& qureg, rust::Slice<const std::int32_t> targets, const CompMatr& matrix);

void apply_hadamard(Qureg& qureg, std::int32_t target);
void apply_pauli_x(Qureg& qureg, std::int32_t target);
void apply_pauli_y(Qureg& qureg, std::int32_t target);
void apply_pauli_z(Qureg& qureg, std::int32_t target);

void mix_dephasing(Qureg& qureg, std::int32_t target, double probability);

std::unique_ptr<SuperOp> create_super_op(std::int32_t num_qubits);
std::unique_ptr<KrausMap> create_kraus_map(std::int32_t num_qubits, std::int32_t num_operators);

std::unique_ptr<PauliStrSum> create_inline_pauli_str_sum(rust::Str spec);
void apply_trotterized_unitary_time_evolution(
    Qureg& qureg,
    const PauliStrSum& hamiltonian,
    double time,
    std::int32_t order,
    std::int32_t reps,
    bool permute_terms);

} // namespace quest_sys
