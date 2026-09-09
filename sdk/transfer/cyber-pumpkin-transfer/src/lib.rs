//! Transfer planning, lifecycle state, and first file-copy executor.

use cyber_pumpkin_backend::{Backend, BackendError, ErrorKind};
use cyber_pumpkin_core::{BackendId, BackendPath, EntryKind};
use std::fmt;
use std::io::{self, Write as _};

/// Stable identifier for a transfer job.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TransferId(u64);

impl TransferId {
    /// Creates a transfer id. Zero is reserved as invalid.
    ///
    /// # Errors
    ///
    /// Returns [`TransferError::InvalidId`] when `value` is zero.
    pub fn new(value: u64) -> Result<Self, TransferError> {
        if value == 0 {
            return Err(TransferError::InvalidId);
        }
        Ok(Self(value))
    }

    /// Returns the numeric id.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// One side of a copy operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Endpoint {
    /// Configured backend.
    pub backend: BackendId,
    /// Backend-local path.
    pub path: BackendPath,
}

/// Immutable transfer intent.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransferSpec {
    /// Source object.
    pub source: Endpoint,
    /// Destination object.
    pub destination: Endpoint,
}

/// Policy used when the destination path already exists.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum DestinationPolicy {
    /// Replace the destination using the backend's normal write semantics.
    #[default]
    Replace,
    /// Refuse to start when the destination is already present.
    FailIfExists,
}

/// Observable lifecycle state. UI and CLI render this state; they do not invent it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransferState {
    /// Accepted but not yet started.
    Queued,
    /// Establishing or validating backend connections.
    Connecting,
    /// Enumerating children/metadata before transfer.
    Enumerating,
    /// Bytes are moving.
    Transferring,
    /// Content or metadata verification is running.
    Verifying,
    /// Waiting before a bounded retry.
    RetryWaiting,
    /// Explicitly paused.
    Paused,
    /// Finished successfully.
    Completed,
    /// Terminal failure.
    Failed,
    /// Terminal user cancellation.
    Cancelled,
}

impl TransferState {
    /// Returns whether no further state transition is legal.
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }

    /// Validates a lifecycle transition.
    #[must_use]
    pub const fn can_transition_to(self, next: Self) -> bool {
        use TransferState::{
            Cancelled, Completed, Connecting, Enumerating, Failed, Paused, Queued, RetryWaiting,
            Transferring, Verifying,
        };
        match self {
            Queued => matches!(next, Connecting | Cancelled),
            Connecting => matches!(
                next,
                Enumerating | Transferring | RetryWaiting | Failed | Cancelled
            ),
            Enumerating => matches!(next, Transferring | RetryWaiting | Failed | Cancelled),
            Transferring => matches!(
                next,
                Verifying | RetryWaiting | Paused | Completed | Failed | Cancelled
            ),
            Verifying => matches!(next, Completed | RetryWaiting | Failed | Cancelled),
            RetryWaiting => matches!(next, Connecting | Cancelled | Failed),
            Paused => matches!(next, Connecting | Transferring | Cancelled | Failed),
            Completed | Failed | Cancelled => false,
        }
    }
}

/// Transfer state-machine errors.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TransferError {
    /// Transfer id zero is invalid.
    InvalidId,
    /// Attempted transition violates the documented state machine.
    InvalidTransition {
        /// State the job was in when the transition was requested.
        from: TransferState,
        /// Requested destination state.
        to: TransferState,
    },
}

impl fmt::Display for TransferError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidId => f.write_str("transfer id zero is reserved"),
            Self::InvalidTransition { from, to } => {
                write!(f, "invalid transfer transition: {from:?} -> {to:?}")
            }
        }
    }
}

impl std::error::Error for TransferError {}

/// State holder for one transfer job.
#[derive(Debug)]
pub struct TransferJob {
    id: TransferId,
    spec: TransferSpec,
    state: TransferState,
}

