use super::{ProgramState, ProgramStatusKind, ProgramStatusRecord};
#[derive(Debug)]
pub(super) enum Mutation {
    Report(Box<ProgramStatusRecord>),
    Clear(String),
    Exit,
    Reset,
}
pub(super) fn parse(body: &[u8]) -> Option<Mutation> {
    let (mut state, mut id, mut app, mut kind, mut progress, mut msg, mut title) =
        (None, None, None, None, None, None, None);
    for pair in body.split(|b| *b == b':') {
        let Some(separator) = pair.iter().position(|b| *b == b'=') else {
            continue;
        };
        let (key, value) = (
            pair[..separator].trim_ascii(),
            pair[separator + 1..].trim_ascii(),
        );
        if key.len() > 16 {
            return None;
        }
        let Ok(key) = std::str::from_utf8(key) else {
            continue;
        };
        if key.is_empty() || !key.bytes().all(|b| b.is_ascii_lowercase()) {
            continue;
        }
        // Even a malformed occurrence must not evade a protocol byte limit.
        match key {
            "msg" if value.len() > 2732 => return None,
            "title" if value.len() > 256 => return None,
            "app" if value.len() > 32 => return None,
            "id" if value.len() > 128 => return None,
            _ => {}
        }
        let value = match std::str::from_utf8(value) {
            Ok(value) => value,
            Err(_) if key == "id" => return None,
            Err(_) => continue,
        };
        if key == "id" && !valid_id(value) {
            return None;
        }
        if !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_.,+/=-".contains(&b))
        {
            continue;
        }
        match key {
            "state" => state = Some(value),
            "app" => app = Some(value),
            "id" => id = Some(value),
            "kind" => kind = Some(value),
            "progress" => progress = Some(value),
            "msg" => msg = Some(decode(value, 2048)?),
            "title" => title = Some(decode(value, 192)?),
            _ => {}
        }
    }
    let id = id.unwrap_or_default().to_owned();
    let state = match state? {
        "idle" => ProgramState::Idle,
        "working" => ProgramState::Working,
        "done" => ProgramState::Done,
        "blocked" => ProgramState::Blocked,
        "error" => ProgramState::Error,
        "clear" => return Some(Mutation::Clear(id)),
        _ => return None,
    };
    let kind = if state == ProgramState::Blocked {
        match kind {
            Some("permission") => Some(ProgramStatusKind::Permission),
            Some("question") => Some(ProgramStatusKind::Question),
            Some("auth") => Some(ProgramStatusKind::Auth),
            _ => None,
        }
    } else {
        None
    };
    let progress = if matches!(state, ProgramState::Working | ProgramState::Blocked) {
        progress
            .filter(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
            .and_then(|s| s.parse::<u8>().ok())
            .filter(|n| *n <= 100)
    } else {
        None
    };
    let app = app
        .filter(|v| !v.is_empty() && v.bytes().all(id_byte))
        .map(str::to_owned);
    Some(Mutation::Report(Box::new(ProgramStatusRecord {
        id,
        state,
        app,
        resolved_app: None,
        kind,
        progress,
        msg,
        title,
    })))
}
fn id_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b"_.+-".contains(&b)
}
fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.split('/').count() <= 8
        && id
            .split('/')
            .all(|s| !s.is_empty() && s.len() <= 32 && s.bytes().all(id_byte))
}
fn decode(value: &str, limit: usize) -> Option<String> {
    let data = value.trim_end_matches('=');
    let padding = value.len() - data.len();
    if padding > 2
        || data.len() % 4 == 1
        || (padding > 0 && (value.len() % 4 != 0 || padding != (4 - data.len() % 4) % 4))
    {
        return None;
    }
    let mut out = Vec::with_capacity(data.len() * 3 / 4);
    let (mut bits, mut count) = (0u32, 0u32);
    for b in data.bytes() {
        let n = match b {
            b'A'..=b'Z' => b - b'A',
            b'a'..=b'z' => b - b'a' + 26,
            b'0'..=b'9' => b - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        };
        bits = (bits << 6) | u32::from(n);
        count += 6;
        if count >= 8 {
            count -= 8;
            out.push((bits >> count) as u8);
            bits &= (1 << count) - 1;
        }
    }
    if bits != 0 || out.len() > limit {
        return None;
    }
    let text = String::from_utf8(out).ok()?;
    if text.chars().any(|c| c.is_control()) {
        return None;
    }
    Some(text)
}
