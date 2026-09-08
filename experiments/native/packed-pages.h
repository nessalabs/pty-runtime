#ifndef PTY_EXPERIMENT_PACKED_PAGES_H
#define PTY_EXPERIMENT_PACKED_PAGES_H
/* Experimental single-threaded pool. Size classes pack independent small native
 * buffers into shared mappings; the last free releases the entire mapping.
 * This prototype is not the production runtime allocator. */
#include <stddef.h>
#include <stdbool.h>
#include <stdint.h>
#include <assert.h>
#include <sys/mman.h>
#include <unistd.h>

typedef struct PackedPage PackedPage;
typedef struct { PackedPage *page; size_t mapping; } PackedHeader;
struct PackedPage {
    PackedPage *prev, *next;
    size_t mapping, slot, count, capacity;
    uint64_t occupied[8];
};
typedef struct {
    PackedPage *available[8];
    size_t mapped, peak, requested, maps, unmaps;
} PackedPool;
static PackedPool packed;
static size_t packed_round(size_t n) {
    size_t page = (size_t)getpagesize();
    assert(n <= SIZE_MAX - page + 1);
    return (n + page - 1) / page * page;
}
static size_t packed_start(void) { return (sizeof(PackedPage) + 15) & ~(size_t)15; }
static size_t packed_class(size_t n) {
    size_t index = 0, size = 32;
    while (size < n && index < 7) { size *= 2; index++; }
    return index;
}
static void packed_link(PackedPage *p, size_t c) {
    p->prev = NULL; p->next = packed.available[c];
    if (p->next) p->next->prev = p;
    packed.available[c] = p;
}
static void packed_unlink(PackedPage *p, size_t c) {
    if (p->prev) p->prev->next = p->next;
    else { assert(packed.available[c] == p); packed.available[c] = p->next; }
    if (p->next) p->next->prev = p->prev;
    p->prev = p->next = NULL;
}
static void *packed_mapping(size_t n) {
    void *p = mmap(NULL, n, PROT_READ | PROT_WRITE, MAP_PRIVATE | MAP_ANON, -1, 0);
    if (p == MAP_FAILED) return NULL;
    packed.mapped += n; packed.maps++;
    if (packed.mapped > packed.peak) packed.peak = packed.mapped;
    return p;
}
static void packed_release(void *p, size_t n) {
    assert(packed.mapped >= n);
    assert(munmap(p, n) == 0);
    packed.mapped -= n; packed.unmaps++;
}
static void *packed_alloc(size_t n) {
    if (n > SIZE_MAX - sizeof(PackedHeader)) return NULL;
    size_t total = n + sizeof(PackedHeader);
    PackedHeader *h;
    if (total > 4096) {
        size_t mapping = packed_round(total);
        h = packed_mapping(mapping);
        if (!h) return NULL;
        h->page = NULL; h->mapping = mapping;
    } else {
        size_t c = packed_class(total), slot = (size_t)32 << c;
        PackedPage *p = packed.available[c];
        if (!p) {
            size_t mapping = packed_round(16384);
            p = packed_mapping(mapping);
            if (!p) return NULL;
            p->mapping = mapping; p->slot = slot;
            p->capacity = (mapping - packed_start()) / slot;
            assert(p->capacity && p->capacity <= 512);
            packed_link(p, c);
        }
        size_t index = 0;
        while (index < p->capacity && (p->occupied[index / 64] & (UINT64_C(1) << (index % 64)))) index++;
        assert(index < p->capacity);
        p->occupied[index / 64] |= UINT64_C(1) << (index % 64);
        p->count++;
        if (p->count == p->capacity) packed_unlink(p, c);
        h = (PackedHeader *)((uint8_t *)p + packed_start() + index * slot);
        h->page = p; h->mapping = 0;
    }
    packed.requested += n;
    return h + 1;
}
static void packed_free(void *ptr, size_t n) {
    PackedHeader *h = (PackedHeader *)ptr - 1;
    PackedPage *p = h->page;
    assert(packed.requested >= n);
    packed.requested -= n;
    volatile uint8_t *wipe = ptr;
    for (size_t i = 0; i < n; i++) wipe[i] = 0;
    if (!p) { packed_release(h, h->mapping); return; }
    size_t index = ((uint8_t *)h - (uint8_t *)p - packed_start()) / p->slot;
    assert(index < p->capacity && p->count);
    uint64_t mask = UINT64_C(1) << (index % 64);
    assert(p->occupied[index / 64] & mask);
    p->occupied[index / 64] &= ~mask;
    size_t c = packed_class(p->slot);
    if (p->count == p->capacity) packed_link(p, c);
    if (--p->count == 0) { packed_unlink(p, c); packed_release(p, p->mapping); }
}
#endif
