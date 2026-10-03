#pragma once
#include <array>
#include <cmath>
#include <glaze/glaze.hpp>
#include <optional>
#include <ostream>
#include <stdexcept>
#include <string>
#include <vector>

struct ReferenceQspReport {
  std::size_t degree;
  double seconds;
  bool success;
  std::optional<double> response_bound;
  std::array<double, 24> semantic_task_seconds;
  std::array<double, 24> exclusive_task_seconds;
  std::size_t unfinished_tasks;
};

struct ReferenceExecutionReport {
  std::string source_revision;
  bool cpu_local;
  int repetitions;
  double response_real;
  double response_imag;
  double retained_mass;
  double hadamard_real;
  double hadamard_imag;
  double hadamard_total_success_probability;
  double hadamard_active_probability_absolute;
  double construction_seconds;
  double combined_lowering_native_prepare_seconds;
  double execution_with_coordinate_projections_seconds;
  double native_condition_seconds;
  double hadamard_observation_seconds;
};

inline void require_finite_reference_value(double value) {
  if (!std::isfinite(value))
    throw std::runtime_error("non-finite reference report value");
}

inline void validate_reference_report(ReferenceQspReport const &report) {
  require_finite_reference_value(report.seconds);
  if (report.response_bound)
    require_finite_reference_value(*report.response_bound);
  for (double value : report.semantic_task_seconds)
    require_finite_reference_value(value);
  for (double value : report.exclusive_task_seconds)
    require_finite_reference_value(value);
}

inline void validate_reference_report(ReferenceExecutionReport const &report) {
  for (double value :
       {report.response_real, report.response_imag, report.retained_mass,
        report.hadamard_real, report.hadamard_imag,
        report.hadamard_total_success_probability,
        report.hadamard_active_probability_absolute,
        report.construction_seconds,
        report.combined_lowering_native_prepare_seconds,
        report.execution_with_coordinate_projections_seconds,
        report.native_condition_seconds, report.hadamard_observation_seconds})
    require_finite_reference_value(value);
}

inline void
validate_reference_report(std::vector<std::array<double, 2>> const &controls) {
  for (auto const &value : controls) {
    require_finite_reference_value(value[0]);
    require_finite_reference_value(value[1]);
  }
}

template <class Report>
void write_reference_report(std::ostream &output, Report const &report) {
  validate_reference_report(report);
  auto json = glz::write<glz::opts{.skip_null_members = true}>(report);
  if (!json)
    throw std::runtime_error("failed to serialize reference JSON report");
  output << *json << '\n';
  if (!output)
    throw std::runtime_error("failed to write reference JSON report");
}
