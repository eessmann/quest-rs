#include "../reference_reports.hpp"
#include <limits>
#include <sstream>
#include <stdexcept>

void require(bool condition) {
  if (!condition)
    throw std::runtime_error("reference JSON serialization regression");
}

int main() {
  ReferenceQspReport qsp{16, 0.125, false, std::nullopt, {}, {}, 0};
  std::ostringstream output;
  write_reference_report(output, qsp);
  auto json = output.str();
  require(json.back() == '\n' &&
          json.find("response_bound") == std::string::npos);
  auto parsed = glz::read_json<ReferenceQspReport>(json);
  require(parsed && parsed->degree == 16 && !parsed->success &&
          !parsed->response_bound);
  qsp.success = true;
  qsp.response_bound = 1e-12;
  output.str("");
  write_reference_report(output, qsp);
  parsed = glz::read_json<ReferenceQspReport>(output.str());
  require(parsed && parsed->response_bound == qsp.response_bound);
  ReferenceExecutionReport execution{};
  execution.source_revision = "revision with \"quotes\" and a newline\n";
  execution.cpu_local = true;
  execution.repetitions = 1000;
  output.str("");
  write_reference_report(output, execution);
  auto replay = glz::read_json<ReferenceExecutionReport>(output.str());
  require(replay && replay->source_revision == execution.source_revision &&
          replay->repetitions == 1000);
  std::vector<std::array<double, 2>> controls{{0.25, 0.0}, {0.0, -0.2}};
  output.str("");
  write_reference_report(output, controls);
  auto matrices =
      glz::read_json<std::vector<std::array<double, 2>>>(output.str());
  require(matrices && *matrices == controls);
  qsp.response_bound = std::numeric_limits<double>::infinity();
  try {
    write_reference_report(output, qsp);
    return 1;
  } catch (std::runtime_error const &) {
  }
  output.setstate(std::ios::badbit);
  try {
    write_reference_report(output, execution);
    return 1;
  } catch (std::runtime_error const &) {
  }
}
