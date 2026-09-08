use pty_runtime_domain::process::ProcessError;
use std::{fs::File, process::Child};
/// Verify without changing host signal disposition. The host must keep this
/// policy stable and must never reap children owned by this backend.
pub(super) fn validate_host() -> Result<(), ProcessError> {
    // SAFETY: sigaction's output is a valid initialized writable object;
    // a null new-action pointer queries without changing process-global state.
    let mut action: libc::sigaction = unsafe { std::mem::zeroed() };
    // SAFETY: SIGCHLD is valid and the output remains live through this call.
    if unsafe { libc::sigaction(libc::SIGCHLD, std::ptr::null(), &mut action) } < 0 {
        return Err(super::error(std::io::Error::last_os_error()));
    }
    if action.sa_sigaction == libc::SIG_IGN || action.sa_flags & libc::SA_NOCLDWAIT != 0 {
        return Err(ProcessError::Unsupported);
    }
    Ok(())
}
pub(super) fn signal(child: &Child, _host: &File, signal: i32) {
    let pid = child.id() as libc::pid_t;
    // SAFETY: under the documented host contract only this child's owner reaps
    // it; its unreaped PID anchors both its original process group and itself.
    // Do not use tcgetpgrp→killpg: foreground group IDs can be recycled between calls.
    unsafe {
        libc::kill(-pid, signal);
        libc::kill(pid, signal);
    }
}
