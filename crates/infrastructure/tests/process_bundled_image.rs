//! Bundled helper validation uses exact regular-file bytes and cannot wait for FIFO peers.
#[path = "fixtures/process_events.rs"]
mod support;
use pty_runtime_application::process::IProcessBackend;
use pty_runtime_domain::{
    SessionLifetime,
    process::{ExitStatus, ProcessError, ProcessLimits},
};
use pty_runtime_infrastructure::process::UnixProcessBackend;
use std::{
    ffi::CString,
    fs::{self, DirBuilder, OpenOptions},
    os::unix::{
        ffi::OsStrExt,
        fs::{DirBuilderExt, symlink},
    },
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::Arc,
    time::{Duration, Instant},
};

const IMAGE: &[u8] = include_bytes!(env!("PTY_RUNTIME_GUARDIAN_IMAGE_PATH"));
const PROBE_PATH: &str = "PTY_BUNDLED_IMAGE_FIFO_PROBE";
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let mut random = [0; 16];
        getrandom::getrandom(&mut random).unwrap();
        let name: String = random.iter().map(|byte| format!("{byte:02x}")).collect();
        let path = std::env::temp_dir().join(format!("pty-bundled-image-test-{name}"));
        DirBuilder::new().mode(0o700).create(&path).unwrap();
        Self(path)
    }
    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn bundled(path: &Path) -> Result<UnixProcessBackend, ProcessError> {
    UnixProcessBackend::with_bundled_guardian(vec![std::env::temp_dir()], 1, path.to_owned())
}

#[test]
fn bundled_exact_image_is_staged_before_source_disappears() {
    let directory = Directory::new();
    let source = directory.path("exact");
    fs::write(&source, IMAGE).unwrap();
    let owner = bundled(&source).unwrap();
    fs::remove_file(source).unwrap();
    let events = Arc::new(support::Events::default());
    let _session = owner
        .spawn(
            &support::command("printf bundled-staged"),
            support::size(),
            SessionLifetime::new(811, 1),
            ProcessLimits::default(),
            events.clone(),
        )
        .unwrap();
    events.wait(|state| state.exit.is_some() && state.drain.is_some());
    let state = events.state.lock().unwrap();
    assert_eq!(state.bytes, b"bundled-staged");
    assert_eq!(state.exit, Some(ExitStatus::Code(0)));
    drop(state);
    owner.shutdown();
}

#[test]
fn bundled_image_rejects_altered_and_oversized_bytes() {
    let directory = Directory::new();
    let altered = directory.path("altered");
    let mut bytes = IMAGE.to_vec();
    bytes[0] ^= 1;
    fs::write(&altered, bytes).unwrap();
    assert!(matches!(bundled(&altered), Err(ProcessError::Unsupported)));
    let oversized = directory.path("oversized");
    fs::write(&oversized, IMAGE).unwrap();
    OpenOptions::new()
        .write(true)
        .open(&oversized)
        .unwrap()
        .set_len(IMAGE.len() as u64 + 1)
        .unwrap();
    assert!(matches!(
        bundled(&oversized),
        Err(ProcessError::Unsupported)
    ));
}

#[test]
fn bundled_image_rejects_symlink_and_directory() {
    let directory = Directory::new();
    let exact = directory.path("exact");
    fs::write(&exact, IMAGE).unwrap();
    let link = directory.path("link");
    symlink(&exact, &link).unwrap();
    assert!(bundled(&link).is_err());
    assert!(matches!(
        bundled(&directory.0),
        Err(ProcessError::InvalidCommand)
    ));
}

// Command::env applies only to this isolated subprocess; the parent environment
// is never mutated. A separate process bounds the current blocking-open defect.
#[test]
fn bundled_fifo_probe() {
    let Some(path) = std::env::var_os(PROBE_PATH).map(PathBuf::from) else {
        return;
    };
    fs::write(path.with_extension("entered"), b"entered constructor").unwrap();
    let result = bundled(&path);
    fs::write(
        path.with_extension("result"),
        format!("{:?}", result.as_ref().err()),
    )
    .unwrap();
    assert!(matches!(result, Err(ProcessError::InvalidCommand)));
}

struct Probe(Child);
impl Drop for Probe {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}
#[test]
fn bundled_fifo_is_rejected_without_waiting_for_a_writer() {
    let directory = Directory::new();
    let fifo = directory.path("image.fifo");
    let name = CString::new(fifo.as_os_str().as_bytes()).unwrap();
    // SAFETY: name is a live NUL-terminated path in our fresh private directory.
    assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
    let mut child = Probe(
        Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "bundled_fifo_probe", "--test-threads=1"])
            .env(PROBE_PATH, &fifo)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let startup = Instant::now() + Duration::from_secs(2);
    while !fifo.with_extension("entered").exists() && Instant::now() < startup {
        assert!(
            child.0.try_wait().unwrap().is_none(),
            "probe exited before constructor"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        fifo.with_extension("entered").exists(),
        "probe startup deadline exceeded"
    );
    let deadline = Instant::now() + Duration::from_secs(1);
    let status = loop {
        if let Some(status) = child.0.try_wait().unwrap() {
            break Some(status);
        }
        if Instant::now() >= deadline {
            break None;
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    // Drop kills and reaps a blocked probe before the assertion can unwind.
    drop(child);
    assert!(
        status.is_some(),
        "bundled FIFO validation waited for a writer instead of rejecting its file type"
    );
    assert!(
        status.unwrap().success(),
        "probe result: {:?}",
        fs::read_to_string(fifo.with_extension("result"))
    );
    assert_eq!(
        fs::read_to_string(fifo.with_extension("result")).unwrap(),
        "Some(InvalidCommand)"
    );
}
