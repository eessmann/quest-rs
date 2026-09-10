# rsgridsynth hardening notes

This directory is a source fork of `rsgridsynth` 0.2.2 from upstream commit
`ffbf9163d4c4bd1cec04febb04d969c954483c8c`. The original crates.io archive has
SHA-256 `133fdfbe9489a7c448e35e82c8443c9097ed6b5c22a492c3c78c19a472cacfad`.
The upstream MIT license is retained in `LICENSE`.

The fork's Cargo package identity is `quest-rsgridsynth` version
`0.2.2-quest.1`, with publishing disabled. Its Rust library name remains
`rsgridsynth` for the narrow adapter. This distinct package identity prevents a
packaged worker from silently resolving the upstream registry crate after a path
dependency is removed during packaging.

The fork is deliberately a standalone Cargo workspace. It is linked only into
the optional, isolated optimizer worker and does not inherit the main
workspace's lints. When built as a worker dependency it uses the parent build
profile and root lockfile; standalone fork test builds use their own local,
untracked lockfile.

The hardened entry point accepts exact integer ratios, sets bounded working
precision before constructing floating constants, uses a seeded RNG, disables
up-to-phase synthesis, and applies explicit grid-exponent, candidate, and
output limits. The Diophantine solver's returned value is retained only after
the exact `w†w = xi` ring identity succeeds. The main project then reverses the
upstream product string into execution order, retains scalar `W` gates, and
requires an independent full-phase `quest-math` interval certificate before a
candidate can leave the worker.

The worker process supplies the outer 30 second, 512 MiB, and 64 KiB protocol
limits. These remain necessary because this research implementation contains
recursive and allocation-heavy internals outside the small audited adapter.
