//! Recursive transfer planning and controlled execution.

use super::{
    CancellationToken, ControlledTransferOutcome, Endpoint, ExecutionError, TransferId,
    TransferJob, TransferProgress, TransferSpec, validate_backend,
};
use cyber_pumpkin_backend::{Backend, BackendError, ErrorKind};
use cyber_pumpkin_core::{BackendPath, EntryKind};
use std::collections::VecDeque;
use std::fmt;

/// Aggregate recursive-transfer progress.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TreeTransferProgress {
    bytes_copied: u64,
    total_bytes: Option<u64>,
    files_copied: u64,
    total_files: u64,
    directories_created: u64,
}

impl TreeTransferProgress {
    /// Payload bytes written so far.
    #[must_use]
    pub const fn bytes_copied(self) -> u64 {
        self.bytes_copied
    }

    /// Total payload bytes when all source sizes were known during planning.
    #[must_use]
    pub const fn total_bytes(self) -> Option<u64> {
        self.total_bytes
    }

    /// Files completely copied so far.
    #[must_use]
    pub const fn files_copied(self) -> u64 {
        self.files_copied
    }

    /// Total regular files in the plan.
    #[must_use]
    pub const fn total_files(self) -> u64 {
        self.total_files
    }

    /// Destination directories created so far.
    #[must_use]
    pub const fn directories_created(self) -> u64 {
        self.directories_created
    }

    /// Overall progress in thousandths.
    #[must_use]
    pub fn permille(self) -> Option<u16> {
        if let Some(total) = self.total_bytes
            && total > 0
        {
            let scaled = self.bytes_copied.min(total).saturating_mul(1000) / total;
            return u16::try_from(scaled).ok();
        }

        if self.total_files == 0 {
            return Some(1000);
        }

        let scaled =
            self.files_copied.min(self.total_files).saturating_mul(1000) / self.total_files;
        u16::try_from(scaled).ok()
    }
}

/// Aggregate result for a file or directory-tree transfer.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TreeTransferReport {
    bytes_copied: u64,
    files_copied: u64,
    directories_created: u64,
}

impl TreeTransferReport {
    /// Payload bytes copied.
    #[must_use]
    pub const fn bytes_copied(self) -> u64 {
        self.bytes_copied
    }

    /// Regular files copied.
    #[must_use]
    pub const fn files_copied(self) -> u64 {
        self.files_copied
    }

    /// Directories created.
    #[must_use]
    pub const fn directories_created(self) -> u64 {
        self.directories_created
    }
}

/// Terminal outcome for a recursively controlled transfer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TreeTransferOutcome {
    /// The entire plan completed successfully.
    Completed(TreeTransferReport),
    /// Cancellation was observed and everything created by the operation was removed.
    Cancelled(TreeTransferReport),
}

/// Recursive-transfer failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RecursiveError {
    /// File executor or endpoint validation failed.
    Execution(ExecutionError),
    /// Backend enumeration, creation, or cleanup failed.
    Backend(BackendError),
    /// Destination root already exists.
    DestinationExists(BackendPath),
    /// Symbolic links and special entries are not copied by this milestone.
    UnsupportedEntry {
        /// Unsupported source path.
        path: BackendPath,
        /// Unsupported entry kind.
        kind: EntryKind,
    },
    /// A backend child path could not be represented.
    InvalidChildPath(String),
    /// Cleanup failed after another failure.
    Cleanup(String),
}

impl fmt::Display for RecursiveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Execution(error) => write!(f, "{error}"),
            Self::Backend(error) => write!(f, "{error}"),
            Self::DestinationExists(path) => {
                write!(f, "destination already exists: {}", path.as_str())
            }
            Self::UnsupportedEntry { path, kind } => {
                write!(f, "unsupported {kind:?} entry: {}", path.as_str())
            }
            Self::InvalidChildPath(message) => write!(f, "invalid child path: {message}"),
            Self::Cleanup(message) => write!(f, "recursive transfer cleanup failed: {message}"),
        }
    }
}

impl std::error::Error for RecursiveError {}

impl From<ExecutionError> for RecursiveError {
    fn from(value: ExecutionError) -> Self {
        Self::Execution(value)
    }
}

impl From<BackendError> for RecursiveError {
    fn from(value: BackendError) -> Self {
        Self::Backend(value)
    }
}

#[derive(Clone, Debug)]
struct PlannedFile {
    source: BackendPath,
    destination: BackendPath,
}

#[derive(Clone, Debug)]
struct TreePlan {
    directories: Vec<BackendPath>,
    files: Vec<PlannedFile>,
    total_bytes: Option<u64>,
}