impl TransferJob {
    /// Creates a queued job.
    #[must_use]
    pub const fn new(id: TransferId, spec: TransferSpec) -> Self {
        Self {
            id,
            spec,
            state: TransferState::Queued,
        }
    }

    /// Job identifier.
    #[must_use]
    pub const fn id(&self) -> TransferId {
        self.id
    }

    /// Immutable transfer intent.
    #[must_use]
    pub const fn spec(&self) -> &TransferSpec {
        &self.spec
    }

    /// Current lifecycle state.
    #[must_use]
    pub const fn state(&self) -> TransferState {
        self.state
    }

    /// Applies a validated state transition.
    ///
    /// # Errors
    ///
    /// Returns [`TransferError::InvalidTransition`] when `next` is not a legal
    /// successor of the current state.
    pub fn transition(&mut self, next: TransferState) -> Result<(), TransferError> {
        if !self.state.can_transition_to(next) {
            return Err(TransferError::InvalidTransition {
                from: self.state,
                to: next,
            });
        }
        self.state = next;
        Ok(())
    }
}

/// Result of one completed file copy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TransferReport {
    bytes_copied: u64,
}

impl TransferReport {
    /// Returns the number of payload bytes copied.
    #[must_use]
    pub const fn bytes_copied(self) -> u64 {
        self.bytes_copied
    }
}

/// Failures produced by the first transfer executor.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExecutionError {
    /// A lifecycle transition failed.
    Lifecycle(TransferError),
    /// A backend operation failed.
    Backend(BackendError),
    /// The backend passed to the executor does not match the transfer spec.
    BackendMismatch {
        /// Transfer endpoint being validated.
        endpoint: &'static str,
        /// Backend identifier required by the transfer specification.
        expected: BackendId,
        /// Backend identifier supplied to the executor.
        actual: BackendId,
    },
    /// The source endpoint is not a regular file.
    SourceNotFile {
        /// Entry type reported by the source backend.
        kind: EntryKind,
    },
    /// The requested destination already exists and replacement was disabled.
    DestinationExists {
        /// Existing destination path.
        path: BackendPath,
    },
    /// Streaming bytes between opened backend handles failed.
    Stream(String),
    /// Destination size did not match the number of bytes copied.
    SizeMismatch {
        /// Bytes reported copied by the stream operation.
        copied: u64,
        /// Bytes reported by destination metadata.
        destination: u64,
    },
}

impl fmt::Display for ExecutionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Lifecycle(error) => write!(f, "transfer lifecycle failed: {error}"),
            Self::Backend(error) => write!(f, "backend operation failed: {error}"),
            Self::BackendMismatch {
                endpoint,
                expected,
                actual,
            } => write!(
                f,
                "{endpoint} backend mismatch: expected {}, got {}",
                expected.as_str(),
                actual.as_str()
            ),
            Self::SourceNotFile { kind } => {
                write!(f, "source endpoint is not a regular file: {kind:?}")
            }
            Self::DestinationExists { path } => {
                write!(f, "destination already exists: {}", path.as_str())
            }
            Self::Stream(message) => write!(f, "streaming copy failed: {message}"),
            Self::SizeMismatch {
                copied,
                destination,
            } => write!(
                f,
                "destination verification failed: copied {copied} bytes, destination reports {destination}"
            ),
        }
    }
}

impl std::error::Error for ExecutionError {}

impl From<TransferError> for ExecutionError {
    fn from(value: TransferError) -> Self {
        Self::Lifecycle(value)
    }
}

impl From<BackendError> for ExecutionError {
    fn from(value: BackendError) -> Self {
        Self::Backend(value)
    }
}

/// Executes one regular-file copy through two backend implementations.
///
/// The executor validates backend identity, streams bytes without loading the
/// full file into memory, flushes the destination, verifies destination size
/// when available, and drives the shared lifecycle state machine.
///
/// # Errors
///
/// Returns [`ExecutionError`] when backend validation, metadata, streaming,
/// verification, or lifecycle transitions fail. A failure after execution has
/// started moves the job to [`TransferState::Failed`].
pub fn execute_file(
    job: &mut TransferJob,
    source: &dyn Backend,
    destination: &dyn Backend,
) -> Result<TransferReport, ExecutionError> {
    execute_file_with_policy(job, source, destination, DestinationPolicy::Replace)
}

