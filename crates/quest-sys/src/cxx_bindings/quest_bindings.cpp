#include "quest_bindings.hpp"

#include "quest-sys/src/lib.rs.h"

#include <algorithm>
#include <array>
#include <atomic>
#include <cfenv>
#include <cmath>
#include <complex>
#include <exception>
#include <limits>
#include <stdexcept>
#include <string>
#include <utility>
#include <vector>

#if defined(__x86_64__) || defined(__i386__)
#include <xmmintrin.h>
#endif

namespace quest_sys {
namespace {

enum class Lifecycle { Unattempted, Initializing, Active, Finalized, Failed };
std::recursive_mutex lifecycle_mutex;
Lifecycle lifecycle = Lifecycle::Unattempted;
// One-way admission barrier independent of the lifecycle mutex: even failure
// to acquire that mutex during owner destruction must retire the runtime.
std::atomic<bool> retired{false};
std::uint64_t owner_token = 0;
std::uint64_t next_thread_token = 0;

void ensure_not_retired() {
  if (retired.load(std::memory_order_acquire)) {
    throw std::runtime_error(
        "quest-sys lifecycle: environment permanently retired after "
        "unsuccessful cleanup");
  }
}

// Access only under lifecycle_mutex. Unlike std::thread::id, this token cannot
// be reused by a later thread after the initialization thread has exited.
std::uint64_t calling_thread_token() {
  thread_local std::uint64_t token = 0;
  if (token == 0) {
    if (next_thread_token == std::numeric_limits<std::uint64_t>::max()) {
      std::terminate();
    }
    token = ++next_thread_token;
  }
  return token;
}

void ensure_owner_thread() {
  if (owner_token != calling_thread_token()) {
    throw std::runtime_error(
        "quest-sys lifecycle: native calls require the environment owner "
        "thread");
  }
}

template <typename Initialize>
void initialize_environment(Initialize initialize) {
  const std::lock_guard lock(lifecycle_mutex);
  ensure_not_retired();
  if (lifecycle != Lifecycle::Unattempted) {
    throw std::runtime_error(
        "quest-sys lifecycle: environment initialization may only be attempted "
        "once");
  }
  owner_token = calling_thread_token();
  lifecycle = Lifecycle::Initializing;
  try {
    initialize();
    lifecycle = Lifecycle::Active;
  } catch (...) {
    lifecycle = Lifecycle::Failed;
    throw;
  }
}

enum class ResourceKind : std::size_t {
  Qureg = 0,
  CompMatr,
  DiagMatr,
  FullStateDiagMatr,
  SuperOp,
  KrausMap,
  PauliStrSum,
  Count,
};

constexpr auto RESOURCE_KIND_COUNT =
    static_cast<std::size_t>(ResourceKind::Count);

std::array<std::atomic<std::int64_t>, RESOURCE_KIND_COUNT> live_resources{};
constexpr std::array<const char*, RESOURCE_KIND_COUNT> resource_names = {
    "Qureg",   "CompMatr", "DiagMatr",    "FullStateDiagMatr",
    "SuperOp", "KrausMap", "PauliStrSum",
};

std::size_t resource_index(ResourceKind kind) noexcept {
  return static_cast<std::size_t>(kind);
}

void register_resource(ResourceKind kind) noexcept {
  live_resources[resource_index(kind)].fetch_add(1, std::memory_order_relaxed);
}

void unregister_resource(ResourceKind kind) noexcept {
  const auto previous = live_resources[resource_index(kind)].fetch_sub(
      1, std::memory_order_relaxed);
  if (previous <= 0) {
    std::terminate();
  }
}

std::int64_t live_resource_count() noexcept {
  std::int64_t total = 0;
  for (const auto& count : live_resources) {
    total += count.load(std::memory_order_relaxed);
  }
  return total;
}

std::string live_resource_summary() {
  std::string summary;
  for (std::size_t index = 0; index < live_resources.size(); ++index) {
    const auto count = live_resources[index].load(std::memory_order_relaxed);
    if (count == 0) {
      continue;
    }
    if (!summary.empty()) {
      summary += ", ";
    }
    summary += resource_names[index];
    summary += "=";
    summary += std::to_string(count);
  }
  return summary.empty() ? "none" : summary;
}

void ensure_no_live_resources_before_finalize() {
  if (live_resource_count() != 0) {
    throw std::runtime_error(
        "quest-sys lifecycle: cannot finalize QuEST environment while RAII "
        "resources are live (" +
        live_resource_summary() + ")");
  }
}

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

std::size_t checked_extent(std::int64_t value) {
  if (value <= 0 || static_cast<std::uint64_t>(value) >
                        std::numeric_limits<std::size_t>::max()) {
    throw std::invalid_argument(
        "array dimensions must be positive and representable as size_t");
  }
  return static_cast<std::size_t>(value);
}

std::size_t checked_product(std::size_t left, std::size_t right) {
  // Bound both native addressable storage and iterator difference arithmetic.
  constexpr auto limit =
      static_cast<std::size_t>(std::numeric_limits<std::ptrdiff_t>::max()) /
      sizeof(qcomp);
  if (right != 0 && left > limit / right) {
    throw std::invalid_argument(
        "complex array size exceeds addressable storage");
  }
  return left * right;
}

std::size_t density_rectangle_size(::Qureg raw,
                                   std::int64_t start_row,
                                   std::int64_t start_col,
                                   std::int64_t num_rows,
                                   std::int64_t num_cols) {
  if (!raw.isDensityMatrix) {
    throw std::invalid_argument(
        "density matrix access requires a density Qureg");
  }
  const auto rows = checked_extent(num_rows);
  const auto cols = checked_extent(num_cols);
  const auto dimension = qindex{1} << raw.numQubits;
  if (start_row < 0 || start_col < 0 || start_row >= dimension ||
      start_col >= dimension || num_rows > dimension - start_row ||
      num_cols > dimension - start_col) {
    throw std::invalid_argument(
        "density block lies outside the register dimension");
  }
  return checked_product(rows, cols);
}

std::vector<qcomp*> row_pointers(std::vector<qcomp>& values,
                                 std::size_t rows,
                                 std::size_t cols) {
  std::vector<qcomp*> pointers;
  pointers.reserve(rows);
  for (std::size_t row = 0; row < rows; ++row) {
    pointers.push_back(values.data() + row * cols);
  }
  return pointers;
}

}  // namespace

std::unique_lock<std::recursive_mutex> admit_native_call() {
  std::unique_lock lock(lifecycle_mutex);
  ensure_not_retired();
  if (lifecycle != Lifecycle::Active) {
    throw std::runtime_error(
        "quest-sys lifecycle: native calls require an active environment");
  }
  ensure_owner_thread();
  return lock;
}

Qureg::Qureg(::Qureg qureg) noexcept : qureg_(qureg), owns_(true) {
  register_resource(ResourceKind::Qureg);
}

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

