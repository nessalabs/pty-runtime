//! Reclaim only unlocked adapter namespaces; no recursive/path-following deletion.
use super::{
    arena::{Arena, namespace_name},
    filesystem::{Directory, map},
};
use pty_runtime_domain::checkpoint::{
    CheckpointCleanupLimits, CheckpointCleanupReport, CheckpointError,
};

pub(super) fn reclaim(
    arena: &Arena,
    limits: CheckpointCleanupLimits,
) -> Result<CheckpointCleanupReport, CheckpointError> {
    let mut report = CheckpointCleanupReport::default();
    let mut entries = arena.0.entries()?;
    while report.examined_entries < limits.entries && report.examined_namespaces < limits.namespaces
    {
        let Some(name) = entries.next()? else {
            report.scan_complete = true;
            break;
        };
        report.examined_entries += 1;
        if !namespace_name(name.to_bytes()) {
            report.failure.get_or_insert(CheckpointError::Unavailable);
            continue;
        }
        report.examined_namespaces += 1;
        let directory = match arena.0.child(&name, false) {
            Ok(directory) => directory,
            Err(CheckpointError::NotFound) => continue,
            Err(error) => {
                failed(&mut report, error);
                continue;
            }
        };
        match directory.try_lock() {
            Ok(false) => {
                report.live_namespaces += 1;
                continue;
            }
            Err(error) => {
                failed(&mut report, error);
                continue;
            }
            Ok(true) => {}
        }
        match remove_objects(&directory, limits, &mut report) {
            Ok(true) => match directory.remove_namespace() {
                Ok(()) => report.reclaimed_namespaces += 1,
                // Normal Drop may unlink the namespace after we opened it but
                // before its final directory handle released the owner flock.
                Err(CheckpointError::NotFound) => {}
                Err(error) => {
                    report.abandoned_incomplete = true;
                    failed(&mut report, error);
                }
            },
            Ok(false) => report.abandoned_incomplete = true,
            Err(error) => {
                report.abandoned_incomplete = true;
                failed(&mut report, error);
            }
        }
    }
    Ok(report)
}
fn remove_objects(
    directory: &Directory,
    limits: CheckpointCleanupLimits,
    report: &mut CheckpointCleanupReport,
) -> Result<bool, CheckpointError> {
    let mut entries = directory.entries()?;
    while report.examined_entries < limits.entries {
        let Some(name) = entries.next()? else {
            return Ok(true);
        };
        report.examined_entries += 1;
        let name = name.to_str().map_err(|_| CheckpointError::Unavailable)?;
        if !object_name(name) {
            return Err(CheckpointError::Unavailable);
        }
        // O_NOFOLLOW/O_NONBLOCK plus metadata reject symlinks, directories,
        // devices, foreign ownership, unexpected permissions and hard links.
        let file = directory.open(name, false)?;
        let bytes = file.metadata().map_err(map)?.len();
        if bytes > limits.bytes.saturating_sub(report.reclaimed_bytes) {
            return Ok(false);
        }
        directory.remove(name)?;
        report.reclaimed_objects += 1;
        report.reclaimed_bytes += bytes;
    }
    Ok(false)
}
fn object_name(name: &str) -> bool {
    let stem = name
        .strip_suffix(".checkpoint")
        .or_else(|| name.strip_suffix(".pending"));
    stem.is_some_and(|stem| {
        stem.len() == 16
            && stem
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            && u64::from_str_radix(stem, 16).is_ok_and(|value| value != 0)
    })
}
fn failed(report: &mut CheckpointCleanupReport, error: CheckpointError) {
    report.failed_namespaces += 1;
    report.failure.get_or_insert(error);
}
