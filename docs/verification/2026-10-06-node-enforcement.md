# Cirrus node memory enforcement observations

Read-only diagnostic job `537644` completed successfully on two exclusive
Cirrus nodes. The observed Slurm job cgroup had `memory.max` and `memory.high`
of **780,945,850,368 bytes per node (744,768 MiB; 727.3125 GiB)**, with
`memory.swap.max=0`. This is the configured site memory-controller limit,
not the requested 8 GiB application limit. No inspected Slurm ancestor
directory or control file was writable by the user. **Strict application
memory enforcement remains unverified.**

The [machine-readable summary](data/2026-10-06-node-enforcement/summary.json)
records source identities, all 21 independently verified raw artifact hashes,
the completion hash, scheduler evidence, and sanitized process observations.
Private raw receipts retain exact process, host, user and filesystem identities;
the public summary uses generic node and job-path placeholders.

## Execution and evidence

The job requested the `standard` partition, `short` QoS, two exclusive nodes,
one MPI rank and 288 physical CPU slots per node, and a twenty-minute limit.
It depended on completed job `537617`. Job `537644`, its batch/extern steps,
and MPI step `537644.0` all finished with exit code `0:0`. The batch elapsed
time was 22 seconds; the MPI supervisor measured 3.774731043 seconds, including
a deliberate three-second observation hold. There was no timeout, truncated
log, failed stage, or cleanup cancellation.

The small C++17 MPI probe was compiled through GNU `CC` with
`-O1 -g0 -Wall -Wextra -Werror`. It reused the admitted GNU build `536807`
module profile, including central `cray-hdf5/1.14.3.5`, and verified the frozen
`8d85c0e8075cec3057b4da14055c73ebc35bb04ab4b01b155153f678bdfa0f4d`
source before and after execution. The probe links MPI, not QuEST or HDF5.
Native QuEST source and installed prefixes were unchanged.

The reviewed payload manifest SHA-256 is
`444c7673e446c950f58ad614a4a2afecbb7335497ffbc40689efda1e6a89cb7b`;
the actual executable SHA-256 is
`772cc855a80c798fcd2736acb210399aeb078dc28c62b92a409775701c7fc507`.
The executed Slurm spool script matched the reviewed `run.sh`. The receipt
contains a differently formatted serialization of the same source manifest;
both byte hashes are recorded separately. Its completion SHA-256 is
`277d0c18983e62a77ffa30eb9fc62f18fd437c08b3ace7ef409c2042ea02abb4`.

The coordinator inspected its own Python process. A bounded observer inspected
the actual `srun` process after the launcher shim had executed it; the before-
and after-exec snapshots identify the same process. Each rank inspected itself
after `MPI_Init_thread`. Distinct hosts and shared-memory communicator size one
confirmed one MPI rank on each node. `OMP_NUM_THREADS=288` was configured,
but no OpenMP team or QuEST kernel was executed or measured.

## Observed containment and limits