struct CreatedObjects {
    files: Vec<BackendPath>,
    directories: Vec<BackendPath>,
}

impl CreatedObjects {
    const fn new() -> Self {
        Self {
            files: Vec::new(),
            directories: Vec::new(),
        }
    }
}

/// Copies one regular file or one complete directory tree.
///
/// The destination root must not already exist. Directory transfers are
/// pre-enumerated in this first implementation so aggregate progress has a
/// stable total; later recursive-spooling work can replace the planner without
/// changing this API.
///
/// Cancellation and execution failures remove files/directories created by the
/// operation, because fail-if-exists ownership guarantees they belong to this
/// transfer.
///
/// # Errors
///
/// Returns [`RecursiveError`] for endpoint mismatch, enumeration, unsupported
/// entry kinds, destination conflicts, file execution, or cleanup failures.
pub fn execute_tree_controlled<F>(
    spec: &TransferSpec,
    source: &dyn Backend,
    destination: &dyn Backend,
    cancellation: &CancellationToken,
    mut on_progress: F,
) -> Result<TreeTransferOutcome, RecursiveError>
where
    F: FnMut(TreeTransferProgress),
{
    validate_backend("source", &spec.source.backend, source.id())?;
    validate_backend("destination", &spec.destination.backend, destination.id())?;
    ensure_destination_absent(destination, &spec.destination.path)?;

    let source_entry = source.stat(&spec.source.path)?;
    match source_entry.kind {
        EntryKind::File => execute_single_file(
            spec,
            source,
            destination,
            cancellation,
            source_entry.size,
            &mut on_progress,
        ),
        EntryKind::Directory => {
            let plan = plan_directory_tree(spec, source)?;
            execute_directory_plan(
                spec,
                source,
                destination,
                cancellation,
                &plan,
                &mut on_progress,
            )
        }
        kind => Err(RecursiveError::UnsupportedEntry {
            path: spec.source.path.clone(),
            kind,
        }),
    }
}

fn execute_single_file<F>(
    spec: &TransferSpec,
    source: &dyn Backend,
    destination: &dyn Backend,
    cancellation: &CancellationToken,
    total_bytes: Option<u64>,
    on_progress: &mut F,
) -> Result<TreeTransferOutcome, RecursiveError>
where
    F: FnMut(TreeTransferProgress),
{
    let id = TransferId::new(1).map_err(ExecutionError::from)?;
    let mut job = TransferJob::new(id, spec.clone());

    let outcome =
        super::execute_file_controlled(&mut job, source, destination, cancellation, |progress| {
            on_progress(tree_progress_from_file(progress, total_bytes, 0, 1, 0));
        });

    let outcome = outcome?;

    match outcome {
        ControlledTransferOutcome::Completed(report) => {
            let tree = TreeTransferReport {
                bytes_copied: report.bytes_copied(),
                files_copied: 1,
                directories_created: 0,
            };
            on_progress(progress_from_report(tree, total_bytes, 1));
            Ok(TreeTransferOutcome::Completed(tree))
        }
        ControlledTransferOutcome::Cancelled { bytes_copied } => {
            Ok(TreeTransferOutcome::Cancelled(TreeTransferReport {
                bytes_copied,
                files_copied: 0,
                directories_created: 0,
            }))
        }
    }
}

fn plan_directory_tree(
    spec: &TransferSpec,
    source: &dyn Backend,
) -> Result<TreePlan, RecursiveError> {
    let mut directories = vec![spec.destination.path.clone()];
    let mut files = Vec::new();
    let mut total_bytes = Some(0_u64);
    let mut queue = VecDeque::new();
    queue.push_back((spec.source.path.clone(), spec.destination.path.clone()));

    while let Some((source_dir, destination_dir)) = queue.pop_front() {
        for entry in source.list(&source_dir)? {
            let destination_child = child_path(&destination_dir, &entry.name)?;
            match entry.kind {
                EntryKind::Directory => {
                    directories.push(destination_child.clone());
                    queue.push_back((entry.path, destination_child));
                }
                EntryKind::File => {
                    total_bytes = add_known_size(total_bytes, entry.size);
                    files.push(PlannedFile {
                        source: entry.path,
                        destination: destination_child,
                    });
                }
                kind => {
                    return Err(RecursiveError::UnsupportedEntry {
                        path: entry.path,
                        kind,
                    });
                }
            }
        }
    }

    Ok(TreePlan {
        directories,
        files,
        total_bytes,
    })
}

