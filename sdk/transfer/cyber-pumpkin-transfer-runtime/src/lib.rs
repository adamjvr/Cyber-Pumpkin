//! Product-level copy orchestration.
//!
//! This layer owns existing-item policy, Keep Both naming, and reliable tree
//! replacement. Native frontends own presentation and decision dialogs.

use cyber_pumpkin_backend::{Backend, BackendError, ErrorKind};
use cyber_pumpkin_core::BackendPath;
use cyber_pumpkin_reliability::{
    ReliabilityError, ReliableTreeOutcome, ReliableTreeReport, execute_tree_reliable,
};
use cyber_pumpkin_transfer::{
    CancellationToken, Endpoint, ExecutionError, RecursiveError, TransferId, TransferSpec,
    TreeTransferProgress, TreeTransferReport,
};
use std::fmt;
use std::time::Duration;

const MAX_RETRY_ATTEMPTS: u8 = 5;

/// Bounded retry policy for transient transfer failures.
///
/// `max_attempts` includes the initial attempt. A default policy therefore
/// performs at most three total attempts with 250 ms and 500 ms backoffs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetryPolicy {
    max_attempts: u8,
    initial_backoff_ms: u64,
    max_backoff_ms: u64,
}

impl RetryPolicy {
    /// Creates a bounded retry policy.
    ///
    /// # Errors
    ///
    /// Returns [`RetryPolicyError`] when the attempt count is outside 1..=5,
    /// the initial backoff is zero, or the maximum backoff is smaller than the
    /// initial backoff.
    pub const fn new(
        max_attempts: u8,
        initial_backoff_ms: u64,
        max_backoff_ms: u64,
    ) -> Result<Self, RetryPolicyError> {
        if max_attempts == 0 || max_attempts > MAX_RETRY_ATTEMPTS {
            return Err(RetryPolicyError::InvalidAttempts(max_attempts));
        }
        if initial_backoff_ms == 0 || max_backoff_ms < initial_backoff_ms {
            return Err(RetryPolicyError::InvalidBackoff {
                initial_ms: initial_backoff_ms,
                maximum_ms: max_backoff_ms,
            });
        }
        Ok(Self {
            max_attempts,
            initial_backoff_ms,
            max_backoff_ms,
        })
    }

    /// Maximum total attempts including the first attempt.
    #[must_use]
    pub const fn max_attempts(self) -> u8 {
        self.max_attempts
    }

    /// Returns whether another attempt is allowed after `failed_attempt`.
    #[must_use]
    pub const fn allows_retry_after(self, failed_attempt: u8) -> bool {
        failed_attempt < self.max_attempts
    }

    /// Computes capped exponential backoff after one failed attempt.
    #[must_use]
    pub fn backoff_after(self, failed_attempt: u8) -> Duration {
        let mut delay = self.initial_backoff_ms;
        for _ in 1..failed_attempt {
            delay = delay.saturating_mul(2).min(self.max_backoff_ms);
        }
        Duration::from_millis(delay.min(self.max_backoff_ms))
    }
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_backoff_ms: 250,
            max_backoff_ms: 2_000,
        }
    }
}

/// Retry-policy validation failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RetryPolicyError {
    /// Attempt count must remain within the product safety bound.
    InvalidAttempts(u8),
    /// Backoff values are inconsistent.
    InvalidBackoff {
        /// Initial delay in milliseconds.
        initial_ms: u64,
        /// Maximum delay in milliseconds.
        maximum_ms: u64,
    },
}

impl fmt::Display for RetryPolicyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidAttempts(attempts) => write!(
                formatter,
                "retry attempts must be between 1 and {MAX_RETRY_ATTEMPTS}, got {attempts}"
            ),
            Self::InvalidBackoff {
                initial_ms,
                maximum_ms,
            } => write!(
                formatter,
                "retry backoff requires non-zero initial delay <= maximum delay, got {initial_ms} ms / {maximum_ms} ms"
            ),
        }
    }
}

impl std::error::Error for RetryPolicyError {}

/// Existing-destination behavior for a copy operation.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum CopyConflictPolicy {
    /// Refuse to overwrite an existing destination.
    #[default]
    Fail,
    /// Replace the existing destination through the reliability layer.
    Replace,
    /// Preserve the existing destination and report a skipped operation.
    Skip,
    /// Generate a sibling destination name and copy there.
    KeepBoth,
}

/// Immutable copy request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CopyRequest {
    /// Stable transfer identifier.
    pub id: TransferId,
    /// Source endpoint.
    pub source: Endpoint,
    /// Requested destination endpoint.
    pub destination: Endpoint,
    /// Existing-item policy.
    pub conflict_policy: CopyConflictPolicy,
}

