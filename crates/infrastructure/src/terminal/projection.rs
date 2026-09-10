use super::{GhosttyTerminal, ffi};
use pty_runtime_domain::terminal::*;
impl GhosttyTerminal {
    pub(super) fn info(&mut self) -> Result<ffi::Info, TerminalError> {
        self.healthy()?;
        let mut info = ffi::Info::default();
        // SAFETY: Exclusive live native owner and correctly laid-out initialized ABI output.
        let result = unsafe { ffi::rt_info(self.raw.as_ptr(), &mut info) };
        if result != 0 {
            return Err(TerminalError::EngineFailure);
        }
        Ok(info)
    }
    /// Read retained rows without moving anything the engine or another
    /// reader depends on.
    pub(super) fn history_rows(
        &mut self,
        start: u64,
        count: u16,
    ) -> Result<TerminalHistory, TerminalError> {
        self.healthy()?;
        let info = self.info()?;
        let size = TerminalSize::new(info.cols, info.rows)?;

        let mut total = 0usize;
        let mut scrollback = 0usize;
        // SAFETY: Exclusive live native owner; both outputs are initialized
        // scalars and the terminal is only read.
        let result = unsafe { ffi::rt_rows(self.raw.as_ptr(), &mut total, &mut scrollback) };
        if result != 0 {
            return Err(TerminalError::EngineFailure);
        }
        let total = total as u64;

        // A request past the window is not an error: it is what a reader that
        // was scrolled back sees once old rows are discarded. Clamp and report
        // where the read actually began.
        let start = start.min(total);
        let available = (total - start).min(u64::from(count));
        let cols = usize::from(size.cols());
        let cells_wanted = cols
            .checked_mul(available as usize)
            .ok_or(TerminalError::BudgetExceeded)?;

        let mut cells = Vec::new();
        cells
            .try_reserve_exact(cells_wanted)
            .map_err(|_| TerminalError::BudgetExceeded)?;
        let mut remaining = self.config.view_bytes;
        for row in 0..available {
            let y = u16::try_from(start + row).map_err(|_| TerminalError::BudgetExceeded)?;
            for x in 0..size.cols() {
                cells.push(self.read_cell(1, x, y, &mut remaining)?);
            }
        }

        Ok(TerminalHistory {
            start,
            cols: size.cols(),
            cells,
            total,
            scrollback: scrollback as u64,
        })
    }

    /// Copy one cell into owned domain values.
    ///
    /// `history` selects absolute screen addressing, which covers retained
    /// scrollback as well as the active area and moves nothing, rather than
    /// the active-relative addressing a live view uses.
    fn read_cell(
        &mut self,
        history: i32,
        x: u16,
        y: u16,
        remaining: &mut usize,
    ) -> Result<TerminalCell, TerminalError> {
        let mut style = ffi::Style::default();
        let mut small = [0u32; 32];
        let mut len = 0;
        // SAFETY: C reads an ephemeral grid reference and copies to bounded
        // initialized buffers before returning. The terminal is not mutated.
        let result = unsafe {
            ffi::rt_cell(
                self.raw.as_ptr(),
                history,
                x,
                y,
                small.as_mut_ptr(),
                small.len(),
                &mut len,
                &mut style,
            )
        };
        let text = if result == -2 {
            if len > *remaining / 4 {
                return Err(TerminalError::BudgetExceeded);
            }
            let mut larger = Vec::new();
            larger
                .try_reserve_exact(len)
                .map_err(|_| TerminalError::BudgetExceeded)?;
            larger.resize(len, 0);
            // SAFETY: Same exclusive terminal has not changed; allocated capacity
            // is the engine-reported codepoint count and C checks that bound again.
            let retry = unsafe {
                ffi::rt_cell(
                    self.raw.as_ptr(),
                    history,
                    x,
                    y,
                    larger.as_mut_ptr(),
                    larger.len(),
                    &mut len,
                    &mut style,
                )
            };
            if retry != 0 {
                return Err(TerminalError::EngineFailure);
            }
            decode(&larger[..len], remaining)?
        } else if result == 0 {
            decode(&small[..len], remaining)?
        } else {
            return Err(TerminalError::EngineFailure);
        };
        Ok(TerminalCell {
            text,
            width: style.width,
            style: convert_style(style)?,
        })
    }