fn execute_directory_plan<F>(
    spec: &TransferSpec,
    source: &dyn Backend,
    destination: &dyn Backend,
    cancellation: &CancellationToken,
    plan: &TreePlan,
    on_progress: &mut F,
) -> Result<TreeTransferOutcome, RecursiveError>
where
    F: FnMut(TreeTransferProgress),
{
    let total_files = u64::try_from(plan.files.len())
        .map_err(|error| RecursiveError::InvalidChildPath(error.to_string()))?;
    let mut report = TreeTransferReport::default();
    let mut created = CreatedObjects::new();

    for directory in &plan.directories {
        if cancellation.is_cancelled() {
            cleanup_created(destination, &created)?;
            return Ok(TreeTransferOutcome::Cancelled(report));
        }
        if let Err(error) = destination.create_dir(directory) {
            cleanup_after_error(destination, &created, &error.to_string())?;
            return Err(error.into());
        }
        created.directories.push(directory.clone());
        report.directories_created = report.directories_created.saturating_add(1);
        on_progress(progress_from_report(report, plan.total_bytes, total_files));
    }

    for (index, file) in plan.files.iter().enumerate() {
        if cancellation.is_cancelled() {
            cleanup_created(destination, &created)?;
            return Ok(TreeTransferOutcome::Cancelled(report));
        }

        let id_value = u64::try_from(index)
            .map_err(|error| RecursiveError::InvalidChildPath(error.to_string()))?
            .saturating_add(1);
        let id = TransferId::new(id_value).map_err(ExecutionError::from)?;
        let child_spec = TransferSpec {
            source: Endpoint {
                backend: spec.source.backend.clone(),
                path: file.source.clone(),
            },
            destination: Endpoint {
                backend: spec.destination.backend.clone(),
                path: file.destination.clone(),
            },
        };
        let mut job = TransferJob::new(id, child_spec);
        let base_bytes = report.bytes_copied;
        let files_copied = report.files_copied;
        let directories_created = report.directories_created;

        let outcome = super::execute_file_controlled(
            &mut job,
            source,
            destination,
            cancellation,
            |progress| {
                on_progress(TreeTransferProgress {
                    bytes_copied: base_bytes.saturating_add(progress.bytes_copied()),
                    total_bytes: plan.total_bytes,
                    files_copied,
                    total_files,
                    directories_created,
                });
            },
        );

        match outcome {
            Ok(ControlledTransferOutcome::Completed(file_report)) => {
                report.bytes_copied = report
                    .bytes_copied
                    .saturating_add(file_report.bytes_copied());
                report.files_copied = report.files_copied.saturating_add(1);
                created.files.push(file.destination.clone());
                on_progress(progress_from_report(report, plan.total_bytes, total_files));
            }
            Ok(ControlledTransferOutcome::Cancelled { bytes_copied }) => {
                let cancelled_report = TreeTransferReport {
                    bytes_copied: report.bytes_copied.saturating_add(bytes_copied),
                    files_copied: report.files_copied,
                    directories_created: report.directories_created,
                };
                cleanup_created(destination, &created)?;
                return Ok(TreeTransferOutcome::Cancelled(cancelled_report));
            }
            Err(error) => {
                cleanup_after_error(destination, &created, &error.to_string())?;
                return Err(error.into());
            }
        }
    }

    Ok(TreeTransferOutcome::Completed(report))
}

fn progress_from_report(
    report: TreeTransferReport,
    total_bytes: Option<u64>,
    total_files: u64,
) -> TreeTransferProgress {
    TreeTransferProgress {
        bytes_copied: report.bytes_copied,
        total_bytes,
        files_copied: report.files_copied,
        total_files,
        directories_created: report.directories_created,
    }
}

fn tree_progress_from_file(
    progress: TransferProgress,
    total_bytes: Option<u64>,
    files_copied: u64,
    total_files: u64,
    directories_created: u64,
) -> TreeTransferProgress {
    TreeTransferProgress {
        bytes_copied: progress.bytes_copied(),
        total_bytes,
        files_copied,
        total_files,
        directories_created,
    }
}

