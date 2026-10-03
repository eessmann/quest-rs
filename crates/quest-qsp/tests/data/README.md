The degree-8,105 inverse Chebyshev fixture remains a self-contained historical
binary64 input for packaged QSP tests. It was copied from the original C++ port
at revision `7fe7f740579b03c52a8cf48be6a31268b029c19f`. The workspace precision
regression compares every byte against the official PennyLane HDF5 catalog's
`/poly/0.001/1500` dataset. The family label is provenance; it does not establish
an approximation bound. This independent fixture does not make QSP depend on HDF5.
