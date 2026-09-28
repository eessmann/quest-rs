# quest-optimizer-client

Linux process client for optional quantum optimization candidates. The caller supplies an explicit executable path to `Client::new`; the client canonicalizes that path and invokes it through `/usr/bin/prlimit`, without searching `PATH` for a worker. Other platforms, including macOS, return `Error::Capability` when constructing a client; the Linux process-execution tests remain Linux-only.

`WorkerLimits` caps each invocation at 30 seconds wall time, 512 MiB address space, and 64 KiB each for stdout and stderr. CPU time is also bounded. Each worker gets a new process group. Exit observation uses `waitid` with `NOWAIT`, retaining the leader's PID until group cleanup precedes reaping. Timeout, output overflow, and I/O failure also clean up the owned group. These are process resource limits, not a security sandbox: address-space limits apply per process and do not constitute an aggregate cgroup limit.

`request` returns untrusted protocol data. Use `synthesize` or `optimize_zx` to obtain a project-owned certificate: both independently check candidates with `quest-math`, including their complete scalar phase. Engine names, precision reports, and engine residuals do not grant admission. Synthesis distinguishes exact rational multiples of pi from exact binary64 radian identities and requires a finite tolerance strictly between zero and one. ZX admission restores an eighth-root phase only when exact equality proves it.

Build an optional worker from the workspace root:

```sh
cargo build -p quest-optimizer-worker --features synthesis,zx --locked
cargo test -p quest-optimizer-client --locked
```

The native QuEST runtime is not required by this client. Circuit and structured-program adapters decide which regions are eligible and retain certificate provenance; this crate does not publish circuit mutations.
