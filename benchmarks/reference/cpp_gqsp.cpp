// Generate a full-phase complex GQSP fixture from the pinned C++ implementation.
#include "qsp_solver.hpp"
#include "polynomial.hpp"
#include <hpx/modules/executors.hpp>
#include <complex>
#include <fstream>
#include <iostream>
#include <iomanip>
int main(int argc, char** argv) {
    if (argc != 2) return 2;
    spdlog::set_level(spdlog::level::off);
    Eigen::VectorX<std::complex<double>> values(3);
    values << std::complex<double>{0.25,0.0}, std::complex<double>{0.0,0.2}, std::complex<double>{-0.1,0.1};
    auto request = qsp_tools::solvers::make_polynomial_control_request(
        qsp_tools::polynomial::Laurent<std::complex<double>>{std::move(values),0},
        qsp_tools::solvers::PipelineOptions<double>{});
    if (!request) return 3;
    auto future = qsp_tools::solvers::detail::submit_qsp_observed(
        hpx::execution::sequenced_executor{}, std::move(*request),
        qsp_tools::detail::graph_execution::RootDescriptor{}, 1, 1,
        qsp_tools::detail::graph_execution::NoopObserver{});
    auto result = future.get();
    if (!result) return 4;
    auto const* controls = std::get_if<qsp_tools::solvers::ControlPipelineOutcome<double>>(&result->outcome());
    if (!controls) return 5;
    std::ofstream output(argv[1]);
    output << std::setprecision(17) << "[";
    bool first = true;
    for (auto const& gate : controls->controls.gates) {
        for (int r=0;r<2;++r) for (int c=0;c<2;++c) {
            if (!first) output << ',';
            first = false;
            output << '[' << gate(r,c).real() << ',' << gate(r,c).imag() << ']';
        }
    }
    output << "]\n";
    return output ? 0 : 6;
}
