//! Backend-neutral domain model for Cyber-Pumpkin.

use std::fmt;

/// Stable identifier for a configured backend instance.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BackendId(String);

impl BackendId {
    /// Creates an identifier after validating that it is not blank.
    pub fn new(value: impl Into<String>) -> Result<Self, CoreError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(CoreError::InvalidBackendId);
        }
        Ok(Self(value))
    }

    /// Returns the identifier as text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Path interpreted by a backend rather than the host operating system.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BackendPath(String);

impl BackendPath {
    /// Creates a backend path.
    pub fn new(value: impl Into<String>) -> Result<Self, CoreError> {
        let value = value.into();
        if value.is_empty() {
            return Err(CoreError::EmptyPath);
        }
        Ok(Self(value))
    }

    /// Returns the path string exactly as stored.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// High-level kind of a file entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EntryKind {
    /// Regular file.
    File,
    /// Directory/container.
    Directory,
    /// Symbolic link or backend equivalent.
    Symlink,
    /// Entry kind not representable by the common model.
    Other,
}

/// Backend-neutral directory entry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileEntry {
    /// Full backend path.
    pub path: BackendPath,
    /// Display name.
    pub name: String,
    /// File kind.
    pub kind: EntryKind,
    /// Logical byte size when known.
    pub size: Option<u64>,
}

/// Capabilities used to adapt behavior without backend-specific UI branching.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BackendCapabilities {
    /// Backend can seek/restart writes for resume.
    pub resumable_write: bool,
    /// Backend can atomically rename an object into place.
    pub atomic_rename: bool,
    /// Backend exposes POSIX-like permissions.
    pub unix_permissions: bool,
    /// Backend can provide a server-side checksum.
    pub server_checksum: bool,
}

/// Common core failures that are not protocol-specific.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CoreError {
    /// Backend identifier contained no meaningful text.
    InvalidBackendId,
    /// Empty paths are rejected at the common boundary.
    EmptyPath,
}

impl fmt::Display for CoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidBackendId => f.write_str("backend id must not be blank"),
            Self::EmptyPath => f.write_str("backend path must not be empty"),
        }
    }
}

impl std::error::Error for CoreError {}

#[cfg(test)]
mod tests {
    use super::{BackendId, BackendPath};

    #[test]
    fn backend_id_rejects_blank_text() {
        assert!(BackendId::new("   ").is_err());
    }

    #[test]
    fn backend_path_preserves_backend_syntax() -> Result<(), Box<dyn std::error::Error>> {
        let path = BackendPath::new("/var/www")?;
        assert_eq!(path.as_str(), "/var/www");
        Ok(())
    }
}
