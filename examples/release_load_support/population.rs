use super::{
    DEADLINE, Result,
    config::Config,
    wire::{Frame, Reader},
};
use pty_runtime::{terminal::TerminalConfig, *};
use std::{
    os::unix::net::{UnixListener, UnixStream},
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};
pub struct Child {
    pub session: Session,
    pub id: SessionId,
    pub socket: UnixStream,
    pub reader: Reader,
    pub producer: usize,
    pub pid: u64,
    pub total: u64,
    pub replies: u64,
    pub phase_done: bool,
    pub summary: [u64; 7],
    pub window: [u64; 3],
    pub writes: [u64; 7],
}
pub struct Population {
    pub runtime: Runtime,
    pub children: Vec<Child>,
    directory: PathBuf,
}
impl Population {
    pub fn new(config: &Config, diagnostics: Arc<RuntimeDiagnostics>) -> Result<Self> {
        let directory = std::env::temp_dir().join(format!("pty-load-{}", std::process::id()));
        std::fs::create_dir(&directory)?;
        let options = RuntimeOptions {
            max_sessions: config.sessions + 1,
            replay_bytes: (config.sessions + 1) * 1024 * 1024,
            ..RuntimeOptions::default()
        };
        println!(
            "{{\"event\":\"runtime_options\",\"value\":{:?}}}",
            format!("{options:?}")
        );
        let runtime = Runtime::new(vec![std::env::current_dir()?], options)?
            .with_diagnostics(diagnostics.clone());
        let mut population = Self {
            runtime,
            children: Vec::with_capacity(config.sessions),
            directory,
        };
        super::report::checkpoint("runtime", Some(&diagnostics))?;
        for producer in 0..config.sessions {
            let child = population.spawn(config, producer, false)?;
            population.children.push(child);
        }
        Ok(population)
    }
    pub fn spawn(&self, config: &Config, producer: usize, transient: bool) -> Result<Child> {
        let path = self.directory.join(format!("s{producer}"));
        let listener = UnixListener::bind(&path)?;
        listener.set_nonblocking(true)?;
        let executable = std::env::current_exe()?;
        let mut arguments = vec![
            "--child".into(),
            path.as_os_str().to_owned(),
            producer.to_string().into(),
        ];
        let executable = if transient {
            let mut wrapped = vec![
                "-c".into(),
                "trap '' TERM; exec \"$@\"".into(),
                "load-cancel-probe".into(),
                executable.into_os_string(),
            ];
            wrapped.append(&mut arguments);
            arguments = wrapped;
            PathBuf::from("/bin/sh")
        } else {
            executable
        };
        let command = CommandSpec::new(executable, std::env::current_dir()?, arguments)?;
        let size = TerminalSize::new(config.cols, config.rows)
            .map_err(|_| std::io::Error::other("invalid grid"))?;
        let mut options = if config.raw || transient {
            SessionOptions::raw(size)
        } else {
            SessionOptions::projected(TerminalConfig::new(size))
        };
        options.replay_bytes = 1024 * 1024;
        options.max_observers = config.observers.max(1) + 4;
        options.process.terminate_grace = Duration::from_millis(if transient { 250 } else { 20 });
        if let Some(projection) = &mut options.projection {
            projection.park_after = Duration::from_secs(3600);
            projection.staging_slots = config.staging_slots;
        }
        let id = SessionId::new(format!("load-{producer}")).unwrap();
        let session = self.runtime.spawn(id.clone(), &command, options)?;
        let deadline = Instant::now() + DEADLINE;
        let mut socket = loop {
            match listener.accept() {
                Ok((socket, _)) => break socket,
                Err(error)
                    if error.kind() == std::io::ErrorKind::WouldBlock
                        && Instant::now() < deadline =>
                {
                    std::thread::sleep(Duration::from_millis(1))
                }
                Err(error) => return Err(error.into()),
            }
        };
        let ready = ready_frame(&mut socket)?;
        assert_eq!(ready.kind, b'r');
        std::fs::remove_file(path)?;
        Ok(Child {
            session,
            id,
            socket,
            reader: Reader::default(),
            producer,
            pid: ready.values[0],
            total: 0,
            replies: 0,
            phase_done: false,
            summary: [0; 7],
            window: [0; 3],
            writes: [0; 7],
        })
    }
    pub async fn finish(&self, mut child: Child, cancel: bool) -> Result<()> {
        if cancel {
            child.session.cancel()?;
        } else {
            Frame::new(b'x', &[]).write(&mut child.socket)?;
        }
        let result = tokio::time::timeout(DEADLINE, child.session.wait()?).await??;
        assert!(result.status.supervision_error.is_none(), "{result:?}");
        assert!(result.status.admission_error.is_none(), "{result:?}");
        assert!(result.status.exit.is_some(), "{result:?}");
        if !cancel {
            assert_eq!(result.status.exit, Some(ExitStatus::Code(0)));
        }
        assert_eq!(result.status.drain, Some(DrainOutcome::Eof));
        self.runtime.forget(&child.id)?;
        Ok(())
    }
}
impl Drop for Population {
    fn drop(&mut self) {
        self.runtime.shutdown();
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}

fn ready_frame(socket: &mut UnixStream) -> std::io::Result<Frame> {
    // Accepted sockets may inherit the listener's nonblocking mode. Readiness
    // is a bounded handshake; only later control polling is nonblocking.
    socket.set_nonblocking(false)?;
    socket.set_read_timeout(Some(DEADLINE))?;
    socket.set_write_timeout(Some(DEADLINE))?;
    let frame = Frame::read(socket)?;
    socket.set_nonblocking(true)?;
    Ok(frame)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn readiness_waits_for_delayed_frame_on_inherited_nonblocking_socket() {
        let (mut owner, mut child) = UnixStream::pair().unwrap();
        owner.set_nonblocking(true).unwrap();
        let sender = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(30));
            Frame::new(b'r', &[123]).write(&mut child).unwrap();
            child
        });
        let result = ready_frame(&mut owner);
        let _child = sender.join().unwrap();
        let frame = result.expect("readiness must wait despite inherited nonblocking mode");
        assert_eq!(frame.kind, b'r');
        assert_eq!(frame.values[0], 123);
        assert!(Reader::default().next(&mut owner).unwrap().is_none());
    }
}
