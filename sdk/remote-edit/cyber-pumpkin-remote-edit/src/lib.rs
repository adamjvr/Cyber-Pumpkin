//! Backend-neutral remote-edit session lifecycle.

use cyber_pumpkin_backend::{Backend, BackendError};
use cyber_pumpkin_core::{BackendId, BackendPath, EntryKind};
use cyber_pumpkin_reliability::{ReliableTransferOutcome, execute_file_reliable};
use cyber_pumpkin_transfer::{CancellationToken, Endpoint, TransferId};
use cyber_pumpkin_transfer_runtime::{
    CopyConflictPolicy, CopyRequest, CopyRuntimeError, CopyRuntimeOutcome, execute_copy,
};
use std::fmt;
use std::io::Read as _;
use std::path::Path;

/// Snapshot used to detect local edits.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FileStamp {
    /// Logical size.
    pub size: Option<u64>,
    /// Modification time in Unix seconds.
    pub modified: Option<u64>,
    /// Content fingerprint used to catch same-size edits within coarse timestamp windows.
    pub fnv1a64: u64,
}

/// One remote-edit session.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RemoteEditSession {
    /// Stable operation identifier.
    pub id: TransferId,
    /// Remote source/destination endpoint.
    pub remote: Endpoint,
    /// Local working-copy endpoint.
    pub local: Endpoint,
    /// Initial or last-uploaded local stamp.
    pub baseline: FileStamp,
}

/// Remote-edit failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RemoteEditError {
    /// Source is not a regular file.
    SourceNotFile(EntryKind),
    /// Backend operation failed.
    Backend(BackendError),
    /// Download orchestration failed.
    Download(CopyRuntimeError),
    /// Download unexpectedly skipped.
    DownloadSkipped,
    /// Download was cancelled.
    DownloadCancelled,
    /// Upload reliability failed.
    Upload(String),
    /// Upload was cancelled.
    UploadCancelled,
    /// Workspace path could not be represented.
    InvalidWorkspacePath,
}

impl fmt::Display for RemoteEditError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SourceNotFile(kind) => {
                write!(formatter, "remote edit requires a file, got {kind:?}")
            }
            Self::Backend(error) => write!(formatter, "{error}"),
            Self::Download(error) => write!(formatter, "remote-edit download failed: {error}"),
            Self::DownloadSkipped => formatter.write_str("remote-edit download was skipped"),
            Self::DownloadCancelled => formatter.write_str("remote-edit download was cancelled"),
            Self::Upload(error) => write!(formatter, "remote-edit upload failed: {error}"),
            Self::UploadCancelled => formatter.write_str("remote-edit upload was cancelled"),
            Self::InvalidWorkspacePath => {
                formatter.write_str("remote-edit workspace path is invalid")
            }
        }
    }
}

impl std::error::Error for RemoteEditError {}

impl From<BackendError> for RemoteEditError {
    fn from(value: BackendError) -> Self {
        Self::Backend(value)
    }
}

/// Creates a deterministic local workspace endpoint for a remote file.
///
/// # Errors
///
/// Returns [`RemoteEditError`] when the workspace path cannot be represented.
pub fn create_session(
    id: TransferId,
    remote: Endpoint,
    local_backend: BackendId,
    workspace_root: &Path,
    file_name: &str,
) -> Result<RemoteEditSession, RemoteEditError> {
    let safe_name = sanitize_file_name(file_name);
    let local_path = workspace_root
        .join(format!("session-{}", id.get()))
        .join(safe_name);
    let local_text = local_path
        .to_str()
        .ok_or(RemoteEditError::InvalidWorkspacePath)?;
    let local = Endpoint {
        backend: local_backend,
        path: BackendPath::new(local_text.to_owned())
            .map_err(|_| RemoteEditError::InvalidWorkspacePath)?,
    };
    Ok(RemoteEditSession {
        id,
        remote,
        local,
        baseline: FileStamp::default(),
    })
}

/// Downloads the remote file into the local workspace and establishes baseline
/// metadata.
///
/// # Errors
///
/// Returns [`RemoteEditError`] for backend, copy, or cancellation failures.
pub fn download_initial(
    session: &mut RemoteEditSession,
    remote_backend: &dyn Backend,
    local_backend: &dyn Backend,
    cancellation: &CancellationToken,
) -> Result<(), RemoteEditError> {
    let remote_entry = remote_backend.stat(&session.remote.path)?;
    if remote_entry.kind != EntryKind::File {
        return Err(RemoteEditError::SourceNotFile(remote_entry.kind));
    }

    ensure_local_parent(local_backend, &session.local.path)?;

    let request = CopyRequest {
        id: session.id,
        source: session.remote.clone(),
        destination: session.local.clone(),
        conflict_policy: CopyConflictPolicy::Replace,
    };
    let outcome = execute_copy(
        &request,
        remote_backend,
        local_backend,
        cancellation,
        |_| {},
    )
    .map_err(RemoteEditError::Download)?;

    match outcome {
        CopyRuntimeOutcome::Completed(_) => {
            session.baseline = stamp(local_backend, &session.local.path)?;
            Ok(())
        }
        CopyRuntimeOutcome::Skipped { .. } => Err(RemoteEditError::DownloadSkipped),
        CopyRuntimeOutcome::Cancelled(_) => Err(RemoteEditError::DownloadCancelled),
    }
}

