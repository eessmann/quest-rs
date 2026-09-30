#include "common.hpp"
#include <hpx/modules/executors.hpp>
int main() {
    spdlog::set_level(spdlog::level::off);
    std::cout<<std::setprecision(17)<<"REFERENCE develop 4fc35983138d07a990862a4d83ad16f2b737c98f\n";
    for(auto const& f:fixtures()) {
        begin(f);
        if(f.real_parity) {
            auto request=qsp_tools::solvers::make_polynomial_phase_request(qsp_tools::polynomial::Chebyshev<Scalar>{coefficients(f),f.offset});
            if(!request){std::cout<<"FAIL admission\nEND\n";continue;}
            auto result=qsp_tools::solvers::run_qsp(hpx::execution::sequenced_executor{},std::move(*request));
            if(!result){std::cout<<"FAIL synthesis\nEND\n";continue;}
            auto const* outcome=std::get_if<qsp_tools::solvers::PhasePipelineOutcome<double>>(&result->outcome());
            if(!outcome)return 2;
            phases(outcome->phases);
            std::cout<<"CERT exported_phase "<<outcome->evidence.exported_phase_certificate.response_upper_bound()<<'\n';
        } else {
            auto request=qsp_tools::solvers::make_polynomial_control_request(qsp_tools::polynomial::Laurent<Scalar>{coefficients(f),f.offset});
            if(!request){std::cout<<"FAIL admission\nEND\n";continue;}
            auto result=qsp_tools::solvers::run_qsp(hpx::execution::sequenced_executor{},std::move(*request));
            if(!result){std::cout<<"FAIL synthesis\nEND\n";continue;}
            auto const* outcome=std::get_if<qsp_tools::solvers::ControlPipelineOutcome<double>>(&result->outcome());
            if(!outcome)return 2;
            controls(outcome->controls);
            std::cout<<"CERT pre_lowering "<<result->reflection().direct_certificate.response_upper_bound()<<'\n';
        }
        std::cout<<"NORMALIZATION 1\nEND\n";
    }
    auto probe=qsp_tools::solvers::make_polynomial_phase_request(
      qsp_tools::polynomial::Chebyshev<Scalar>{{Scalar{0.2,1e-14},Scalar{1e-14,0},Scalar{0.1,0}},0});
    if(!probe){std::cout<<"PROJECTION rejected\n";}
    else {auto projected=qsp_tools::solvers::run_qsp(hpx::execution::sequenced_executor{},std::move(*probe));
      std::cout<<"PROJECTION "<<(projected?"accepted":"rejected")<<'\n';}

}
