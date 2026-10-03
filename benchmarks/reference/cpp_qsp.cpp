// Sequential comparison adapter for the pinned quest-qsvt source tree.
#include "polynomial.hpp"
#include "qsp_solver.hpp"
#include "reference_reports.hpp"
#include <array>
#include <bit>
#include <chrono>
#include <complex>
#include <cstdint>
#include <fstream>
#include <hpx/modules/executors.hpp>
#include <iostream>
#include <memory>
#include <unordered_map>
#include <vector>
namespace graph = qsp_tools::detail::graph_execution;
using Clock = std::chrono::steady_clock;
struct Measurements {
  struct Active {
    std::uint64_t correlation;
    std::size_t semantic;
  };
  std::unordered_map<std::uint64_t, Clock::time_point> starts;
  std::array<double, 24> seconds{};
  std::array<double, 24> exclusive_seconds{};
  std::vector<Active> active;
  Clock::time_point last = Clock::now();
};
struct Observer {
  std::shared_ptr<Measurements> data;
  void operator()(graph::GraphEvent event) const noexcept {
    auto now = Clock::now();
    if (!data->active.empty()) {
      auto i = data->active.back().semantic;
      if (i < data->exclusive_seconds.size())
        data->exclusive_seconds[i] +=
            std::chrono::duration<double>(now - data->last).count();
    }
    data->last = now;
    if (event.kind == graph::GraphEventKind::task_start) {
      data->starts[event.correlation_id] = now;
      data->active.push_back(
          {event.correlation_id, static_cast<std::size_t>(event.semantic)});
    } else if (event.kind == graph::GraphEventKind::task_finish) {
      auto start = data->starts.find(event.correlation_id);
      if (start != data->starts.end()) {
        auto i = static_cast<std::size_t>(event.semantic);
        if (i < data->seconds.size())
          data->seconds[i] +=
              std::chrono::duration<double>(now - start->second).count();
        data->starts.erase(start);
      }
      auto active = std::find_if(
          data->active.begin(), data->active.end(), [&](auto const &item) {
            return item.correlation == event.correlation_id;
          });
      if (active != data->active.end())
        data->active.erase(active);
    }
  }
};
int main(int argc, char **argv) {
  if (argc != 3 || std::endian::native != std::endian::little)
    return 2;
  spdlog::set_level(spdlog::level::off);
  std::ifstream input(argv[1], std::ios::binary | std::ios::ate);
  if (!input)
    return 3;
  auto bytes = input.tellg();
  if (bytes <= 0 || bytes % 8 != 0)
    return 4;
  input.seekg(0);
  Eigen::VectorX<std::complex<double>> coefficients(bytes / 8);
  for (Eigen::Index i = 0; i < coefficients.size(); ++i) {
    double value;
    input.read(reinterpret_cast<char *>(&value), sizeof(value));
    if (!input || !std::isfinite(value))
      return 5;
    coefficients(i) = {value, 0.0};
  }
  auto degree = coefficients.size() - 1;
  auto request = qsp_tools::solvers::make_polynomial_phase_request(
      qsp_tools::polynomial::Chebyshev<std::complex<double>>{
          std::move(coefficients), 0},
      qsp_tools::solvers::PipelineOptions<double>{});
  if (!request)
    return 6;
  auto measurements = std::make_shared<Measurements>();
  auto started = Clock::now();
  auto future = qsp_tools::solvers::detail::submit_qsp_observed(
      hpx::execution::sequenced_executor{}, std::move(*request),
      graph::RootDescriptor{}, 1, 1, Observer{measurements});
  auto result = future.get();
  auto seconds = std::chrono::duration<double>(Clock::now() - started).count();
  double response_bound = 0;
  if (result) {
    auto const *phases =
        std::get_if<qsp_tools::solvers::PhasePipelineOutcome<double>>(
            &result->outcome());
    if (!phases)
      return 7;
    response_bound =
        phases->evidence.exported_phase_certificate.response_upper_bound();
    std::ofstream output(argv[2], std::ios::binary);
    for (double phase : phases->phases.values())
      output.write(reinterpret_cast<char const *>(&phase), sizeof(phase));
    output.close();
    if (!output) {
      std::cerr << "Could not write complete phase output\n";
      return 8;
    }
  }
  ReferenceQspReport report{static_cast<std::size_t>(degree),
                            seconds,
                            bool(result),
                            result ? std::optional<double>{response_bound}
                                   : std::nullopt,
                            measurements->seconds,
                            measurements->exclusive_seconds,
                            measurements->active.size()};
  try {
    write_reference_report(std::cout, report);
  } catch (std::exception const &error) {
    std::cerr << error.what() << '\n';
    return 9;
  }
  return result ? 0 : 1;
}
