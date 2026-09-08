use super::ProcessError;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

/// Initial environment policy; explicit removals and overrides apply afterward.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum EnvironmentPolicy {
    /// Start with no inherited environment variables.
    #[default]
    Empty,
    /// Inherit the host environment at spawn time.
    Inherit,
}

/// Validated, compact command data. Filesystem policy is checked by the adapter.
/// Debug deliberately excludes executable, arguments, cwd, and environment.
#[derive(Clone)]
pub struct CommandSpec {
    executable: Box<Path>,
    cwd: Box<Path>,
    arguments: Box<[Box<OsStr>]>,
    environment: EnvironmentPolicy,
    overrides: Box<[(Box<OsStr>, Box<OsStr>)]>,
    removals: Box<[Box<OsStr>]>,
}

impl CommandSpec {
    /// Validate absolute paths and literal arguments with bounded metadata.
    /// Total explicit command data is limited to 64 KiB, each field to 8 KiB,
    /// and each collection to 128 entries. NUL bytes are rejected.
    pub fn new(
        executable: PathBuf,
        cwd: PathBuf,
        arguments: Vec<OsString>,
    ) -> Result<Self, ProcessError> {
        if !executable.is_absolute() || !cwd.is_absolute() || arguments.len() > 128 {
            return Err(ProcessError::InvalidCommand);
        }
        let mut total = 0;
        for value in std::iter::once(executable.as_os_str())
            .chain(std::iter::once(cwd.as_os_str()))
            .chain(arguments.iter().map(OsString::as_os_str))
        {
            validate_field(value, &mut total)?;
        }
        Ok(Self {
            executable: executable.into_boxed_path(),
            cwd: cwd.into_boxed_path(),
            arguments: arguments
                .into_iter()
                .map(OsString::into_boxed_os_str)
                .collect(),
            environment: EnvironmentPolicy::Empty,
            overrides: Box::new([]),
            removals: Box::new([]),
        })
    }

    /// Set explicit environment policy, removal names and final overrides.
    /// Existing variables are never included in ordinary diagnostics.
    pub fn with_environment(
        mut self,
        policy: EnvironmentPolicy,
        removals: Vec<OsString>,
        overrides: Vec<(OsString, OsString)>,
    ) -> Result<Self, ProcessError> {
        if removals.len() > 128 || overrides.len() > 128 {
            return Err(ProcessError::InvalidCommand);
        }
        let mut total = self.executable.as_os_str().len()
            + self.cwd.as_os_str().len()
            + self.arguments.iter().map(|v| v.len()).sum::<usize>();
        for name in &removals {
            validate_name(name, &mut total)?;
        }
        for (name, value) in &overrides {
            validate_name(name, &mut total)?;
            validate_field(value, &mut total)?;
        }
        self.environment = policy;
        self.removals = removals
            .into_iter()
            .map(OsString::into_boxed_os_str)
            .collect();
        self.overrides = overrides
            .into_iter()
            .map(|(k, v)| (k.into_boxed_os_str(), v.into_boxed_os_str()))
            .collect();
        Ok(self)
    }

    /// Absolute executable; the adapter launches it without a shell wrapper.
    pub fn executable(&self) -> &Path {
        &self.executable
    }
    /// Absolute working directory requiring canonical root validation.
    pub fn cwd(&self) -> &Path {
        &self.cwd
    }
    /// Literal arguments in order.
    pub fn arguments(&self) -> impl Iterator<Item = &OsStr> {
        self.arguments.iter().map(AsRef::as_ref)
    }
    /// Initial environment selection.
    pub fn environment(&self) -> EnvironmentPolicy {
        self.environment
    }
    /// Names removed before overrides apply.
    pub fn removals(&self) -> impl Iterator<Item = &OsStr> {
        self.removals.iter().map(AsRef::as_ref)
    }
    /// Overrides applied last.
    pub fn overrides(&self) -> impl Iterator<Item = (&OsStr, &OsStr)> {
        self.overrides.iter().map(|(k, v)| (k.as_ref(), v.as_ref()))
    }
}

impl std::fmt::Debug for CommandSpec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CommandSpec([redacted])")
    }
}

fn validate_field(value: &OsStr, total: &mut usize) -> Result<(), ProcessError> {
    let bytes = value.as_encoded_bytes();
    *total = total
        .checked_add(bytes.len())
        .ok_or(ProcessError::InvalidCommand)?;
    if bytes.len() > 8192 || *total > 65536 || bytes.contains(&0) {
        return Err(ProcessError::InvalidCommand);
    }
    Ok(())
}
fn validate_name(value: &OsStr, total: &mut usize) -> Result<(), ProcessError> {
    validate_field(value, total)?;
    if value.is_empty() || value.as_encoded_bytes().contains(&b'=') {
        return Err(ProcessError::InvalidCommand);
    }
    Ok(())
}
