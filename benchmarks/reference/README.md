# Pinned C++ references

`cpp_qsp.cpp` and `cpp_gqsp.cpp` export synthesis fixtures. `cpp_execution.cpp`
uses public standard-QSVT construction, `QuESTExecutor::prepare`, prepared circuit
execution, and public Hadamard overlap preparation/observation. Its block encoding
represents the complex scalar `0.3 + 0.4i`, normalization 1, with symmetric Wx phases
`[0.1, 0.2, 0.2, 0.1]`. Coordinate projections are derived from the transform's
public input/output layouts. There is no handwritten transform gate sequence.

## Fresh execution build

The execution CMake project checks the source revision
`7fe7f740579b03c52a8cf48be6a31268b029c19f` and builds the required QSVT libraries
from that checkout. Dependency prefixes may contain installed third-party
libraries; do not link old QSVT archives. Use an empty build directory and the
same installed QuEST package selected for Rust. The adapter explicitly initializes
CPU, local, single-thread native execution even if QuEST supports MPI/CUDA.

Example from the Rust workspace root (adjust dependency locations):

```sh
reference_checkout=/path/to/quest-qsvt
reference_dependencies="$reference_checkout/out/build/audit-gcc-release/vcpkg_installed/x64-linux"
cmake -S benchmarks/reference -B /tmp/quest-qsvt-execution-reference -G Ninja \
  -DCMAKE_BUILD_TYPE=Release \
  -DCMAKE_C_COMPILER=/usr/bin/gcc -DCMAKE_CXX_COMPILER=/usr/bin/g++ \
  -DREFERENCE_SOURCE="$reference_checkout" \
  -DREFERENCE_CPM_BOOTSTRAP="$reference_checkout/out/build/audit-gcc-release/cmake/CPM_0.42.0.cmake" \
  -DCMAKE_PREFIX_PATH="$reference_dependencies;/path/to/installed/hpx;/path/to/installed/quest;/path/to/vcpkg/packages/mimalloc_x64-linux" \
  -Dautodiff_DIR="$reference_checkout/extern/autodiff/lib/cmake/autodiff" \
  -Dh5pp_DIR="$reference_checkout/extern/h5pp/lib/cmake/h5pp" \
  -DCMAKE_FIND_PACKAGE_PREFER_CONFIG=ON -DHDF5_USE_STATIC_LIBRARIES=ON
cmake --build /tmp/quest-qsvt-execution-reference --target cpp_execution -j2
/tmp/quest-qsvt-execution-reference/cpp_execution
```

The optional existing CPM bootstrap is checked against its pinned SHA256 and
copied into the new build; this permits offline configuration. The external
checkout and native installation are never modified. This reproducer was tested
with GCC 16.1.1 and C++23 on Linux; upstream compilation produced warnings.

## Measured comparison, 2026-09-11

The identical Rust fixture is printed by `quest`'s `qsvt_execution` benchmark
before timing. Full complex output, including global phase, agrees:

| Quantity | Absolute C++/Rust difference |
| --- | ---: |
| Retained complex amplitude | 1.756e-16 |
| Retained probability | 4.371e-19 |
| Hadamard complex overlap, input=reference=[1] | 3.141e-16 |
| Hadamard total success probability | 0 |

C++ response is `-0.0023524139242611097 - 0.003136551899014739i`, with retained
probability `1.537180908627052e-5`. Both implementations return Hadamard total
success probability `0.5000076859045433`. This is a local differential fixture,
not a numerical certificate or distributed/GPU validation.

The following durations are microseconds. C++ numbers are medians of five
separate runs, each averaging 1000 repetitions; Rust numbers are Criterion
estimates from the matching fixture. They are smoke measurements with different
sampling methods, not a general throughput comparison.

| Stage | C++ | Rust |
| --- | ---: | ---: |
| Encoding and transform construction | 22.97 | 9.274 |
| Lowering and projector admission | included below | 1.864 |
| Native preparation | 36.28 including lowering | 12.681 |
| Repeated subnormalized execution | 51.39 | 139.45 |
| Conditioning | 1.301 native call | 1.229 consuming public transition |
| Public Hadamard observation | 83.62 | not measured here |

The pinned C++ public API combines lowering with native preparation for this
standard transform. Its timed execution applies layout-derived coordinate
projections and three probability reads around public prepared circuit execution;
Rust uses its admitted transform result API with its own preflight and lifetime
checks. Input reset and diagnostic snapshots are outside execution timing. C++
construction/preparation destruction is outside the timer; Rust Criterion
construction includes ownership turnover. No speedup claim follows from this tiny
fixture: Rust repeated execution is slower in this measurement.

The [QSP/QSVT verification record](../../docs/verification/2026-09-11-qsvt-port.md)
summarizes this comparison and its validation limits. The measured
[C++ execution rows](../../docs/verification/data/2026-09-11-qsvt/cpp-execution-smoke.jsonl)
and [cross-language comparison](../../docs/verification/data/2026-09-11-qsvt/cpp-rust-execution-comparison.json)
retain the numerical results and five-run medians. See the
[verification index](../../docs/verification/README.md) for other measurements.
