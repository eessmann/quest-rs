// Reuse the pinned source fixtures verbatim, never reimplement their RNG or
// basis-conversion algorithms. The include also registers their Catch tests.
#include "nlft/benchmark_fixture_tests.cpp"
#include <bit>
#include <cstdint>
#include <filesystem>
#include <fstream>
#include <iomanip>

namespace {
template <typename Polynomial>
void coefficients(std::ostream& out, Polynomial const& value, bool reverse_conjugate = false) {
    out << '[';
    for (Eigen::Index index = 0; index < value.size(); ++index) {
        if (index) out << ',';
        auto coefficient = value.coeffs_dense()[reverse_conjugate ? value.size()-index-1 : index];
        if (reverse_conjugate) coefficient = std::conj(coefficient);
        out << '[' << std::bit_cast<std::uint64_t>(coefficient.real()) << ',' << std::bit_cast<std::uint64_t>(coefficient.imag()) << ']';
    }
    out << ']';
}
void save_pair(std::filesystem::path const& output, int degree,
               test::nlft_bench::PreparedInverseFixture const& fixture) {
    auto stream = std::ofstream(output / ("order" + std::to_string(degree) + ".json"));
    stream << std::setprecision(std::numeric_limits<double>::max_digits10);
    stream << "{\"degree\":" << degree << ",\"canonical_coefficients_bits\":";
    coefficients(stream, fixture.target);
    stream << ",\"conjugate_complement_bits\":";
    coefficients(stream, fixture.factor, true);
    if (degree > 1000) {
        auto reflections = test::nlft_bench::make_forward_fixture_coefficients(degree);
        if (!reflections) throw std::runtime_error("source reflection fixture failed");
        stream << ",\"requested_tolerance_bits\":" << std::bit_cast<std::uint64_t>(test::nlft_bench::forward_fixture_component_tolerance(reflections->size()));
        stream << ",\"expected_reflections_bits\":[";
        for (std::size_t index = 0; index < reflections->size(); ++index) {
            if (index) stream << ',';
            auto value = (*reflections)[index];
            stream << '[' << std::bit_cast<std::uint64_t>(value.real()) << ',' << std::bit_cast<std::uint64_t>(value.imag()) << ']';
        }
        stream << ']';
    }
    stream << "}\n";
    if (!stream) throw std::runtime_error("cannot save exact pair");
}
}
int main(int argc, char** argv) {
    if (argc != 2) return 2;
    auto output = std::filesystem::path(argv[1]);
    std::filesystem::create_directories(output);
    // The named solver targets retain a basis/lowering contract. They are not
    // canonical scattering targets and must never be transported under that
    // label. Keep an explicit capability receipt instead of exporting arrays
    // that the Laurent-only Rust input reader would silently misinterpret.
    {
        auto stream = std::ofstream(output / "named-solver-export-status.json");
        stream << "{\"status\":\"unsupported\",\"scope\":\"named_solver_fixture_export\","
                  "\"reason\":\"Source basis and lowering contract are not represented by the canonical Laurent input schema\","
                  "\"case_count\":9}\n";
        if (!stream) throw std::runtime_error("cannot save solver export capability");
    }
    std::mt19937 rng(2723225244U);
    for (int degree : {5,10,20,50,100,200,500,1000}) {
        auto target = test::qsp::random_poly<Poly>(degree, 0.5, rng);
        auto fixture = test::nlft_bench::prepare_weiss_inverse_fixture(std::move(target), 1e-12);
        if (!fixture) throw std::runtime_error("source Weiss fixture preparation failed");
        save_pair(output, degree, *fixture);
    }
    for (int degree : {2000,5000,10000,20000,50000,100000,200000,500000,1000000}) {
        auto fixture = test::nlft_bench::prepare_forward_inverse_fixture(degree);
        if (!fixture) throw std::runtime_error("source forward fixture preparation failed");
        save_pair(output, degree, *fixture);
    }
}