/// Successful copy report.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CopyRuntimeReport {
    /// Actual destination used.
    pub destination: BackendPath,
    /// Recursive transfer report.
    pub transfer: TreeTransferReport,
    /// Whether the requested destination was replaced.
    pub replaced_existing: bool,
}

/// Terminal runtime copy outcome.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CopyRuntimeOutcome {
    /// Copy completed.
    Completed(CopyRuntimeReport),
    /// Existing item was intentionally skipped.
    Skipped {
        /// Existing destination that was preserved.
        destination: BackendPath,
    },
    /// Cooperative cancellation occurred.
    Cancelled(TreeTransferReport),
}

/// Product-level copy error.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CopyRuntimeError {
    /// Destination exists and policy refused replacement.
    DestinationExists(BackendPath),
    /// Backend metadata or naming operation failed.
    Backend(BackendError),
    /// Safe replacement failed.
    Reliability(ReliabilityError),
    /// Keep Both could not find a free sibling name.
    KeepBothExhausted(BackendPath),
}

impl fmt::Display for CopyRuntimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DestinationExists(path) => {
                write!(formatter, "destination already exists: {}", path.as_str())
            }
            Self::Backend(error) => write!(formatter, "{error}"),
            Self::Reliability(error) => write!(formatter, "{error}"),
            Self::KeepBothExhausted(path) => write!(
                formatter,
                "could not generate a free Keep Both name beside {}",
                path.as_str()
            ),
        }
    }
}

impl std::error::Error for CopyRuntimeError {}

impl From<BackendError> for CopyRuntimeError {
    fn from(value: BackendError) -> Self {
        Self::Backend(value)
    }
}

impl From<ReliabilityError> for CopyRuntimeError {
    fn from(value: ReliabilityError) -> Self {
        Self::Reliability(value)
    }
}

/// Executes one file-or-tree copy with product conflict semantics.
///
/// # Errors
///
/// Returns [`CopyRuntimeError`] for preflight, naming, backend, or reliable
/// transfer failures.
pub fn execute_copy<F>(
    request: &CopyRequest,
    source: &dyn Backend,
    destination: &dyn Backend,
    cancellation: &CancellationToken,
    on_progress: F,
) -> Result<CopyRuntimeOutcome, CopyRuntimeError>
where
    F: FnMut(TreeTransferProgress),
{
    let exists = destination_exists(destination, &request.destination.path)?;
    if exists && request.conflict_policy == CopyConflictPolicy::Fail {
        return Err(CopyRuntimeError::DestinationExists(
            request.destination.path.clone(),
        ));
    }
    if exists && request.conflict_policy == CopyConflictPolicy::Skip {
        return Ok(CopyRuntimeOutcome::Skipped {
            destination: request.destination.path.clone(),
        });
    }

    let actual_destination = if exists && request.conflict_policy == CopyConflictPolicy::KeepBoth {
        keep_both_destination(destination, &request.destination.path)?
    } else {
        request.destination.path.clone()
    };

    let replace_existing = exists && request.conflict_policy == CopyConflictPolicy::Replace;
    let spec = TransferSpec {
        source: request.source.clone(),
        destination: Endpoint {
            backend: request.destination.backend.clone(),
            path: actual_destination.clone(),
        },
    };

    let outcome = execute_tree_reliable(
        request.id,
        &spec,
        source,
        destination,
        cancellation,
        replace_existing,
        on_progress,
    )?;

    match outcome {
        ReliableTreeOutcome::Completed(ReliableTreeReport {
            transfer,
            replaced_existing,
        }) => Ok(CopyRuntimeOutcome::Completed(CopyRuntimeReport {
            destination: actual_destination,
            transfer,
            replaced_existing,
        })),
        ReliableTreeOutcome::Cancelled(report) => Ok(CopyRuntimeOutcome::Cancelled(report)),
    }
}

