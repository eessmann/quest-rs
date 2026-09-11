// Public-API execution reference from pinned quest-qsvt source, CPU/local only.
#include <algorithm>
#include <array>
#include <backend/quest_executor.hpp>
#include <chrono>
#include <cmath>
#include <complex>
#include <cstdint>
#include <encoding/projected_unitary_encoding.hpp>
#include <expected>
#include <hadamard.hpp>
#include <iomanip>
#include <iostream>
#include <optional>
#include <standard_qsvt.hpp>
#include <stdexcept>
#include <utility>
#include <vector>
namespace {
using Clock = std::chrono::steady_clock;
using Complex = std::complex<double>;
using qsvt_tools::backend::quest::QuESTExecutor;
template <class T, class E>
T require(std::expected<T, E> result, const char* stage) {
  if (!result)
    throw std::runtime_error(stage);
  return std::move(*result);
}
template <class E>
void require(std::expected<void, E> result, const char* stage) {
  if (!result)
    throw std::runtime_error(stage);
}
struct Environment {
  Environment() { initCustomQuESTEnv(0, 0, 0); }
  ~Environment() {
    if (isQuESTEnvInit())
      finalizeQuESTEnv();
  }
};
struct Register {
  Qureg value;
  explicit Register(int qubits)
      : value(createCustomQureg(qubits, 0, 0, 0, 0)) {}
  ~Register() { destroyQureg(value); }
  Register(const Register&) = delete;
};
auto fixture() {
  Eigen::MatrixXcd oracle(2, 2);
  auto a = Complex{0.3, 0.4};
  double complement = std::sqrt(1.0 - std::norm(a));
  oracle << a, complement, complement, -std::conj(a);
  Eigen::MatrixXcd basis = Eigen::MatrixXcd::Zero(2, 1);
  basis(0, 0) = 1.0;
  qsvt_tools::block_encoding::BlockEncoding<Eigen::MatrixXcd> block{
      .U = std::move(oracle),
      .PiL = basis,
      .PiR = std::move(basis),
      .alpha = 1.0};
  auto encoding = require(qsvt_tools::encoding::from_block_encoding(block),
                          "encoding admission");
  auto phases = qsp_tools::solvers::make_pyqsp_wx_symmetric_phases<double>(
      std::array{0.1, 0.2, 0.2, 0.1});
  if (!phases)
    throw std::runtime_error("phase admission");
  return require(qsvt_tools::create_qsvt_transform(encoding, *phases),
                 "standard construction");
}
// The scalar fixture's admitted input and output are single coordinate states.
// Derive those coordinates from public layout metadata, not a guessed
// convention.
std::size_t coordinate(const qsvt_tools::LogicalSubspaceLayout& layout) {
  const auto& basis = layout.isometry()->value();
  if (basis.cols() != 1)
    throw std::runtime_error("fixture is not scalar");
  std::size_t physical = 0;
  for (auto control : layout.ancillas().values())
    if (control.on)
      physical |= std::size_t{1} << control.qubit.value;
  std::optional<std::size_t> selected;
  for (Eigen::Index r = 0; r < basis.rows(); ++r) {
    if (basis(r, 0) == Complex{0.0, 0.0})
      continue;
    if (basis(r, 0) != Complex{1.0, 0.0} || selected)
      throw std::runtime_error("noncoordinate fixture");
    selected = static_cast<std::size_t>(r);
  }
  if (!selected)
    throw std::runtime_error("empty fixture projection");
  for (std::size_t bit = 0; bit < layout.query().size(); ++bit)
    if ((*selected & (std::size_t{1} << bit)) != 0)
      physical |= std::size_t{1} << layout.query()[bit].value;
  return physical;
}
struct Projection {
  std::vector<int> qubits, outcomes;
  Projection(std::size_t index, std::uint32_t width) {
    for (std::uint32_t q = 0; q < width; ++q) {
      qubits.push_back(static_cast<int>(q));
      outcomes.push_back(static_cast<int>((index >> q) & 1));
    }
  }
  void apply(Qureg q) const { applyMultiQubitProjector(q, qubits, outcomes); }
};
double elapsed(Clock::time_point start) {
  return std::chrono::duration<double>(Clock::now() - start).count();
}
}  // namespace
int main() {
  try {
    Environment environment;
    QuESTExecutor executor;
    auto transform = fixture();
    auto width = transform.circuit().shape().qubits;
    auto input_index = coordinate(*transform.input_layout());
    auto output_index = coordinate(*transform.output_layout());
    if (input_index != 0)
      throw std::runtime_error("fixture reset needs coordinate zero");
    Projection input(input_index, width), output(output_index, width);
    auto prepared =
        require(executor.prepare(transform.circuit()), "circuit preparation");
    Register state(static_cast<int>(width));
    initZeroState(state.value);
    input.apply(state.value);
    require(prepared.execute(state.value), "reference execution");
    output.apply(state.value);
    auto response = getQuregAmp(state.value, static_cast<qindex>(output_index));
    auto mass = calcTotalProb(state.value);
    if (!std::isfinite(mass) || std::abs(mass - std::norm(response)) > 1e-12)
      throw std::runtime_error("mass/reference mismatch");
    qsvt_tools::hadamard::OverlapStates vectors{Eigen::VectorXcd::Ones(1),
                                                Eigen::VectorXcd::Ones(1)};
    auto overlap = require(
        qsvt_tools::hadamard::prepare_overlap(executor, transform, vectors),
        "Hadamard preparation");
    auto observation = require(overlap.observe(), "Hadamard observation");
    if (std::abs(observation.overlap - response) > 1e-11)
      throw std::runtime_error("Hadamard/full-phase mismatch");
    constexpr int repetitions = 1000;
    double construction = 0, preparation = 0, execution = 0, conditioning = 0,
           hadamard = 0;
    for (int i = 0; i < repetitions; ++i) {
      auto start = Clock::now();
      {
        auto candidate = fixture();
        construction += elapsed(start);
      }
      start = Clock::now();
      {
        auto candidate =
            require(executor.prepare(transform.circuit()), "timed preparation");
        preparation += elapsed(start);
      }
      initZeroState(state.value);
      start = Clock::now();
      auto initial_mass = calcTotalProb(state.value);
      input.apply(state.value);
      auto input_mass = calcTotalProb(state.value);
      require(prepared.execute(state.value), "timed execution");
      output.apply(state.value);
      auto retained_mass = calcTotalProb(state.value);
      if (!std::isfinite(initial_mass) || !std::isfinite(input_mass) ||
          !std::isfinite(retained_mass))
        throw std::runtime_error("invalid timed mass");
      execution += elapsed(start);
      start = Clock::now();
      setQuregToRenormalized(state.value);
      conditioning += elapsed(start);
      start = Clock::now();
      require(overlap.observe(), "timed Hadamard observation");
      hadamard += elapsed(start);
    }
    std::cout
        << std::setprecision(17)
        << "{\"source_revision\":\"7fe7f740579b03c52a8cf48be6a31268b029c19f\","
           "\"cpu_local\":true,\"repetitions\":"
        << repetitions << ",\"response_real\":" << response.real()
        << ",\"response_imag\":" << response.imag()
        << ",\"retained_mass\":" << mass
        << ",\"hadamard_real\":" << observation.overlap.real()
        << ",\"hadamard_imag\":" << observation.overlap.imag()
        << ",\"hadamard_total_success_probability\":"
        << observation.total_success_probability
        << ",\"hadamard_active_probability_absolute\":"
        << observation.active_probability_absolute
        << ",\"construction_seconds\":" << construction / repetitions
        << ",\"combined_lowering_native_prepare_seconds\":"
        << preparation / repetitions
        << ",\"execution_with_coordinate_projections_seconds\":"
        << execution / repetitions
        << ",\"native_condition_seconds\":" << conditioning / repetitions
        << ",\"hadamard_observation_seconds\":" << hadamard / repetitions
        << "}\n";
    return 0;
  } catch (const std::exception& e) {
    std::cerr << e.what() << '\n';
    return 1;
  }
}
