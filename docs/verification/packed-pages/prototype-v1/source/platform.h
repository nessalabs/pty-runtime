#ifndef PTY_EXPERIMENT_PLATFORM_H
#define PTY_EXPERIMENT_PLATFORM_H

/* Platform measurements are intentionally named separately. Linux PSS is not
 * a substitute for macOS charged physical footprint. */
static uint64_t platform_now_ns(void) {
    struct timespec t;
    assert(clock_gettime(CLOCK_MONOTONIC, &t) == 0);
    return (uint64_t)t.tv_sec * 1000000000ULL + (uint64_t)t.tv_nsec;
}

#if defined(__APPLE__)
#include <libproc.h>
#include <malloc/malloc.h>
static void platform_memory_json(void) {
    struct proc_taskinfo t = {0};
    struct rusage_info_v4 r = {0};
    struct rusage high = {0};
    malloc_statistics_t m = {0};
    assert(proc_pidinfo(getpid(), PROC_PIDTASKINFO, 0, &t, sizeof(t)) == sizeof(t));
    assert(proc_pid_rusage(getpid(), RUSAGE_INFO_V4, (rusage_info_t *)&r) == 0);
    assert(getrusage(RUSAGE_SELF, &high) == 0);
    malloc_zone_statistics(NULL, &m);
    printf("\"rss_bytes\":%" PRIu64 ",\"charged_footprint_bytes\":%" PRIu64
           ",\"pss_bytes\":null,\"private_bytes\":null,\"virtual_bytes\":%" PRIu64
           ",\"threads\":%d,\"allocator_live_bytes\":%zu,\"peak_rss_bytes\":%" PRIu64
           ",\"memory_source\":\"proc-pid-rusage\"",
           (uint64_t)t.pti_resident_size, (uint64_t)r.ri_phys_footprint,
           (uint64_t)t.pti_virtual_size, t.pti_threadnum, m.size_in_use, (uint64_t)high.ru_maxrss);
}
/* On macOS the result is reported bytes relieved by the allocator. */
static size_t platform_allocator_relief(void) { return malloc_zone_pressure_relief(NULL, 0); }
#elif defined(__linux__)
#include <malloc.h>
static uint64_t platform_proc_number(const char *file, const char *key, uint64_t scale) {
    FILE *f = fopen(file, "r");
    assert(f);
    char line[256];
    uint64_t result = UINT64_MAX;
    while (fgets(line, sizeof(line), f)) {
        if (!strncmp(line, key, strlen(key))) {
            assert(sscanf(line + strlen(key), "%" SCNu64, &result) == 1);
            break;
        }
    }
    assert(fclose(f) == 0 && result != UINT64_MAX);
    return result * scale;
}
static void platform_memory_json(void) {
    const char *rollup = "/proc/self/smaps_rollup";
    const char *status = "/proc/self/status";
    uint64_t rss = platform_proc_number(rollup, "Rss:", 1024);
    uint64_t pss = platform_proc_number(rollup, "Pss:", 1024);
    uint64_t private_bytes = platform_proc_number(rollup, "Private_Clean:", 1024)
        + platform_proc_number(rollup, "Private_Dirty:", 1024);
    uint64_t virtual_bytes = platform_proc_number(status, "VmSize:", 1024);
    uint64_t threads = platform_proc_number(status, "Threads:", 1);
    struct rusage high = {0};
    assert(getrusage(RUSAGE_SELF, &high) == 0);
    printf("\"rss_bytes\":%" PRIu64 ",\"charged_footprint_bytes\":null,\"pss_bytes\":%" PRIu64
           ",\"private_bytes\":%" PRIu64 ",\"virtual_bytes\":%" PRIu64
           ",\"threads\":%" PRIu64 ",\"peak_rss_bytes\":%" PRIu64
           ",\"memory_source\":\"proc-smaps-rollup\"",
           rss, pss, private_bytes, virtual_bytes, threads, (uint64_t)high.ru_maxrss * 1024);
#if defined(__GLIBC__) && __GLIBC_PREREQ(2, 33)
    struct mallinfo2 m = mallinfo2();
    printf(",\"allocator_live_bytes\":%zu", (size_t)(m.uordblks + m.hblkhd));
#else
    printf(",\"allocator_live_bytes\":null");
#endif
}
/* On glibc the return is a success flag, not a byte count. */
static size_t platform_allocator_relief(void) {
#if defined(__GLIBC__)
    return (size_t)malloc_trim(0);
#else
    return 0;
#endif
}
#else
#error "Native experiments support macOS and Linux"
#endif
#endif