  try {
    const auto admission = admit_native_call();
    ::destroyQureg(qureg_);
    owns_ = false;
    unregister_resource(ResourceKind::Qureg);
  } catch (...) {
    // Fail closed: retain the native allocation and live count.
    // Destruction must never cross FFI with an exception.
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

CompMatr::CompMatr(::CompMatr matrix) noexcept : matrix_(matrix), owns_(true) {
  register_resource(ResourceKind::CompMatr);
}

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

  try {
    const auto admission = admit_native_call();
    ::destroyCompMatr(matrix_);
    owns_ = false;
    unregister_resource(ResourceKind::CompMatr);
  } catch (...) {
    // Fail closed: retain the native allocation and live count.
    // Destruction must never cross FFI with an exception.
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

DiagMatr::DiagMatr(::DiagMatr matrix) noexcept : matrix_(matrix), owns_(true) {
  register_resource(ResourceKind::DiagMatr);
}

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

  try {
    const auto admission = admit_native_call();
    ::destroyDiagMatr(matrix_);
    owns_ = false;
    unregister_resource(ResourceKind::DiagMatr);
  } catch (...) {
    // Fail closed: retain the native allocation and live count.
    // Destruction must never cross FFI with an exception.
  }
}

FullStateDiagMatr::FullStateDiagMatr(::FullStateDiagMatr matrix) noexcept
    : matrix_(matrix), owns_(true) {
  register_resource(ResourceKind::FullStateDiagMatr);
}

FullStateDiagMatr::~FullStateDiagMatr() noexcept {
  reset();
}

FullStateDiagMatr::FullStateDiagMatr(FullStateDiagMatr&& other) noexcept
    : matrix_(other.matrix_), owns_(std::exchange(other.owns_, false)) {}

FullStateDiagMatr& FullStateDiagMatr::operator=(
    FullStateDiagMatr&& other) noexcept {
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

  try {
    const auto admission = admit_native_call();
    ::destroyFullStateDiagMatr(matrix_);
    owns_ = false;
    unregister_resource(ResourceKind::FullStateDiagMatr);
  } catch (...) {
    // Fail closed: retain the native allocation and live count.
    // Destruction must never cross FFI with an exception.
  }
}

SuperOp::SuperOp(::SuperOp op) noexcept : op_(op), owns_(true) {
  register_resource(ResourceKind::SuperOp);
}

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

  try {
    const auto admission = admit_native_call();
    ::destroySuperOp(op_);
    owns_ = false;
    unregister_resource(ResourceKind::SuperOp);
  } catch (...) {
    // Fail closed: retain the native allocation and live count.
    // Destruction must never cross FFI with an exception.
  }
}

KrausMap::KrausMap(::KrausMap map) noexcept : map_(map), owns_(true) {
  register_resource(ResourceKind::KrausMap);
}

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

