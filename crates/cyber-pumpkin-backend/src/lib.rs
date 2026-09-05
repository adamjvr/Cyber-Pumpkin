//! Backend-neutral I/O contract for Cyber-Pumpkin.
//!
//! Platform shells and transfer orchestration depend on this contract rather
//! than protocol-specific implementations.

use cyber_pumpkin_core::{BackendCapabilities, BackendId, BackendPath, FileEntry};
use std::fmt;
use std::io::{Read, Write};

/// Read stream returned by a backend.
pub type ReadStream = Box<dyn Read + Send>;

/// Write stream returned by a backend.
pub type WriteStream = Box<dyn Write + Send>;

/// Stable category for backend failures.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ErrorKind {
    /// Caller supplied an invalid configuration or path.
    InvalidInput,
    /// Local or remote object was not found.
    NotFound,
    /// Operation was rejected by permissions or authentication.
    PermissionDenied,
    /// Network connection or transport failed.
    Transport,
    /// SSH/SFTP or another protocol-level operation failed.
    Protocol,
    /// Host identity validation failed.
    HostKey,
    /// Authentication failed.
    Authentication,
    /// Filesystem or stream I/O failed.
    Io,
}

/// Backend failure with operation and path context.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BackendError {
    kind: ErrorKind,
    operation: &'static str,
    path: Option<BackendPath>,
    message: String,
}

impl BackendError {
    /// Creates a backend error without exposing protocol-specific error types.
    #[must_use]
    pub fn new(
        kind: ErrorKind,
        operation: &'static str,
        path: Option<BackendPath>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            operation,
            path,
            message: message.into(),
        }
    }

    /// Returns the stable error category.
    #[must_use]
    pub const fn kind(&self) -> ErrorKind {
        self.kind
    }

    /// Returns the operation that failed.
    #[must_use]
    pub const fn operation(&self) -> &'static str {
        self.operation
    }

    /// Returns the affected path when one exists.
    #[must_use]
    pub const fn path(&self) -> Option<&BackendPath> {
        self.path.as_ref()
    }

    /// Returns the backend-provided diagnostic text.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for BackendError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(path) = &self.path {
            write!(
                f,
                "{} failed for {}: {}",
                self.operation,
                path.as_str(),
                self.message
            )
        } else {
            write!(f, "{} failed: {}", self.operation, self.message)
        }
    }
}

impl std::error::Error for BackendError {}

/// Common file-like operations required by the browser and transfer engine.
///
/// Implementations own protocol details. Consumers must branch on advertised
/// capabilities rather than concrete backend types.
pub trait Backend {
    /// Returns the configured backend identifier.
    fn id(&self) -> &BackendId;

    /// Returns optional capabilities implemented by this backend.
    fn capabilities(&self) -> BackendCapabilities;

    /// Lists one directory.
    ///
    /// # Errors
    ///
    /// Returns [`BackendError`] when the directory cannot be enumerated.
    fn list(&self, path: &BackendPath) -> Result<Vec<FileEntry>, BackendError>;

    /// Reads metadata for one path without following a symbolic link when the
    /// underlying backend can preserve that distinction.
    ///
    /// # Errors
    ///
    /// Returns [`BackendError`] when metadata cannot be read.
    fn stat(&self, path: &BackendPath) -> Result<FileEntry, BackendError>;

    /// Opens a file for sequential reading from byte zero.
    ///
    /// # Errors
    ///
    /// Returns [`BackendError`] when the file cannot be opened.
    fn open_read(&self, path: &BackendPath) -> Result<ReadStream, BackendError>;

    /// Opens a file for sequential writing, creating or truncating it.
    ///
    /// # Errors
    ///
    /// Returns [`BackendError`] when the file cannot be opened.
    fn open_write(&self, path: &BackendPath) -> Result<WriteStream, BackendError>;

    /// Creates one directory. The parent must already exist.
    ///
    /// # Errors
    ///
    /// Returns [`BackendError`] when the directory cannot be created.
    fn create_dir(&self, path: &BackendPath) -> Result<(), BackendError>;

    /// Renames or moves an entry within one backend.
    ///
    /// # Errors
    ///
    /// Returns [`BackendError`] when the operation fails.
    fn rename(&self, source: &BackendPath, destination: &BackendPath) -> Result<(), BackendError>;

    /// Removes one file, symbolic link, or empty directory.
    ///
    /// # Errors
    ///
    /// Returns [`BackendError`] when the entry cannot be removed.
    fn remove(&self, path: &BackendPath) -> Result<(), BackendError>;
}