fn destination_exists(backend: &dyn Backend, path: &BackendPath) -> Result<bool, CopyRuntimeError> {
    match backend.stat(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

fn keep_both_destination(
    backend: &dyn Backend,
    requested: &BackendPath,
) -> Result<BackendPath, CopyRuntimeError> {
    for sequence in 1_u16..=999 {
        let candidate = copy_name(requested, sequence)?;
        if !destination_exists(backend, &candidate)? {
            return Ok(candidate);
        }
    }
    Err(CopyRuntimeError::KeepBothExhausted(requested.clone()))
}

fn copy_name(path: &BackendPath, sequence: u16) -> Result<BackendPath, CopyRuntimeError> {
    let text = path.as_str();
    let (parent, name) = text.rsplit_once('/').unwrap_or(("", text));
    let (stem, extension) = split_extension(name);
    let suffix = if sequence == 1 {
        " copy".to_owned()
    } else {
        format!(" copy {sequence}")
    };
    let copied_name = match extension {
        Some(extension) => format!("{stem}{suffix}.{extension}"),
        None => format!("{stem}{suffix}"),
    };
    let joined = if parent.is_empty() {
        copied_name
    } else if parent == "/" {
        format!("/{copied_name}")
    } else {
        format!("{parent}/{copied_name}")
    };
    BackendPath::new(joined).map_err(|error| {
        BackendError::new(
            ErrorKind::InvalidInput,
            "construct Keep Both destination",
            Some(path.clone()),
            error.to_string(),
        )
        .into()
    })
}

fn split_extension(name: &str) -> (&str, Option<&str>) {
    if name.starts_with('.') {
        return (name, None);
    }
    match name.rsplit_once('.') {
        Some((stem, extension)) if !stem.is_empty() && !extension.is_empty() => {
            (stem, Some(extension))
        }
        _ => (name, None),
    }
}

/// Stable retry classification for backend connection/open failures.
#[must_use]
pub const fn retryable_error_kind(kind: ErrorKind) -> bool {
    matches!(
        kind,
        ErrorKind::Transport | ErrorKind::Protocol | ErrorKind::Io
    )
}

/// Returns whether a completed copy attempt failed in a state that is safe and
/// useful to retry after reconnecting both endpoints.
///
/// Recovery failures and cleanup failures are intentionally non-retryable. The
/// retry loop must never continue when the previous attempt may have left an
/// ambiguous destination state.
#[must_use]
pub fn retryable_copy_error(error: &CopyRuntimeError) -> bool {
    match error {
        CopyRuntimeError::Backend(error) => retryable_error_kind(error.kind()),
        CopyRuntimeError::Reliability(error) => retryable_reliability_error(error),
        CopyRuntimeError::DestinationExists(_) | CopyRuntimeError::KeepBothExhausted(_) => false,
    }
}

fn retryable_reliability_error(error: &ReliabilityError) -> bool {
    match error {
        ReliabilityError::Transfer(error) => retryable_execution_error(error),
        ReliabilityError::Recursive(error) => retryable_recursive_error(error),
        ReliabilityError::Backend(error) => retryable_error_kind(error.kind()),
        ReliabilityError::AtomicRenameUnsupported
        | ReliabilityError::SidecarCollision { .. }
        | ReliabilityError::RecoveryJournal { .. }
        | ReliabilityError::RecoveryFailed { .. }
        | ReliabilityError::BackupCleanupFailed { .. } => false,
    }
}

fn retryable_recursive_error(error: &RecursiveError) -> bool {
    match error {
        RecursiveError::Execution(error) => retryable_execution_error(error),
        RecursiveError::Backend(error) => retryable_error_kind(error.kind()),
        RecursiveError::DestinationExists(_)
        | RecursiveError::UnsupportedEntry { .. }
        | RecursiveError::InvalidChildPath(_)
        | RecursiveError::Cleanup(_) => false,
    }
}

fn retryable_execution_error(error: &ExecutionError) -> bool {
    match error {
        ExecutionError::Backend(error) => retryable_error_kind(error.kind()),
        ExecutionError::Stream(_) => true,
        ExecutionError::Lifecycle(_)
        | ExecutionError::Cleanup { .. }
        | ExecutionError::BackendMismatch { .. }
        | ExecutionError::SourceNotFile { .. }
        | ExecutionError::DestinationExists { .. }
        | ExecutionError::SizeMismatch { .. } => false,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CopyConflictPolicy, CopyRequest, CopyRuntimeError, CopyRuntimeOutcome, RetryPolicy,
        execute_copy, retryable_copy_error,
    };
    use cyber_pumpkin_backend::{BackendError, ErrorKind};
    use cyber_pumpkin_core::{BackendId, BackendPath};
    use cyber_pumpkin_local::LocalBackend;
    use cyber_pumpkin_reliability::ReliabilityError;
    use cyber_pumpkin_transfer::{CancellationToken, Endpoint, TransferId};
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(1);

    fn root() -> PathBuf {
        let serial = NEXT.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "cyber-pumpkin-transfer-runtime-{}-{serial}",
            std::process::id()
        ))
    }

    fn path(path: &Path) -> Result<BackendPath, Box<dyn std::error::Error>> {
        Ok(BackendPath::new(path.to_string_lossy().into_owned())?)
    }

    fn request(
        backend: &BackendId,
        source: &Path,
        destination: &Path,
        policy: CopyConflictPolicy,
    ) -> Result<CopyRequest, Box<dyn std::error::Error>> {
        Ok(CopyRequest {
            id: TransferId::new(1)?,
            source: Endpoint {
                backend: backend.clone(),
                path: path(source)?,
            },
            destination: Endpoint {
                backend: backend.clone(),
                path: path(destination)?,
            },
            conflict_policy: policy,
        })
    }

    #[test]
    fn default_retry_policy_is_bounded_exponential() {
        let policy = RetryPolicy::default();
        assert_eq!(policy.max_attempts(), 3);
        assert!(policy.allows_retry_after(1));
        assert!(policy.allows_retry_after(2));
        assert!(!policy.allows_retry_after(3));
        assert_eq!(policy.backoff_after(1).as_millis(), 250);
        assert_eq!(policy.backoff_after(2).as_millis(), 500);
    }

    #[test]
    fn retry_policy_rejects_retry_storm_configuration() {
        assert!(RetryPolicy::new(0, 250, 2_000).is_err());
        assert!(RetryPolicy::new(6, 250, 2_000).is_err());
        assert!(RetryPolicy::new(3, 0, 2_000).is_err());
        assert!(RetryPolicy::new(3, 2_000, 250).is_err());
    }

    #[test]
    fn retry_classification_excludes_permission_and_conflict_errors()
    -> Result<(), Box<dyn std::error::Error>> {
        let transport = CopyRuntimeError::Backend(BackendError::new(
            ErrorKind::Transport,
            "test transport",
            None,
            "connection reset",
        ));
        let permission = CopyRuntimeError::Backend(BackendError::new(
            ErrorKind::PermissionDenied,
            "test permission",
            None,
            "denied",
        ));
        let conflict = CopyRuntimeError::DestinationExists(BackendPath::new("/tmp/existing")?);

        assert!(retryable_copy_error(&transport));
        assert!(!retryable_copy_error(&permission));
        assert!(!retryable_copy_error(&conflict));
        Ok(())
    }

    #[test]
    fn recovery_journal_errors_are_never_retryable() {
        let error = CopyRuntimeError::Reliability(ReliabilityError::RecoveryJournal {
            path: PathBuf::from("/tmp/cyber-pumpkin-recovery.json"),
            message: "journal unavailable".to_owned(),
        });

        assert!(!retryable_copy_error(&error));
    }

    #[test]
    fn replace_directory_tree_is_reliable() -> Result<(), Box<dyn std::error::Error>> {
        let root = root();
        let source = root.join("source");
        let destination = root.join("destination");
        fs::create_dir_all(source.join("new"))?;
        fs::create_dir_all(destination.join("old"))?;
        fs::write(source.join("new/value.txt"), b"new")?;
        fs::write(destination.join("old/value.txt"), b"old")?;

        let id = BackendId::new("local")?;
        let backend = LocalBackend::new(id.clone());
        let outcome = execute_copy(
            &request(&id, &source, &destination, CopyConflictPolicy::Replace)?,
            &backend,
            &backend,
            &CancellationToken::new(),
            |_| {},
        )?;

        assert!(matches!(
            outcome,
            CopyRuntimeOutcome::Completed(report) if report.replaced_existing
        ));
        assert_eq!(fs::read(destination.join("new/value.txt"))?, b"new");
        assert!(!destination.join("old").exists());
        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn keep_both_preserves_original_file() -> Result<(), Box<dyn std::error::Error>> {
        let root = root();
        fs::create_dir_all(&root)?;
        let source = root.join("source.txt");
        let destination = root.join("track.txt");
        fs::write(&source, b"new")?;
        fs::write(&destination, b"old")?;

        let id = BackendId::new("local")?;
        let backend = LocalBackend::new(id.clone());
        let outcome = execute_copy(
            &request(&id, &source, &destination, CopyConflictPolicy::KeepBoth)?,
            &backend,
            &backend,
            &CancellationToken::new(),
            |_| {},
        )?;

        assert_eq!(fs::read(&destination)?, b"old");
        assert_eq!(fs::read(root.join("track copy.txt"))?, b"new");
        assert!(matches!(outcome, CopyRuntimeOutcome::Completed(_)));
        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn skip_preserves_existing_destination() -> Result<(), Box<dyn std::error::Error>> {
        let root = root();
        fs::create_dir_all(&root)?;
        let source = root.join("source.txt");
        let destination = root.join("destination.txt");
        fs::write(&source, b"new")?;
        fs::write(&destination, b"old")?;

        let id = BackendId::new("local")?;
        let backend = LocalBackend::new(id.clone());
        let outcome = execute_copy(
            &request(&id, &source, &destination, CopyConflictPolicy::Skip)?,
            &backend,
            &backend,
            &CancellationToken::new(),
            |_| {},
        )?;

        assert!(matches!(outcome, CopyRuntimeOutcome::Skipped { .. }));
        assert_eq!(fs::read(destination)?, b"old");
        fs::remove_dir_all(root)?;
        Ok(())
    }
}