    pub(super) fn project(&mut self) -> Result<TerminalView, TerminalError> {
        let info = self.info()?;
        let size = TerminalSize::new(info.cols, info.rows)?;
        let count = usize::from(size.cols()) * usize::from(size.rows());
        let mut cells = Vec::new();
        cells
            .try_reserve_exact(count)
            .map_err(|_| TerminalError::BudgetExceeded)?;
        let mut remaining = self.config.view_bytes;
        for y in 0..size.rows() {
            for x in 0..size.cols() {
                cells.push(self.read_cell(0, x, y, &mut remaining)?);
            }
        }
        Ok(TerminalView {
            size,
            cursor: TerminalCursor {
                col: info.x,
                row: info.y,
                visible: info.visible != 0,
                pending_wrap: info.pending_wrap != 0,
            },
            modes: TerminalModes {
                alternate_screen: info.alternate != 0,
                bracketed_paste: info.paste != 0,
                application_cursor: info.application_cursor != 0,
                // Later modes win: a program that enables any-event
                // tracking over button tracking wants the wider set.
                mouse: if info.mouse_any != 0 {
                    MouseTracking::AnyMotion
                } else if info.mouse_button != 0 {
                    MouseTracking::ButtonMotion
                } else if info.mouse_normal != 0 {
                    MouseTracking::PressRelease
                } else if info.mouse_x10 != 0 {
                    MouseTracking::Press
                } else {
                    MouseTracking::None
                },
                mouse_encoding: if info.mouse_sgr != 0 {
                    MouseEncoding::Sgr
                } else {
                    MouseEncoding::Legacy
                },
            },
            palette: TerminalPalette {
                foreground: (info.has_foreground != 0).then_some(info.foreground),
                background: (info.has_background != 0).then_some(info.background),
                cursor: (info.has_cursor_color != 0).then_some(info.cursor_color),
                indexed: info.palette,
            },
            cells,
        })
    }
}
fn decode(codepoints: &[u32], remaining: &mut usize) -> Result<String, TerminalError> {
    let required = codepoints
        .len()
        .checked_mul(4)
        .ok_or(TerminalError::BudgetExceeded)?;
    if required > *remaining {
        return Err(TerminalError::BudgetExceeded);
    }
    let mut text = String::new();
    text.try_reserve_exact(required)
        .map_err(|_| TerminalError::BudgetExceeded)?;
    for &point in codepoints {
        text.push(char::from_u32(point).ok_or(TerminalError::EngineFailure)?);
    }
    *remaining -= required;
    Ok(text)
}
fn color(c: ffi::Color) -> Result<TerminalColor, TerminalError> {
    match c.tag {
        0 => Ok(TerminalColor::Default),
        1 => Ok(TerminalColor::Palette(c.red)),
        2 => Ok(TerminalColor::Rgb(c.red, c.green, c.blue)),
        _ => Err(TerminalError::EngineFailure),
    }
}
fn convert_style(s: ffi::Style) -> Result<TerminalStyle, TerminalError> {
    let underline = match s.underline {
        0 => Underline::None,
        1 => Underline::Single,
        2 => Underline::Double,
        3 => Underline::Curly,
        4 => Underline::Dotted,
        5 => Underline::Dashed,
        _ => return Err(TerminalError::EngineFailure),
    };
    Ok(TerminalStyle {
        foreground: color(s.foreground)?,
        background: color(s.background)?,
        underline_color: color(s.underline_color)?,
        underline,
        bold: s.flags & 1 != 0,
        italic: s.flags & 2 != 0,
        faint: s.flags & 4 != 0,
        blink: s.flags & 8 != 0,
        inverse: s.flags & 16 != 0,
        invisible: s.flags & 32 != 0,
        strikethrough: s.flags & 64 != 0,
        overline: s.flags & 128 != 0,
    })
}
