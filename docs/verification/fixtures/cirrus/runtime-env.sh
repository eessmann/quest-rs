#!/bin/bash
# Source once from a build/test coordinator, never from an MPI child launcher.
: "${QUEST_CAMPAIGN_ROOT:?}"
: "${QUEST_SNAPSHOT:?}"
: "${QUEST_COMPILER:?gnu or cray}"
: "${SLURM_CPUS_PER_TASK:?}"
: "${QUEST_SOURCE_DIGEST:?verify the snapshot before selecting its build profile}"
[[ "$SLURM_CPUS_PER_TASK" =~ ^[1-9][0-9]*$ ]] || return 2
[[ "$QUEST_SOURCE_DIGEST" =~ ^[a-f0-9]{64}$ ]] || return 2
QUEST_CAMPAIGN_ROOT=$(cd -- "$QUEST_CAMPAIGN_ROOT" && pwd)
QUEST_SNAPSHOT=$(cd -- "$QUEST_SNAPSHOT" && pwd)
export QUEST_CAMPAIGN_ROOT QUEST_SNAPSHOT
module restore
case "$QUEST_COMPILER" in
    gnu) module switch PrgEnv-cray PrgEnv-gnu;;
    cray) ;;
    *) return 2;;
esac
module load cmake cray-hdf5
if [[ "$QUEST_COMPILER" = cray ]]; then
    # Match the native build's dependency profile without disabling OpenMP.
    module unload cray-libsci
fi
export CC=cc CXX=CC
export QUEST_ROOT="$QUEST_CAMPAIGN_ROOT/native/$QUEST_COMPILER"
export CARGO_TARGET_DIR="$QUEST_CAMPAIGN_ROOT/targets/$QUEST_COMPILER/$QUEST_SOURCE_DIGEST"
export CARGO_BUILD_JOBS=$(( SLURM_CPUS_PER_TASK < 8 ? SLURM_CPUS_PER_TASK : 8 ))
export CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER=cc
export LIBCLANG_PATH=/usr/lib64
export CLANG=/opt/cray/pe/cce/19.0.0/cce-clang/x86_64/bin/clang
export OMP_NUM_THREADS="$SLURM_CPUS_PER_TASK"
export OMP_PLACES=cores OMP_PROC_BIND=close
export SRUN_CPUS_PER_TASK="$SLURM_CPUS_PER_TASK"
if [[ "$QUEST_COMPILER" = cray ]]; then
    # Cray's wrapper adds linker-plugin arguments which rust-lld cannot accept.
    # It also links crtfastmath.o by default, changing the process FP policy.
    # Rustc and rustdoc have independent flag environments. Preserve each one's
    # encoded-flags precedence, use its linker and retain gradual underflow.
    for quest_flag_family in RUSTFLAGS RUSTDOCFLAGS; do
        quest_encoded_flags=$(python3 - "$quest_flag_family" <<'PY'
import os
import sys
separator = chr(31)
family = sys.argv[1]
flags = os.environ.get('CARGO_ENCODED_' + family)
if flags is None:
    flags = separator.join(os.environ.get(family, '').split())
policy = ['-C', 'linker-features=-lld', '-C', 'link-arg=-mno-daz-ftz']
print((flags + separator if flags else '') + separator.join(policy), end='')
PY
)
        export "CARGO_ENCODED_$quest_flag_family=$quest_encoded_flags"
        unset "$quest_flag_family"
    done
fi

# Snapshot archives deliberately normalize source mtimes. A separate Cargo
# directory per digest prevents stale binaries from another extracted tree.
# Compiler/module changes within that directory are refused before Cargo runs.
quest_profile_modules=$(module -t list 2>&1)
quest_profile_compiler=$(CC --version 2>&1)
quest_profile_rustc=$(rustc -vV)
python3 - "$quest_profile_modules" "$quest_profile_compiler" "$quest_profile_rustc" <<'PY'
import json
import os
from pathlib import Path
import sys
import tempfile

target = Path(os.environ['CARGO_TARGET_DIR'])
target.mkdir(parents=True, exist_ok=True)
revision = Path(os.environ['QUEST_ROOT']) / 'source-revision.txt'
keys = ('QUEST_ROOT', 'QUEST_COMPILER', 'QUEST_SOURCE_DIGEST', 'CC', 'CXX',
        'CFLAGS', 'CXXFLAGS', 'CPPFLAGS', 'LDFLAGS', 'CPATH', 'LIBRARY_PATH',
        'LD_LIBRARY_PATH', 'LIBCLANG_PATH', 'CLANG', 'HDF5_DIR', 'HDF5_VERSION',
        'CRAY_MPICH_DIR', 'MPI_PKG_CONFIG', 'MPICC', 'PKG_CONFIG_PATH',
        'CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER')
def selected_flags(family):
    encoded = os.environ.get('CARGO_ENCODED_' + family)
    if encoded is not None:
        return encoded.split(chr(31)) if encoded else []
    return os.environ.get(family, '').split()

profile = dict(schema='quest-cirrus-build-profile-v1', modules=sys.argv[1],
               compiler=sys.argv[2], rustc=sys.argv[3],
               native_revision=revision.read_text().strip() if revision.exists() else None,
               inputs={key: os.environ.get(key) for key in keys},
               rustflags=selected_flags('RUSTFLAGS'),
               rustdocflags=selected_flags('RUSTDOCFLAGS'))
path = target / '.native-build-profile.json'
with tempfile.NamedTemporaryFile(mode='w', dir=target, delete=False) as temporary:
    json.dump(profile, temporary, indent=2)
    temporary.write('\n')
try:
    try:
        os.link(temporary.name, path)
    except FileExistsError:
        if json.loads(path.read_text()) != profile:
            raise SystemExit('Compiler/module/flags profile changed for this source digest; use a separate campaign directory')
finally:
    Path(temporary.name).unlink()
PY
