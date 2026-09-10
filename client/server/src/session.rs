//! One PTY session driving one rendered terminal.
//!
//! The runtime's session and terminal types are synchronous and polled, so
//! this runs on its own thread and talks to the socket over channels. That
//! keeps every runtime type off the async executor and out of the WebSocket
//! handler entirely.
use crate::wire::{self, ServerMessage, Sent, StyleTable, VERSION};
use pty_runtime::{
    AttachPosition, CommandSpec, EnvironmentPolicy, ExitStatus, OutputEvent, ReplayPage, Runtime,
    RuntimeOptions, SessionId, SessionOptions, TerminalSize,
    ports::ITerminalFactory,
    terminal::TerminalConfig,
};
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll, Waker},
    time::{Duration, Instant},
};
use tokio::sync::mpsc;

/// The interval between rendered frames.
///
/// Output arrives in bursts far faster than a display can use, so the terminal
/// is fed as fast as bytes appear but only projected on this cadence. Without
/// it a `yes` loop would generate a frame per read.
const FRAME_INTERVAL: Duration = Duration::from_millis(16);

pub enum Command {
    Input(Vec<u8>),
    Resize { cols: u16, rows: u16 },
    History { start: u64, count: u16 },
}

fn poll<F: Future + Unpin>(future: &mut F) -> Option<F::Output> {
    match Pin::new(future).poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(value) => Some(value),
        Poll::Pending => None,
    }
}

fn terminal_config(size: TerminalSize) -> TerminalConfig {
    TerminalConfig {
        size,
        history_bytes: 16 * 1024 * 1024,
        continuation_bytes: 64 * 1024,
        reply_bytes: 4096,
        checkpoint_bytes: 8 * 1024 * 1024,
        feed_bytes: 4096,
        native_bytes: 32 * 1024 * 1024,
        view_bytes: 1024 * 1024,
    }
}

fn fail(outbound: &mpsc::Sender<ServerMessage>, message: impl Into<String>) {
    let _ = outbound.blocking_send(ServerMessage::Error {
        v: VERSION,
        message: message.into(),
    });
}

