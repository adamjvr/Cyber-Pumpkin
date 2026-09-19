//! Crash-resistant transfer finalization primitives.
//!
//! This layer owns the safe replacement transaction used by synchronization
//! and later ordinary copy/upload operations. Payload bytes are first written
//! to an operation-owned sibling path, verified by the transfer layer, then
//! finalized with backend renames. Existing destinations are temporarily moved
//! to an operation-owned backup and restored if finalization fails.

use cyber_pumpkin_backend::{Backend, BackendError, ErrorKind};
use cyber_pumpkin_core::{BackendPath, CapabilitySupport, EntryKind};
use cyber_pumpkin_recovery::{RecoveryEntry, RecoveryJournal, RecoveryPhase, recover_journal};
use cyber_pumpkin_transfer::{
    CancellationToken, ControlledTransferOutcome, Endpoint, ExecutionError, RecursiveError,
    TransferId, TransferJob, TransferProgress, TransferSpec, TreeTransferOutcome,
    TreeTransferProgress, TreeTransferReport, execute_file_controlled, execute_tree_controlled,
};
use std::fmt;
use std::path::{Path, PathBuf};

/// Report produced by a safely finalized file replacement.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReliableTransferReport {
    bytes_copied: u64,
    replaced_existing: bool,
}

impl ReliableTransferReport {
    /// Returns payload bytes copied into the staged object.
    #[must_use]
    pub const fn bytes_copied(self) -> u64 {
        self.bytes_copied
    }

    /// Returns whether an existing destination was replaced.
    #[must_use]
    pub const fn replaced_existing(self) -> bool {
        self.replaced_existing
    }
}

/// Terminal result of a reliable file transfer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReliableTransferOutcome {
    /// Staging, verification, and finalization completed.
    Completed(ReliableTransferReport),
    /// Cancellation occurred before the finalization transaction began.
    Cancelled {
        /// Payload bytes written before cancellation was observed.
        bytes_copied: u64,
    },
}

/// Failure produced by the reliability layer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReliabilityError {
    /// Destination backend cannot provide the atomic rename primitive required
    /// for safe finalization.
    AtomicRenameUnsupported,
    /// An operation-owned stage or backup path already exists.
    SidecarCollision {
        /// Existing path that prevented safe execution.
        path: BackendPath,
    },
    /// Durable recovery-journal I/O or replay failed.
    RecoveryJournal {
        /// Local journal path.
        path: PathBuf,
        /// Journal diagnostic.
        message: String,
    },
    /// Payload transfer or verification failed.
    Transfer(ExecutionError),
    /// Recursive file-or-tree transfer failed.
    Recursive(RecursiveError),
    /// A backend operation failed outside the payload transfer.
    Backend(BackendError),
    /// Finalization failed and restoring the original destination also failed.
    RecoveryFailed {
        /// Finalization failure text.
        finalize: String,
        /// Recovery failure text.
        recovery: String,
    },
    /// Finalization succeeded but removing the operation-owned backup failed.
    BackupCleanupFailed {
        /// Backup left behind for manual recovery.
        path: BackendPath,
        /// Backend diagnostic.
        message: String,
    },
}

impl fmt::Display for ReliabilityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AtomicRenameUnsupported => formatter
                .write_str("destination backend does not support atomic rename finalization"),
            Self::SidecarCollision { path } => write!(
                formatter,
                "safe-transfer sidecar already exists: {}",
                path.as_str()
            ),
            Self::RecoveryJournal { path, message } => write!(
                formatter,
                "recovery journal failed for {}: {message}",
                path.display()
            ),
            Self::Transfer(error) => write!(formatter, "staged transfer failed: {error}"),
            Self::Recursive(error) => write!(formatter, "reliable tree transfer failed: {error}"),
            Self::Backend(error) => write!(formatter, "safe finalization backend failed: {error}"),
            Self::RecoveryFailed { finalize, recovery } => write!(
                formatter,
                "safe finalization failed ({finalize}) and original restore failed ({recovery})"
            ),
            Self::BackupCleanupFailed { path, message } => write!(
                formatter,
                "replacement completed but backup cleanup failed for {}: {message}",
                path.as_str()
            ),
        }
    }
}

