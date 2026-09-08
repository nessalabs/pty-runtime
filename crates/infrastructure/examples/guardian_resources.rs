//! External census driver for the actual default packaged Unix process adapter.
use pty_runtime_application::process::{IProcessBackend, IProcessEvents, OutputAcceptance};
use pty_runtime_domain::{
    SessionLifetime,
    process::{CommandSpec, DrainOutcome, ExitStatus, ProcessError, ProcessLimits},
    terminal::TerminalSize,
};
use pty_runtime_infrastructure::process::UnixProcessBackend;
use std::{
    io::{self, Write},
    sync::Arc,
};

struct Sink;
impl IProcessEvents for Sink {
    fn output(&self, _: &[u8]) -> OutputAcceptance {
        OutputAcceptance::Accepted
    }
    fn wait_for_capacity(&self, _: std::time::Instant) {}
    fn exited(&self, _: ExitStatus) {}
    fn drained(&self, _: DrainOutcome) {}
    fn supervision_failed(&self, _: ProcessError) {}
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let count: usize = std::env::args().nth(1).ok_or("missing count")?.parse()?;
    if ![1, 32, 128].contains(&count) {
        return Err("unsupported count".into());
    }
    println!("baseline {}", std::process::id());
    io::stdout().flush()?;
    let mut line = String::new();
    io::stdin().read_line(&mut line)?;
    let root = std::env::temp_dir();
    let owner = UnixProcessBackend::new(vec![root.clone()], count).map_err(|e| format!("{e:?}"))?;
    let command = CommandSpec::new("/bin/sleep".into(), root, vec!["300".into()])
        .map_err(|e| format!("{e:?}"))?;
    let mut sessions = Vec::with_capacity(count);
    for index in 0..count {
        sessions.push(
            owner
                .spawn(
                    &command,
                    TerminalSize::new(80, 24).map_err(|e| format!("{e:?}"))?,
                    SessionLifetime::new(702, index as u64),
                    ProcessLimits::default(),
                    Arc::new(Sink),
                )
                .map_err(|e| format!("{e:?}"))?,
        );
    }
    print!("ready");
    for session in &sessions {
        print!(" {}", session.process_id());
    }
    println!();
    io::stdout().flush()?;
    line.clear();
    io::stdin().read_line(&mut line)?;
    owner.shutdown_now();
    drop(owner);
    drop(sessions);
    println!("closed");
    io::stdout().flush()?;
    line.clear();
    io::stdin().read_line(&mut line)?;
    Ok(())
}
