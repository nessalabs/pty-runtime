//! Filesystem outcomes and calling-thread signal state of the owned materializer.
use super::write;
use pty_runtime_domain::process::ProcessError;
use std::{fs, mem::MaybeUninit, os::unix::fs::PermissionsExt, path::PathBuf};

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let mut random = [0; 16];
        getrandom::getrandom(&mut random).unwrap();
        let suffix: String = random.iter().map(|byte| format!("{byte:02x}")).collect();
        let path = std::env::temp_dir().join(format!("pty-image-test-{suffix}"));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct SignalMask(libc::sigset_t);
impl SignalMask {
    fn install() -> Self {
        let mut previous = MaybeUninit::uninit();
        let mut selected = MaybeUninit::uninit();
        // SAFETY: initialize both sets before reading; alter this test thread only.
        unsafe {
            assert_eq!(libc::sigemptyset(selected.as_mut_ptr()), 0);
            assert_eq!(libc::sigaddset(selected.as_mut_ptr(), libc::SIGUSR1), 0);
            assert_eq!(
                libc::pthread_sigmask(libc::SIG_BLOCK, selected.as_ptr(), previous.as_mut_ptr()),
                0
            );
            Self(previous.assume_init())
        }
    }
    fn current() -> Vec<i32> {
        let mut current = MaybeUninit::uninit();
        // SAFETY: a null input queries the initialized output without mutation.
        unsafe {
            assert_eq!(
                libc::pthread_sigmask(libc::SIG_SETMASK, std::ptr::null(), current.as_mut_ptr()),
                0
            );
            // Compare membership, not platform-dependent sigset_t padding.
            #[cfg(target_os = "linux")]
            let last_signal = libc::SIGRTMAX();
            #[cfg(target_os = "macos")]
            let last_signal = 31;
            (1..=last_signal)
                .map(|signal| libc::sigismember(current.as_ptr(), signal))
                .collect()
        }
    }
}
impl Drop for SignalMask {
    fn drop(&mut self) {
        // SAFETY: stored mask was initialized by a successful pthread_sigmask.
        unsafe {
            libc::pthread_sigmask(libc::SIG_SETMASK, &self.0, std::ptr::null_mut());
        }
    }
}

#[test]
fn materializer_preserves_bytes_permissions_and_caller_mask() {
    let directory = Directory::new();
    let path = directory.0.join("image");
    let _mask = SignalMask::install();
    let before = SignalMask::current();
    let bytes = b"arbitrary\0image\xffbytes\n";
    let result = write(&path, bytes);
    assert_eq!(SignalMask::current(), before);
    result.unwrap();
    assert_eq!(fs::read(&path).unwrap(), bytes);
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o500
    );
}

#[test]
fn materializer_missing_parent_preserves_mask_and_returns_not_found() {
    let directory = Directory::new();
    let path = directory.0.join("absent/image");
    let _mask = SignalMask::install();
    let before = SignalMask::current();
    let result = write(&path, b"new");
    assert_eq!(SignalMask::current(), before);
    assert!(matches!(result, Err(ProcessError::NotFound)));
    assert!(!path.exists());
}

#[test]
fn materializer_existing_destination_preserves_bytes_and_mask() {
    let directory = Directory::new();
    let path = directory.0.join("image");
    fs::write(&path, b"original").unwrap();
    let _mask = SignalMask::install();
    let before = SignalMask::current();
    let result = write(&path, b"replacement");
    assert_eq!(SignalMask::current(), before);
    assert!(matches!(result, Err(ProcessError::Io)));
    assert_eq!(fs::read(path).unwrap(), b"original");
}