impl std::error::Error for ReliabilityError {}

impl From<ExecutionError> for ReliabilityError {
    fn from(value: ExecutionError) -> Self {
        Self::Transfer(value)
    }
}

impl From<RecursiveError> for ReliabilityError {
    fn from(value: RecursiveError) -> Self {
        Self::Recursive(value)
    }
}

impl From<BackendError> for ReliabilityError {
    fn from(value: BackendError) -> Self {
        Self::Backend(value)
    }
}

/// Request for a reliable regular-file transfer.
///
/// `recovery_journal` should be a path unique to the live transaction/session.
/// When present, durable phase records are written before filesystem mutations
/// that could otherwise leave an ambiguous replacement after process failure.
pub struct ReliableFileRequest<'a> {
    /// Stable transfer identifier.
    pub id: TransferId,
    /// Source endpoint.
    pub source_endpoint: Endpoint,
    /// Final destination endpoint.
    pub destination_endpoint: Endpoint,
    /// Source backend.
    pub source: &'a dyn Backend,
    /// Destination backend.
    pub destination: &'a dyn Backend,
    /// Cooperative cancellation token.
    pub cancellation: &'a CancellationToken,
    /// Optional durable recovery-journal path.
    pub recovery_journal: Option<&'a Path>,
}

/// Copies one regular file through an operation-owned stage and safely
/// finalizes it over the requested destination.
///
/// This compatibility entry point preserves the pre-journal API. Call
/// [`execute_file_reliable_request`] with `recovery_journal` set when the caller
/// has a durable local place to store transaction state.
///
/// # Errors
///
/// Returns [`ReliabilityError`] for capability, sidecar, transfer, backend,
/// finalization, recovery, or cleanup failures.
pub fn execute_file_reliable<F>(
    id: TransferId,
    source_endpoint: Endpoint,
    destination_endpoint: Endpoint,
    source: &dyn Backend,
    destination: &dyn Backend,
    cancellation: &CancellationToken,
    on_progress: F,
) -> Result<ReliableTransferOutcome, ReliabilityError>
where
    F: FnMut(TransferProgress),
{
    execute_file_reliable_request(
        ReliableFileRequest {
            id,
            source_endpoint,
            destination_endpoint,
            source,
            destination,
            cancellation,
            recovery_journal: None,
        },
        on_progress,
    )
}

/// Executes a regular-file replacement with optional durable recovery phases.
///
/// Before beginning a journaled transaction, an existing journal at the same
/// path is recovered against the supplied destination backend. Callers must not
/// share one journal path across concurrent transactions.
///
/// # Errors
///
/// Returns [`ReliabilityError`] for journal replay/persistence, capability,
/// transfer, backend, finalization, recovery, or cleanup failures.
pub fn execute_file_reliable_request<F>(
    request: ReliableFileRequest<'_>,
    mut on_progress: F,
) -> Result<ReliableTransferOutcome, ReliabilityError>
where
    F: FnMut(TransferProgress),
{
    let ReliableFileRequest {
        id,
        source_endpoint,
        destination_endpoint,
        source,
        destination,
        cancellation,
        recovery_journal,
    } = request;

    if let Some(journal_path) = recovery_journal {
        recover_pending_journal(destination, journal_path)?;
    }

    if destination.capabilities().atomic_rename != CapabilitySupport::Supported {
        return Err(ReliabilityError::AtomicRenameUnsupported);
    }

    let final_path = destination_endpoint.path.clone();
    let stage_path = sidecar_path(&final_path, "stage", id)?;
    let backup_path = sidecar_path(&final_path, "backup", id)?;
    ensure_absent(destination, &stage_path)?;
    ensure_absent(destination, &backup_path)?;

    let destination_exists = match destination.stat(&final_path) {
        Ok(_) => true,
        Err(error) if error.kind() == ErrorKind::NotFound => false,
        Err(error) => return Err(error.into()),
    };

    let mut recovery_entry = RecoveryEntry {
        operation_id: id.get(),
        destination: final_path.as_str().to_owned(),
        stage: Some(stage_path.as_str().to_owned()),
        backup: if destination_exists {
            Some(backup_path.as_str().to_owned())
        } else {
            None
        },
        phase: RecoveryPhase::Staging,
    };
    if let Some(journal_path) = recovery_journal {
        persist_recovery_entry(journal_path, &recovery_entry)?;
    }

    let stage_spec = TransferSpec {
        source: source_endpoint,
        destination: Endpoint {
            backend: destination_endpoint.backend,
            path: stage_path.clone(),
        },
    };
    let mut job = TransferJob::new(id, stage_spec);
    let staged = execute_file_controlled(
        &mut job,
        source,
        destination,
        cancellation,
        &mut on_progress,
    )?;

    let bytes_copied = match staged {
        ControlledTransferOutcome::Completed(report) => report.bytes_copied(),
        ControlledTransferOutcome::Cancelled { bytes_copied } => {
            remove_if_present(destination, &stage_path)?;
            if let Some(journal_path) = recovery_journal {
                clear_recovery_entry(journal_path, id.get())?;
            }
            return Ok(ReliableTransferOutcome::Cancelled { bytes_copied });
        }
    };

    if cancellation.is_cancelled() {
        remove_if_present(destination, &stage_path)?;
        if let Some(journal_path) = recovery_journal {
            clear_recovery_entry(journal_path, id.get())?;
        }
        return Ok(ReliableTransferOutcome::Cancelled { bytes_copied });
    }

    finalize_stage(
        destination,
        &stage_path,
        &final_path,
        &backup_path,
        destination_exists,
        recovery_journal,
        &mut recovery_entry,
    )?;

    Ok(ReliableTransferOutcome::Completed(ReliableTransferReport {
        bytes_copied,
        replaced_existing: destination_exists,
    }))
}