/// Run one session to completion. Returns when the child exits or the socket
/// closes, and always tears the session down before returning.
pub fn run(
    shell: String,
    cols: u16,
    rows: u16,
    mut inbound: mpsc::Receiver<Command>,
    outbound: mpsc::Sender<ServerMessage>,
) {
    let Ok(size) = TerminalSize::new(cols, rows) else {
        fail(&outbound, "invalid terminal size");
        return;
    };
    let Ok(cwd) = std::env::current_dir() else {
        fail(&outbound, "no working directory");
        return;
    };

    // TERM is what tells the child which escape sequences it may emit. The
    // server's own environment often has none, and without it `clear` and every
    // curses program refuse to run. The runtime renders with Ghostty, so
    // advertising xterm-256color is accurate rather than merely convenient.
    // Strip host NO_COLOR / FORCE_COLOR=0: agent and CI shells often set them,
    // and htop/ncurses then draw a monochrome UI even though this PTY is a
    // full color terminal.
    let removals = vec!["NO_COLOR".into(), "FORCE_COLOR".into()];
    let overrides = vec![
        ("TERM".into(), "xterm-256color".into()),
        ("COLORTERM".into(), "truecolor".into()),
    ];
    let command = match CommandSpec::new(shell.into(), cwd.clone(), Vec::new())
        .and_then(|spec| spec.with_environment(EnvironmentPolicy::Inherit, removals, overrides))
    {
        Ok(command) => command,
        Err(error) => return fail(&outbound, format!("command rejected: {error:?}")),
    };

    let runtime = match Runtime::new(
        vec![cwd],
        RuntimeOptions {
            max_sessions: 1,
            replay_bytes: 1024 * 1024,
            ..RuntimeOptions::default()
        },
    ) {
        Ok(runtime) => runtime,
        Err(error) => return fail(&outbound, format!("runtime unavailable: {error:?}")),
    };

    let id = match SessionId::new("client".to_owned()) {
        Ok(id) => id,
        Err(error) => return fail(&outbound, format!("invalid session id: {error:?}")),
    };
    let mut options = SessionOptions::raw(size);
    options.replay_bytes = 1024 * 1024;

    let session = match runtime.spawn(id.clone(), &command, options) {
        Ok(session) => session,
        Err(error) => return fail(&outbound, format!("spawn failed: {error:?}")),
    };
    let mut observer = match session.attach(AttachPosition::Oldest) {
        Ok(observer) => observer,
        Err(error) => return fail(&outbound, format!("attach failed: {error:?}")),
    };

    // The engine that turns bytes into a grid. This is the same terminal the
    // runtime uses for checkpointing, so what the browser draws is what a
    // snapshot would preserve.
    let factory = pty_runtime_infrastructure::terminal::GhosttyTerminalFactory;
    let mut terminal = match factory.create(terminal_config(size)) {
        Ok(terminal) => terminal,
        Err(error) => return fail(&outbound, format!("terminal unavailable: {error:?}")),
    };

    let mut sent = Sent::default();
    let mut styles = StyleTable::default();
    // History rows resolve colours against the same palette as frames, so the
    // client keeps one style table for both.
    let mut palette = pty_runtime::terminal::TerminalPalette {
        foreground: None,
        background: None,
        cursor: None,
        indexed: [[0; 3]; 256],
    };
    let mut pending_input: std::collections::VecDeque<u8> = std::collections::VecDeque::new();
    let mut writing = None;
    let mut resizing = None;
    let mut generation = 0u64;
    let mut next_frame = Instant::now();
    let mut dirty = true;
    let mut exited = None;

    loop {
        // Client commands. Input is written through the session so the child
        // sees it; the terminal only ever sees the child's output.
        match inbound.try_recv() {
            Ok(Command::Input(bytes)) => {
                // Only one write may be in flight, so queue rather than drop:
                // typing faster than the child reads must not lose keystrokes.
                pending_input.extend(bytes);
            }
            Ok(Command::Resize { cols, rows }) => {
                if let Ok(size) = TerminalSize::new(cols, rows) {
                    generation += 1;
                    if terminal.resize(size, generation).is_err() {
                        fail(&outbound, "terminal resize failed");
                        break;
                    }
                    match session.resize(size) {
                        Ok(operation) => resizing = Some(operation),
                        Err(error) => {
                            fail(&outbound, format!("session resize failed: {error:?}"));
                            break;
                        }
                    }
                    // Project immediately: waiting for the child to redraw leaves
                    // the previous geometry on screen while the window moves.
                    dirty = true;
                }
            }
            Ok(Command::History { start, count }) => {
                // Reading history moves nothing and cannot fail the session:
                // a refusal is reported and the terminal carries on.
                match terminal.history(start, count.min(500)) {
                    Ok(history) => {
                        let (rows, pending) =
                            wire::encode_history(&history, &mut styles, &palette);
                        if !pending.is_empty() {
                            let _ = outbound.blocking_send(ServerMessage::Styles {
                                v: VERSION,
                                styles: pending,
                            });
                        }
                        if outbound
                            .blocking_send(ServerMessage::History {
                                v: VERSION,
                                start: history.start,
                                rows,
                                total: history.total,
                                scrollback: history.scrollback,
                            })
                            .is_err()
                        {
                            break;
                        }
                    }
                    Err(error) => {
                        fail(&outbound, format!("history unavailable: {error:?}"));
                    }
                }
            }
            Err(mpsc::error::TryRecvError::Empty) => {}
            Err(mpsc::error::TryRecvError::Disconnected) => break,
        }

        if let Some(operation) = &mut writing {
            if poll(operation).is_some() {
                writing = None;
            }
        }
        if writing.is_none() && !pending_input.is_empty() {
            let bytes: Vec<u8> = pending_input.drain(..).collect();
            match session.write(&bytes) {
                Ok(operation) => writing = Some(operation),
                Err(error) => {
                    fail(&outbound, format!("write failed: {error:?}"));
                    break;
                }
            }
        }
        if let Some(operation) = &mut resizing {
            if poll(operation).is_some() {
                resizing = None;
            }
        }

        // Output. A gap means the display fell behind its replay window; for a
        // demo the honest thing is to say so rather than draw a wrong screen.
        let mut fed = false;
        loop {
            match observer.try_next() {
                Ok(Some(OutputEvent::Replay(ReplayPage::Bytes { bytes, .. }))) => {
                    for chunk in bytes.chunks(4096) {
                        if terminal.feed(chunk).is_err() {
                            fail(&outbound, "terminal rejected output");
                            return;
                        }
                    }
                    fed = true;
                }
                Ok(Some(OutputEvent::Replay(ReplayPage::Gap { .. }))) => {
                    fail(&outbound, "display fell behind the replay window");
                    return;
                }
                Ok(Some(OutputEvent::Complete(completion))) => {
                    exited = Some(match completion.status.exit {
                        Some(ExitStatus::Code(code)) => Some(code),
                        _ => None,
                    });
                    break;
                }
                Ok(Some(_)) | Ok(None) => break,
                Err(error) => {
                    fail(&outbound, format!("output failed: {error:?}"));
                    return;
                }
            }
        }

        // Anything fed leaves the projection stale until a frame actually goes
        // out. Tracking that separately from "bytes arrived this iteration"
        // matters: output that lands before the next deadline must still be
        // drawn, not dropped because the tick was early.
        dirty |= fed;

        // Project on the frame cadence, or immediately once the child is gone
        // so the last output is never left undrawn.
        let now = Instant::now();
        if exited.is_some() || (dirty && now >= next_frame) {
            next_frame = now + FRAME_INTERVAL;
            match terminal.view() {
                Ok(view) => {
                    dirty = false;
                    palette = view.palette.clone();
                    // Row counts ride along with every frame so a client never
                    // has to request history just to learn the window size.
                    let mut total = 0usize;
                    let mut scrollback = 0usize;
                    // SAFETY-equivalent: this is a read-only projection call.
                    let counts = terminal.history(u64::MAX, 0);
                    if let Ok(counts) = counts {
                        total = counts.total as usize;
                        scrollback = counts.scrollback as usize;
                    }
                    for message in
                        sent.diff(&view, &mut styles, total as u64, scrollback as u64)
                    {
                        if outbound.blocking_send(message).is_err() {
                            return;
                        }
                    }
                }
                Err(error) => {
                    fail(&outbound, format!("projection failed: {error:?}"));
                    return;
                }
            }
        }

        if let Some(status) = exited {
            let _ = outbound.blocking_send(ServerMessage::Exit { v: VERSION, status });
            break;
        }

        if !fed && writing.is_none() && pending_input.is_empty() {
            // Nothing arrived and nothing is owed; yield rather than spin the
            // core. Stay responsive while a frame is still pending.
            std::thread::sleep(Duration::from_millis(if dirty { 1 } else { 4 }));
        }
    }

    let _ = session.cancel();
    let _ = runtime.forget(&id);
}
