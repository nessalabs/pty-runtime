/* Fault injection verifies bridge error translation and cleanup only. It does
 * not claim these failures can be induced in the real engine by terminal input. */
#include "boundary-faults.h"
#include <assert.h>
#include <stdio.h>
static const size_t LIMIT = 64 * 1024 * 1024;
static RuntimeTerminal *terminal(void) {
  inject(NULL, 0);
  RuntimeTerminal *o = rt_new(20, 4, 1024 * 1024, 65536, LIMIT);
  assert(o); return o;
}
static void constructors_and_allocator(void) {
  checked_free(NULL);
  inject("calloc", 0); assert(!rt_new(20, 4, 1024, 1024, LIMIT));
  assert(fault_hits == 1 && live_allocations == 0);
  inject("malloc", 0); assert(!rt_new(20, 4, 1024, 1024, LIMIT));
  assert(fault_hits && live_allocations == 0);
  const int options[] = {GHOSTTY_TERMINAL_OPT_SCROLLBACK_MAX_BYTES,
    GHOSTTY_TERMINAL_OPT_CONTINUATION_MAX_BYTES, GHOSTTY_TERMINAL_OPT_KITTY_IMAGE_STORAGE_LIMIT,
    GHOSTTY_TERMINAL_OPT_KITTY_IMAGE_MEDIUM_FILE, GHOSTTY_TERMINAL_OPT_KITTY_IMAGE_MEDIUM_TEMP_FILE,
    GHOSTTY_TERMINAL_OPT_KITTY_IMAGE_MEDIUM_SHARED_MEM, GHOSTTY_TERMINAL_OPT_USERDATA,
    GHOSTTY_TERMINAL_OPT_WRITE_PTY};
  for (size_t i = 0; i < sizeof(options) / sizeof(options[0]); i++) {
    inject("set", options[i]); assert(!rt_new(20, 4, 1024, 1024, LIMIT));
    assert(fault_hits == 1 && live_allocations == 0);
  }
  RuntimeTerminal *o = terminal();
  size_t before = o->used;
  inject("posix_memalign", 0);
  assert(!o->allocator.vtable->alloc(o, 128, 6, 0));
  assert(fault_hits == 1 && o->used == before && o->denied);
  checked_free(o);
}
static void observation_failures(void) {
  RuntimeTerminal *o = terminal(); RuntimeInfo info;
  const int keys[] = {GHOSTTY_TERMINAL_DATA_COLS, GHOSTTY_TERMINAL_DATA_ROWS,
    GHOSTTY_TERMINAL_DATA_CURSOR_X, GHOSTTY_TERMINAL_DATA_CURSOR_Y,
    GHOSTTY_TERMINAL_DATA_CURSOR_VISIBLE, GHOSTTY_TERMINAL_DATA_CURSOR_PENDING_WRAP,
    GHOSTTY_TERMINAL_DATA_ACTIVE_SCREEN,
    GHOSTTY_TERMINAL_DATA_COLOR_FOREGROUND, GHOSTTY_TERMINAL_DATA_COLOR_BACKGROUND,
    GHOSTTY_TERMINAL_DATA_COLOR_PALETTE, GHOSTTY_TERMINAL_DATA_COLOR_CURSOR};
  for (size_t i = 0; i < sizeof(keys) / sizeof(keys[0]); i++) {
    inject("get", keys[i]); assert(rt_info(o, &info) == -1); assert(fault_hits == 1);
  }
  /* Modes are queried one at a time: bracketed paste, cursor keys, and the
   * five that describe mouse tracking and its encoding. A failed query is not
   * fatal to observation, so every one reports its mode as unset. */
  inject("get", GHOSTTY_TERMINAL_DATA_MODE); assert(rt_info(o, &info) == 0);
  assert(fault_hits == 7 && !info.paste && !info.application_cursor);
  assert(!info.mouse_x10 && !info.mouse_normal && !info.mouse_button);
  assert(!info.mouse_any && !info.mouse_sgr);
  uint32_t text[8]; size_t len = 0; RuntimeStyle style;
  inject(NULL, 0); assert(rt_cell(o, 20, 0, text, 8, &len, &style) == -1);
  const char *failures[] = {"style", "cell", "cell-get", "graphemes"};
  for (size_t i = 0; i < sizeof(failures) / sizeof(failures[0]); i++) {
    inject(failures[i], 0); assert(rt_cell(o, 0, 0, text, 8, &len, &style) == -1);
    assert(fault_hits == 1);
  }
  inject("style-color", 0); assert(rt_cell(o, 0, 0, text, 8, &len, &style) == 0);
  assert(fault_hits == 1 && style.foreground.tag == 255);
  checked_free(o);
}
static void mutation_failures(void) {
  RuntimeTerminal *o = terminal(); size_t len = 123; uint8_t bytes[256];
  inject("get", GHOSTTY_TERMINAL_DATA_VT_PROCESSING_ERROR);
  assert(rt_feed(o, (const uint8_t *)"a", 1, bytes, sizeof(bytes), &len) == -1);
  assert(fault_hits == 1 && len == 0 && !o->reply && !o->reply_cap);
  inject("processing-error", 0);
  assert(rt_feed(o, (const uint8_t *)"", 0, bytes, sizeof(bytes), &len) == -1);
  assert(fault_hits == 1);
  inject("empty-reply", 0);
  assert(rt_feed(o, (const uint8_t *)"", 0, bytes, sizeof(bytes), &len) == 0);
  assert(fault_hits == 1 && len == 0);
  inject("resize", 0); assert(rt_resize(o, 30, 5) == -1); assert(fault_hits == 1);
  inject("compress-error", 0); assert(rt_compress(o) == -1); assert(fault_hits == 1);
  inject("compress-unsupported", 0); assert(rt_compress(o) == -3); assert(fault_hits == 1);
  inject("encode", 0); assert(rt_checkpoint(o, bytes, sizeof(bytes), &len) == -1);
  assert(fault_hits == 1 && len == 0);
  inject(NULL, 0); size_t rows = 99; assert(rt_history(o, &rows) == 1 && rows == 0);
  /* Denial is established through the actual callback and must remain sticky. */
  assert(!o->allocator.vtable->alloc(o, LIMIT + 1, 0, 0)); assert(o->denied);
  assert(rt_feed(o, (const uint8_t *)"", 0, bytes, sizeof(bytes), &len) == -2);
  assert(rt_resize(o, 20, 4) == -2);
  assert(rt_compress(o) == -2);
  assert(rt_checkpoint(o, bytes, sizeof(bytes), &len) == -2);
  checked_free(o);
}
static void checkpoint_failures(void) {
  RuntimeTerminal *o = terminal(); size_t len = 0;
  const size_t capacity = 8 * 1024 * 1024;
  uint8_t *bytes = malloc(capacity), *output = malloc(capacity); assert(bytes && output);
  for (size_t i = 0; i < 5000; i++) {
    size_t reply_len = 0;
    assert(rt_feed(o, (const uint8_t *)"history row\r\n", 13, output, capacity, &reply_len) == 0);
  }
  assert(rt_checkpoint(o, bytes, capacity, &len) == 0); checked_free(o);
  int error = 0;
  inject("calloc", 0); assert(!rt_restore(bytes, len, 65536, 1024, LIMIT, &error));
  assert(error == -2 && fault_hits == 1 && !live_allocations);
  const int options[] = {GHOSTTY_SNAPSHOT_DECODER_OPT_MAX_CONTINUATION_BYTES,
                         GHOSTTY_SNAPSHOT_DECODER_OPT_RETAIN_CONTINUATION};
  for (size_t i = 0; i < sizeof(options) / sizeof(options[0]); i++) {
    inject("decoder-set", options[i]);
    assert(!rt_restore(bytes, len, 65536, 1024, LIMIT, &error));
    assert(error == -1 && fault_hits == 1 && !live_allocations);
  }
  inject("set", GHOSTTY_TERMINAL_OPT_WRITE_PTY);
  assert(!rt_restore(bytes, len, 65536, 1024, LIMIT, &error));
  assert(error == -1 && fault_hits == 1 && !live_allocations);
  inject(NULL, 0); o = rt_restore(bytes, len, 65536, 1024, LIMIT, &error); assert(o && !error);
  size_t rows = 99;
  inject("decoder-next", 0); assert(rt_history(o, &rows) == -1);
  assert(fault_hits == 1 && rows == 0 && o->decoder);
  inject("decoder-get", GHOSTTY_SNAPSHOT_DECODER_DATA_SOURCE_OFFSET);
  int result = 0;
  for (size_t step = 0; step < 8192 && result == 0; step++) result = rt_history(o, &rows);
  assert(result == -1 && fault_hits == 1 && o->decoder); checked_free(o);
  inject(NULL, 0); o = rt_restore(bytes, len, 65536, 1024 * 1024, LIMIT, &error); assert(o);
  inject("decoder-get", GHOSTTY_SNAPSHOT_DECODER_DATA_PROGRESS_ROWS);
  assert(rt_history(o, &rows) == -1 && fault_hits == 1); checked_free(o);
  inject(NULL, 0); o = rt_restore(bytes, len, 65536, 1024 * 1024, LIMIT, &error); assert(o);
  assert(!o->allocator.vtable->alloc(o, LIMIT + 1, 0, 0));
  assert(rt_history(o, &rows) == -2); checked_free(o);
  size_t formatted = 0;
  inject(NULL, 0); assert(rt_verify_format(bytes, len, output, 0, &formatted) != 0);
  assert(!live_allocations);
  inject(NULL, 0); assert(rt_verify_format(bytes, 1, output, capacity, &formatted) != 0);
  assert(!live_allocations);
  inject("malloc", 0); assert(rt_verify_format(bytes, len, output, capacity, &formatted) != 0);
  assert(fault_hits && !live_allocations);
  inject("calloc", 0); assert(rt_verify_format(bytes, len, output, capacity, &formatted) == -1);
  assert(fault_hits == 1 && !live_allocations);
  inject("formatter-new", 0);
  assert(rt_verify_format(bytes, len, output, capacity, &formatted) == -1);
  assert(fault_hits == 1 && !live_allocations);
  inject(NULL, 0); free(bytes); free(output);
}
int main(void) {
  constructors_and_allocator(); observation_failures(); mutation_failures(); checkpoint_failures();
  puts("C boundary fault contracts passed (synthetic failures; real handle ownership).");
}