/// Report produced by a reliable file-or-directory-tree copy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReliableTreeReport {
    /// Aggregate transfer report.
    pub transfer: TreeTransferReport,
    /// Whether a pre-existing destination was replaced.
    pub replaced_existing: bool,
}

/// Terminal outcome of a reliable tree transfer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReliableTreeOutcome {
    /// Transfer and finalization completed.
    Completed(ReliableTreeReport),
    /// Cooperative cancellation was observed and any original destination was restored.
    Cancelled(TreeTransferReport),
}

/// Request for a reliable file-or-directory-tree transfer.
pub struct ReliableTreeRequest<'a> {
    /// Stable transfer identifier.
    pub id: TransferId,
    /// Source/destination transfer specification.
    pub spec: &'a TransferSpec,
    /// Source backend.
    pub source: &'a dyn Backend,
    /// Destination backend.
    pub destination: &'a dyn Backend,
    /// Cooperative cancellation token.
    pub cancellation: &'a CancellationToken,
    /// Whether an existing destination may be replaced.
    pub replace_existing: bool,
    /// Optional durable recovery-journal path unique to this transaction.
    pub recovery_journal: Option<&'a Path>,
}

/// Executes a file or directory-tree copy while preserving an existing
/// destination across failure and cancellation.
///
/// This compatibility entry point preserves the pre-journal API. Use
/// [`execute_tree_reliable_request`] to enable durable transaction phases.
///
/// # Errors
///
/// Returns [`ReliabilityError`] for backend, recursive-transfer, cleanup, or
/// recovery failures.
pub fn execute_tree_reliable<F>(
    id: TransferId,
    spec: &TransferSpec,
    source: &dyn Backend,
    destination: &dyn Backend,
    cancellation: &CancellationToken,
    replace_existing: bool,
    on_progress: F,
) -> Result<ReliableTreeOutcome, ReliabilityError>
where
    F: FnMut(TreeTransferProgress),
{
    let request = ReliableTreeRequest {
        id,
        spec,
        source,
        destination,
        cancellation,
        replace_existing,
        recovery_journal: None,
    };
    execute_tree_reliable_request(&request, on_progress)
}

