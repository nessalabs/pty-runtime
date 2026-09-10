#ifndef PTY_RUNTIME_GHOSTTY_BRIDGE_H
#define PTY_RUNTIME_GHOSTTY_BRIDGE_H
#include <ghostty/vt.h>
#include <stdlib.h>
#include <string.h>

typedef struct {
  GhosttyTerminal terminal;
  GhosttySnapshotDecoder decoder;
  GhosttyAllocator allocator;
  size_t used, limit, source_len;
  bool denied;
  uint8_t *reply;
  size_t reply_len, reply_cap;
  bool reply_overflow;
} RuntimeTerminal;
typedef struct {
  uint16_t cols, rows, x, y;
  uint8_t visible, pending_wrap, alternate, paste, application_cursor;
  /* Individual mouse modes: which events are wanted, and how they are
   * encoded. A single aggregate cannot answer either question. */
  uint8_t mouse_x10, mouse_normal, mouse_button, mouse_any, mouse_sgr;
  uint8_t foreground[3], background[3], cursor_color[3], has_cursor_color, has_foreground, has_background;
  uint8_t palette[256][3];
} RuntimeInfo;
typedef struct { uint8_t tag, red, green, blue; } RuntimeColor;
typedef struct {
  RuntimeColor foreground, background, underline_color;
  uint16_t flags;
  uint8_t underline, width;
} RuntimeStyle;
int rt_configure(RuntimeTerminal *, size_t, size_t);
int rt_info(RuntimeTerminal *, RuntimeInfo *);
void rt_free(RuntimeTerminal *);
RuntimeTerminal *rt_owner(size_t);
#endif
