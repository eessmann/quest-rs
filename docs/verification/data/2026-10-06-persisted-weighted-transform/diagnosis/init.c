#include <mpi.h>
#include <stdio.h>
int main(int argc, char **argv) {
 int provided, rank, size;
 int status = MPI_Init_thread(&argc, &argv, MPI_THREAD_FUNNELED, &provided);
 if (status != MPI_SUCCESS) return 2;
 MPI_Comm_rank(MPI_COMM_WORLD, &rank);
 MPI_Comm_size(MPI_COMM_WORLD, &size);
 printf("initialized rank=%d size=%d provided=%d\n", rank, size, provided);
 fflush(stdout);
 return MPI_Finalize() == MPI_SUCCESS ? 0 : 3;
}
