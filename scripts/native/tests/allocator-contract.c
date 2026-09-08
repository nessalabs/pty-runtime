#include "../bridge.h"
#include <assert.h>
#include <stdio.h>

/* The pinned Zig wrapper passes @intFromEnum(std.mem.Alignment), which is
 * log2(bytes), despite the upstream C header's byte-alignment description. */
int main(void) {
  RuntimeTerminal *owner = rt_owner(8192);
  assert(owner);
  const uint8_t alignments[] = {0, 1, 2, 3, 4, 5, 6, 12};
  for (size_t index = 0; index < sizeof(alignments); index++) {
    uint8_t alignment = alignments[index];
    void *memory = owner->allocator.vtable->alloc(owner, 33, alignment, 0);
    assert(memory && !owner->denied && owner->used == 33);
    assert((uintptr_t)memory % ((uintptr_t)1 << alignment) == 0);
    memset(memory, 0xa5, 33);
    assert(!owner->allocator.vtable->resize(owner, memory, 33, alignment, 64, 0));
    assert(owner->allocator.vtable->remap(owner, memory, 33, alignment, 64, 0) == NULL);
    assert(owner->used == 33 && !owner->denied);
    for (size_t byte = 0; byte < 33; byte++) assert(((uint8_t *)memory)[byte] == 0xa5);
    owner->allocator.vtable->free(owner, memory, 33, alignment, 0);
    assert(owner->used == 0);
  }
  void *unsupported = owner->allocator.vtable->alloc(owner, 33, 255, 0);
  assert(unsupported == NULL && owner->denied && owner->used == 0);
  rt_free(owner);
  owner = rt_owner(64);
  assert(owner);
  void *oversized = owner->allocator.vtable->alloc(owner, 65, 0, 0);
  assert(oversized == NULL && owner->denied && owner->used == 0);
  rt_free(owner);
  owner = rt_owner(64);
  assert(owner);
  void *overaligned = owner->allocator.vtable->alloc(owner, 33, 7, 0);
  assert(overaligned == NULL && owner->denied && owner->used == 0);
  rt_free(owner);
  puts("allocator log2 alignments 0..6 and 12 aligned/accounted/freed; resize/remap refuse without changing bytes/accounting; invalid exponent, byte cap, and alignment cap denied");
  return 0;
}
