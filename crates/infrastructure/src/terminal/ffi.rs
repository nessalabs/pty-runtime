//! Private frozen bridge ABI, compiled against the pinned upstream C headers.
use std::ffi::c_void;
#[repr(C)]
pub(super) struct Info {
    pub cols: u16,
    pub rows: u16,
    pub x: u16,
    pub y: u16,
    pub visible: u8,
    pub pending_wrap: u8,
    pub alternate: u8,
    pub paste: u8,
    pub application_cursor: u8,
    pub mouse_x10: u8,
    pub mouse_normal: u8,
    pub mouse_button: u8,
    pub mouse_any: u8,
    pub mouse_sgr: u8,
    pub foreground: [u8; 3],
    pub background: [u8; 3],
    pub cursor_color: [u8; 3],
    pub has_cursor_color: u8,
    pub has_foreground: u8,
    pub has_background: u8,
    pub palette: [[u8; 3]; 256],
}
impl Default for Info {
    fn default() -> Self {
        Self {
            cols: 0,
            rows: 0,
            x: 0,
            y: 0,
            visible: 0,
            pending_wrap: 0,
            alternate: 0,
            paste: 0,
            application_cursor: 0,
            mouse_x10: 0,
            mouse_normal: 0,
            mouse_button: 0,
            mouse_any: 0,
            mouse_sgr: 0,
            foreground: [0; 3],
            background: [0; 3],
            cursor_color: [0; 3],
            has_cursor_color: 0,
            has_foreground: 0,
            has_background: 0,
            palette: [[0; 3]; 256],
        }
    }
}
#[repr(C)]
#[derive(Default, Clone, Copy)]
pub(super) struct Color {
    pub tag: u8,
    pub red: u8,
    pub green: u8,
    pub blue: u8,
}
#[repr(C)]
#[derive(Default)]
pub(super) struct Style {
    pub foreground: Color,
    pub background: Color,
    pub underline_color: Color,
    pub flags: u16,
    pub underline: u8,
    pub width: u8,
}
unsafe extern "C" {
    pub fn rt_new(
        cols: u16,
        rows: u16,
        history: usize,
        continuation: usize,
        limit: usize,
    ) -> *mut c_void;
    pub fn rt_free(owner: *mut c_void);
    pub fn rt_feed(
        owner: *mut c_void,
        bytes: *const u8,
        len: usize,
        reply: *mut u8,
        cap: usize,
        written: *mut usize,
    ) -> i32;
    pub fn rt_resize(owner: *mut c_void, cols: u16, rows: u16) -> i32;
    pub fn rt_info(owner: *mut c_void, out: *mut Info) -> i32;
    pub fn rt_rows(owner: *mut c_void, total: *mut usize, scrollback: *mut usize) -> i32;
    pub fn rt_cell(
        owner: *mut c_void,
        history: i32,
        x: u16,
        // Ghostty's point coordinate is a u32 row and documents that it "may
        // exceed page size for screen/history tags". Narrowing it here would
        // put a 65,535-row ceiling on retained history that nothing else in
        // the contract states.
        y: u32,
        text: *mut u32,
        cap: usize,
        len: *mut usize,
        out: *mut Style,
    ) -> i32;
    pub fn rt_checkpoint(owner: *mut c_void, bytes: *mut u8, cap: usize, len: *mut usize) -> i32;
    pub fn rt_restore(
        bytes: *const u8,
        len: usize,
        continuation: usize,
        history: usize,
        limit: usize,
        error: *mut i32,
    ) -> *mut c_void;
    pub fn rt_history(owner: *mut c_void, rows: *mut usize) -> i32;
    pub fn rt_compress(owner: *mut c_void) -> i32;
}
