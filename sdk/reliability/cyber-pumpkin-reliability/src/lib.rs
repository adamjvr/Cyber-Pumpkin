//! Crash-resistant transfer finalization primitives.
//!
//! This layer owns the safe replacement transaction used by synchronization
//! and later ordinary copy/upload operations. Payload bytes are first written
//! to an operation-owned sibling path, verified by the transfer layer, then
//! finalized with backend renames. Existing destinations are temporarily moved
//! to an operation-owned backup and restored if finalization fails.

use cyber_pumpkin_backend::{Backend, BackendError, ErrorKind};
use cyber_pumpkin_core::{BackendPath, CapabilitySupport};
use cyber_pumpkin_transfer::{
    CancellationToken, ControlledTransferOutcome, Endpoint, ExecutionError, TransferId,
    TransferJob, TransferProgress, TransferSpec, execute_file_controlled,
};
use std::fmt;

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
    /// Payload transfer or verification failed.
    Transfer(ExecutionError),
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
            Self::Transfer(error) => write!(formatter, "staged transfer failed: {error}"),
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

impl From<BackendError> for ReliabilityError {
    fn from(value: BackendError) -> Self {
        Self::Backend(value)
    }
}

/// Copies one regular file through an operation-owned stage and safely
/// finalizes it over the requested destination.
///
/// Cancellation is honored while staging and immediately before finalization.
/// Once the rename transaction begins it runs to completion or attempts to
/// restore the original destination; this prevents cancellation from leaving a
/// half-finalized replacement.
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
    mut on_progress: F,
) -> Result<ReliableTransferOutcome, ReliabilityError>
where
    F: FnMut(TransferProgress),
{
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
            return Ok(ReliableTransferOutcome::Cancelled { bytes_copied });
        }
    };

    if cancellation.is_cancelled() {
        remove_if_present(destination, &stage_path)?;
        return Ok(ReliableTransferOutcome::Cancelled { bytes_copied });
    }

    finalize_stage(
        destination,
        &stage_path,
        &final_path,
        &backup_path,
        destination_exists,
    )?;

    Ok(ReliableTransferOutcome::Completed(ReliableTransferReport {
        bytes_copied,
        replaced_existing: destination_exists,
    }))
}

fn finalize_stage(
    destination: &dyn Backend,
    stage: &BackendPath,
    final_path: &BackendPath,
    backup: &BackendPath,
    destination_exists: bool,
) -> Result<(), ReliabilityError> {
    if !destination_exists {
        if let Err(error) = destination.rename(stage, final_path) {
            let _cleanup_result = remove_if_present(destination, stage);
            return Err(error.into());
        }
        return Ok(());
    }

    destination.rename(final_path, backup)?;

    if let Err(finalize_error) = destination.rename(stage, final_path) {
        let recovery = destination.rename(backup, final_path);
        let _cleanup_result = remove_if_present(destination, stage);
        return match recovery {
            Ok(()) => Err(finalize_error.into()),
            Err(recovery_error) => Err(ReliabilityError::RecoveryFailed {
                finalize: finalize_error.to_string(),
                recovery: recovery_error.to_string(),
            }),
        };
    }

    if let Err(error) = destination.remove(backup) {
        return Err(ReliabilityError::BackupCleanupFailed {
            path: backup.clone(),
            message: error.to_string(),
        });
    }

    Ok(())
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
    use super::{ReliableTransferOutcome, execute_file_reliable};
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
