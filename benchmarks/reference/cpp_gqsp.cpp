// Generate a full-phase complex GQSP fixture from the pinned C++
// implementation.
#include "polynomial.hpp"
#include "qsp_solver.hpp"
#include "reference_reports.hpp"
#include <complex>
#include <fstream>
#include <hpx/modules/executors.hpp>
#include <iostream>
int main(int argc, char **argv) {
  if (argc != 2)
    return 2;
  spdlog::set_level(spdlog::level::off);
  Eigen::VectorX<std::complex<double>> values(3);
  values << std::complex<double>{0.25, 0.0}, std::complex<double>{0.0, 0.2},
      std::complex<double>{-0.1, 0.1};
  auto request = qsp_tools::solvers::make_polynomial_control_request(
      qsp_tools::polynomial::Laurent<std::complex<double>>{std::move(values),
                                                           0},
      qsp_tools::solvers::PipelineOptions<double>{});
  if (!request)
    return 3;
  auto future = qsp_tools::solvers::detail::submit_qsp_observed(
      hpx::execution::sequenced_executor{}, std::move(*request),
      qsp_tools::detail::graph_execution::RootDescriptor{}, 1, 1,
      qsp_tools::detail::graph_execution::NoopObserver{});
  auto result = future.get();
  if (!result)
    return 4;
  auto const *controls =
      std::get_if<qsp_tools::solvers::ControlPipelineOutcome<double>>(
          &result->outcome());
  if (!controls)
    return 5;
  std::vector<std::array<double, 2>> values_json;
  values_json.reserve(controls->controls.gates.size() * 4);
  for (auto const &gate : controls->controls.gates) {
    for (int r = 0; r < 2; ++r)
      for (int c = 0; c < 2; ++c)
        values_json.push_back({gate(r, c).real(), gate(r, c).imag()});
  }
  std::ofstream output(argv[1]);
  try {
    write_reference_report(output, values_json);
  } catch (std::exception const &error) {
    std::cerr << error.what() << '\n';
    return 6;
  }
  output.close();
  return output ? 0 : 6;
}
