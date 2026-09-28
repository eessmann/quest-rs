#include <quest.h>
#include <vector>
#include <stdexcept>
#include <cstdio>
#include <cstdlib>
static void handler(const char*, const char* message) { throw std::runtime_error(message); }
int main(int argc, char** argv) {
  initCustomQuESTEnv(0,0,0);
  setQuESTInputErrorHandler(handler);
  Qureg q=createQureg(1);
  try { auto p=calcProbsOfAllMultiQubitOutcomes(q,std::vector<int>(argc > 1 ? std::atoi(argv[1]) : 64,0)); }
  catch(const std::exception& e) { std::fprintf(stderr,"caught: %s\n",e.what()); }
  destroyQureg(q);
  finalizeQuESTEnv();
}
