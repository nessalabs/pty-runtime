use std::fmt;

/// A program's explicitly reported state, without presentation or authorization policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgramState {
    /// The program is present but not busy.
    Idle,
    /// The program is actively working.
    Working,
    /// The reported work completed.
    Done,
    /// The program reports that it needs intervention.
    Blocked,
    /// The reported work failed.
    Error,
}
/// Why an untrusted program reports that it is blocked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgramStatusKind {
    /// The program requests permission; this is never authorization.
    Permission,
    /// The program requests an answer.
    Question,
    /// The program requests authentication.
    Auth,
}
/// An owned, bounded record. Text is plain UTF-8, never markup.
#[derive(Clone, PartialEq, Eq)]
pub struct ProgramStatusRecord {
    /// Empty for the root record; otherwise a validated hierarchical identifier.
    pub id: String,
    /// Explicitly reported state.
    pub state: ProgramState,
    /// App explicitly supplied in this report.
    pub app: Option<String>,
    /// Explicit app or nearest ancestor's app, resolved when taking a snapshot.
    pub resolved_app: Option<String>,
    /// Meaningful only for blocked reports.
    pub kind: Option<ProgramStatusKind>,
    /// Meaningful only for working or blocked; None means indeterminate.
    pub progress: Option<u8>,
    /// Exact decoded message.
    pub msg: Option<String>,
    /// Exact decoded title.
    pub title: Option<String>,
}
impl fmt::Debug for ProgramStatusRecord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProgramStatusRecord")
            .field("state", &self.state)
            .field("text", &"[REDACTED]")
            .finish()
    }
}
impl ProgramStatusRecord {
    /// Copy the message with invisible format characters made visible as escape notation.
    pub fn display_msg(&self) -> Option<String> {
        self.msg.as_deref().map(display_text)
    }
    /// Copy the title with invisible format characters made visible as escape notation.
    pub fn display_title(&self) -> Option<String> {
        self.title.as_deref().map(display_text)
    }
    pub(super) fn heap_bytes(&self) -> usize {
        self.id.capacity()
            + [&self.app, &self.resolved_app, &self.msg, &self.title]
                .into_iter()
                .map(|v| v.as_ref().map_or(0, String::capacity))
                .sum::<usize>()
    }
}
fn display_text(text: &str) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    for c in text.chars() {
        if matches!(c,'\u{0600}'..='\u{0605}' | '\u{06dd}' | '\u{070f}' | '\u{0890}'..='\u{0891}' | '\u{08e2}' | '\u{110bd}' | '\u{110cd}' | '\u{13430}'..='\u{1343f}' | '\u{00ad}'|'\u{034f}'|'\u{061c}'|'\u{115f}'|'\u{1160}'|'\u{17b4}'|'\u{17b5}'|'\u{180b}'..='\u{180f}'|'\u{200b}'..='\u{200f}'|'\u{202a}'..='\u{202e}'|'\u{2060}'..='\u{206f}'|'\u{3164}'|'\u{fe00}'..='\u{fe0f}'|'\u{feff}'|'\u{ffa0}'|'\u{fff9}'..='\u{fffb}'|'\u{1bca0}'..='\u{1bca3}'|'\u{1d173}'..='\u{1d17a}'|'\u{e0000}'..='\u{e0fff}')
        {
            let _ = write!(out, "\\u{{{:x}}}", c as u32);
        } else {
            out.push(c);
        }
    }
    out
}
