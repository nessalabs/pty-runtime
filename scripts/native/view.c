#include "bridge.h"
#define GET(key, out) do { if (ghostty_terminal_get(o->terminal, key, out)) return -1; } while (0)
static bool mode(RuntimeTerminal *o, GhosttyMode value) {
  GhosttyTerminalModeConfig m = {value, false};
  return !ghostty_terminal_get(o->terminal, GHOSTTY_TERMINAL_DATA_MODE, &m) && m.value;
}
int rt_info(RuntimeTerminal *o, RuntimeInfo *out) {
  bool visible, pending, mouse;
  GhosttyTerminalScreen screen;
  GET(GHOSTTY_TERMINAL_DATA_COLS, &out->cols);
  GET(GHOSTTY_TERMINAL_DATA_ROWS, &out->rows);
  GET(GHOSTTY_TERMINAL_DATA_CURSOR_X, &out->x);
  GET(GHOSTTY_TERMINAL_DATA_CURSOR_Y, &out->y);
  GET(GHOSTTY_TERMINAL_DATA_CURSOR_VISIBLE, &visible);
  GET(GHOSTTY_TERMINAL_DATA_CURSOR_PENDING_WRAP, &pending);
  GET(GHOSTTY_TERMINAL_DATA_ACTIVE_SCREEN, &screen);
  GET(GHOSTTY_TERMINAL_DATA_MOUSE_TRACKING, &mouse);
  out->visible = visible; out->pending_wrap = pending; out->mouse = mouse;
  out->alternate = screen == GHOSTTY_TERMINAL_SCREEN_ALTERNATE;
  out->paste = mode(o, GHOSTTY_MODE_BRACKETED_PASTE);
  out->application_cursor = mode(o, GHOSTTY_MODE_DECCKM);
  GhosttyColorRgb foreground = {0}, background = {0}, cursor, palette[256];
  int fr = ghostty_terminal_get(o->terminal, GHOSTTY_TERMINAL_DATA_COLOR_FOREGROUND, &foreground);
  int br = ghostty_terminal_get(o->terminal, GHOSTTY_TERMINAL_DATA_COLOR_BACKGROUND, &background);
  if ((fr && fr != GHOSTTY_NO_VALUE) || (br && br != GHOSTTY_NO_VALUE)) return -1;
  out->has_foreground = fr == GHOSTTY_SUCCESS; out->has_background = br == GHOSTTY_SUCCESS;
  GET(GHOSTTY_TERMINAL_DATA_COLOR_PALETTE, &palette);
  int cursor_result = ghostty_terminal_get(o->terminal, GHOSTTY_TERMINAL_DATA_COLOR_CURSOR, &cursor);
  if (cursor_result != GHOSTTY_SUCCESS && cursor_result != GHOSTTY_NO_VALUE) return -1;
  out->has_cursor_color = cursor_result == GHOSTTY_SUCCESS;
  if (out->has_cursor_color) {
    out->cursor_color[0] = cursor.r; out->cursor_color[1] = cursor.g; out->cursor_color[2] = cursor.b;
  }
  out->foreground[0] = foreground.r; out->foreground[1] = foreground.g; out->foreground[2] = foreground.b;
  out->background[0] = background.r; out->background[1] = background.g; out->background[2] = background.b;
  for (size_t i = 0; i < 256; i++) {
    out->palette[i][0] = palette[i].r; out->palette[i][1] = palette[i].g; out->palette[i][2] = palette[i].b;
  }
  return 0;
}
static RuntimeColor color(GhosttyStyleColor c) {
  switch (c.tag) {
    case GHOSTTY_STYLE_COLOR_NONE: return (RuntimeColor){0,0,0,0};
    case GHOSTTY_STYLE_COLOR_PALETTE: return (RuntimeColor){1,c.value.palette,0,0};
    case GHOSTTY_STYLE_COLOR_RGB: return (RuntimeColor){2,c.value.rgb.r,c.value.rgb.g,c.value.rgb.b};
    default: return (RuntimeColor){255,0,0,0};
  }
}
int rt_cell(RuntimeTerminal *o, uint16_t x, uint16_t y, uint32_t *text,
            size_t cap, size_t *len, RuntimeStyle *out) {
  GhosttyGridRef ref = {.size = sizeof(ref)};
  GhosttyPoint point = {.tag = GHOSTTY_POINT_TAG_ACTIVE, .value.coordinate = {x,y}};
  GhosttyStyle s = {.size = sizeof(s)};
  GhosttyCell cell;
  GhosttyCellWide wide;
  if (ghostty_terminal_grid_ref(o->terminal, point, &ref) ||
      ghostty_grid_ref_style(&ref, &s) || ghostty_grid_ref_cell(&ref, &cell) ||
      ghostty_cell_get(cell, GHOSTTY_CELL_DATA_WIDE, &wide)) return -1;
  int r = ghostty_grid_ref_graphemes(&ref, text, cap, len);
  if (r) return r == GHOSTTY_OUT_OF_SPACE ? -2 : -1;
  out->foreground = color(s.fg_color); out->background = color(s.bg_color);
  out->underline_color = color(s.underline_color);
  out->flags = s.bold | s.italic << 1 | s.faint << 2 | s.blink << 3 |
    s.inverse << 4 | s.invisible << 5 | s.strikethrough << 6 | s.overline << 7;
  out->underline = s.underline;
  switch (wide) {
    case GHOSTTY_CELL_WIDE_NARROW: out->width = 1; break;
    case GHOSTTY_CELL_WIDE_WIDE: out->width = 2; break;
    default: out->width = 0;
  }
  return 0;
}
