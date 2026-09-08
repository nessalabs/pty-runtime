//! Interactive raw PTY client. Ctrl-] cancels; Ctrl-C and Ctrl-Z go to the child.
#[path = "interactive_support/host.rs"]
mod host;
use pty_runtime::*;
use std::{
    future::Future,
    io,
    pin::Pin,
    task::{Context, Poll, Waker},
    time::{Duration, Instant},
};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
fn poll<F: Future + Unpin>(future: &mut F) -> Option<F::Output> {
    match Pin::new(future).poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(value) => Some(value),
        Poll::Pending => None,
    }
}
fn run() -> Result<i32> {
    let mut arguments = std::env::args_os().skip(1);
    let executable = arguments.next().unwrap_or_else(|| "/bin/sh".into());
    if executable == "--help" {
        println!(
            "Usage: interactive [/absolute/program [arguments...]]\nDefault: /bin/sh. Ctrl-] cancels; Ctrl-C/Ctrl-Z are forwarded to the child."
        );
        return Ok(0);
    }
    let cwd = std::env::current_dir()?;
    let command = CommandSpec::new(executable.into(), cwd.clone(), arguments.collect())?
        .with_environment(EnvironmentPolicy::Inherit, vec![], vec![])?;
    let terminal = host::Terminal::enter()?;
    let (cols, rows) = terminal.size()?;
    let mut size = TerminalSize::new(cols, rows)
        .map_err(|_| io::Error::other("invalid host terminal size"))?;
    let owner = Runtime::new(
        vec![cwd],
        RuntimeOptions {
            max_sessions: 1,
            replay_bytes: 1024 * 1024,
            ..RuntimeOptions::default()
        },
    )?;
    let id = SessionId::new("interactive".to_owned())
        .map_err(|_| io::Error::other("invalid session identity"))?;
    let mut options = SessionOptions::raw(size);
    options.replay_bytes = 1024 * 1024;
    let session = owner.spawn(id.clone(), &command, options)?;
    let mut observer = session.attach(AttachPosition::Oldest)?;
    let mut input = [0; 4096];
    let mut output = Vec::<u8>::new();
    let mut output_offset = 0;
    let mut writing = None;
    let mut resizing = None;
    let mut stopped = None;
    let mut completed = None;
    let mut last_size = Instant::now();
    loop {
        if terminal.signal() != 0 && stopped.is_none() {
            session.cancel()?;
            stopped = Some(Instant::now());
        }
        if let Some(operation) = &mut writing {
            if let Some(result) = poll(operation) {
                let result: WriteOutcome = result;
                if result.error.is_some() && stopped.is_none() {
                    return Err(io::Error::other(format!(
                        "input stopped after {} bytes: {:?}",
                        result.written, result.error
                    ))
                    .into());
                }
                writing = None;
            }
        }
        if let Some(operation) = &mut resizing {
            if let Some(result) = poll(operation) {
                if stopped.is_none() {
                    result?;
                }
                resizing = None;
            }
        }
        if stopped.is_none() && writing.is_none() {
            if let Some(count) = host::read(&mut input)? {
                if count == 0 || input[..count].contains(&29) {
                    session.cancel()?;
                    stopped = Some(Instant::now());
                } else {
                    writing = Some(session.write(&input[..count])?);
                }
                input[..count].fill(0);
            }
        }
        if output_offset == output.len() {
            output.clear();
            output_offset = 0;
            match observer.try_next()? {
                Some(OutputEvent::Replay(ReplayPage::Bytes { bytes, .. })) => output = bytes,
                Some(OutputEvent::Replay(ReplayPage::Gap { from, to })) => {
                    return Err(io::Error::other(format!(
                        "display fell behind; missing bytes [{}, {})",
                        from.offset, to.offset
                    ))
                    .into());
                }
                Some(OutputEvent::Complete(result)) => completed = Some(result),
                None => (),
                _ => return Err(io::Error::other("unexpected replay state").into()),
            }
        }
        if output_offset < output.len() {
            if let Some(count) = host::write(&output[output_offset..])? {
                output_offset += count;
            }
        }
        if let Some(completion) = completed {
            if completion.status.supervision_error.is_some()
                || completion.status.drain != Some(DrainOutcome::Eof)
            {
                return Err(io::Error::other(format!(
                    "incomplete terminal cleanup: {completion:?}"
                ))
                .into());
            }
            owner.forget(&id)?;
            return Ok(if terminal.signal() != 0 {
                128 + terminal.signal()
            } else {
                match completion.status.exit {
                    Some(ExitStatus::Code(code)) => code,
                    Some(ExitStatus::Signal(signal)) => 128 + signal,
                    None => 1,
                }
            });
        }
        if last_size.elapsed() >= Duration::from_millis(50)
            && resizing.is_none()
            && stopped.is_none()
        {
            let (cols, rows) = terminal.size()?;
            let updated = TerminalSize::new(cols, rows)
                .map_err(|_| io::Error::other("invalid host terminal size"))?;
            if updated != size {
                resizing = Some(session.resize(updated)?);
                size = updated;
            }
            last_size = Instant::now();
        }
        if stopped.is_some_and(|started| started.elapsed() > Duration::from_secs(10)) {
            return Err(io::Error::other("cancellation did not finish within ten seconds").into());
        }
        if terminal.poll(
            stopped.is_none() && writing.is_none(),
            output_offset < output.len(),
        )? && stopped.is_none()
        {
            session.cancel()?;
            stopped = Some(Instant::now());
        }
    }
}
fn main() {
    match run() {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("interactive: {error}");
            std::process::exit(1);
        }
    }
}
