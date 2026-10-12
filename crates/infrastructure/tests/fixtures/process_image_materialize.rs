//! Filesystem outcomes and calling-thread signal state of the owned materializer.
use super::{write, write_abort_after_open};
use pty_runtime_domain::process::ProcessError;
use std::{
    ffi::OsString,
    fs,
    mem::MaybeUninit,
    os::unix::{ffi::OsStringExt, fs::PermissionsExt},
    path::PathBuf,
};

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
        let _ = fs::set_permissions(&self.0, fs::Permissions::from_mode(0o700));
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

#[test]
fn materializer_aborted_open_unlinks_partial_file_and_preserves_mask() {
    let directory = Directory::new();
    let path = directory.0.join("image");
    let _mask = SignalMask::install();
    let before = SignalMask::current();
    let result = write_abort_after_open(&path, b"partial");
    assert_eq!(SignalMask::current(), before);
    assert!(matches!(result, Err(ProcessError::Io)));
    assert!(
        !path.exists(),
        "failed materialize must not leave a partial inode"
    );
}

#[test]
fn materializer_open_eacces_preserves_existing_files_and_caller_mask() {
    use super::fault::{self, Point};
    let directory = Directory::new();
    let path = directory.0.join("image");
    let retained = directory.0.join("retained");
    fs::write(&retained, b"original").unwrap();
    let _mask = SignalMask::install();
    let before = SignalMask::current();
    // Inject the open result: directory permissions need not deny UID 0.
    // This checks materializer recovery from EACCES, not OS permission policy.
    let (result, consumed) = fault::write(&path, b"new", Point::Open, -1, libc::EACCES, 1);
    assert_eq!(
        consumed, b"F",
        "the open fault must be consumed exactly once"
    );
    assert_eq!(SignalMask::current(), before);
    assert_eq!(result, Err(ProcessError::PermissionDenied));
    assert!(!path.exists());
    assert_eq!(fs::read(&retained).unwrap(), b"original");
    write(&path, b"retry after open failure").unwrap();
    assert_eq!(SignalMask::current(), before);
    assert_eq!(fs::read(&path).unwrap(), b"retry after open failure");
}

#[test]
fn materializer_embedded_nul_is_rejected_without_truncating_the_path_or_changing_mask() {
    let directory = Directory::new();
    let prefix = directory.0.join("image");
    fs::write(&prefix, b"original").unwrap();
    let mut invalid = prefix.clone().into_os_string().into_vec();
    invalid.extend_from_slice(b"\0suffix");
    let path = PathBuf::from(OsString::from_vec(invalid));
    let _mask = SignalMask::install();
    let before = SignalMask::current();
    assert_eq!(
        write(&path, b"replacement"),
        Err(ProcessError::InvalidCommand)
    );
    assert_eq!(SignalMask::current(), before);
    assert_eq!(fs::read(&prefix).unwrap(), b"original");
    assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 1);
    let valid = directory.0.join("valid-image");
    write(&valid, b"valid").unwrap();
    assert_eq!(SignalMask::current(), before);
    assert_eq!(fs::read(valid).unwrap(), b"valid");
}

#[test]
fn interrupted_materializer_operations_retry_without_losing_bytes_or_signal_state() {
    use super::fault::{self, Point};
    for point in [Point::Open, Point::Write, Point::Wait, Point::RestoreMask] {
        let directory = Directory::new();
        let path = directory.0.join("image");
        let _mask = SignalMask::install();
        let before = SignalMask::current();
        let failure = if point == Point::RestoreMask {
            libc::EINTR
        } else {
            -1
        };
        let (result, consumed) =
            fault::write(&path, b"exact retry bytes", point, failure, libc::EINTR, 1);
        assert_eq!(
            consumed, b"F",
            "the selected operation must fail exactly once"
        );
        assert_eq!(result, Ok(()));
        assert_eq!(SignalMask::current(), before);
        assert_eq!(fs::read(&path).unwrap(), b"exact retry bytes");
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o500
        );
    }
}

#[test]
fn materializer_write_and_finalize_failures_remove_the_inode_and_allow_a_clean_retry() {
    use super::fault::{self, Point};
    for (point, result, errno, expected) in [
        (Point::Open, -1, libc::EIO, ProcessError::Io),
        (Point::Write, -1, libc::EIO, ProcessError::Io),
        (Point::Write, 0, 0, ProcessError::Io),
        (
            Point::Chmod,
            -1,
            libc::EPERM,
            ProcessError::PermissionDenied,
        ),
        (Point::Close, -1, libc::EIO, ProcessError::Io),
    ] {
        let directory = Directory::new();
        let path = directory.0.join("image");
        let _mask = SignalMask::install();
        let before = SignalMask::current();
        let (result, consumed) = fault::write(&path, b"must not survive", point, result, errno, 1);
        assert_eq!(consumed, b"F");
        assert_eq!(result, Err(expected));
        assert_eq!(SignalMask::current(), before);
        assert!(
            !path.exists(),
            "failure must remove an opened staging inode"
        );
        write(&path, b"clean retry").unwrap();
        assert_eq!(SignalMask::current(), before);
        assert_eq!(fs::read(&path).unwrap(), b"clean retry");
    }
}

#[test]
fn materializer_pre_fork_failures_never_create_an_inode_or_change_the_caller_mask() {
    use super::fault::{self, Point};
    for (point, result, errno, expected) in [
        (Point::SigFill, -1, libc::EIO, ProcessError::Io),
        (Point::InstallMask, libc::ENOMEM, 0, ProcessError::Capacity),
        (Point::Fork, -1, libc::ENOMEM, ProcessError::Capacity),
    ] {
        let directory = Directory::new();
        let path = directory.0.join("image");
        let _mask = SignalMask::install();
        let before = SignalMask::current();
        let (result, consumed) = fault::write(&path, b"not admitted", point, result, errno, 1);
        assert_eq!(consumed, b"F");
        assert_eq!(result, Err(expected));
        assert_eq!(SignalMask::current(), before);
        assert!(!path.exists());
        write(&path, b"retry").unwrap();
        assert_eq!(SignalMask::current(), before);
        assert_eq!(fs::read(&path).unwrap(), b"retry");
    }
}

#[test]
fn materializer_persistent_mask_restore_failure_is_reported_and_removes_the_successful_child_image()
{
    use super::fault::{self, Point};
    let directory = Directory::new();
    let path = directory.0.join("image");
    let original_mask = SignalMask::current();
    let mask = SignalMask::install();
    let (result, consumed) = fault::write(
        &path,
        b"completed child",
        Point::RestoreMask,
        libc::EINVAL,
        0,
        2,
    );
    assert_eq!(consumed, b"FF", "both bounded restore attempts must fail");
    assert_eq!(result, Err(ProcessError::Io));
    assert!(
        !path.exists(),
        "restore failure must not publish the child image"
    );
    // A failed pthread_sigmask does not restore this thread. Keep the test's
    // real restoration guard alive until the observed failure is asserted.
    assert_eq!(SignalMask::current()[(libc::SIGTERM - 1) as usize], 1);
    drop(mask);
    assert_eq!(SignalMask::current(), original_mask);
    write(&path, b"clean retry").unwrap();
    assert_eq!(fs::read(path).unwrap(), b"clean retry");
    assert_eq!(SignalMask::current(), original_mask);
}