/// Executes one regular-file copy with an explicit destination policy.
///
/// `FailIfExists` performs a backend metadata preflight before opening the
/// destination for writing. This prevents known accidental replacement but is
/// not an atomic create-if-absent primitive; stronger guarantees belong in
/// backend capabilities added by a later reliability milestone.
///
/// # Errors
///
/// Returns [`ExecutionError`] for lifecycle, backend, streaming, verification,
/// or destination-policy failures.
pub fn execute_file_with_policy(
    job: &mut TransferJob,
    source: &dyn Backend,
    destination: &dyn Backend,
    policy: DestinationPolicy,
) -> Result<TransferReport, ExecutionError> {
    validate_backend("source", &job.spec.source.backend, source.id())?;
    validate_backend(
        "destination",
        &job.spec.destination.backend,
        destination.id(),
    )?;

    job.transition(TransferState::Connecting)?;
    let result = execute_started(job, source, destination, policy);

    if result.is_err() && job.state().can_transition_to(TransferState::Failed) {
        job.transition(TransferState::Failed)?;
    }

    result
}

fn validate_backend(
    endpoint: &'static str,
    expected: &BackendId,
    actual: &BackendId,
) -> Result<(), ExecutionError> {
    if expected == actual {
        Ok(())
    } else {
        Err(ExecutionError::BackendMismatch {
            endpoint,
            expected: expected.clone(),
            actual: actual.clone(),
        })
    }
}

