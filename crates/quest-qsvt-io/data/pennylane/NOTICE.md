# PennyLane inverse dataset

`inverse.h5` is the unchanged official **Inverse** dataset by **Guillermo Alonso**,
distributed by PennyLane at <https://pennylane.ai/datasets/inverse> under
[Creative Commons Attribution-ShareAlike 4.0 International](https://creativecommons.org/licenses/by-sa/4.0/).
The dataset license is separate from the software license of this workspace.

The official download URL, resolved URL, retrieval date, file size and SHA-256
are recorded in `source.json`. The HDF5 bytes have not been modified. The Rust
catalog reads the stored ascending Chebyshev coefficients of
`1 / (2 * kappa * x)` directly at runtime. Its epsilon labels and provenance
are not mathematical accuracy certificates. The stored upstream QSP/QSVT
angles remain in the original file but are not imported as execution payloads.
