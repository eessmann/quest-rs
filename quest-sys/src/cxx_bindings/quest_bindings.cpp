#include "quest_bindings.hpp"

#include "quest-sys/src/lib.rs.h"

#include <algorithm>
#include <array>
#include <complex>
#include <stdexcept>
#include <string>
#include <utility>
#include <vector>

namespace quest_sys {
namespace {

void throw_quest_input_error(const char* func, const char* msg) {
    std::string message = func != nullptr ? func : "QuEST";
    message += ": ";
    message += msg != nullptr ? msg : "input validation failed";
    throw std::runtime_error(message);
}

void install_quest_input_error_handler() {
    ::setQuESTInputErrorHandler(&throw_quest_input_error);
}

qcomp to_qcomp(const QuestComplex& value) {
    return qcomp(value.re, value.im);
}

QuestComplex from_qcomp(qcomp value) {
    return QuestComplex{
        static_cast<double>(std::real(value)),
        static_cast<double>(std::imag(value)),
    };
}

std::vector<qcomp> to_qcomp_vec(rust::Slice<const QuestComplex> values) {
    std::vector<qcomp> out;
    out.reserve(values.size());
    for (const auto& value : values) {
        out.push_back(to_qcomp(value));
    }
    return out;
}

std::vector<int> to_int_vec(rust::Slice<const std::int32_t> values) {
    std::vector<int> out;
    out.reserve(values.size());
    for (const auto value : values) {
        out.push_back(static_cast<int>(value));
    }
    return out;
}

} // namespace

Qureg::Qureg(::Qureg qureg) noexcept : qureg_(qureg), owns_(true) {}

Qureg::~Qureg() noexcept {
    reset();
}

Qureg::Qureg(Qureg&& other) noexcept
    : qureg_(other.qureg_), owns_(std::exchange(other.owns_, false)) {}

Qureg& Qureg::operator=(Qureg&& other) noexcept {
    if (this != &other) {
        reset();
        qureg_ = other.qureg_;
        owns_ = std::exchange(other.owns_, false);
    }
    return *this;
}

::Qureg Qureg::raw() const noexcept {
    return qureg_;
}

void Qureg::reset() noexcept {
    if (!owns_) {
        return;
    }

    owns_ = false;
    if (!::isQuESTEnvInit()) {
        return;
    }

    try {
        ::destroyQureg(qureg_);
    } catch (...) {
    }
}

CompMatr1::CompMatr1(::CompMatr1 matrix) noexcept : matrix_(matrix) {}

::CompMatr1 CompMatr1::raw() const noexcept {
    return matrix_;
}

CompMatr2::CompMatr2(::CompMatr2 matrix) noexcept : matrix_(matrix) {}

::CompMatr2 CompMatr2::raw() const noexcept {
    return matrix_;
}

CompMatr::CompMatr(::CompMatr matrix) noexcept : matrix_(matrix), owns_(true) {}

CompMatr::~CompMatr() noexcept {
    reset();
}

CompMatr::CompMatr(CompMatr&& other) noexcept
    : matrix_(other.matrix_), owns_(std::exchange(other.owns_, false)) {}

CompMatr& CompMatr::operator=(CompMatr&& other) noexcept {
    if (this != &other) {
        reset();
        matrix_ = other.matrix_;
        owns_ = std::exchange(other.owns_, false);
    }
    return *this;
}

::CompMatr CompMatr::raw() const noexcept {
    return matrix_;
}

std::int64_t CompMatr::num_rows() const noexcept {
    return static_cast<std::int64_t>(matrix_.numRows);
}

void CompMatr::reset() noexcept {
    if (!owns_) {
        return;
    }

    owns_ = false;
    try {
        ::destroyCompMatr(matrix_);
    } catch (...) {
    }
}

DiagMatr1::DiagMatr1(::DiagMatr1 matrix) noexcept : matrix_(matrix) {}

::DiagMatr1 DiagMatr1::raw() const noexcept {
    return matrix_;
}

DiagMatr2::DiagMatr2(::DiagMatr2 matrix) noexcept : matrix_(matrix) {}

::DiagMatr2 DiagMatr2::raw() const noexcept {
    return matrix_;
}

DiagMatr::DiagMatr(::DiagMatr matrix) noexcept : matrix_(matrix), owns_(true) {}

DiagMatr::~DiagMatr() noexcept {
    reset();
}

DiagMatr::DiagMatr(DiagMatr&& other) noexcept
    : matrix_(other.matrix_), owns_(std::exchange(other.owns_, false)) {}

DiagMatr& DiagMatr::operator=(DiagMatr&& other) noexcept {
    if (this != &other) {
        reset();
        matrix_ = other.matrix_;
        owns_ = std::exchange(other.owns_, false);
    }
    return *this;
}

::DiagMatr DiagMatr::raw() const noexcept {
    return matrix_;
}

std::int64_t DiagMatr::num_elems() const noexcept {
    return static_cast<std::int64_t>(matrix_.numElems);
}

void DiagMatr::reset() noexcept {
    if (!owns_) {
        return;
    }

    owns_ = false;
    try {
        ::destroyDiagMatr(matrix_);
    } catch (...) {
    }
}

FullStateDiagMatr::FullStateDiagMatr(::FullStateDiagMatr matrix) noexcept : matrix_(matrix), owns_(true) {}

FullStateDiagMatr::~FullStateDiagMatr() noexcept {
    reset();
}

FullStateDiagMatr::FullStateDiagMatr(FullStateDiagMatr&& other) noexcept
    : matrix_(other.matrix_), owns_(std::exchange(other.owns_, false)) {}

FullStateDiagMatr& FullStateDiagMatr::operator=(FullStateDiagMatr&& other) noexcept {
    if (this != &other) {
        reset();
        matrix_ = other.matrix_;
        owns_ = std::exchange(other.owns_, false);
    }
    return *this;
}

::FullStateDiagMatr FullStateDiagMatr::raw() const noexcept {
    return matrix_;
}

std::int64_t FullStateDiagMatr::num_elems() const noexcept {
    return static_cast<std::int64_t>(matrix_.numElems);
}

void FullStateDiagMatr::reset() noexcept {
    if (!owns_) {
        return;
    }

    owns_ = false;
    try {
        ::destroyFullStateDiagMatr(matrix_);
    } catch (...) {
    }
}

SuperOp::SuperOp(::SuperOp op) noexcept : op_(op), owns_(true) {}

SuperOp::~SuperOp() noexcept {
    reset();
}

SuperOp::SuperOp(SuperOp&& other) noexcept
    : op_(other.op_), owns_(std::exchange(other.owns_, false)) {}

SuperOp& SuperOp::operator=(SuperOp&& other) noexcept {
    if (this != &other) {
        reset();
        op_ = other.op_;
        owns_ = std::exchange(other.owns_, false);
    }
    return *this;
}

::SuperOp SuperOp::raw() const noexcept {
    return op_;
}

void SuperOp::reset() noexcept {
    if (!owns_) {
        return;
    }

    owns_ = false;
    try {
        ::destroySuperOp(op_);
    } catch (...) {
    }
}

KrausMap::KrausMap(::KrausMap map) noexcept : map_(map), owns_(true) {}

KrausMap::~KrausMap() noexcept {
    reset();
}

KrausMap::KrausMap(KrausMap&& other) noexcept
    : map_(other.map_), owns_(std::exchange(other.owns_, false)) {}

KrausMap& KrausMap::operator=(KrausMap&& other) noexcept {
    if (this != &other) {
        reset();
        map_ = other.map_;
        owns_ = std::exchange(other.owns_, false);
    }
    return *this;
}

::KrausMap KrausMap::raw() const noexcept {
    return map_;
}

void KrausMap::reset() noexcept {
    if (!owns_) {
        return;
    }

    owns_ = false;
    try {
        ::destroyKrausMap(map_);
    } catch (...) {
    }
}

PauliStr::PauliStr(::PauliStr str) noexcept : str_(str) {}

::PauliStr PauliStr::raw() const noexcept {
    return str_;
}

PauliStrSum::PauliStrSum(::PauliStrSum sum) noexcept : sum_(sum), owns_(true) {}

PauliStrSum::~PauliStrSum() noexcept {
    reset();
}

PauliStrSum::PauliStrSum(PauliStrSum&& other) noexcept
    : sum_(other.sum_), owns_(std::exchange(other.owns_, false)) {}

PauliStrSum& PauliStrSum::operator=(PauliStrSum&& other) noexcept {
    if (this != &other) {
        reset();
        sum_ = other.sum_;
        owns_ = std::exchange(other.owns_, false);
    }
    return *this;
}

::PauliStrSum PauliStrSum::raw() const noexcept {
    return sum_;
}

void PauliStrSum::reset() noexcept {
    if (!owns_) {
        return;
    }

    owns_ = false;
    try {
        ::destroyPauliStrSum(sum_);
    } catch (...) {
    }
}

void init_quest_env() {
    ::initQuESTEnv();
    install_quest_input_error_handler();
}

void init_custom_quest_env(bool use_distrib, bool use_gpu_accel, bool use_multithread) {
    ::initCustomQuESTEnv(
        use_distrib ? 1 : 0,
        use_gpu_accel ? 1 : 0,
        use_multithread ? 1 : 0);
    install_quest_input_error_handler();
}

void finalize_quest_env() {
    if (::isQuESTEnvInit()) {
        ::finalizeQuESTEnv();
    }
}

void sync_quest_env() {
    ::syncQuESTEnv();
}

bool is_quest_env_init() {
    return ::isQuESTEnvInit() != 0;
}

QuestEnvironment get_quest_env() {
    const auto env = ::getQuESTEnv();
    return QuestEnvironment{
        env.isMultithreaded,
        env.isGpuAccelerated,
        env.isDistributed,
        env.isMpiUserOwned,
        env.isCuQuantumEnabled,
        env.isGpuSharingEnabled,
        env.isMpiGpuAware,
        env.rank,
        env.numNodes,
    };
}

rust::String get_environment_string() {
    std::array<char, 200> buffer{};
    ::getQuESTEnvironmentString(buffer.data());
    return rust::String(buffer.data());
}

std::unique_ptr<Qureg> create_qureg(std::int32_t num_qubits) {
    return std::make_unique<Qureg>(::createQureg(static_cast<int>(num_qubits)));
}

std::unique_ptr<Qureg> create_density_qureg(std::int32_t num_qubits) {
    return std::make_unique<Qureg>(::createDensityQureg(static_cast<int>(num_qubits)));
}

std::unique_ptr<CompMatr1> unique_ptr_marker_comp_matr1() {
    return {};
}

std::unique_ptr<CompMatr2> unique_ptr_marker_comp_matr2() {
    return {};
}

std::unique_ptr<DiagMatr1> unique_ptr_marker_diag_matr1() {
    return {};
}

std::unique_ptr<DiagMatr2> unique_ptr_marker_diag_matr2() {
    return {};
}

std::unique_ptr<DiagMatr> unique_ptr_marker_diag_matr() {
    return {};
}

std::unique_ptr<FullStateDiagMatr> unique_ptr_marker_full_state_diag_matr() {
    return {};
}

std::unique_ptr<PauliStr> unique_ptr_marker_pauli_str() {
    return {};
}

void init_zero_state(Qureg& qureg) {
    ::initZeroState(qureg.raw());
}

void init_plus_state(Qureg& qureg) {
    ::initPlusState(qureg.raw());
}

void init_arbitrary_pure_state(Qureg& qureg, rust::Slice<const QuestComplex> amps) {
    const auto raw = qureg.raw();
    if (raw.numAmps != static_cast<qindex>(amps.size())) {
        throw std::invalid_argument("init_arbitrary_pure_state: amplitude slice length must match Qureg dimension");
    }

    auto converted = to_qcomp_vec(amps);
    ::initArbitraryPureState(raw, converted.data());
}

QuestComplex get_qureg_amp(const Qureg& qureg, std::int64_t index) {
    return from_qcomp(::getQuregAmp(qureg.raw(), static_cast<qindex>(index)));
}

rust::Vec<QuestComplex> get_qureg_amps(const Qureg& qureg, std::int64_t start_index, std::int64_t num_amps) {
    const auto amps = ::getQuregAmps(
        qureg.raw(),
        static_cast<qindex>(start_index),
        static_cast<qindex>(num_amps));

    rust::Vec<QuestComplex> out;
    out.reserve(amps.size());
    for (const auto amp : amps) {
        out.push_back(from_qcomp(amp));
    }
    return out;
}

double calc_total_prob(const Qureg& qureg) {
    return static_cast<double>(::calcTotalProb(qureg.raw()));
}

std::int32_t apply_qubit_measurement(Qureg& qureg, std::int32_t target) {
    return static_cast<std::int32_t>(::applyQubitMeasurement(qureg.raw(), static_cast<int>(target)));
}

QubitMeasurement apply_qubit_measurement_and_get_prob(Qureg& qureg, std::int32_t target) {
    qreal probability = 0;
    const int outcome = ::applyQubitMeasurementAndGetProb(
        qureg.raw(),
        static_cast<int>(target),
        &probability);

    return QubitMeasurement{
        static_cast<std::int32_t>(outcome),
        static_cast<double>(probability),
    };
}

std::unique_ptr<CompMatr> create_comp_matr(std::int32_t num_qubits) {
    return std::make_unique<CompMatr>(::createCompMatr(static_cast<int>(num_qubits)));
}

void set_comp_matr_flat(CompMatr& matrix, rust::Slice<const QuestComplex> values, std::int64_t num_rows) {
    if (num_rows <= 0) {
        throw std::invalid_argument("set_comp_matr: matrix must have at least one row");
    }
    if (matrix.num_rows() != num_rows) {
        throw std::invalid_argument("set_comp_matr: row count must match CompMatr dimension");
    }

    const auto rows = static_cast<std::size_t>(num_rows);
    if (values.size() != rows * rows) {
        throw std::invalid_argument("set_comp_matr: flattened values length must equal rows squared");
    }

    std::vector<std::vector<qcomp>> nested;
    nested.reserve(rows);
    for (std::size_t row = 0; row < rows; ++row) {
        std::vector<qcomp> converted_row;
        converted_row.reserve(rows);
        for (std::size_t col = 0; col < rows; ++col) {
            converted_row.push_back(to_qcomp(values[row * rows + col]));
        }
        nested.push_back(std::move(converted_row));
    }

    ::setCompMatr(matrix.raw(), std::move(nested));
}

void apply_comp_matr(Qureg& qureg, rust::Slice<const std::int32_t> targets, const CompMatr& matrix) {
    ::applyCompMatr(qureg.raw(), to_int_vec(targets), matrix.raw());
}

void leftapply_comp_matr(Qureg& qureg, rust::Slice<const std::int32_t> targets, const CompMatr& matrix) {
    ::leftapplyCompMatr(qureg.raw(), to_int_vec(targets), matrix.raw());
}

void rightapply_comp_matr(Qureg& qureg, rust::Slice<const std::int32_t> targets, const CompMatr& matrix) {
    ::rightapplyCompMatr(qureg.raw(), to_int_vec(targets), matrix.raw());
}

void apply_hadamard(Qureg& qureg, std::int32_t target) {
    ::applyHadamard(qureg.raw(), static_cast<int>(target));
}

void apply_pauli_x(Qureg& qureg, std::int32_t target) {
    ::applyPauliX(qureg.raw(), static_cast<int>(target));
}

void apply_pauli_y(Qureg& qureg, std::int32_t target) {
    ::applyPauliY(qureg.raw(), static_cast<int>(target));
}

void apply_pauli_z(Qureg& qureg, std::int32_t target) {
    ::applyPauliZ(qureg.raw(), static_cast<int>(target));
}

void mix_dephasing(Qureg& qureg, std::int32_t target, double probability) {
    ::mixDephasing(qureg.raw(), static_cast<int>(target), static_cast<qreal>(probability));
}

std::unique_ptr<SuperOp> create_super_op(std::int32_t num_qubits) {
    return std::make_unique<SuperOp>(::createSuperOp(static_cast<int>(num_qubits)));
}

std::unique_ptr<KrausMap> create_kraus_map(std::int32_t num_qubits, std::int32_t num_operators) {
    return std::make_unique<KrausMap>(
        ::createKrausMap(static_cast<int>(num_qubits), static_cast<int>(num_operators)));
}

std::unique_ptr<PauliStrSum> create_inline_pauli_str_sum(rust::Str spec) {
    const std::string text(spec.data(), spec.size());
    return std::make_unique<PauliStrSum>(::createInlinePauliStrSum(text));
}

void apply_trotterized_unitary_time_evolution(
    Qureg& qureg,
    const PauliStrSum& hamiltonian,
    double time,
    std::int32_t order,
    std::int32_t reps,
    bool permute_terms) {
    ::applyTrotterizedUnitaryTimeEvolution(
        qureg.raw(),
        hamiltonian.raw(),
        static_cast<qreal>(time),
        static_cast<int>(order),
        static_cast<int>(reps),
        permute_terms);
}

} // namespace quest_sys
