#pragma once
#include <qsp_solver.hpp>
#include <polynomial.hpp>
#include <complex>
#include <iostream>
#include <iomanip>
#include <string>
#include <vector>
using Scalar=std::complex<double>;
using Values=std::vector<Scalar>;
struct Fixture { std::string name; bool real_parity; int offset; Values values; };
inline auto fixtures() -> std::vector<Fixture> {
    return {{"wx_constant",true,0,{{0.3,0}}},
            {"wx_odd",true,0,{{0,0},{0.6,0}}},
            {"wx_even",true,0,{{-0.1,0},{0,0},{0.2,0}}},
            {"wx_degree5",true,0,{{0,0},{0.15,0},{0,0},{-0.03,0},{0,0},{0.11,0}}},
            {"circle_constant",false,0,{{0.3,-0.4}}},
            {"circle_complex",false,0,{{0.25,0},{0,0.2},{-0.1,0.1}}},
            {"circle_offset",false,2,{{0.23,0.11},{-0.07,0.03}}}};
}
inline auto coefficients(Fixture const& f) -> Eigen::VectorX<Scalar> {
    Eigen::VectorX<Scalar> result(f.values.size());
    for(std::size_t i=0;i<f.values.size();++i) result(static_cast<Eigen::Index>(i))=f.values[i];
    return result;
}
inline void begin(Fixture const& f) {
    std::cout<<"CASE "<<f.name<<' '<<(f.real_parity?"wx":"circle")<<' '<<f.offset<<' '<<f.values.size()<<'\n';
    for(auto c:f.values)std::cout<<"COEFF "<<c.real()<<' '<<c.imag()<<'\n';
}
template<class Phases> void phases(Phases const& p) {
    for(auto value:p.values())std::cout<<"PHASE "<<value<<'\n';
}
template<class Controls> void controls(Controls const& c) {
    for(auto const& gate:c.gates){std::cout<<"CONTROL";for(int r=0;r<2;++r)for(int col=0;col<2;++col)std::cout<<' '<<gate(r,col).real()<<' '<<gate(r,col).imag();std::cout<<'\n';}
}