fn ensure_destination_absent(
    destination: &dyn Backend,
    path: &BackendPath,
) -> Result<(), RecursiveError> {
    match destination.stat(path) {
        Ok(_) => Err(RecursiveError::DestinationExists(path.clone())),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn child_path(parent: &BackendPath, name: &str) -> Result<BackendPath, RecursiveError> {
    if name.is_empty() || name.contains('/') {
        return Err(RecursiveError::InvalidChildPath(name.to_owned()));
    }

    let base = parent.as_str();
    let joined = if base.ends_with('/') {
        format!("{base}{name}")
    } else {
        format!("{base}/{name}")
    };
    BackendPath::new(joined).map_err(|error| RecursiveError::InvalidChildPath(error.to_string()))
}

const fn add_known_size(total: Option<u64>, size: Option<u64>) -> Option<u64> {
    match (total, size) {
        (Some(total), Some(size)) => Some(total.saturating_add(size)),
        _ => None,
    }
}

fn cleanup_after_error(
    destination: &dyn Backend,
    created: &CreatedObjects,
    original: &str,
) -> Result<(), RecursiveError> {
    cleanup_created(destination, created)
        .map_err(|cleanup| RecursiveError::Cleanup(format!("{original}; cleanup error: {cleanup}")))
}

fn cleanup_created(
    destination: &dyn Backend,
    created: &CreatedObjects,
) -> Result<(), RecursiveError> {
    for file in created.files.iter().rev() {
        remove_owned(destination, file)?;
    }
    for directory in created.directories.iter().rev() {
        remove_owned(destination, directory)?;
    }
    Ok(())
}

fn remove_owned(destination: &dyn Backend, path: &BackendPath) -> Result<(), RecursiveError> {
    match destination.remove(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::{TreeTransferOutcome, execute_tree_controlled};
    use crate::{CancellationToken, Endpoint, TransferSpec};
    use cyber_pumpkin_core::{BackendId, BackendPath};
    use cyber_pumpkin_local::LocalBackend;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEST_DIRECTORY: AtomicU64 = AtomicU64::new(1);

    fn test_directory() -> PathBuf {
        let serial = NEXT_TEST_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "cyber-pumpkin-tree-transfer-{}-{serial}",
            std::process::id()
        ))
    }

    fn backend_path(path: &Path) -> Result<BackendPath, Box<dyn std::error::Error>> {
        let text = path.to_str().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, "test path must be UTF-8")
        })?;
        Ok(BackendPath::new(text)?)
    }

    fn spec(
        backend: &BackendId,
        source: &Path,
        destination: &Path,
    ) -> Result<TransferSpec, Box<dyn std::error::Error>> {
        Ok(TransferSpec {
            source: Endpoint {
                backend: backend.clone(),
                path: backend_path(source)?,
            },
            destination: Endpoint {
                backend: backend.clone(),
                path: backend_path(destination)?,
            },
        })
    }

    #[test]
    fn nested_tree_copy_preserves_content() -> Result<(), Box<dyn std::error::Error>> {
        let root = test_directory();
        let source = root.join("source");
        let destination = root.join("destination");
        fs::create_dir_all(source.join("nested/deeper"))?;
        fs::write(source.join("root.txt"), b"root")?;
        fs::write(source.join("nested/deeper/file.bin"), b"nested")?;

        let backend_id = BackendId::new("local")?;
        let backend = LocalBackend::new(backend_id.clone());
        let spec = spec(&backend_id, &source, &destination)?;
        let token = CancellationToken::new();

        let outcome = execute_tree_controlled(&spec, &backend, &backend, &token, |_| {})?;
        assert!(matches!(
            outcome,
            TreeTransferOutcome::Completed(report)
                if report.files_copied() == 2 && report.directories_created() == 3
        ));
        assert_eq!(fs::read(destination.join("root.txt"))?, b"root");
        assert_eq!(
            fs::read(destination.join("nested/deeper/file.bin"))?,
            b"nested"
        );

        fs::remove_dir_all(&root)?;
        Ok(())
    }

    #[test]
    fn cancelled_tree_removes_every_created_object() -> Result<(), Box<dyn std::error::Error>> {
        let root = test_directory();
        let source = root.join("source");
        let destination = root.join("destination");
        fs::create_dir_all(source.join("nested"))?;
        fs::write(source.join("nested/a.bin"), vec![0x55; 256 * 1024])?;
        fs::write(source.join("nested/b.bin"), vec![0x66; 256 * 1024])?;

        let backend_id = BackendId::new("local")?;
        let backend = LocalBackend::new(backend_id.clone());
        let spec = spec(&backend_id, &source, &destination)?;
        let token = CancellationToken::new();
        let cancel_from_progress = token.clone();

        let outcome =
            execute_tree_controlled(&spec, &backend, &backend, &token, move |progress| {
                if progress.bytes_copied() > 0 {
                    cancel_from_progress.cancel();
                }
            })?;
        assert!(matches!(outcome, TreeTransferOutcome::Cancelled(_)));
        assert!(!destination.exists());

        fs::remove_dir_all(&root)?;
        Ok(())
    }
}
