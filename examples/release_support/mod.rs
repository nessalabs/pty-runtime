pub mod cycles;
pub mod fixture;
mod metrics;
pub mod soak;
use pty_runtime::terminal::TerminalConfig;
use pty_runtime::*;
use std::{
    io::{self, Read, Write},
    os::unix::net::{UnixListener, UnixStream},
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
static NEXT: AtomicU64 = AtomicU64::new(1);
pub const DEADLINE: Duration = Duration::from_secs(15);

pub struct Child {
    pub pattern_seed: u64,
    pub session: Session,
    pub id: SessionId,
    pub control: UnixStream,
}
pub struct Harness {
    pub owner: Runtime,
    diagnostics: std::sync::Arc<RuntimeDiagnostics>,
    directory: PathBuf,
}
impl Harness {
    pub fn new() -> Result<Self> {
        let cwd = std::env::current_dir()?;
        let directory = std::env::temp_dir().join(format!(
            "pty-release-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&directory)?;
        let diagnostics = RuntimeDiagnostics::new();
        Ok(Self {
            owner: Runtime::new(vec![cwd], RuntimeOptions::default())?
                .with_diagnostics(diagnostics.clone()),
            diagnostics,
            directory,
        })
    }
    pub fn spawn(&self, projected: bool) -> Result<Child> {
        let sequence = NEXT.fetch_add(1, Ordering::Relaxed);
        let socket = self.directory.join(format!("s{sequence}"));
        let listener = UnixListener::bind(&socket)?;
        listener.set_nonblocking(true)?;
        let command = CommandSpec::new(
            std::env::current_exe()?,
            std::env::current_dir()?,
            vec![
                "--child".into(),
                socket.as_os_str().to_owned(),
                sequence.to_string().into(),
            ],
        )?;
        let size = TerminalSize::new(80, 24).unwrap();
        let mut options = if projected {
            SessionOptions::projected(TerminalConfig::new(size))
        } else {
            SessionOptions::raw(size)
        };
        options.replay_bytes = 1024;
        options.max_observers = 4;
        options.process.terminate_grace = Duration::from_millis(20);
        let id = SessionId::new(format!("release-{sequence}")).unwrap();
        let session = self.owner.spawn(id.clone(), &command, options)?;
        let deadline = Instant::now() + DEADLINE;
        let mut control = loop {
            match listener.accept() {
                Ok((socket, _)) => break socket,
                Err(error)
                    if error.kind() == io::ErrorKind::WouldBlock && Instant::now() < deadline =>
                {
                    std::thread::sleep(Duration::from_millis(1))
                }
                Err(error) => return Err(error.into()),
            }
        };
        control.set_nonblocking(false)?;
        control.set_read_timeout(Some(DEADLINE))?;
        control.set_write_timeout(Some(DEADLINE))?;
        let mut ready = [0];
        control.read_exact(&mut ready)?;
        assert_eq!(ready, [b'r']);
        std::fs::remove_file(socket)?;
        Ok(Child {
            pattern_seed: sequence,
            session,
            id,
            control,
        })
    }
    pub async fn finish(&self, child: Child, cancel: bool) -> Result<()> {
        self.finish_observed(child, cancel, None).await
    }
    async fn finish_observed(&self, child: Child, cancel: bool, turn: Option<u64>) -> Result<()> {
        let mut child = child;
        if cancel {
            soak::diagnose(
                child.session.cancel(),
                "transient_cancel",
                turn,
                Some(&child),
            )?;
        } else {
            soak::diagnose(
                child.control.write_all(b"x"),
                "transient_exit_request",
                turn,
                Some(&child),
            )?;
        }
        let wait = soak::diagnose(
            child.session.wait(),
            "transient_wait_admit",
            turn,
            Some(&child),
        )?;
        let completion = soak::diagnose(
            tokio::time::timeout(DEADLINE, wait).await,
            "transient_wait_timeout",
            turn,
            Some(&child),
        )?;
        let done = soak::diagnose(completion, "transient_wait_complete", turn, Some(&child))?;
        assert!(done.status.supervision_error.is_none(), "{done:?}");
        assert!(done.status.admission_error.is_none(), "{done:?}");
        assert!(done.status.exit.is_some(), "{done:?}");
        if !cancel {
            assert_eq!(done.status.exit, Some(ExitStatus::Code(0)));
        }
        assert_eq!(done.status.drain, Some(DrainOutcome::Eof));
        soak::diagnose(
            self.owner.forget(&child.id),
            "transient_forget",
            turn,
            Some(&child),
        )?;
        Ok(())
    }
}
impl Drop for Harness {
    fn drop(&mut self) {
        self.owner.shutdown();
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}
pub fn verify(observer: &mut Attachment, seed: u64) -> Result<(u64, u64)> {
    let mut bytes_seen = 0;
    let mut gaps = 0;
    while let Some(event) = observer.try_next()? {
        match event {
            OutputEvent::Replay(ReplayPage::Bytes { from, next, bytes }) => {
                assert_eq!(next.offset - from.offset, bytes.len() as u64);
                for (index, byte) in bytes.iter().enumerate() {
                    assert_eq!(*byte, pattern(seed, from.offset + index as u64));
                }
                bytes_seen += bytes.len() as u64;
            }
            OutputEvent::Replay(ReplayPage::Gap { from, to }) => {
                assert!(to.offset > from.offset);
                gaps += to.offset - from.offset;
            }
            OutputEvent::Complete(_) => break,
            _ => panic!("unexpected pending replay event"),
        }
    }
    Ok((bytes_seen, gaps))
}

pub fn checkpoint(phase: &str, completed: usize) -> Result<()> {
    if std::env::var_os("PTY_RELEASE_CENSUS").is_some() {
        println!("{{\"event\":\"checkpoint\",\"phase\":\"{phase}\",\"completed\":{completed}}}");
        let mut acknowledgement = String::new();
        std::io::stdin().read_line(&mut acknowledgement)?;
        assert_eq!(acknowledgement, "continue\n");
    }
    Ok(())
}

pub fn pattern(seed: u64, offset: u64) -> u8 {
    let index = offset % 32;
    if index < 16 {
        b"0123456789abcdef"[((seed >> (index * 4)) & 15) as usize]
    } else {
        let mut word = (offset / 32)
            .wrapping_add(seed)
            .wrapping_mul(0x9e37_79b9_7f4a_7c15);
        word ^= word >> 30;
        b'a' + ((word.wrapping_mul(index) >> 32) % 26) as u8
    }
}

impl Child {
    pub fn burst(&mut self) -> Result<()> {
        self.control.write_all(b"b")?;
        let mut acknowledgement = [0];
        self.control.read_exact(&mut acknowledgement)?;
        assert_eq!(acknowledgement, [b'a']);
        Ok(())
    }
}