/// Executes a tree transfer with optional durable recovery phases.
///
/// For a replacement, `BackupMoved` is persisted before the original object is
/// renamed away. `Finalized` is persisted only after the new tree completes and
/// before the backup is removed. Recovery therefore rolls back any transaction
/// that did not durably reach `Finalized`.
///
/// # Errors
///
/// Returns [`ReliabilityError`] for journal replay/persistence, backend,
/// recursive-transfer, cleanup, or recovery failures.
pub fn execute_tree_reliable_request<F>(
    request: &ReliableTreeRequest<'_>,
    on_progress: F,
) -> Result<ReliableTreeOutcome, ReliabilityError>
where
    F: FnMut(TreeTransferProgress),
{
    if let Some(journal_path) = request.recovery_journal {
        recover_pending_journal(request.destination, journal_path)?;
    }

    let final_path = &request.spec.destination.path;
    let destination_exists = match request.destination.stat(final_path) {
        Ok(_) => true,
        Err(error) if error.kind() == ErrorKind::NotFound => false,
        Err(error) => return Err(error.into()),
    };

    if destination_exists && !request.replace_existing {
        return Err(RecursiveError::DestinationExists(final_path.clone()).into());
    }

    if !destination_exists {
        return execute_new_tree_reliable(request, on_progress);
    }

    execute_replacing_tree_reliable(request, on_progress)
}

fn execute_new_tree_reliable<F>(
    request: &ReliableTreeRequest<'_>,
    on_progress: F,
) -> Result<ReliableTreeOutcome, ReliabilityError>
where
    F: FnMut(TreeTransferProgress),
{
    let Some(journal_path) = request.recovery_journal else {
        return execute_tree_controlled(
            request.spec,
            request.source,
            request.destination,
            request.cancellation,
            on_progress,
        )
        .map(|outcome| match outcome {
            TreeTransferOutcome::Completed(report) => {
                ReliableTreeOutcome::Completed(ReliableTreeReport {
                    transfer: report,
                    replaced_existing: false,
                })
            }
            TreeTransferOutcome::Cancelled(report) => ReliableTreeOutcome::Cancelled(report),
        })
        .map_err(ReliabilityError::from);
    };

    let final_path = &request.spec.destination.path;
    let mut recovery_entry = RecoveryEntry {
        operation_id: request.id.get(),
        destination: final_path.as_str().to_owned(),
        stage: Some(final_path.as_str().to_owned()),
        backup: None,
        phase: RecoveryPhase::Staging,
    };
    persist_recovery_entry(journal_path, &recovery_entry)?;

    match execute_tree_controlled(
        request.spec,
        request.source,
        request.destination,
        request.cancellation,
        on_progress,
    ) {
        Ok(TreeTransferOutcome::Completed(report)) => {
            recovery_entry.phase = RecoveryPhase::Finalized;
            persist_recovery_entry(journal_path, &recovery_entry)?;
            clear_recovery_entry(journal_path, request.id.get())?;
            Ok(ReliableTreeOutcome::Completed(ReliableTreeReport {
                transfer: report,
                replaced_existing: false,
            }))
        }
        Ok(TreeTransferOutcome::Cancelled(report)) => {
            remove_tree_if_present(request.destination, final_path)?;
            clear_recovery_entry(journal_path, request.id.get())?;
            Ok(ReliableTreeOutcome::Cancelled(report))
        }
        Err(error) => recover_failed_new_tree(request, journal_path, error),
    }
}

fn recover_failed_new_tree(
    request: &ReliableTreeRequest<'_>,
    journal_path: &Path,
    error: RecursiveError,
) -> Result<ReliableTreeOutcome, ReliabilityError> {
    let final_path = &request.spec.destination.path;
    let finalize = error.to_string();
    match remove_tree_if_present(request.destination, final_path) {
        Ok(()) => {
            clear_recovery_entry(journal_path, request.id.get())?;
            Err(ReliabilityError::Recursive(error))
        }
        Err(recovery) => Err(ReliabilityError::RecoveryFailed {
            finalize,
            recovery: recovery.to_string(),
        }),
    }
}

