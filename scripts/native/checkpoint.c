#include "bridge.h"

typedef struct { uint8_t *bytes; size_t used, cap; bool overflow; } Writer;
static bool write_bounded(void *ctx, const uint8_t *p, size_t n) {
  Writer *w = ctx;
  if (n > w->cap - w->used) { w->overflow = true; return false; }
  memcpy(w->bytes + w->used, p, n); w->used += n;
  return true;
}
int rt_checkpoint(RuntimeTerminal *o, uint8_t *bytes, size_t cap, size_t *len) {
  Writer w = {bytes, 0, cap, false};
  int r = ghostty_snapshot_encode(o->terminal, (GhosttyWriter){write_bounded, &w});
  *len = w.used;
  if (o->denied || w.overflow) return -2;
  return r ? -1 : 0;
}
/* The caller retains bytes until rt_history finishes or rt_free destroys the
 * decoder. READY transfers terminal ownership to this wrapper. */
RuntimeTerminal *rt_restore(const uint8_t *bytes, size_t len, size_t continuation,
                            size_t history, size_t limit, int *error) {
  *error = -1;
  RuntimeTerminal *o = rt_owner(limit);
  if (!o) { *error = -2; return NULL; }
  bool retain = true;
  if (ghostty_snapshot_decoder_new_buf(&o->allocator, &o->decoder, bytes, len) ||
      ghostty_snapshot_decoder_set(o->decoder, GHOSTTY_SNAPSHOT_DECODER_OPT_MAX_CONTINUATION_BYTES, &continuation) ||
      ghostty_snapshot_decoder_set(o->decoder, GHOSTTY_SNAPSHOT_DECODER_OPT_RETAIN_CONTINUATION, &retain) ||
      ghostty_snapshot_decoder_ready(o->decoder, &o->terminal) ||
      rt_configure(o, history, continuation)) {
    if (o->denied) *error = -2;
    rt_free(o); return NULL;
  }
  o->source_len = len;
  *error = 0;
  return o;
}
int rt_history(RuntimeTerminal *o) {
  if (!o->decoder) return 1;
  int r = ghostty_snapshot_decoder_next(o->decoder);
  if (r == GHOSTTY_NO_VALUE) {
    size_t offset = 0;
    if (ghostty_snapshot_decoder_get(o->decoder, GHOSTTY_SNAPSHOT_DECODER_DATA_SOURCE_OFFSET, &offset) ||
        offset != o->source_len) return -1;
    ghostty_snapshot_decoder_free(o->decoder); o->decoder = NULL; return 1;
  }
  if (o->denied) return -2;
  return r == GHOSTTY_SUCCESS ? 0 : -1;
}
