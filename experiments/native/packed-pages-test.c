/* Deterministic allocation/release interleavings for the page-packing prototype. */
#include <stdio.h>
#include <string.h>
#include "packed-pages.h"
int main(void) {
    enum { COUNT = 2048 };
    void *ptr[COUNT] = {0};
    size_t sizes[COUNT] = {0};
    uint32_t rng = 0x31415926;
    for (size_t round = 0; round < 20; round++) {
        for (size_t i = 0; i < COUNT; i++) {
            if (ptr[i]) continue;
            rng ^= rng << 13; rng ^= rng >> 17; rng ^= rng << 5;
            sizes[i] = rng % 9000;
            ptr[i] = packed_alloc(sizes[i]);
            assert(ptr[i] && (uintptr_t)ptr[i] % 16 == 0);
            memset(ptr[i], (int)(i % 251), sizes[i]);
        }
        for (size_t i = 0; i < COUNT; i++) {
            for (size_t j = 0; j < sizes[i]; j++) assert(((uint8_t *)ptr[i])[j] == i % 251);
            if (i % 3 == round % 3 || round == 19) {
                packed_free(ptr[i], sizes[i]); ptr[i] = NULL;
            }
        }
    }
    assert(packed.mapped == 0 && packed.requested == 0 && packed.maps == packed.unmaps);
    for (size_t i = 0; i < 8; i++) assert(!packed.available[i]);
    printf("{\"passed\":true,\"rounds\":20,\"slots\":2048,\"maps\":%zu,\"peak_mapped\":%zu}\n", packed.maps, packed.peak);
    return 0;
}
