#include "include/SFCrashProbeC.h"

#include <fcntl.h>
#include <sys/mman.h>
#include <unistd.h>

__attribute__((noinline)) void sfprobe_illegal_instruction(void) {
#if defined(__arm64__)
    __asm__ volatile(".long 0x00000000");   // udf #0
#else
    __asm__ volatile("ud2");
#endif
}

__attribute__((noinline)) int sfprobe_bus_error(const char *path) {
    int fd = open(path, O_RDWR | O_CREAT | O_TRUNC, 0600);
    if (fd < 0) return -1;
    long page = getpagesize();
    if (ftruncate(fd, page) != 0) return -1;
    volatile char *p = mmap(NULL, (size_t)page, PROT_READ, MAP_SHARED, fd, 0);
    if (p == MAP_FAILED) return -1;
    if (ftruncate(fd, 0) != 0) return -1;
    return p[16];   // the page has no file behind it now
}