  try {
    const auto admission = admit_native_call();
    ::destroyKrausMap(map_);
    owns_ = false;
    unregister_resource(ResourceKind::KrausMap);
  } catch (...) {
    // Fail closed: retain the native allocation and live count.
    // Destruction must never cross FFI with an exception.
  }
}

PauliStr::PauliStr(::PauliStr str) noexcept : str_(str) {}

::PauliStr PauliStr::raw() const noexcept {
  return str_;
}

PauliStrSum::PauliStrSum(::PauliStrSum sum) noexcept : sum_(sum), owns_(true) {
  register_resource(ResourceKind::PauliStrSum);
}

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

  try {
    const auto admission = admit_native_call();
    ::destroyPauliStrSum(sum_);
    owns_ = false;
    unregister_resource(ResourceKind::PauliStrSum);
  } catch (...) {
    // Fail closed: retain the native allocation and live count.
    // Destruction must never cross FFI with an exception.
  }
}

void init_quest_env() {
  initialize_environment([] {
    ::initQuESTEnv();
    install_quest_input_error_handler();
  });
}

void init_custom_quest_env(bool use_distrib,
                           bool use_gpu_accel,
                           bool use_multithread) {
  init_custom_quest_env_modes(use_distrib ? 1 : 0, use_gpu_accel ? 1 : 0,
                              use_multithread ? 1 : 0);
}

void init_custom_quest_env_modes(std::int32_t use_distrib,
                                 std::int32_t use_gpu_accel,
                                 std::int32_t use_multithread) {
  for (const auto mode : {use_distrib, use_gpu_accel, use_multithread}) {
    if (mode < -1 || mode > 1) {
      throw std::invalid_argument(
          "deployment modes must be -1 (automatic), 0, or 1");
    }
  }
  initialize_environment([=] {
    ::initCustomQuESTEnv(use_distrib, use_gpu_accel, use_multithread);
    install_quest_input_error_handler();
  });
}

