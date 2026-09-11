#pragma once

#include "quest.h"
#include "quest_rsmpi_config.hpp"
#include "rust/cxx.h"

#include <cstddef>
#include <cstdint>

#define QUEST_SYS_MPI_ENABLED (QUEST_COMPILE_MPI && QUEST_COMPILE_SUBCOMM)
#if QUEST_SYS_MPI_ENABLED
#include <mpi.h>
#endif

namespace quest_sys {
// Minimal native lifecycle/handoff support. rsmpi owns every MPI runtime and
// communicator and implements every communication operation.
[[noreturn]] void mpi_abort_job() noexcept;
bool mpi_quest_is_quiescent() noexcept;
#if QUEST_SYS_RSMPI_ENABLED
bool mpi_available() noexcept;
bool mpi_quest_can_initialize() noexcept;
void mpi_validate_rsmpi_abi(std::size_t comm_size,
                            std::size_t fint_size,
                            std::size_t status_size,
                            std::uint32_t version,
                            std::uint32_t subversion,
                            std::int32_t multiple,
                            rust::Str library);
void mpi_init_quest(std::int64_t communicator,
                    std::int32_t rank,
                    std::int32_t size,
                    bool gpu,
                    bool threads);
void mpi_drop_quest() noexcept;
#endif
}  // namespace quest_sys
