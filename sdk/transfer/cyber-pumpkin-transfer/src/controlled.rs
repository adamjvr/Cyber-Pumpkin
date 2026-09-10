//! Cooperative transfer progress and cancellation.

use super::{
    DestinationPolicy, ExecutionError, TransferJob, TransferReport, TransferState,
    enforce_destination_policy, validate_backend,
};
use cyber_pumpkin_backend::{Backend, ErrorKind, ReadStream, WriteStream};
use cyber_pumpkin_core::EntryKind;
use std::io::{Read as _, Write as _};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

const COPY_BUFFER_BYTES: usize = 64 * 1024;

/// Thread-safe cancellation request shared between a frontend and transfer worker.
#[derive(Clone, Debug)]
pub struct CancellationToken {
    cancelled: Arc<AtomicBool>,
}

impl CancellationToken {
    /// Creates a token in the non-cancelled state.
    #[must_use]
    pub fn new() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Requests cooperative cancellation.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    /// Returns whether cancellation has been requested.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}

impl Default for CancellationToken {
    fn default() -> Self {
        Self::new()
    }
}

/// Byte-level progress snapshot emitted by the controlled executor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TransferProgress {
    bytes_copied: u64,
    total_bytes: Option<u64>,
}

impl TransferProgress {
    /// Returns bytes written so far.
    #[must_use]
    pub const fn bytes_copied(self) -> u64 {
        self.bytes_copied
    }

    /// Returns the known source size when available.
    #[must_use]
    pub const fn total_bytes(self) -> Option<u64> {
        self.total_bytes
    }

    /// Returns progress in thousandths when total size is known.
    #[must_use]
    pub fn permille(self) -> Option<u16> {
        let total = self.total_bytes?;
        if total == 0 {
            return Some(1000);
        }

        let scaled = self.bytes_copied.min(total).saturating_mul(1000) / total;
        u16::try_from(scaled).ok()
    }
}

/// Terminal result of a cooperatively controlled transfer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ControlledTransferOutcome {
    /// Transfer completed and was verified.
    Completed(TransferReport),
    /// Transfer was cancelled and its partial destination was removed.
    Cancelled {
        /// Payload bytes written before cancellation was observed.
        bytes_copied: u64,
    },
}

enum StreamOutcome {
    Completed(u64),
    Cancelled(u64),
}

/// Executes a regular-file copy with byte progress and cooperative cancellation.
///
/// This first controlled executor intentionally uses fail-if-exists semantics.
/// That guarantees any destination created by this call belongs to this
/// transfer, so cancellation can safely remove a partial destination. Safe
/// replacement of an existing destination will use staging + atomic finalize
/// in the reliability milestone.
///
/// # Errors
///
/// Returns [`ExecutionError`] for backend validation, metadata, streaming,
/// cleanup, verification, or lifecycle failures.
pub fn execute_file_controlled<F>(
    job: &mut TransferJob,
    source: &dyn Backend,
    destination: &dyn Backend,
    cancellation: &CancellationToken,
    mut on_progress: F,
) -> Result<ControlledTransferOutcome, ExecutionError>
where
    F: FnMut(TransferProgress),
{
    validate_backend("source", &job.spec().source.backend, source.id())?;
    validate_backend(
        "destination",
        &job.spec().destination.backend,
        destination.id(),
    )?;

    job.transition(TransferState::Connecting)?;

    if cancellation.is_cancelled() {
        job.transition(TransferState::Cancelled)?;
        return Ok(ControlledTransferOutcome::Cancelled { bytes_copied: 0 });
    }

    let result =
        execute_started_controlled(job, source, destination, cancellation, &mut on_progress);

    if result.is_err() && job.state().can_transition_to(TransferState::Failed) {
        job.transition(TransferState::Failed)?;
    }

    result
}

fn execute_started_controlled<F>(
    job: &mut TransferJob,
    source: &dyn Backend,
    destination: &dyn Backend,
    cancellation: &CancellationToken,
    on_progress: &mut F,
) -> Result<ControlledTransferOutcome, ExecutionError>
where
    F: FnMut(TransferProgress),
{
    let source_entry = source.stat(&job.spec().source.path)?;
    if source_entry.kind != EntryKind::File {
        return Err(ExecutionError::SourceNotFile {
            kind: source_entry.kind,
        });
    }

    enforce_destination_policy(job, destination, DestinationPolicy::FailIfExists)?;

    if cancellation.is_cancelled() {
        job.transition(TransferState::Cancelled)?;
        return Ok(ControlledTransferOutcome::Cancelled { bytes_copied: 0 });
    }

    let mut reader = source.open_read(&job.spec().source.path)?;
    let mut writer = destination.open_write(&job.spec().destination.path)?;

    job.transition(TransferState::Transferring)?;
    let stream_result = copy_stream_controlled(
        &mut reader,
        &mut writer,
        source_entry.size,
        cancellation,
        on_progress,
    );

    drop(writer);
    drop(reader);

    match stream_result? {
        StreamOutcome::Cancelled(bytes_copied) => {
            cleanup_partial_destination(job, destination)?;
            job.transition(TransferState::Cancelled)?;
            Ok(ControlledTransferOutcome::Cancelled { bytes_copied })
        }
        StreamOutcome::Completed(bytes_copied) => {
            job.transition(TransferState::Verifying)?;
            let destination_entry = destination.stat(&job.spec().destination.path)?;
            if let Some(destination_size) = destination_entry.size
                && destination_size != bytes_copied
            {
                return Err(ExecutionError::SizeMismatch {
                    copied: bytes_copied,
                    destination: destination_size,
                });
            }

            job.transition(TransferState::Completed)?;
            Ok(ControlledTransferOutcome::Completed(TransferReport {
                bytes_copied,
            }))
        }
    }
}

