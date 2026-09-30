#include "common.hpp"
int main() {
    spdlog::set_level(spdlog::level::off);
    std::cout<<std::setprecision(17)<<"REFERENCE main a932e7e081ac3766cad19ad6f8f4b920c8fa7fcf\n";
    for(auto const& f:fixtures()) {
        begin(f);
        if(f.real_parity) {
            auto result=qsp_tools::solvers::NLFTSolver<Scalar>{}.find_qsp_angles(qsp_tools::polynomial::Chebyshev<Scalar>{coefficients(f),f.offset});
            if(result)phases(*result);else std::cout<<"FAIL "<<static_cast<int>(result.error())<<'\n';
        } else {
            auto result=qsp_tools::solvers::NLFTSolver<Scalar>{}.find_gqsp_control_gates(qsp_tools::polynomial::Laurent<Scalar>{coefficients(f),f.offset});
            if(result)controls(*result);else std::cout<<"FAIL "<<static_cast<int>(result.error())<<'\n';
        }
        std::cout<<"CERT none\nNORMALIZATION 1\nEND\n";
    }
    auto projected=qsp_tools::solvers::NLFTSolver<Scalar>{}.find_qsp_angles(
      qsp_tools::polynomial::Chebyshev<Scalar>{{Scalar{0.2,1e-14},Scalar{1e-14,0},Scalar{0.1,0}},0});
    std::cout<<"PROJECTION "<<(projected?"accepted":"rejected")<<'\n';

}
