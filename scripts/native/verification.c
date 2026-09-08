/* Independent contract oracle. Tests decode opaque checkpoints and compare the
 * upstream canonical VT formatter including history and extra terminal state.
 * Binary page layout may change after mutation without changing semantics. */
#include "bridge.h"
typedef struct { uint8_t *bytes; size_t used, cap; } VerificationWriter;
static bool verify_write(void *ctx, const uint8_t *p, size_t n) {
  VerificationWriter *w = ctx;
  if (n > w->cap - w->used) return false;
  memcpy(w->bytes + w->used, p, n); w->used += n; return true;
}
int rt_verify_format(const uint8_t *p, size_t n, uint8_t *out, size_t cap, size_t *len) {
  RuntimeTerminal *o = rt_owner(256 * 1024 * 1024);
  if (!o) return -1;
  GhosttyFormatter formatter = NULL;
  int result = -1;
  if (ghostty_snapshot_decoder_new_buf(&o->allocator, &o->decoder, p, n) ||
      ghostty_snapshot_decoder_decode(o->decoder, &o->terminal)) goto done;
  GhosttyFormatterTerminalOptions options = {
    .size = sizeof(options), .emit = GHOSTTY_FORMATTER_FORMAT_VT, .unwrap = true, .trim = true
  };
  options.extra = (GhosttyFormatterTerminalExtra){
    .size = sizeof(options.extra), .palette = true, .modes = true,
    .scrolling_region = true, .tabstops = true, .pwd = true, .keyboard = true
  };
  options.extra.screen = (GhosttyFormatterScreenExtra){
    .size = sizeof(options.extra.screen), .cursor = true, .style = true,
    .hyperlink = true, .protection = true, .kitty_keyboard = true, .charsets = true
  };
  if (ghostty_formatter_terminal_new(&o->allocator, &formatter, o->terminal, options)) goto done;
  VerificationWriter writer = {out, 0, cap};
  result = ghostty_formatter_format(formatter, (GhosttyWriter){verify_write, &writer});
  *len = writer.used;
done:
  if (formatter) ghostty_formatter_free(formatter);
  rt_free(o);
  return result;
}