fn enforce_destination_policy(
    job: &TransferJob,
    destination: &dyn Backend,
    policy: DestinationPolicy,
) -> Result<(), ExecutionError> {
    if policy == DestinationPolicy::Replace {
        return Ok(());
    }

    match destination.stat(&job.spec.destination.path) {
        Ok(_) => Err(ExecutionError::DestinationExists {
            path: job.spec.destination.path.clone(),
        }),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn execute_started(
    job: &mut TransferJob,
    source: &dyn Backend,
    destination: &dyn Backend,
    policy: DestinationPolicy,
) -> Result<TransferReport, ExecutionError> {
    let source_entry = source.stat(&job.spec.source.path)?;
    if source_entry.kind != EntryKind::File {
        return Err(ExecutionError::SourceNotFile {
            kind: source_entry.kind,
        });
    }

    enforce_destination_policy(job, destination, policy)?;

    let mut reader = source.open_read(&job.spec.source.path)?;
    let mut writer = destination.open_write(&job.spec.destination.path)?;

    job.transition(TransferState::Transferring)?;
    let bytes_copied = io::copy(&mut reader, &mut writer)
        .map_err(|error| ExecutionError::Stream(error.to_string()))?;
    writer
        .flush()
        .map_err(|error| ExecutionError::Stream(error.to_string()))?;
    drop(writer);
    drop(reader);

    job.transition(TransferState::Verifying)?;
    let destination_entry = destination.stat(&job.spec.destination.path)?;
    if let Some(destination_size) = destination_entry.size
        && destination_size != bytes_copied
    {
        return Err(ExecutionError::SizeMismatch {
            copied: bytes_copied,
            destination: destination_size,
        });
    }

    job.transition(TransferState::Completed)?;
    Ok(TransferReport { bytes_copied })
}

#[cfg(test)]
mod tests {
    use super::{
        DestinationPolicy, Endpoint, ExecutionError, TransferId, TransferJob, TransferSpec,
        TransferState, execute_file, execute_file_with_policy,
    };
    use cyber_pumpkin_core::{BackendId, BackendPath};
    use cyber_pumpkin_local::LocalBackend;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEST_DIRECTORY: AtomicU64 = AtomicU64::new(1);

    fn job() -> Result<TransferJob, Box<dyn std::error::Error>> {
        Ok(TransferJob::new(
            TransferId::new(1)?,
            TransferSpec {
                source: Endpoint {
                    backend: BackendId::new("local")?,
                    path: BackendPath::new("/tmp/input")?,
                },
                destination: Endpoint {
                    backend: BackendId::new("production")?,
                    path: BackendPath::new("/var/www/input")?,
                },
            },
        ))
    }

    fn test_directory() -> PathBuf {
        let serial = NEXT_TEST_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "cyber-pumpkin-transfer-{}-{serial}",
            std::process::id()
        ))
    }

    fn backend_path(path: &Path) -> Result<BackendPath, Box<dyn std::error::Error>> {
        let text = path.to_str().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, "test path must be UTF-8")
        })?;
        Ok(BackendPath::new(text)?)
    }

    #[test]
    fn terminal_states_never_transition() {
        for state in [
            TransferState::Completed,
            TransferState::Failed,
            TransferState::Cancelled,
        ] {
            assert!(!state.can_transition_to(TransferState::Queued));
        }
    }

    #[test]
    fn happy_path_is_explicit() -> Result<(), Box<dyn std::error::Error>> {
        let mut job = job()?;
        job.transition(TransferState::Connecting)?;
        job.transition(TransferState::Transferring)?;
        job.transition(TransferState::Verifying)?;
        job.transition(TransferState::Completed)?;
        assert!(job.state().is_terminal());
        Ok(())
    }

    #[test]
    fn illegal_transition_is_rejected() -> Result<(), Box<dyn std::error::Error>> {
        let mut job = job()?;
        assert!(job.transition(TransferState::Completed).is_err());
        Ok(())
    }

    #[test]
    fn fail_if_exists_preserves_existing_destination() -> Result<(), Box<dyn std::error::Error>> {
        let root = test_directory();
        fs::create_dir(&root)?;
        let source_path = root.join("source.bin");
        let destination_path = root.join("destination.bin");
        fs::write(&source_path, b"new-content")?;
        fs::write(&destination_path, b"keep-content")?;

        let backend_id = BackendId::new("local")?;
        let backend = LocalBackend::new(backend_id.clone());
        let spec = TransferSpec {
            source: Endpoint {
                backend: backend_id.clone(),
                path: backend_path(&source_path)?,
            },
            destination: Endpoint {
                backend: backend_id,
                path: backend_path(&destination_path)?,
            },
        };
        let mut transfer = TransferJob::new(TransferId::new(1)?, spec);
        let result = execute_file_with_policy(
            &mut transfer,
            &backend,
            &backend,
            DestinationPolicy::FailIfExists,
        );

        assert!(matches!(
            result,
            Err(ExecutionError::DestinationExists { .. })
        ));
        assert_eq!(transfer.state(), TransferState::Failed);
        assert_eq!(fs::read(&destination_path)?, b"keep-content");

        fs::remove_dir_all(&root)?;
        Ok(())
    }

    #[test]
    fn local_executor_copies_and_verifies_file() -> Result<(), Box<dyn std::error::Error>> {
        let root = test_directory();
        fs::create_dir(&root)?;
        let source_path = root.join("source.bin");
        let destination_path = root.join("destination.bin");
        fs::write(&source_path, b"cyber-pumpkin-phase-one")?;

        let backend_id = BackendId::new("local")?;
        let backend = LocalBackend::new(backend_id.clone());
        let spec = TransferSpec {
            source: Endpoint {
                backend: backend_id.clone(),
                path: backend_path(&source_path)?,
            },
            destination: Endpoint {
                backend: backend_id,
                path: backend_path(&destination_path)?,
            },
        };
        let mut transfer = TransferJob::new(TransferId::new(1)?, spec);
        let report = execute_file(&mut transfer, &backend, &backend)?;

        assert_eq!(transfer.state(), TransferState::Completed);
        assert_eq!(report.bytes_copied(), 23);
        assert_eq!(fs::read(&destination_path)?, b"cyber-pumpkin-phase-one");

        fs::remove_dir_all(&root)?;
        Ok(())
    }
}
