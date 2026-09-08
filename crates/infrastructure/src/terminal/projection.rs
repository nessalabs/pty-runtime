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
                let mut style = ffi::Style::default();
                let mut small = [0u32; 32];
                let mut len = 0;
                // SAFETY: C reads an ephemeral grid reference and copies to bounded
                // initialized buffers before returning. The terminal is not mutated.
                let result = unsafe {
                    ffi::rt_cell(
                        self.raw.as_ptr(),
                        x,
                        y,
                        small.as_mut_ptr(),
                        small.len(),
                        &mut len,
                        &mut style,
                    )
                };
                let text = if result == -2 {
                    if len > remaining / 4 {
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
                    decode(&larger[..len], &mut remaining)?
                } else if result == 0 {
                    decode(&small[..len], &mut remaining)?
                } else {
                    return Err(TerminalError::EngineFailure);
                };
                cells.push(TerminalCell {
                    text,
                    width: style.width,
                    style: convert_style(style)?,
                });
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
                mouse_reporting: info.mouse != 0,
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
