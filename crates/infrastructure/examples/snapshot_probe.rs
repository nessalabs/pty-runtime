//! Restore a snapshot file and report what the runtime decided.
//!
//! This exists so a snapshot can be examined by hand rather than only through
//! the corpus. Point it at a file and it prints whether the bytes were
//! accepted, refused, and why. A refusal is the correct outcome for a damaged
//! or hostile snapshot; the runtime must never fault on one.
//!
//! Usage: snapshot_probe --make <file> [cols] [rows]
//!        snapshot_probe <file> [cols] [rows]
//!        snapshot_probe <file> [cols] [rows] --flip <offset>=<byte>
//!        snapshot_probe <file> [cols] [rows] --sweep [stride]
//!
//! `--make` writes a real snapshot of a terminal carrying scrollback, styled
//! text and a hyperlink, so the page has the string and hyperlink storage the
//! decoder is most exposed on. `--flip` rewrites one byte before restoring, so
//! that snapshot can be damaged at a chosen offset and the outcome compared
//! against the original. `--sweep` damages every byte in turn and reports how
//! each one was handled. Restore must always either accept the bytes and give
//! back a usable terminal, or refuse them. It must never fault, so a sweep
//! that finishes at all is the result: a fault would end the process.
#[cfg(feature = "ghostty")]
mod probe {
    use pty_runtime_application::terminal::ITerminalFactory;
    use pty_runtime_domain::{ReplayCursor, SessionLifetime, terminal::*};
    use pty_runtime_infrastructure::terminal::GhosttyTerminalFactory;

    fn config(size: TerminalSize) -> TerminalConfig {
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

    /// Write a snapshot of a terminal that actually holds content worth losing.
    fn make(path: &str, size: TerminalSize) {
        let mut terminal = GhosttyTerminalFactory
            .create(config(size))
            .expect("create terminal");

        // Scrollback, so the snapshot carries history page records.
        for line in 0..200 {
            terminal
                .feed(format!("line {line}: the quick brown fox\r\n").as_bytes())
                .expect("feed");
        }
        // A hyperlink and a style put entries in the page's string and hyperlink
        // storage, which is the capacity the decoder was faulting on.
        terminal
            .feed(b"\x1b]8;id=probe;https://example.test/probe\x1b\\linked\x1b]8;;\x1b\\")
            .expect("feed hyperlink");
        terminal
            .feed("\x1b[1;31mstyled\x1b[0m and wide: 界 combining: e\u{301}\r\n".as_bytes())
            .expect("feed styled");

        let checkpoint = terminal
            .checkpoint(CheckpointDescriptor {
                compatibility: CompatibilityId::new(GhosttyTerminalFactory.compatibility())
                    .unwrap(),
                processed: ReplayCursor {
                    lifetime: SessionLifetime::new(1, 1),
                    offset: 0,
                },
                control_generation: ControlGeneration::from_raw(0),
            })
            .expect("checkpoint");

        std::fs::write(path, &checkpoint.bytes).expect("write snapshot");
        println!(
            "wrote {path}: {} bytes at {}x{}",
            checkpoint.bytes.len(),
            size.cols(),
            size.rows(),
        );
    }

    /// Try one damaged copy and report which bucket the outcome falls in.
    fn probe_quietly(bytes: Vec<u8>, size: TerminalSize) -> &'static str {
        let checkpoint = TerminalCheckpoint {
            descriptor: CheckpointDescriptor {
                compatibility: CompatibilityId::new(GhosttyTerminalFactory.compatibility())
                    .unwrap(),
                processed: ReplayCursor {
                    lifetime: SessionLifetime::new(1, 1),
                    offset: 0,
                },
                control_generation: ControlGeneration::from_raw(0),
            },
            bytes,
        };
        let Ok(mut terminal) = GhosttyTerminalFactory.restore(checkpoint, config(size)) else {
            return "refused";
        };
        let mut steps = 0u32;
        loop {
            match terminal.restore_history_step() {
                Ok(progress) => {
                    if progress.is_finished() {
                        break;
                    }
                    steps += 1;
                    assert!(steps < 100_000, "history restoration did not terminate");
                }
                Err(_) => return "refused",
            }
        }
        if terminal.view().is_err() {
            return "decoded but unusable";
        }
        match terminal.feed(b"\x1b[0mprobe") {
            Ok(_) => {}
            Err(_) => return "decoded but unusable",
        }
        if terminal.view().is_err() {
            return "decoded but unusable";
        }
        "accepted"
    }

    /// Damage every byte in turn. Reaching the end is the assertion: a fault
    /// would take the process with it.
    fn sweep(original: &[u8], size: TerminalSize, stride: usize) {
        let mut accepted = 0u64;
        let mut refused = 0u64;
        let mut unusable = Vec::new();
        let mut tried = 0u64;

        for offset in (0..original.len()).step_by(stride) {
            for value in [0x00u8, 0x01, 0x7f, 0xff] {
                if original[offset] == value {
                    continue;
                }
                let mut bytes = original.to_vec();
                bytes[offset] = value;
                tried += 1;
                match probe_quietly(bytes, size) {
                    "accepted" => accepted += 1,
                    "refused" => refused += 1,
                    _ => unusable.push((offset, value)),
                }
            }
            if offset % (stride * 200) == 0 {
                println!("  ... offset {offset}/{}", original.len());
            }
        }

        println!("swept {tried} damaged copies (stride {stride})");
        println!("  accepted (still a usable terminal): {accepted}");
        println!("  refused:                            {refused}");
        println!("  decoded but unusable:               {}", unusable.len());
        if unusable.is_empty() {
            println!("no fault and no unusable terminal: this is the outcome we want");
        } else {
            println!("FAILURES at {unusable:?}");
            std::process::exit(1);
        }
    }

