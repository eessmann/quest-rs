#ifndef _GNU_SOURCE
#define _GNU_SOURCE
#endif
#ifdef QUEST_RSMPI_NATIVE_PROBE
#include <quest.h>
#if !(QUEST_COMPILE_MPI && QUEST_COMPILE_SUBCOMM)
#error "quest-sys mpi feature requires native MPI and subcommunicator support"
#endif
#else
#include <mpi.h>
#endif
#include <dlfcn.h>
#include <stdio.h>

#ifdef __cplusplus
#define QUEST_MPI_ALIGNOF(type) alignof(type)
#else
#define QUEST_MPI_ALIGNOF(type) _Alignof(type)
#endif

int main(void) {
  Dl_info library;
  void* init = dlsym(RTLD_DEFAULT, "MPI_Init_thread");
  if (!init || !dladdr(init, &library) || !library.dli_fname)
    return 1;
  char version[MPI_MAX_LIBRARY_VERSION_STRING];
  int length = 0;
  if (MPI_Get_library_version(version, &length) != MPI_SUCCESS || length < 0 ||
      length > MPI_MAX_LIBRARY_VERSION_STRING)
    return 2;
  printf("%s\n", library.dli_fname);
  printf("comm=%zu fint=%zu status=%zu request=%zu request_align=%zu "
         "multiple=%d standard=%d.%d\n",
         sizeof(MPI_Comm), sizeof(MPI_Fint), sizeof(MPI_Status),
         sizeof(MPI_Request), QUEST_MPI_ALIGNOF(MPI_Request),
         MPI_THREAD_MULTIPLE, MPI_VERSION, MPI_SUBVERSION);
  fwrite(version, 1, (size_t)length, stdout);
  return 0;
}