void finalize_quest_env() {
  const std::lock_guard lock(lifecycle_mutex);
  if (lifecycle == Lifecycle::Finalized) {
    return;
  }
  ensure_not_retired();
  if (lifecycle == Lifecycle::Unattempted) {
    return;
  }
  const auto admission = admit_native_call();
  ensure_no_live_resources_before_finalize();
  try {
    ::finalizeQuESTEnv();
    lifecycle = Lifecycle::Finalized;
  } catch (...) {
    lifecycle = Lifecycle::Failed;
    throw;
  }
}

void finalize_quest_env_on_drop() noexcept {
  try {
    const std::lock_guard lock(lifecycle_mutex);
    try {
      // Retain the ordinary finalizer's owner-thread and live-resource checks.
      finalize_quest_env();
    } catch (...) {
      // The owner is disappearing: unlike an explicit low-level finalization
      // rejection, this failure can never leave the runtime available for use.
      // Resource destructors will preserve native storage after retirement.
      lifecycle = Lifecycle::Failed;
      retired.store(true, std::memory_order_release);
    }
  } catch (...) {
    // Do not touch mutex-protected state without its lock. The independent
    // latch still prevents initialization and subsequent native admission.
    retired.store(true, std::memory_order_release);
  }
}

void sync_quest_env() {
  const auto admission = admit_native_call();
  ::syncQuESTEnv();
}

bool is_quest_env_init() {
  if (retired.load(std::memory_order_acquire)) {
    return false;
  }
  const std::lock_guard lock(lifecycle_mutex);
  return !retired.load(std::memory_order_acquire) &&
         lifecycle == Lifecycle::Active;
}

QuestEnvironment get_quest_env() {
  const auto admission = admit_native_call();
  const auto env = ::getQuESTEnv();
  return QuestEnvironment{
      env.isMultithreaded,    env.isGpuAccelerated,
      env.isDistributed,      env.isMpiUserOwned,
      env.isCuQuantumEnabled, env.isGpuSharingEnabled,
      env.isMpiGpuAware,      env.rank,
      env.numNodes,
  };
}

rust::String get_environment_string() {
  const auto admission = admit_native_call();
  std::array<char, 200> buffer{};
  ::getQuESTEnvironmentString(buffer.data());
  return rust::String(buffer.data());
}

NumericalFingerprint get_numerical_fingerprint() {
  const auto admission = admit_native_call();
  const auto rounding = std::fegetround();
  std::uint64_t control = 0;
  bool nearest = rounding == FE_TONEAREST;
  bool flush = false;
  bool denormals = false;
  bool supported = false;
#if defined(__x86_64__) || defined(__i386__)
  // MXCSR rounding, flush-to-zero, and denormals-are-zero; exclude status
  // flags.
  control = _mm_getcsr() & ((3u << 13) | (1u << 15) | (1u << 6));
  nearest = nearest && (control & (3u << 13)) == 0;
  flush = (control & (1u << 15)) != 0;
  denormals = (control & (1u << 6)) != 0;
  supported = true;
#elif defined(__aarch64__)
  std::uint64_t fpcr;
  asm volatile("mrs %0, fpcr" : "=r"(fpcr));
  // Include FEAT_AFP's FIZ and AH controls as well as RMode and FZ.
  // Arm AAPCS64 defines these as mutable process numerical policy bits.
  control = fpcr & ((std::uint64_t{3} << 22) | (std::uint64_t{1} << 24) | 3);
  nearest = nearest && (control & (std::uint64_t{3} << 22)) == 0;
  flush = (control & (std::uint64_t{1} << 24)) != 0;
  denormals = flush || (control & 1) != 0;
  // Alternate handling changes underflow semantics; admit only the baseline
  // profile until that policy has its own numerical validation.
  supported = (control & 2) == 0;
#endif
  return NumericalFingerprint{
      rounding,
      control,
      nearest,
      flush,
      denormals,
      supported,
      static_cast<double>(::getQuESTValidationEpsilon())};
}

