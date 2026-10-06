# Matching execution with the native communication buffer

This implements the next capacity stage identified by the
[memory audit](../../research/distributed-capacity-memory.md), within the accepted
CPU/MPI sparse-execution programme. Native QuEST source remains unchanged. The
queued `8e02c49a` campaign retains its frozen source and is reported separately.

## Contract

For a distributed CPU statevector, use its existing communication array as the
matching permutation output. Collectively validate its deployment before the
opening matching-label Hadamards. Copy the complete input partition into that
array, execute the existing bounded routing schedule, commit its output and
then execute the closing Hadamards. Preserve every whole-register sector,
complex phase, signed control, padding state and adjoint orientation.

A private Rust staging owner holds exclusive register access during routing. It
exposes only checked local reads and staging writes, not general native gates,
array pointers or register references. Checked C++ adapters retain lifecycle and
thread admission, reject unsupported deployments and validate all indices before
writing. Native pointers and ownership remain unchanged. One-rank execution
retains its owned scratch register because native deployment has no communication
array there. Failure after the opening gate retains the existing fatal policy.

## Implementation and evidence

1. Add an allocation regression that prepares equal sparse resources at two
   register widths. Distributed preparation must retain bounded metadata only;
   one-rank preparation must still account for its owned scratch. Demonstrate
   failure against the two-register implementation before changing it.
2. Add and test checked native buffer validation, staging, indexed writes and
   commit. Cover unchanged amplitudes before commit, repeated staging, duplicate
   writes, full index preflight and unsupported one-rank/density deployments.
3. Reuse one private staging interface in both scalar and batched routing.
   Change prepared scratch ownership to optional. Model only the actually owned
   scratch register in preparation peaks, retained accounting and LCU resources.
   Keep the generic input register's conservative four-array reservation.
4. Compare the complete unitary with portable gates on arbitrary states at
   1/2/4/8 ranks and split communicators. Include a distributed matching-label
   target, both control values, repeated forward/adjoint operations, failed
   preflight without state mutation and a witnessed failure after a staging write.
5. Version capacity telemetry to distinguish an owned scratch register from a
   borrowed input communication array. Preserve historical receipt contracts.
   Repeat local capped execution, affected persisted/LCU/inverse tests, workspace
   checks, Nextest, doctests, Clippy, formatting and binding freshness.
6. Deploy a separate immutable source identity for subsequent GNU/Cray execution.
   Do not attach its results to the already queued source. Measure actual arrays,
   process memory, communication and numerical action independently.

Removing the owned scratch halves the distributed native array payload for this
kernel. It does not close strict capacity: generic reservations, producer peaks,
resident sparse data, MPI/HDF5 internals and OpenMP stacks still require separate
admission and execution evidence. Admission-policy refinement is a subsequent
stage with its own operation-specific storage contract.