    pub fn run() {
        let arguments: Vec<String> = std::env::args().skip(1).collect();
        let Some(first) = arguments.first() else {
            eprintln!("usage: snapshot_probe --make <file> [cols] [rows]");
            eprintln!("       snapshot_probe <file> [cols] [rows] [--flip <offset>=<byte>]");
            std::process::exit(2);
        };

        if first == "--make" {
            let path = arguments.get(1).expect("--make needs a file");
            let cols = arguments.get(2).map_or(80, |v| v.parse().expect("cols"));
            let rows = arguments.get(3).map_or(24, |v| v.parse().expect("rows"));
            make(path, TerminalSize::new(cols, rows).expect("terminal size"));
            return;
        }
        let path = first;

        let mut bytes = std::fs::read(path).expect("snapshot file");
        let mut cols = 80u16;
        let mut rows = 24u16;

        // Positional sizes are whatever is left once the flags and the values they
        // consume are removed, so order does not matter.
        let mut positional = Vec::new();
        let mut rest = arguments[1..].iter();
        while let Some(argument) = rest.next() {
            match argument.as_str() {
                "--flip" => {
                    let spec = rest.next().expect("--flip needs <offset>=<byte>");
                    let (offset, value) = spec.split_once('=').expect("<offset>=<byte>");
                    let offset: usize = offset.parse().expect("offset");
                    let value: u8 = value.parse().expect("byte");
                    let previous = bytes[offset];
                    bytes[offset] = value;
                    println!("flipped offset {offset}: {previous:#04x} -> {value:#04x}");
                }
                "--sweep" => {
                    // The optional stride is consumed with the flag itself.
                    if rest.clone().next().is_some_and(|v| !v.starts_with('-')) {
                        let _ = rest.next();
                    }
                }
                other => positional.push(other),
            }
        }
        if let Some(value) = positional.first() {
            cols = value.parse().expect("cols");
        }
        if let Some(value) = positional.get(1) {
            rows = value.parse().expect("rows");
        }

        if let Some(index) = arguments.iter().position(|a| a == "--sweep") {
            let stride = arguments
                .get(index + 1)
                .map_or(1, |v| v.parse().expect("stride"));
            let size = TerminalSize::new(cols, rows).expect("terminal size");
            println!(
                "file {path}: {} bytes, sweeping at {cols}x{rows}",
                bytes.len()
            );
            sweep(&bytes, size, stride);
            return;
        }

        println!(
            "file {path}: {} bytes, restoring at {cols}x{rows}",
            bytes.len()
        );

        let size = TerminalSize::new(cols, rows).expect("terminal size");
        let checkpoint = TerminalCheckpoint {
            descriptor: CheckpointDescriptor {
                compatibility: CompatibilityId::new(GhosttyTerminalFactory.compatibility())
                    .unwrap(),
                processed: ReplayCursor {
                    lifetime: SessionLifetime::new(1, 1),
                    offset: 0,
                },
                control_generation: ControlGeneration::from_raw(0),
            },
            bytes,
        };

        // Restoration completes history in bounded steps, then the terminal has to
        // be usable: a snapshot that decodes but cannot project or accept input
        // would be a worse outcome than a clean refusal. Each stage is reported
        // separately: which one refuses says whether the
        // bytes were rejected on admission, during history, or only once the
        // terminal was asked to behave like one.
        let mut terminal = match GhosttyTerminalFactory.restore(checkpoint, config(size)) {
            Ok(terminal) => terminal,
            Err(TerminalError::InvalidConfiguration) => {
                println!("size mismatch: this snapshot was not taken at {cols}x{rows}");
                println!("  pass the size it was taken at, or make one with --make");
                std::process::exit(2);
            }
            Err(error) => {
                println!("REFUSED at restore: {error:?}");
                println!("  correct outcome for damaged or hostile bytes");
                return;
            }
        };

        let mut steps = 0u32;
        loop {
            match terminal.restore_history_step() {
                Ok(progress) => {
                    if progress.is_finished() {
                        break;
                    }
                    steps += 1;
                    assert!(steps < 100_000, "history restoration did not terminate");
                }
                Err(error) => {
                    println!("REFUSED during history after {steps} steps: {error:?}");
                    println!("  correct outcome for damaged or hostile bytes");
                    return;
                }
            }
        }

        let (cells, cursor) = match terminal.view() {
            Ok(view) => (view.cells.len(), view.cursor),
            Err(error) => {
                println!("DECODED but unusable: view failed: {error:?}");
                println!("  a terminal that finished restoring must project");
                std::process::exit(1);
            }
        };

        if let Err(error) = terminal
            .feed(b"\x1b[0mprobe")
            .and_then(|_effects| terminal.view())
        {
            println!("DECODED but unusable: input failed: {error:?}");
            println!("  a terminal that finished restoring must accept input");
            std::process::exit(1);
        }

        println!("ACCEPTED: usable after {steps} history steps");
        println!("  cells {cells}, cursor {cursor:?}");
    }
}

fn main() {
    #[cfg(feature = "ghostty")]
    probe::run();
    #[cfg(not(feature = "ghostty"))]
    eprintln!("snapshot_probe requires --features ghostty");
}