void set_qu_est_seeds(rust::Slice<const std::uint32_t> seeds) {
  const auto admission = admit_native_call();
  if (seeds.empty() || seeds.size() > static_cast<std::size_t>(
                                          std::numeric_limits<int>::max())) {
    throw std::invalid_argument("seed count must be positive and fit int");
  }
  std::vector<unsigned> converted(seeds.begin(), seeds.end());
  ::setQuESTSeeds(converted.data(), static_cast<int>(converted.size()));
}

rust::Vec<std::uint32_t> get_qu_est_seeds() {
  const auto admission = admit_native_call();
  const auto seeds = ::getQuESTSeeds();
  rust::Vec<std::uint32_t> out;
  out.reserve(seeds.size());
  for (const auto seed : seeds) {
    out.push_back(seed);
  }
  return out;
}

std::unique_ptr<Qureg> create_qureg(std::int32_t num_qubits) {
  const auto admission = admit_native_call();
  return std::unique_ptr<Qureg>(
      new Qureg(::createQureg(static_cast<int>(num_qubits))));
}

std::unique_ptr<Qureg> create_density_qureg(std::int32_t num_qubits) {
  const auto admission = admit_native_call();
  return std::unique_ptr<Qureg>(
      new Qureg(::createDensityQureg(static_cast<int>(num_qubits))));
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
  const auto admission = admit_native_call();
  ::initZeroState(qureg.raw());
}

void init_plus_state(Qureg& qureg) {
  const auto admission = admit_native_call();
  ::initPlusState(qureg.raw());
}

void init_arbitrary_pure_state(Qureg& qureg,
                               rust::Slice<const QuestComplex> amps) {
  const auto admission = admit_native_call();
  const auto raw = qureg.raw();
  const auto expected = std::uint64_t{1} << raw.numQubits;
  if (expected != amps.size()) {
    throw std::invalid_argument(
        "init_arbitrary_pure_state: amplitude slice length must match Qureg "
        "dimension");
  }

  auto converted = to_qcomp_vec(amps);
  ::initArbitraryPureState(raw, converted.data());
}

QuestComplex get_qureg_amp(const Qureg& qureg, std::int64_t index) {
  const auto admission = admit_native_call();
  return from_qcomp(::getQuregAmp(qureg.raw(), static_cast<qindex>(index)));
}

rust::Vec<QuestComplex> get_qureg_amps(const Qureg& qureg,
                                       std::int64_t start_index,
                                       std::int64_t num_amps) {
  const auto admission = admit_native_call();
  const auto amps =
      ::getQuregAmps(qureg.raw(), static_cast<qindex>(start_index),
                     static_cast<qindex>(num_amps));

  rust::Vec<QuestComplex> out;
  out.reserve(amps.size());
  for (const auto amp : amps) {
    out.push_back(from_qcomp(amp));
  }
  return out;
}

double calc_total_prob(const Qureg& qureg) {
  const auto admission = admit_native_call();
  return static_cast<double>(::calcTotalProb(qureg.raw()));
}

void set_density_qureg_amps(Qureg& qureg,
                            std::int64_t start_row,
                            std::int64_t start_col,
                            rust::Slice<const QuestComplex> values,
                            std::int64_t num_rows,
                            std::int64_t num_cols) {
  const auto admission = admit_native_call();
  const auto count = density_rectangle_size(qureg.raw(), start_row, start_col,
                                            num_rows, num_cols);
  if (values.size() != count) {
    throw std::invalid_argument(
        "density buffer length must equal rows times columns");
  }
  auto converted = to_qcomp_vec(values);
  auto pointers = row_pointers(converted, static_cast<std::size_t>(num_rows),
                               static_cast<std::size_t>(num_cols));
  // The C++ vector overload in QuEST 4.3.0 incorrectly supplies numRows twice.
  // Use the C overload with independent, checked row and column dimensions.
  ::setDensityQuregAmps(qureg.raw(), start_row, start_col, pointers.data(),
                        num_rows, num_cols);
}