fn execute_replacing_tree_reliable<F>(
    request: &ReliableTreeRequest<'_>,
    on_progress: F,
) -> Result<ReliableTreeOutcome, ReliabilityError>
where
    F: FnMut(TreeTransferProgress),
{
    if request.destination.capabilities().atomic_rename != CapabilitySupport::Supported {
        return Err(ReliabilityError::AtomicRenameUnsupported);
    }

    let final_path = &request.spec.destination.path;
    let backup_path = sidecar_path(final_path, "tree-backup", request.id)?;
    ensure_absent(request.destination, &backup_path)?;
    let mut recovery_entry = RecoveryEntry {
        operation_id: request.id.get(),
        destination: final_path.as_str().to_owned(),
        stage: None,
        backup: Some(backup_path.as_str().to_owned()),
        phase: RecoveryPhase::BackupMoved,
    };
    if let Some(journal_path) = request.recovery_journal {
        persist_recovery_entry(journal_path, &recovery_entry)?;
    }

    request.destination.rename(final_path, &backup_path)?;

    match execute_tree_controlled(
        request.spec,
        request.source,
        request.destination,
        request.cancellation,
        on_progress,
    ) {
        Ok(TreeTransferOutcome::Completed(report)) => {
            complete_replacing_tree(request, &backup_path, &mut recovery_entry, report)
        }
        Ok(TreeTransferOutcome::Cancelled(report)) => {
            restore_tree_backup(request.destination, final_path, &backup_path)?;
            if let Some(journal_path) = request.recovery_journal {
                clear_recovery_entry(journal_path, request.id.get())?;
            }
            Ok(ReliableTreeOutcome::Cancelled(report))
        }
        Err(error) => recover_failed_replacing_tree(request, &backup_path, error),
    }
}

fn complete_replacing_tree(
    request: &ReliableTreeRequest<'_>,
    backup_path: &BackendPath,
    recovery_entry: &mut RecoveryEntry,
    report: TreeTransferReport,
) -> Result<ReliableTreeOutcome, ReliabilityError> {
    if let Some(journal_path) = request.recovery_journal {
        recovery_entry.phase = RecoveryPhase::Finalized;
        persist_recovery_entry(journal_path, recovery_entry)?;
    }
    if let Err(error) = remove_tree_if_present(request.destination, backup_path) {
        return Err(ReliabilityError::BackupCleanupFailed {
            path: backup_path.clone(),
            message: error.to_string(),
        });
    }
    if let Some(journal_path) = request.recovery_journal {
        clear_recovery_entry(journal_path, request.id.get())?;
    }
    Ok(ReliableTreeOutcome::Completed(ReliableTreeReport {
        transfer: report,
        replaced_existing: true,
    }))
}

fn recover_failed_replacing_tree(
    request: &ReliableTreeRequest<'_>,
    backup_path: &BackendPath,
    error: RecursiveError,
) -> Result<ReliableTreeOutcome, ReliabilityError> {
    let final_path = &request.spec.destination.path;
    let finalize = error.to_string();
    match restore_tree_backup(request.destination, final_path, backup_path) {
        Ok(()) => {
            if let Some(journal_path) = request.recovery_journal {
                clear_recovery_entry(journal_path, request.id.get())?;
            }
            Err(ReliabilityError::Recursive(error))
        }
        Err(recovery) => Err(ReliabilityError::RecoveryFailed {
            finalize,
            recovery: recovery.to_string(),
        }),
    }
}

fn restore_tree_backup(
    backend: &dyn Backend,
    final_path: &BackendPath,
    backup_path: &BackendPath,
) -> Result<(), BackendError> {
    remove_tree_if_present(backend, final_path)?;
    backend.rename(backup_path, final_path)
}

fn remove_tree_if_present(backend: &dyn Backend, path: &BackendPath) -> Result<(), BackendError> {
    let entry = match backend.stat(path) {
        Ok(entry) => entry,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };

    if entry.kind == EntryKind::Directory {
        for child in backend.list(path)? {
            remove_tree_if_present(backend, &child.path)?;
        }
    }
    backend.remove(path)
}

