#define _POSIX_C_SOURCE 200112L
#include "boundary-faults.h"
#include <assert.h>
#include <errno.h>
const char *fault;
int fault_key;
size_t fault_hits, live_allocations;
static void *reply_context;
static void (*reply_callback)(GhosttyTerminal, void *, const uint8_t *, size_t);
void inject(const char *name, int key) { fault = name; fault_key = key; fault_hits = 0; }
static bool matches(const char *name) {
  if (!fault || strcmp(fault, name)) return false;
  fault_hits++; return true;
}
void checked_free(RuntimeTerminal *o) {
  inject(NULL, 0); rt_free(o); assert(live_allocations == 0);
}
void *test_malloc(size_t n) {
  if (matches("malloc")) return NULL;
  void *p = malloc(n); if (p) live_allocations++; return p;
}
void *test_calloc(size_t n, size_t size) {
  if (matches("calloc")) return NULL;
  void *p = calloc(n, size); if (p) live_allocations++; return p;
}
int test_posix_memalign(void **out, size_t alignment, size_t n) {
  if (matches("posix_memalign")) return ENOMEM;
  int r = posix_memalign(out, alignment, n); if (!r) live_allocations++; return r;
}
void test_free(void *p) { if (p) { assert(live_allocations); live_allocations--; } free(p); }
GhosttyResult test_ghostty_terminal_set(GhosttyTerminal t, GhosttyTerminalOption k, const void *v) {
  if ((int)k == fault_key && matches("set")) return GHOSTTY_INVALID_VALUE;
  if (k == GHOSTTY_TERMINAL_OPT_USERDATA) reply_context = (void *)v;
  if (k == GHOSTTY_TERMINAL_OPT_WRITE_PTY) reply_callback = (void (*)(GhosttyTerminal, void *, const uint8_t *, size_t))v;
  return ghostty_terminal_set(t, k, v);
}
GhosttyResult test_ghostty_terminal_get(GhosttyTerminal t, GhosttyTerminalData k, void *out) {
  if ((int)k == fault_key && matches("get")) return GHOSTTY_INVALID_VALUE;
  if (k == GHOSTTY_TERMINAL_DATA_VT_PROCESSING_ERROR && matches("processing-error")) {
    *(bool *)out = true; return GHOSTTY_SUCCESS;
  }
  return ghostty_terminal_get(t, k, out);
}
GhosttyResult test_ghostty_terminal_compress(GhosttyTerminal t, GhosttyTerminalCompressionMode m,
                                           GhosttyTerminalCompressionResult *out) {
  if (matches("compress-error")) return GHOSTTY_INVALID_VALUE;
  if (matches("compress-unsupported")) {
    *out = GHOSTTY_TERMINAL_COMPRESSION_RESULT_UNSUPPORTED; return GHOSTTY_SUCCESS;
  }
  return ghostty_terminal_compress(t, m, out);
}
GhosttyResult test_ghostty_terminal_resize(GhosttyTerminal t, uint16_t c, uint16_t r,
                                          uint32_t w, uint32_t h) {
  if (matches("resize")) return GHOSTTY_INVALID_VALUE;
  return ghostty_terminal_resize(t, c, r, w, h);
}
GhosttyResult test_ghostty_grid_ref_style(const GhosttyGridRef *ref, GhosttyStyle *out) {
  if (matches("style")) return GHOSTTY_INVALID_VALUE;
  GhosttyResult r = ghostty_grid_ref_style(ref, out);
  if (!r && matches("style-color")) out->fg_color.tag = (GhosttyStyleColorTag)255;
  return r;
}
GhosttyResult test_ghostty_grid_ref_cell(const GhosttyGridRef *ref, GhosttyCell *out) {
  if (matches("cell")) return GHOSTTY_INVALID_VALUE;
  return ghostty_grid_ref_cell(ref, out);
}
GhosttyResult test_ghostty_cell_get(GhosttyCell cell, GhosttyCellData key, void *out) {
  if (matches("cell-get")) return GHOSTTY_INVALID_VALUE;
  return ghostty_cell_get(cell, key, out);
}
GhosttyResult test_ghostty_grid_ref_graphemes(const GhosttyGridRef *ref, uint32_t *out,
                                           size_t cap, size_t *len) {
  if (matches("graphemes")) return GHOSTTY_INVALID_VALUE;
  return ghostty_grid_ref_graphemes(ref, out, cap, len);
}
GhosttyResult test_ghostty_snapshot_encode(GhosttyTerminal t, GhosttyWriter w) {
  if (matches("encode")) return GHOSTTY_INVALID_VALUE;
  return ghostty_snapshot_encode(t, w);
}
GhosttyResult test_ghostty_snapshot_decoder_set(GhosttySnapshotDecoder d,
    GhosttySnapshotDecoderOption k, const void *v) {
  if ((int)k == fault_key && matches("decoder-set")) return GHOSTTY_INVALID_VALUE;
  return ghostty_snapshot_decoder_set(d, k, v);
}
GhosttyResult test_ghostty_snapshot_decoder_get(GhosttySnapshotDecoder d,
    GhosttySnapshotDecoderData k, void *out) {
  if ((int)k == fault_key && matches("decoder-get")) return GHOSTTY_INVALID_VALUE;
  return ghostty_snapshot_decoder_get(d, k, out);
}
GhosttyResult test_ghostty_snapshot_decoder_next(GhosttySnapshotDecoder d) {
  if (matches("decoder-next")) return GHOSTTY_INVALID_VALUE;
  return ghostty_snapshot_decoder_next(d);
}
GhosttyResult test_ghostty_formatter_terminal_new(const GhosttyAllocator *a, GhosttyFormatter *f,
    GhosttyTerminal t, GhosttyFormatterTerminalOptions o) {
  if (matches("formatter-new")) return GHOSTTY_INVALID_VALUE;
  return ghostty_formatter_terminal_new(a, f, t, o);
}

void test_ghostty_terminal_vt_write(GhosttyTerminal t, const uint8_t *p, size_t n) {
  ghostty_terminal_vt_write(t, p, n);
  if (matches("empty-reply")) { assert(reply_callback); reply_callback(t, reply_context, p, 0); }
}
