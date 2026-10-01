# Parent independent application review

Reviewed task3 changes in execution.rs, synthesis.rs, CLI argument declarations,
distributed root preparation/wire tags, physical_snapshot and HDF5 writers.
Parent did not implement these changes. No blocking correctness issue identified.

- Entry-scaled rectangular SVD compares rank in normalized coordinates before
  recovering physical singular values. Extreme representable scales have
  independent full-rank and native solve tests; nonrepresentable values fail.
- The inverse action encodes A adjoint so odd transforms map the RHS row space
  to the solution column space. Standard requires odd degree; generalized auto
  selects the odd Hermitianized component. Tall residual components outside
  the range remain in the physical residual instead of being erased.
- Physical residual evaluation uses the actual rounded exported vector in
  normalized coordinates. Exponent decomposition avoids avoidable intermediate
  ratio/product overflow; finite solution and residual norms remain explicit
  limits. This is a numerical residual, not a new interval certificate.
- Full-register input width includes auxiliary registers; projection and mass
  accounting occur in the existing runtime. Raw physical output is obtained
  before optional conditioning. Logical and physical outputs stay distinct.
- Auto follows convention and is resolved before MPI freeze; seven existing
  wire discriminants remain unchanged. No fallback after failed admission.
- Catalog family selection is exact. The normalized singular-value domain and
  reciprocal scale remain separate from QSP reconstruction evidence and the
  caller's physical residual tolerance. Existing artifact algorithm identity
  survives current-default changes.
- Matrix presets use a specified deterministic generator, bounded allocation,
  explicit dimensions/scale; square Hermitian construction mirrors conjugates.
  HDF5 matrix writing admits shape/storage/finite values before creating a file.

Validation scope: native Darwin CPU/OpenMP integration passed independently;
MPI execution remains untested locally. No general QSD or hardware resource
claim follows from dense simulator construction or a Pauli expansion.