All four distinct known processes used an interpretable cgroup v2 hierarchy.
On each node, their common job ancestor was
`/system.slice/slurmstepd.scope/job_<JOB>`. The coordinator and launcher shared
`step_batch/user/task_0`; ranks used `step_0/user/task_<rank>`. The batch node
contained both the coordinator/launcher branch and rank 0's branch under that
same job ancestor. This agrees with Slurm's documented separation of steps
and user tasks within the job hierarchy. [Slurm cgroup v2 hierarchy](https://slurm.schedmd.com/cgroup_v2.html#hierarchy-overview)

| Observed level | `memory.max` / `memory.high` | `memory.swap.max` | Available controllers |
| --- | --- | --- | --- |
| Task leaf | `max` / `max` | `max` | `cpuset cpu memory` |
| Step's `user` ancestor | 780,945,850,368 / 780,945,850,368 | 0 | `cpuset cpu memory` |
| Step ancestor | `max` / `max` | `max` | `cpuset cpu memory` |
| Job ancestor | 780,945,850,368 / 780,945,850,368 | 0 | `cpuset cpu memory` |

Every sampled existing `memory.swap.current` value was zero. Task leaves with
`max` remain subject to ancestor limits. The configured job and step/user
limits were observed on both nodes; no pressure or OOM challenge tested their
behavior. `memory.current`, `memory.peak`, and `memory.events` were inspected
for metadata only, so this probe supplies no aggregate memory usage or peak.
The kernel describes hierarchical constraints and permits temporary overshoot
of `memory.max`; a read value is not a proof of an exact no-overshoot bound.
[Linux memory controller](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory)

The seven inspected ancestor directories per snapshot were root-owned, with
`access(W_OK)=false`. Every existing inspected control file was likewise
root-owned and not writable. The job exposed `cpuset`, `cpu`, and `memory`,
while `pids.max` was absent there. Higher observed ancestors exposed `pids`,
but their `pids.max` values were `max`. No writable descendant of the Slurm
job was observed. Permission checks are evidence about these paths, not an
attempted delegation operation or an exhaustive search for privileged APIs.

Both fixed own-user service paths, `user-<UID>.slice` and
`user-<UID>.slice/user@<UID>.service`, were absent on the sampled compute
nodes. A delegated user service on a login node would not establish delegation
inside this compute job. Moving application work to `user.slice` outside the
Slurm job would lose the containment relationship being evaluated and is not
a proposed enforcement route.

All five snapshots, including the launcher before exec, reported unlimited
soft and hard `RLIMIT_AS`. Their finite `RLIMIT_RSS` value is recorded as an
observation, not claimed as an address-space or aggregate-memory enforcement
mechanism. The snapshot process count does not bound MPI helpers, future
descendants, bootstrap/cleanup subprocesses, or their overlapping lifetimes.

## Hugepages and unresolved coverage

Each MPI rank reported **16,777,216 bytes of `HugetlbPages`** after MPI
initialization. Coordinator and launcher snapshots reported zero. The hierarchy
root listed the `hugetlb` controller, but its `cgroup.subtree_control` omitted
it. The observed Slurm ancestry did not expose that controller, and the fixed
2 MiB/1 GiB hugepage limit, current, and reservation files were absent.

This does **not** prove that the observed memory-controller limit excludes
hugepages: current Linux supports the opt-in `memory_hugetlb_accounting` mount
option. The probe read mount information to establish hierarchy identity, but
did not retain mount options or kernel version. HugeTLB coverage by the memory
controller therefore remains **unknown**. The rank measurements make that
coverage a concrete requirement for a future enforcement design.
[Linux cgroup v2 mount options](https://docs.kernel.org/admin-guide/cgroup-v2.html#mounting)

## Consequence for the capacity gate

The current rank-process 8 GiB guard does not establish an application-wide
per-node cap. This diagnostic installed no such guard and changed no resource
limit or cgroup. It observed a much larger site job memory-controller limit and
no writable Slurm subtree through the inspected paths. A smaller supported
job-contained limit would require a verified administrative or delegated
mechanism, explicit hugepage/swap coverage, and a defined policy for transient
overshoot and job-owned helpers. No such mechanism was exercised here.

Cirrus documentation states that users cannot request memory with `--mem`
or `--mem-per-cpu`; allocation follows CPU resources, and exclusive jobs
receive full-node memory. Consequently, this result does not justify replacing
the observed site limit with an assumed 8 GiB scheduler request.
[Cirrus resource limits](https://docs.cirrus.ac.uk/user-guide/batch/#resource-limits)

The probe bounded each ordinary file read to 16 KiB, mount information to
64 KiB, total reads to 1 MiB per snapshot, and ancestor traversal to 32 levels.
Actual reads were 7,918–7,926 bytes per snapshot, with no truncation. It inspected
only explicit same-user coordinator/launcher processes and its own MPI ranks;
process-ID lists were not read and other users' processes were not traversed.
Missing controller files and service paths remain recorded absences. The
diagnostic and its independent receipt audit completed successfully; enforcement,
aggregate process bounds, and strict capacity closure remain unproved.
