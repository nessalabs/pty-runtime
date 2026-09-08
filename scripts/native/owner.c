#ifndef _POSIX_C_SOURCE
#define _POSIX_C_SOURCE 200112L
#endif
#include "bridge.h"
#include <limits.h>

/* Allocator context and native handles share one exclusive owner. No callback
 * escapes a native call or accesses Rust. Denial is sticky until destruction. */
static void *bounded_alloc(void *ctx, size_t n, uint8_t alignment, uintptr_t ra) {
  (void)ra;
  RuntimeTerminal *o = ctx;
  /* The pinned Zig adapter passes log2(alignment). Compression also requests
   * alignments greater than malloc's guarantee, so honor them explicitly. */
  if (alignment >= sizeof(size_t) * CHAR_BIT || n > o->limit - o->used) {
    o->denied = true; return NULL;
  }
  size_t alignment_bytes = (size_t)1 << alignment;
  if (alignment_bytes > o->limit) { o->denied = true; return NULL; }
  void *p = NULL;
  if (alignment_bytes <= _Alignof(max_align_t)) p = malloc(n);
  else if (posix_memalign(&p, alignment_bytes, n) != 0) p = NULL;
  if (!p) { o->denied = true; return NULL; }
  o->used += n;
  return p;
}
static bool no_resize(void *ctx, void *p, size_t n, uint8_t a, size_t next, uintptr_t ra) {
  (void)ctx; (void)p; (void)n; (void)a; (void)next; (void)ra; return false;
}
static void *no_remap(void *ctx, void *p, size_t n, uint8_t a, size_t next, uintptr_t ra) {
  (void)ctx; (void)p; (void)n; (void)a; (void)next; (void)ra; return NULL;
}
static void bounded_free(void *ctx, void *p, size_t n, uint8_t a, uintptr_t ra) {
  (void)a; (void)ra;
  RuntimeTerminal *o = ctx;
  o->used -= n;
  /* The allocation may contain terminal contents. */
  volatile uint8_t *wipe = p;
  for (size_t i = 0; i < n; i++) wipe[i] = 0;
  free(p);
}
static const GhosttyAllocatorVtable allocator_vtable = {
  bounded_alloc, no_resize, no_remap, bounded_free
};
RuntimeTerminal *rt_owner(size_t limit) {
  RuntimeTerminal *o = calloc(1, sizeof(*o));
  if (!o) return NULL;
  o->limit = limit;
  o->allocator = (GhosttyAllocator){o, &allocator_vtable};
  return o;
}
void rt_free(RuntimeTerminal *o) {
  if (!o) return;
  if (o->decoder) ghostty_snapshot_decoder_free(o->decoder);
  if (o->terminal) ghostty_terminal_free(o->terminal);
  memset(o, 0, sizeof(*o));
  free(o);
}
static void capture_reply(GhosttyTerminal t, void *ctx, const uint8_t *p, size_t n) {
  (void)t;
  RuntimeTerminal *o = ctx;
  if (n > o->reply_cap - o->reply_len) { o->reply_overflow = true; return; }
  if (n) memcpy(o->reply + o->reply_len, p, n);
  o->reply_len += n;
}
int rt_configure(RuntimeTerminal *o, size_t history, size_t continuation) {
  size_t images = 0;
  bool no = false;
  if (ghostty_terminal_set(o->terminal, GHOSTTY_TERMINAL_OPT_SCROLLBACK_MAX_BYTES, &history) ||
      ghostty_terminal_set(o->terminal, GHOSTTY_TERMINAL_OPT_CONTINUATION_MAX_BYTES, &continuation) ||
      ghostty_terminal_set(o->terminal, GHOSTTY_TERMINAL_OPT_KITTY_IMAGE_STORAGE_LIMIT, &images) ||
      ghostty_terminal_set(o->terminal, GHOSTTY_TERMINAL_OPT_KITTY_IMAGE_MEDIUM_FILE, &no) ||
      ghostty_terminal_set(o->terminal, GHOSTTY_TERMINAL_OPT_KITTY_IMAGE_MEDIUM_TEMP_FILE, NULL) ||
      ghostty_terminal_set(o->terminal, GHOSTTY_TERMINAL_OPT_KITTY_IMAGE_MEDIUM_SHARED_MEM, &no) ||
      ghostty_terminal_set(o->terminal, GHOSTTY_TERMINAL_OPT_USERDATA, o) ||
      ghostty_terminal_set(o->terminal, GHOSTTY_TERMINAL_OPT_WRITE_PTY, (const void *)capture_reply)) return -1;
  return 0;
}
RuntimeTerminal *rt_new(uint16_t cols, uint16_t rows, size_t history, size_t continuation, size_t limit) {
  RuntimeTerminal *o = rt_owner(limit);
  if (!o) return NULL;
  if (ghostty_terminal_new(&o->allocator, &o->terminal, cols, rows) || rt_configure(o, history, continuation)) {
    rt_free(o); return NULL;
  }
  return o;
}
int rt_feed(RuntimeTerminal *o, const uint8_t *p, size_t n, uint8_t *reply, size_t cap, size_t *len) {
  o->reply = reply; o->reply_cap = cap; o->reply_len = 0; o->reply_overflow = false;
  ghostty_terminal_vt_write(o->terminal, p, n);
  *len = o->reply_len;
  o->reply = NULL; o->reply_cap = 0;
  bool error = false;
  if (ghostty_terminal_get(o->terminal, GHOSTTY_TERMINAL_DATA_VT_PROCESSING_ERROR, &error)) return -1;
  if (o->denied || o->reply_overflow) return -2;
  return error ? -1 : 0;
}
int rt_resize(RuntimeTerminal *o, uint16_t cols, uint16_t rows) {
  int r = ghostty_terminal_resize(o->terminal, cols, rows, 8, 16);
  return o->denied ? -2 : (r ? -1 : 0);
}
int rt_compress(RuntimeTerminal *o) {
  GhosttyTerminalCompressionResult result;
  if (ghostty_terminal_compress(o->terminal, GHOSTTY_TERMINAL_COMPRESSION_MODE_INCREMENTAL, &result)) return -1;
  if (o->denied) return -2;
  switch (result) {
    case GHOSTTY_TERMINAL_COMPRESSION_RESULT_UNSUPPORTED: return -3;
    case GHOSTTY_TERMINAL_COMPRESSION_RESULT_PENDING: return 0;
    default: return 1;
  }
}