/// Returns whether the local working copy differs from the baseline.
///
/// # Errors
///
/// Returns [`RemoteEditError`] when local metadata cannot be read.
pub fn local_changed(
    session: &RemoteEditSession,
    local_backend: &dyn Backend,
) -> Result<bool, RemoteEditError> {
    Ok(stamp(local_backend, &session.local.path)? != session.baseline)
}

/// Uploads a changed local working copy through safe reliable replacement and
/// refreshes the baseline.
///
/// # Errors
///
/// Returns [`RemoteEditError`] for backend, reliability, or cancellation failures.
pub fn upload_changed(
    session: &mut RemoteEditSession,
    local_backend: &dyn Backend,
    remote_backend: &dyn Backend,
    cancellation: &CancellationToken,
) -> Result<bool, RemoteEditError> {
    if !local_changed(session, local_backend)? {
        return Ok(false);
    }

    match execute_file_reliable(
        session.id,
        session.local.clone(),
        session.remote.clone(),
        local_backend,
        remote_backend,
        cancellation,
        |_| {},
    )
    .map_err(|error| RemoteEditError::Upload(error.to_string()))?
    {
        ReliableTransferOutcome::Completed(_) => {
            session.baseline = stamp(local_backend, &session.local.path)?;
            Ok(true)
        }
        ReliableTransferOutcome::Cancelled { .. } => Err(RemoteEditError::UploadCancelled),
    }
}

fn stamp(backend: &dyn Backend, path: &BackendPath) -> Result<FileStamp, RemoteEditError> {
    let entry = backend.stat(path)?;
    let mut reader = backend.open_read(path)?;
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    let mut buffer = [0_u8; 16 * 1024];
    loop {
        let read = reader.read(&mut buffer).map_err(|error| {
            BackendError::new(
                cyber_pumpkin_backend::ErrorKind::Io,
                "fingerprint remote-edit working copy",
                Some(path.clone()),
                error.to_string(),
            )
        })?;
        if read == 0 {
            break;
        }
        for byte in &buffer[..read] {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0100_0000_01b3);
        }
    }
    Ok(FileStamp {
        size: entry.size,
        modified: entry.modified,
        fnv1a64: hash,
    })
}

fn sanitize_file_name(name: &str) -> String {
    let mut sanitized = name
        .chars()
        .map(|character| {
            if character == '/' || character == '\\' || character == '\0' {
                '_'
            } else {
                character
            }
        })
        .collect::<String>();
    if sanitized.trim().is_empty() {
        "remote-edit-file".clone_into(&mut sanitized);
    }
    sanitized
}

fn ensure_local_parent(backend: &dyn Backend, path: &BackendPath) -> Result<(), RemoteEditError> {
    let text = path.as_str();
    let Some((parent, _)) = text.rsplit_once('/') else {
        return Ok(());
    };
    if parent.is_empty() {
        return Ok(());
    }
    let parent_path =
        BackendPath::new(parent.to_owned()).map_err(|_| RemoteEditError::InvalidWorkspacePath)?;
    ensure_directory_chain(backend, &parent_path)
}

fn ensure_directory_chain(
    backend: &dyn Backend,
    path: &BackendPath,
) -> Result<(), RemoteEditError> {
    let text = path.as_str();
    let mut current = String::new();
    for component in text.split('/').filter(|part| !part.is_empty()) {
        current.push('/');
        current.push_str(component);
        let candidate =
            BackendPath::new(current.clone()).map_err(|_| RemoteEditError::InvalidWorkspacePath)?;
        match backend.stat(&candidate) {
            Ok(_) => {}
            Err(error) if error.kind() == cyber_pumpkin_backend::ErrorKind::NotFound => {
                backend.create_dir(&candidate)?;
            }
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{create_session, download_initial, local_changed, upload_changed};
    use cyber_pumpkin_core::{BackendId, BackendPath};
    use cyber_pumpkin_local::LocalBackend;
    use cyber_pumpkin_transfer::{CancellationToken, Endpoint, TransferId};
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(1);

    fn root() -> PathBuf {
        let serial = NEXT.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "cyber-pumpkin-remote-edit-{}-{serial}",
            std::process::id()
        ))
    }

    #[test]
    fn local_change_round_trips_back_to_remote() -> Result<(), Box<dyn std::error::Error>> {
        let root = root();
        let remote_root = root.join("remote");
        let workspace = root.join("workspace");
        fs::create_dir_all(&remote_root)?;
        fs::create_dir_all(&workspace)?;

        let remote_path = remote_root.join("patch.txt");
        fs::write(&remote_path, b"original")?;

        let remote_id = BackendId::new("remote")?;
        let local_id = BackendId::new("local")?;
        let remote_backend = LocalBackend::new(remote_id.clone());
        let local_backend = LocalBackend::new(local_id.clone());

        let remote = Endpoint {
            backend: remote_id,
            path: BackendPath::new(remote_path.to_string_lossy().into_owned())?,
        };
        let mut session = create_session(
            TransferId::new(77)?,
            remote,
            local_id,
            &workspace,
            "patch.txt",
        )?;

        download_initial(
            &mut session,
            &remote_backend,
            &local_backend,
            &CancellationToken::new(),
        )?;
        assert!(!local_changed(&session, &local_backend)?);

        fs::write(session.local.path.as_str(), b"edited")?;
        assert!(local_changed(&session, &local_backend)?);
        assert!(upload_changed(
            &mut session,
            &local_backend,
            &remote_backend,
            &CancellationToken::new(),
        )?);
        assert_eq!(fs::read(remote_path)?, b"edited");

        fs::remove_dir_all(root)?;
        Ok(())
    }
}
