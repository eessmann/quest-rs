#include <cstddef>
#include <quest.h>
#if QUEST_VERSION_MAJOR != 4 || QUEST_VERSION_MINOR != 3
#error The supported native API is QuEST 4.3.x
#endif
#if QUEST_FLOAT_PRECISION != 2 || QUEST_INCLUDE_DEPRECATED_FUNCTIONS != 0
#error QuEST must use binary64 and disable deprecated APIs
#endif
static_assert(sizeof(qreal) == 8);

// Compiled using the complete CMake toolchain: driver wrappers, fixed arguments,
// toolchain files and flags. No vendor-specific command-line query is required.
#if defined(QUEST_RUST_PLATFORM_linux_gnu)
# if !defined(__linux__) || !defined(__GLIBC__)
#  error The C++ compiler does not target GNU/Linux
# endif
#elif defined(QUEST_RUST_PLATFORM_darwin)
# if !defined(__APPLE__) || !defined(__MACH__)
#  error The C++ compiler does not target Darwin
# endif
#else
# error Missing native platform admission
#endif
#if defined(QUEST_RUST_ARCH_x86_64) && !defined(__x86_64__)
# error The C++ compiler does not target x86_64
#elif defined(QUEST_RUST_ARCH_aarch64) && !defined(__aarch64__)
# error The C++ compiler does not target aarch64
#elif defined(QUEST_RUST_ARCH_powerpc64) && (!defined(__powerpc64__) || __BYTE_ORDER__ != __ORDER_BIG_ENDIAN__)
# error The C++ compiler does not target big-endian powerpc64
#elif defined(QUEST_RUST_ARCH_powerpc64le) && (!defined(__powerpc64__) || __BYTE_ORDER__ != __ORDER_LITTLE_ENDIAN__)
# error The C++ compiler does not target little-endian powerpc64
#elif defined(QUEST_RUST_ARCH_riscv64) && (!defined(__riscv) || __riscv_xlen != 64)
# error The C++ compiler does not target riscv64
#elif defined(QUEST_RUST_ARCH_s390x) && !defined(__s390x__)
# error The C++ compiler does not target s390x
#elif defined(QUEST_RUST_ARCH_i686) && !defined(__i386__)
# error The C++ compiler does not target i686
#elif defined(QUEST_RUST_ARCH_armv7) && (!defined(__arm__) || __ARM_ARCH != 7)
# error The C++ compiler does not target armv7
#endif
#if defined(QUEST_RUST_ARCH_i686) || defined(QUEST_RUST_ARCH_armv7)
static_assert(sizeof(void*) == 4, "C++ pointer width differs from Rust target");
#else
static_assert(sizeof(void*) == 8, "C++ pointer width differs from Rust target");
#endif
