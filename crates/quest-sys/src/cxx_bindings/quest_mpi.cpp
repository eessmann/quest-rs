#include "quest_mpi.hpp"

#include <dlfcn.h>
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

void mpi_validate_rsmpi_abi(std::size_t comm_size,
                            std::size_t fint_size,
                            std::size_t status_size,
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
  if (comm_size != sizeof(MPI_Comm) || fint_size != sizeof(MPI_Fint) ||
      status_size != sizeof(MPI_Status) || version != MPI_VERSION ||
      subversion != MPI_SUBVERSION || multiple != MPI_THREAD_MULTIPLE)
    throw std::runtime_error(
        "rsmpi generated MPI ABI differs from QuEST; rebuild mpi-sys with the "
        "verified MPICC selection");
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
