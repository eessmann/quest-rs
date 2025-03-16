//
// Created by erich on 3/16/25.
//
#pragma once
#include <quest.h>
#include <complex>
#include <cstdint>

struct Quest_Complex {
  double re;
  double im;

  Quest_Complex() : re(0), im(0) {}

  Quest_Complex(const qcomp& c) : re(c.real()), im(c.imag()) {}

  operator qcomp() const { return qcomp(re, im); }

  static Quest_Complex* from_qcomp_ptr(qcomp* ptr) {
    static_assert(sizeof(Quest_Complex) == sizeof(qcomp),
                  "Incompatible types for casting");
    static_assert(offsetof(Quest_Complex, re) == 0 &&
                      offsetof(Quest_Complex, im) == sizeof(double),
                  "Memory layout not as expected");
    return reinterpret_cast<Quest_Complex*>(ptr);
  }

  static qcomp* to_qcomp_ptr(Quest_Complex* ptr) {
    static_assert(sizeof(Quest_Complex) == sizeof(qcomp),
                  "Incompatible types for casting");
    static_assert(offsetof(Quest_Complex, re) == 0 &&
                      offsetof(Quest_Complex, im) == sizeof(double),
                  "Memory layout not as expected");
    return reinterpret_cast<qcomp*>(ptr);
  }
};

using Quest_Real = double;
using Quest_Index = std::int64_t;