use super::TerminalSize;

/// Portable terminal color specification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalColor {
    /// Terminal default color.
    Default,
    /// Index into the terminal's 256-color palette.
    Palette(u8),
    /// Literal sRGB components.
    Rgb(u8, u8, u8),
}
/// Text decoration retained independently of any native enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Underline {
    /// No underline.
    None,
    /// Single stroke.
    Single,
    /// Double stroke.
    Double,
    /// Curved stroke.
    Curly,
    /// Dotted stroke.
    Dotted,
    /// Dashed stroke.
    Dashed,
}
/// Complete visual style for one grid cell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalStyle {
    /// Foreground specification.
    pub foreground: TerminalColor,
    /// Background specification.
    pub background: TerminalColor,
    /// Underline color specification.
    pub underline_color: TerminalColor,
    /// Underline decoration.
    pub underline: Underline,
    /// Bold weight.
    pub bold: bool,
    /// Italic face.
    pub italic: bool,
    /// Faint intensity.
    pub faint: bool,
    /// Blinking text.
    pub blink: bool,
    /// Swapped foreground/background.
    pub inverse: bool,
    /// Invisible text.
    pub invisible: bool,
    /// Struck text.
    pub strikethrough: bool,
    /// Overlined text.
    pub overline: bool,
}
/// An owned grapheme and style; empty text represents an unoccupied cell.
#[derive(Clone, PartialEq, Eq)]
pub struct TerminalCell {
    /// Unicode grapheme text in this cell.
    pub text: String,
    /// Terminal display width, including wide-character spacer cells (zero).
    pub width: u8,
    /// Cell display attributes.
    pub style: TerminalStyle,
}
impl std::fmt::Debug for TerminalCell {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TerminalCell")
            .field("text", &"[redacted]")
            .field("width", &self.width)
            .field("style", &self.style)
            .finish()
    }
}
/// Cursor position is zero based in the active screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalCursor {
    /// Character-cell column.
    pub col: u16,
    /// Character-cell row.
    pub row: u16,
    /// Cursor is visible.
    pub visible: bool,
    /// Last-column write has left the cursor pending an automatic wrap.
    pub pending_wrap: bool,
}
/// Which mouse events a program has asked to receive.
///
/// These are ordered by how much they report, and only one is in effect: a
/// program selects the events it wants rather than combining them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseTracking {
    /// No reporting; the display owns the pointer.
    None,
    /// Presses only, without releases or motion.
    Press,
    /// Presses and releases.
    PressRelease,
    /// Presses, releases, and motion while a button is held.
    ButtonMotion,
    /// Presses, releases, and all motion.
    AnyMotion,
}

/// How mouse reports are encoded on the wire.
///
/// This is independent of which events are reported, and a consumer that
/// guesses it will send a program bytes it cannot parse.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseEncoding {
    /// The original encoding, which cannot express a coordinate past 223.
    Legacy,
    /// Unambiguous decimal parameters with a distinct release final byte.
    Sgr,
}

/// Input-affecting modes used by runtime consumers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalModes {
    /// Alternate screen is active.
    pub alternate_screen: bool,
    /// Bracketed paste is enabled.
    pub bracketed_paste: bool,
    /// Application cursor key mode is enabled.
    pub application_cursor: bool,
    /// Mouse events the program has asked to receive.
    pub mouse: MouseTracking,
    /// Encoding the program expects those events in.
    pub mouse_encoding: MouseEncoding,
}

impl TerminalModes {
    /// Whether any mouse reporting is active.
    ///
    /// Derived rather than stored so it can never disagree with the tracking
    /// mode it summarizes.
    pub fn mouse_reporting(self) -> bool {
        !matches!(self.mouse, MouseTracking::None)
    }
}
/// Effective colors, including OSC overrides, needed to resolve palette styles.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalPalette {
    /// Effective default foreground in sRGB.
    pub foreground: Option<[u8; 3]>,
    /// Effective default background in sRGB.
    pub background: Option<[u8; 3]>,
    /// Effective cursor color, or None when it follows foreground.
    pub cursor: Option<[u8; 3]>,
    /// Effective indexed palette in sRGB.
    pub indexed: [[u8; 3]; 256],
}
/// Owned bounded active-screen projection; no native references survive this call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalView {
    /// Active-screen dimensions.
    pub size: TerminalSize,
    /// Cursor and wrapping state.
    pub cursor: TerminalCursor,
    /// Terminal input modes.
    pub modes: TerminalModes,
    /// Effective colors for interpreting cell palette and default colors.
    pub palette: TerminalPalette,
    /// Row-major active-screen cells, exactly columns times rows.
    pub cells: Vec<TerminalCell>,
}