fn finalize_stage(
    destination: &dyn Backend,
    stage: &BackendPath,
    final_path: &BackendPath,
    backup: &BackendPath,
    destination_exists: bool,
    recovery_journal: Option<&Path>,
    recovery_entry: &mut RecoveryEntry,
) -> Result<(), ReliabilityError> {
    if !destination_exists {
        if let Err(error) = destination.rename(stage, final_path) {
            let _cleanup_result = remove_if_present(destination, stage);
            return Err(error.into());
        }
        if let Some(journal_path) = recovery_journal {
            recovery_entry.phase = RecoveryPhase::Finalized;
            persist_recovery_entry(journal_path, recovery_entry)?;
            clear_recovery_entry(journal_path, recovery_entry.operation_id)?;
        }
        return Ok(());
    }

    if let Some(journal_path) = recovery_journal {
        recovery_entry.phase = RecoveryPhase::BackupMoved;
        persist_recovery_entry(journal_path, recovery_entry)?;
    }
    destination.rename(final_path, backup)?;

    if let Err(finalize_error) = destination.rename(stage, final_path) {
        let recovery = destination.rename(backup, final_path);
        let cleanup_succeeded = remove_if_present(destination, stage).is_ok();
        return match recovery {
            Ok(()) => {
                if cleanup_succeeded {
                    if let Some(journal_path) = recovery_journal {
                        clear_recovery_entry(journal_path, recovery_entry.operation_id)?;
                    }
                }
                Err(finalize_error.into())
            }
            Err(recovery_error) => Err(ReliabilityError::RecoveryFailed {
                finalize: finalize_error.to_string(),
                recovery: recovery_error.to_string(),
            }),
        };
    }

    if let Some(journal_path) = recovery_journal {
        recovery_entry.phase = RecoveryPhase::Finalized;
        persist_recovery_entry(journal_path, recovery_entry)?;
    }
    if let Err(error) = destination.remove(backup) {
        return Err(ReliabilityError::BackupCleanupFailed {
            path: backup.clone(),
            message: error.to_string(),
        });
    }
    if let Some(journal_path) = recovery_journal {
        clear_recovery_entry(journal_path, recovery_entry.operation_id)?;
    }
    Ok(())
}

fn journal_error(path: &Path, message: String) -> ReliabilityError {
    ReliabilityError::RecoveryJournal {
        path: path.to_path_buf(),
        message,
    }
}

fn recover_pending_journal(backend: &dyn Backend, path: &Path) -> Result<(), ReliabilityError> {
    recover_journal(backend, path).map_err(|error| journal_error(path, error.to_string()))?;
    Ok(())
}

fn persist_recovery_entry(path: &Path, entry: &RecoveryEntry) -> Result<(), ReliabilityError> {
    let mut journal =
        RecoveryJournal::load(path).map_err(|error| journal_error(path, error.to_string()))?;
    journal.upsert(entry.clone());
    journal
        .save(path)
        .map_err(|error| journal_error(path, error.to_string()))
}

fn clear_recovery_entry(path: &Path, operation_id: u64) -> Result<(), ReliabilityError> {
    let mut journal =
        RecoveryJournal::load(path).map_err(|error| journal_error(path, error.to_string()))?;
    journal.remove(operation_id);
    journal
        .save_or_remove(path)
        .map_err(|error| journal_error(path, error.to_string()))
}

