# Pinned C++ numerical references

The little-endian binary64 phase files were generated on 2026-09-11 from
`quest-qsvt` revision `7fe7f740579b03c52a8cf48be6a31268b029c19f` using
`benchmarks/reference/cpp_qsp.cpp` in this workspace. The inputs are the matching
inverse Chebyshev catalog families in `quest-qsvt-io/data/inverse`.

The adapter and required interval/disc-norm support were compiled freshly from
that clean source revision with GCC 16, `-O3 -march=native -frounding-math
-ffp-contract=off`, Eigen PocketFFT and a sequential HPX executor. It does not
link MPI, QuEST or stale C++ project archives. C++ default response tolerance
remained `1e-11`; all four runs succeeded with their own outward certificates.

The Rust regression independently multiplies full complex Wx matrices from both
exports at 33 real signals. It compares every entry including global phase.
This is an empirical differential check, separate from Rust's arbitrary-precision coefficient
certification, and does not claim GPU or distributed reference execution.

`cpp-complex-controls.json` comes from the companion `cpp_gqsp.cpp` adapter with
the same source/compiler configuration. It retains all four row-major entries
of each control, including the final K factor, for the complex Laurent target
`0.25 + 0.2i z + (-0.1 + 0.1i) z²`. A separate 64-point full-matrix regression
checks its phase, control product ordering and upper-left polynomial response.