rust::Vec<QuestComplex> get_density_qureg_amps(const Qureg& qureg,
                                               std::int64_t start_row,
                                               std::int64_t start_col,
                                               std::int64_t num_rows,
                                               std::int64_t num_cols) {
  const auto admission = admit_native_call();
  const auto count = density_rectangle_size(qureg.raw(), start_row, start_col,
                                            num_rows, num_cols);
  std::vector<qcomp> converted(count);
  auto pointers = row_pointers(converted, static_cast<std::size_t>(num_rows),
                               static_cast<std::size_t>(num_cols));
  ::getDensityQuregAmps(pointers.data(), qureg.raw(), start_row, start_col,
                        num_rows, num_cols);
  rust::Vec<QuestComplex> out;
  out.reserve(count);
  for (const auto value : converted) {
    out.push_back(from_qcomp(value));
  }
  return out;
}

void apply_global_phase(Qureg& qureg, double angle) {
  const auto admission = admit_native_call();
  if (!std::isfinite(angle)) {
    throw std::invalid_argument("global phase angle must be finite");
  }
  auto raw = qureg.raw();
  if (raw.isDensityMatrix) {
    return;
  }
  qcomp coefficient(std::cos(angle), std::sin(angle));
  ::setQuregToWeightedSum(raw, &coefficient, &raw, 1);
}

std::int32_t apply_qubit_measurement(Qureg& qureg, std::int32_t target) {
  const auto admission = admit_native_call();
  return static_cast<std::int32_t>(
      ::applyQubitMeasurement(qureg.raw(), static_cast<int>(target)));
}

QubitMeasurement apply_qubit_measurement_and_get_prob(Qureg& qureg,
                                                      std::int32_t target) {
  const auto admission = admit_native_call();
  qreal probability = 0;
  const int outcome = ::applyQubitMeasurementAndGetProb(
      qureg.raw(), static_cast<int>(target), &probability);

  return QubitMeasurement{
      static_cast<std::int32_t>(outcome),
      static_cast<double>(probability),
  };
}

std::unique_ptr<CompMatr> create_comp_matr(std::int32_t num_qubits) {
  const auto admission = admit_native_call();
  return std::unique_ptr<CompMatr>(
      new CompMatr(::createCompMatr(static_cast<int>(num_qubits))));
}

void set_comp_matr_flat(CompMatr& matrix,
                        rust::Slice<const QuestComplex> values,
                        std::int64_t num_rows) {
  const auto admission = admit_native_call();
  if (num_rows <= 0) {
    throw std::invalid_argument(
        "set_comp_matr: matrix must have at least one row");
  }
  if (matrix.num_rows() != num_rows) {
    throw std::invalid_argument(
        "set_comp_matr: row count must match CompMatr dimension");
  }

  const auto rows = checked_extent(num_rows);
  if (values.size() != checked_product(rows, rows)) {
    throw std::invalid_argument(
        "set_comp_matr: flattened values length must equal rows squared");
  }

  auto converted = to_qcomp_vec(values);
  auto pointers = row_pointers(converted, rows, rows);
  ::setCompMatr(matrix.raw(), pointers.data());
}

void apply_comp_matr(Qureg& qureg,
                     rust::Slice<const std::int32_t> targets,
                     const CompMatr& matrix) {
  const auto admission = admit_native_call();
  ::applyCompMatr(qureg.raw(), to_int_vec(targets), matrix.raw());
}

void leftapply_comp_matr(Qureg& qureg,
                         rust::Slice<const std::int32_t> targets,
                         const CompMatr& matrix) {
  const auto admission = admit_native_call();
  ::leftapplyCompMatr(qureg.raw(), to_int_vec(targets), matrix.raw());
}

