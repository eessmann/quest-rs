#include "quest_mpi.hpp"

#include <dlfcn.h>
#include <array>
#include <cstdio>
#include <cstdlib>
#include <filesystem>
#include <stdexcept>
#include <string>

namespace quest_sys {
[[noreturn]] void mpi_abort_job() noexcept {
  std::fputs(
      "quest-sys: unrecoverable distributed MPI operation or cleanup failure\n",
      stderr);
#if QUEST_SYS_MPI_ENABLED
  int initialized = 0;
  int finalized = 0;
  MPI_Initialized(&initialized);
  MPI_Finalized(&finalized);
  if (initialized && !finalized)
    MPI_Abort(MPI_COMM_WORLD, EXIT_FAILURE);
#endif
  std::abort();
}

#if QUEST_SYS_RSMPI_ENABLED
bool mpi_available() noexcept {
  return QUEST_SYS_MPI_ENABLED;
}

void mpi_validate_rsmpi_abi(rust::Slice<const std::size_t> layout,
                            std::uint32_t version,
                            std::uint32_t subversion,
                            std::int32_t multiple,
                            rust::Str library) {
#if QUEST_SYS_MPI_ENABLED
  Dl_info resolved;
  void* init = dlsym(RTLD_DEFAULT, "MPI_Init_thread");
  if (!init || !dladdr(init, &resolved) || !resolved.dli_fname ||
      std::filesystem::canonical(resolved.dli_fname) !=
          std::string(library.data(), library.size()))
    throw std::runtime_error(
        "loaded MPI library differs from the verified QuEST/MPICC library");
  const std::array<std::size_t, 11> expected = {
      sizeof(MPI_Comm), alignof(MPI_Comm), sizeof(MPI_Fint), alignof(MPI_Fint),
      sizeof(MPI_Status), alignof(MPI_Status), offsetof(MPI_Status, MPI_SOURCE),
      offsetof(MPI_Status, MPI_TAG), offsetof(MPI_Status, MPI_ERROR),
      sizeof(MPI_Request), alignof(MPI_Request)};
  bool same = layout.size() == expected.size();
  if (same)
    for (std::size_t i = 0; i < expected.size(); ++i)
      same = same && layout[i] == expected[i];
  if (!same || version != MPI_VERSION || subversion != MPI_SUBVERSION ||
      multiple != MPI_THREAD_MULTIPLE)
    throw std::runtime_error(
        "rsmpi generated MPI ABI differs from QuEST; rebuild mpi-sys with the "
        "verified MPI header, compiler and parser selection");
#else
  throw std::runtime_error("installed QuEST lacks MPI subcommunicator support");
#endif
}

void mpi_drop_quest() noexcept {
  try {
    extern void finalize_quest_env();
    finalize_quest_env();
  } catch (...) {
    mpi_abort_job();
  }
}
#endif
}  // namespace quest_sys