fn copy_stream_controlled<F>(
    reader: &mut ReadStream,
    writer: &mut WriteStream,
    total_bytes: Option<u64>,
    cancellation: &CancellationToken,
    on_progress: &mut F,
) -> Result<StreamOutcome, ExecutionError>
where
    F: FnMut(TransferProgress),
{
    let mut buffer = vec![0_u8; COPY_BUFFER_BYTES].into_boxed_slice();
    let mut bytes_copied = 0_u64;

    loop {
        if cancellation.is_cancelled() {
            return Ok(StreamOutcome::Cancelled(bytes_copied));
        }

        let read = reader
            .read(&mut buffer)
            .map_err(|error| ExecutionError::Stream(error.to_string()))?;
        if read == 0 {
            break;
        }

        writer
            .write_all(&buffer[..read])
            .map_err(|error| ExecutionError::Stream(error.to_string()))?;

        let read_u64 =
            u64::try_from(read).map_err(|error| ExecutionError::Stream(error.to_string()))?;
        bytes_copied = bytes_copied.saturating_add(read_u64);

        on_progress(TransferProgress {
            bytes_copied,
            total_bytes,
        });
    }

    writer
        .flush()
        .map_err(|error| ExecutionError::Stream(error.to_string()))?;

    Ok(StreamOutcome::Completed(bytes_copied))
}

fn cleanup_partial_destination(
    job: &TransferJob,
    destination: &dyn Backend,
) -> Result<(), ExecutionError> {
    match destination.remove(&job.spec().destination.path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::{CancellationToken, ControlledTransferOutcome, execute_file_controlled};
    use crate::{Endpoint, TransferId, TransferJob, TransferSpec, TransferState};
    use cyber_pumpkin_core::{BackendId, BackendPath};
    use cyber_pumpkin_local::LocalBackend;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEST_DIRECTORY: AtomicU64 = AtomicU64::new(1);

    fn test_directory() -> PathBuf {
        let serial = NEXT_TEST_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "cyber-pumpkin-controlled-transfer-{}-{serial}",
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
    fn cancellation_removes_partial_destination() -> Result<(), Box<dyn std::error::Error>> {
        let root = test_directory();
        fs::create_dir(&root)?;
        let source_path = root.join("source.bin");
        let destination_path = root.join("destination.bin");
        fs::write(&source_path, vec![0x5a; 256 * 1024])?;

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

        let cancellation = CancellationToken::new();
        let cancellation_from_progress = cancellation.clone();
        let mut job = TransferJob::new(TransferId::new(91)?, spec);

        let outcome = execute_file_controlled(
            &mut job,
            &backend,
            &backend,
            &cancellation,
            move |progress| {
                if progress.bytes_copied() > 0 {
                    cancellation_from_progress.cancel();
                }
            },
        )?;

        assert!(matches!(
            outcome,
            ControlledTransferOutcome::Cancelled { bytes_copied } if bytes_copied > 0
        ));
        assert_eq!(job.state(), TransferState::Cancelled);
        assert!(!destination_path.exists());

        fs::remove_dir_all(&root)?;
        Ok(())
    }

    #[test]
    fn completed_controlled_transfer_reports_full_progress()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = test_directory();
        fs::create_dir(&root)?;
        let source_path = root.join("source.bin");
        let destination_path = root.join("destination.bin");
        let payload = vec![0x33; 150 * 1024];
        let expected_len = u64::try_from(payload.len())?;
        fs::write(&source_path, &payload)?;

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

        let cancellation = CancellationToken::new();
        let mut last_progress = None;
        let mut job = TransferJob::new(TransferId::new(92)?, spec);

        let outcome =
            execute_file_controlled(&mut job, &backend, &backend, &cancellation, |progress| {
                last_progress = Some(progress);
            })?;

        assert!(matches!(
            outcome,
            ControlledTransferOutcome::Completed(report)
                if report.bytes_copied() == expected_len
        ));
        assert_eq!(
            last_progress.map(super::TransferProgress::bytes_copied),
            Some(expected_len)
        );
        assert_eq!(job.state(), TransferState::Completed);
        assert_eq!(fs::read(&destination_path)?, payload);

        fs::remove_dir_all(&root)?;
        Ok(())
    }
}