fn ensure_absent(backend: &dyn Backend, path: &BackendPath) -> Result<(), ReliabilityError> {
    match backend.stat(path) {
        Ok(_) => Err(ReliabilityError::SidecarCollision { path: path.clone() }),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn remove_if_present(backend: &dyn Backend, path: &BackendPath) -> Result<(), ReliabilityError> {
    match backend.remove(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn sidecar_path(
    destination: &BackendPath,
    role: &str,
    id: TransferId,
) -> Result<BackendPath, ReliabilityError> {
    BackendPath::new(format!(
        "{}.cyber-pumpkin-{role}-{}",
        destination.as_str(),
        id.get()
    ))
    .map_err(|error| {
        BackendError::new(
            ErrorKind::InvalidInput,
            "construct safe-transfer sidecar",
            Some(destination.clone()),
            error.to_string(),
        )
    })
    .map_err(ReliabilityError::Backend)
}

#[cfg(test)]
mod tests {
    use super::{
        ReliableFileRequest, ReliableTransferOutcome, execute_file_reliable,
        execute_file_reliable_request,
    };
    use cyber_pumpkin_core::{BackendId, BackendPath};
    use cyber_pumpkin_local::LocalBackend;
    use cyber_pumpkin_transfer::{CancellationToken, Endpoint, TransferId};
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(1);

    fn temp_root() -> PathBuf {
        let serial = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "cyber-pumpkin-reliable-{}-{serial}",
            std::process::id()
        ))
    }

    fn backend_path(path: &Path) -> Result<BackendPath, Box<dyn std::error::Error>> {
        Ok(BackendPath::new(path.to_string_lossy().into_owned())?)
    }

    #[test]
    fn journaled_new_file_commits_and_removes_journal() -> Result<(), Box<dyn std::error::Error>> {
        let root = temp_root();
        fs::create_dir(&root)?;
        let source_path = root.join("source.bin");
        let destination_path = root.join("destination.bin");
        let journal_path = root.join("recovery.json");
        fs::write(&source_path, b"new payload")?;
        let source_id = BackendId::new("source")?;
        let destination_id = BackendId::new("destination")?;
        let source = LocalBackend::new(source_id.clone());
        let destination = LocalBackend::new(destination_id.clone());
        let cancellation = CancellationToken::new();
        let outcome = execute_file_reliable_request(
            ReliableFileRequest {
                id: TransferId::new(40)?,
                source_endpoint: Endpoint {
                    backend: source_id,
                    path: backend_path(&source_path)?,
                },
                destination_endpoint: Endpoint {
                    backend: destination_id,
                    path: backend_path(&destination_path)?,
                },
                source: &source,
                destination: &destination,
                cancellation: &cancellation,
                recovery_journal: Some(&journal_path),
            },
            |_| {},
        )?;
        assert!(matches!(outcome, ReliableTransferOutcome::Completed(_)));
        assert_eq!(fs::read(&destination_path)?, b"new payload");
        assert!(!journal_path.exists());
        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn safely_replaces_existing_file() -> Result<(), Box<dyn std::error::Error>> {
        let root = temp_root();
        fs::create_dir(&root)?;
        let source_path = root.join("source.bin");
        let destination_path = root.join("destination.bin");
        fs::write(&source_path, b"new payload")?;
        fs::write(&destination_path, b"old payload")?;

        let source_id = BackendId::new("source")?;
        let destination_id = BackendId::new("destination")?;
        let source = LocalBackend::new(source_id.clone());
        let destination = LocalBackend::new(destination_id.clone());
        let outcome = execute_file_reliable(
            TransferId::new(41)?,
            Endpoint {
                backend: source_id,
                path: backend_path(&source_path)?,
            },
            Endpoint {
                backend: destination_id,
                path: backend_path(&destination_path)?,
            },
            &source,
            &destination,
            &CancellationToken::new(),
            |_| {},
        )?;

        assert!(matches!(
            outcome,
            ReliableTransferOutcome::Completed(report) if report.replaced_existing()
        ));
        assert_eq!(fs::read(&destination_path)?, b"new payload");
        assert_eq!(fs::read_dir(&root)?.count(), 2);

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn cancellation_preserves_existing_file() -> Result<(), Box<dyn std::error::Error>> {
        let root = temp_root();
        fs::create_dir(&root)?;
        let source_path = root.join("source.bin");
        let destination_path = root.join("destination.bin");
        fs::write(&source_path, vec![0x44; 256 * 1024])?;
        fs::write(&destination_path, b"keep me")?;

        let source_id = BackendId::new("source")?;
        let destination_id = BackendId::new("destination")?;
        let source = LocalBackend::new(source_id.clone());
        let destination = LocalBackend::new(destination_id.clone());
        let cancellation = CancellationToken::new();
        let cancel_from_progress = cancellation.clone();
        let outcome = execute_file_reliable(
            TransferId::new(42)?,
            Endpoint {
                backend: source_id,
                path: backend_path(&source_path)?,
            },
            Endpoint {
                backend: destination_id,
                path: backend_path(&destination_path)?,
            },
            &source,
            &destination,
            &cancellation,
            move |progress| {
                if progress.bytes_copied() > 0 {
                    cancel_from_progress.cancel();
                }
            },
        )?;

        assert!(matches!(outcome, ReliableTransferOutcome::Cancelled { .. }));
        assert_eq!(fs::read(&destination_path)?, b"keep me");
        assert_eq!(fs::read_dir(&root)?.count(), 2);

        fs::remove_dir_all(root)?;
        Ok(())
    }
}