void rightapply_comp_matr(Qureg& qureg,
                          rust::Slice<const std::int32_t> targets,
                          const CompMatr& matrix) {
  const auto admission = admit_native_call();
  ::rightapplyCompMatr(qureg.raw(), to_int_vec(targets), matrix.raw());
}

void apply_hadamard(Qureg& qureg, std::int32_t target) {
  const auto admission = admit_native_call();
  ::applyHadamard(qureg.raw(), static_cast<int>(target));
}

void apply_pauli_x(Qureg& qureg, std::int32_t target) {
  const auto admission = admit_native_call();
  ::applyPauliX(qureg.raw(), static_cast<int>(target));
}

void apply_pauli_y(Qureg& qureg, std::int32_t target) {
  const auto admission = admit_native_call();
  ::applyPauliY(qureg.raw(), static_cast<int>(target));
}

void apply_pauli_z(Qureg& qureg, std::int32_t target) {
  const auto admission = admit_native_call();
  ::applyPauliZ(qureg.raw(), static_cast<int>(target));
}

void mix_dephasing(Qureg& qureg, std::int32_t target, double probability) {
  const auto admission = admit_native_call();
  ::mixDephasing(qureg.raw(), static_cast<int>(target),
                 static_cast<qreal>(probability));
}

std::unique_ptr<SuperOp> create_super_op(std::int32_t num_qubits) {
  const auto admission = admit_native_call();
  return std::unique_ptr<SuperOp>(
      new SuperOp(::createSuperOp(static_cast<int>(num_qubits))));
}

std::unique_ptr<KrausMap> create_kraus_map(std::int32_t num_qubits,
                                           std::int32_t num_operators) {
  const auto admission = admit_native_call();
  return std::unique_ptr<KrausMap>(new KrausMap(::createKrausMap(
      static_cast<int>(num_qubits), static_cast<int>(num_operators))));
}

void set_kraus_map_flat(KrausMap& map,
                        rust::Slice<const QuestComplex> values,
                        std::int32_t num_operators,
                        std::int64_t num_rows) {
  const auto admission = admit_native_call();
  const auto raw = map.raw();
  if (num_operators != raw.numMatrices || num_rows != raw.numRows) {
    throw std::invalid_argument(
        "Kraus buffer dimensions must match the native map");
  }
  const auto operators = checked_extent(num_operators);
  const auto rows = checked_extent(num_rows);
  const auto total_rows = checked_product(operators, rows);
  if (values.size() != checked_product(total_rows, rows)) {
    throw std::invalid_argument(
        "Kraus buffer length must equal operators times rows squared");
  }
  auto converted = to_qcomp_vec(values);
  auto pointers = row_pointers(converted, total_rows, rows);
  std::vector<qcomp**> matrices;
  matrices.reserve(operators);
  for (std::size_t op = 0; op < operators; ++op) {
    matrices.push_back(pointers.data() + op * rows);
  }
  ::setKrausMap(raw, matrices.data());
}

std::unique_ptr<PauliStrSum> create_inline_pauli_str_sum(rust::Str spec) {
  const auto admission = admit_native_call();
  const std::string text(spec.data(), spec.size());
  return std::unique_ptr<PauliStrSum>(
      new PauliStrSum(::createInlinePauliStrSum(text)));
}

void apply_trotterized_unitary_time_evolution(Qureg& qureg,
                                              const PauliStrSum& hamiltonian,
                                              double time,
                                              std::int32_t order,
                                              std::int32_t reps,
                                              bool permute_terms) {
  const auto admission = admit_native_call();
  ::applyTrotterizedUnitaryTimeEvolution(
      qureg.raw(), hamiltonian.raw(), static_cast<qreal>(time),
      static_cast<int>(order), static_cast<int>(reps), permute_terms);
}

}  // namespace quest_sys
