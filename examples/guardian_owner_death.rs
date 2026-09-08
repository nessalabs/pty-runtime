//! External-driver fixture for abrupt death of a real public Runtime owner.
use pty_runtime::{CommandSpec, Runtime, RuntimeOptions, SessionId, SessionOptions, TerminalSize};
use std::io::{self, Read, Write};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    if arguments.len() != 4 {
        return Err(
            io::Error::other("expected python, workload script, socket, inherited fd").into(),
        );
    }
    let inherited_fd: i32 = arguments[3].parse()?;
    if inherited_fd < 300 {
        return Err(io::Error::other("probe descriptor must be at least 300").into());
    }
    let cwd = std::env::current_dir()?;
    let owner = Runtime::new(vec![cwd.clone()], RuntimeOptions::default())?;
    let command = CommandSpec::new(
        arguments[0].clone().into(),
        cwd,
        vec![
            arguments[1].clone().into(),
            "--workload".into(),
            arguments[2].clone().into(),
            arguments[3].clone().into(),
        ],
    )?;
    let id = SessionId::new("owner-death-proof".into())
        .map_err(|_| io::Error::other("invalid session id"))?;
    let size = TerminalSize::new(80, 24).map_err(|_| io::Error::other("invalid size"))?;
    let session = owner.spawn(id, &command, SessionOptions::raw(size))?;
    // SAFETY: the external driver transfers this single inherited descriptor to this
    // fixture; it is never wrapped, shared, or used by the fixture after close.
    if unsafe { libc::close(inherited_fd) } != 0 {
        return Err(io::Error::last_os_error().into());
    }
    println!("OWNER_READY {}", session.process_id()?);
    io::stdout().flush()?;
    let _ = io::stdin().read(&mut [0u8; 1])?;
    owner.shutdown();
    Ok(())
}
