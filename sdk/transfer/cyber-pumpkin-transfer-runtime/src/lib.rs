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
    CancellationToken, Endpoint, TransferId, TransferSpec, TreeTransferProgress, TreeTransferReport,
};
use std::fmt;

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

#[cfg(test)]
mod tests {
    use super::{CopyConflictPolicy, CopyRequest, CopyRuntimeOutcome, execute_copy};
    use cyber_pumpkin_core::{BackendId, BackendPath};
    use cyber_pumpkin_local::LocalBackend;
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
