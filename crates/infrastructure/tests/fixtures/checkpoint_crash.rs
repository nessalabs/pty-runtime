//! Isolated crash fixture and filesystem observations for the public disk adapter.
use pty_runtime_application::checkpoint::ICheckpointStore;
use pty_runtime_domain::{
    ReplayCursor, SessionLifetime,
    checkpoint::*,
    terminal::{CheckpointDescriptor, CompatibilityId},
};
use pty_runtime_infrastructure::checkpoint::FileCheckpointStore;
use std::{
    os::unix::fs::DirBuilderExt,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

pub const CHILD_ROOT: &str = "PTY_CHECKPOINT_CRASH_CHILD_ROOT";
pub fn protected(generation: u64) -> ProtectedCheckpoint {
    let lifetime = SessionLifetime::new(910, 1);
    ProtectedCheckpoint::new(
        CheckpointKey {
            lifetime,
            generation,
        },
        CheckpointDescriptor {
            compatibility: CompatibilityId::new("cleanup-fixture").unwrap(),
            processed: ReplayCursor {
                lifetime,
                offset: 60,
            },
            control_generation: 0,
        },
        vec![generation as u8; 60],
    )
}
pub struct Fixture(pub PathBuf);
impl Fixture {
    pub fn new() -> Self {
        let mut random = [0; 16];
        getrandom::getrandom(&mut random).unwrap();
        let path = std::env::temp_dir().join(format!(
            "checkpoint-crash-review-{:032x}",
            u128::from_ne_bytes(random)
        ));
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(&path)
            .unwrap();
        Self(path)
    }
    pub fn empty_arena(&self) -> PathBuf {
        // SAFETY: geteuid only reads this fixture's effective owner identity.
        let path = self
            .0
            .join(format!(".pty-runtime-checkpoints-v1-{}", unsafe {
                libc::geteuid()
            }));
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(&path)
            .unwrap();
        path
    }
    pub fn arena(&self) -> PathBuf {
        std::fs::read_dir(&self.0)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .find(|path| path.is_dir())
            .unwrap()
    }
    pub fn namespace(&self) -> PathBuf {
        std::fs::read_dir(self.arena())
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
pub struct ChildOwner(Child);
impl ChildOwner {
    pub fn new(parent: &Path) -> Self {
        let child = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "checkpoint_crash_fixture", "--test-threads=1"])
            .env(CHILD_ROOT, parent)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let mut child = Self(child);
        let deadline = Instant::now() + Duration::from_secs(5);
        while !parent.join("ready").exists() {
            assert!(
                child.0.try_wait().unwrap().is_none(),
                "checkpoint child failed before admission"
            );
            assert!(
                Instant::now() < deadline,
                "checkpoint child startup deadline"
            );
            std::thread::sleep(Duration::from_millis(2));
        }
        child
    }
    pub fn crash(&mut self) {
        self.0.kill().unwrap();
        assert!(!self.0.wait().unwrap().success());
    }
}
impl Drop for ChildOwner {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
pub fn child() {
    let Some(parent) = std::env::var_os(CHILD_ROOT) else {
        return;
    };
    let parent = Path::new(&parent);
    let store = FileCheckpointStore::temporary(Some(parent), 4096).unwrap();
    store.commit(&protected(1)).unwrap();
    store.commit(&protected(2)).unwrap();
    std::fs::write(parent.join("ready"), b"committed").unwrap();
    // The parent delivers SIGKILL while the store and its directory flock live.
    loop {
        std::thread::sleep(Duration::from_secs(1));
    }
}

/// Hold inherited descriptors without exec; child executes only async-signal-safe calls.
pub struct ForkLockHolder(libc::pid_t);
impl ForkLockHolder {
    pub fn new() -> Self {
        // SAFETY: the copied child calls only pause/_exit, never Rust, allocation,
        // locks or destructors. Parent retains the unreaped child identity.
        let pid = unsafe { libc::fork() };
        assert!(pid >= 0, "{}", std::io::Error::last_os_error());
        if pid == 0 {
            loop {
                unsafe {
                    libc::pause();
                }
            }
        }
        Self(pid)
    }
}
impl Drop for ForkLockHolder {
    fn drop(&mut self) {
        // SAFETY: this fixture exclusively owns the unreaped child, which cannot
        // reuse its PID. Its kernel teardown closes every inherited descriptor.
        unsafe {
            libc::kill(self.0, libc::SIGKILL);
        }
        loop {
            let result = unsafe { libc::waitpid(self.0, std::ptr::null_mut(), 0) };
            if result == self.0
                || std::io::Error::last_os_error().kind() != std::io::ErrorKind::Interrupted
            {
                break;
            }
        }
    }
}
