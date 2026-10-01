# quest-numerics

Binary64 interval, FFT and convolution kernels, reusable workspaces, caller-owned parallelism and scientific timing.

See the workspace mdBook guide and crate rustdoc for executable examples.

Reusable convolution kernels retain their numerical work buffers. The warmed
single-worker path is tested for zero allocations inside its pool scope.
Multithreaded Rayon may allocate scheduler queue blocks and lazy OS sleep
primitives. Batch calls inside `pool.install` to avoid repeated external job
injection, and account for scheduler allocations separately.

This Rust port draws on `quest-qsvt` revision `7fe7f740579b03c52a8cf48be6a31268b029c19f`. Its MIT notice is retained in `LICENSE-quest-qsvt`.
