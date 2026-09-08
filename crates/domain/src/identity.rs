//! Session identities and lifetime-bound replay positions.

/// Caller-selected registry key. Reusing a key does not reuse its lifetime.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct SessionId(Box<str>);

impl SessionId {
    /// Validate a nonempty ID of at most 128 bytes without control characters.
    pub fn new(value: String) -> Result<Self, InvalidSessionId> {
        if value.is_empty() || value.len() > 128 || value.chars().any(char::is_control) {
            return Err(InvalidSessionId);
        }
        Ok(Self(value.into_boxed_str()))
    }
}

impl std::fmt::Debug for SessionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SessionId([redacted])")
    }
}

/// Session ID violates the length or character policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidSessionId;

/// A runtime-issued lifetime; registry names alone cannot identify byte streams.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SessionLifetime {
    owner: u64,
    sequence: u64,
}

impl SessionLifetime {
    /// Compose the owner identity and its monotonically issued session sequence.
    /// The application must never reuse either pair within surviving handles.
    pub fn new(owner: u64, sequence: u64) -> Self {
        Self { owner, sequence }
    }
}

/// An absolute byte position scoped to exactly one session lifetime.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReplayCursor {
    /// Lifetime of the output stream.
    pub lifetime: SessionLifetime,
    /// Absolute offset of the next requested byte.
    pub offset: u64,
}

#[cfg(test)]
#[path = "identity_tests.rs"]
mod tests;
