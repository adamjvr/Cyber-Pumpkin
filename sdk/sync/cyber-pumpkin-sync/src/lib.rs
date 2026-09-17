//! Synchronization execution for immutable Cyber-Pumpkin sync plans.
//!
//! The planner and executor are deliberately separate. A UI can preview a
//! [`cyber_pumpkin_sync_plan::SyncPlan`] and then execute that exact plan
//! without re-planning or silently changing the requested work.

use cyber_pumpkin_backend::{Backend, BackendError, ErrorKind};
use cyber_pumpkin_core::BackendPath;
use cyber_pumpkin_reliability::{
    ReliabilityError, ReliableTransferOutcome, ReliableTreeOutcome, execute_file_reliable,
    execute_tree_reliable,
};
use cyber_pumpkin_sync_plan::{SyncAction, SyncActionKind, SyncPlan};
use cyber_pumpkin_transfer::{
    CancellationToken, Endpoint, ExecutionError, TransferId, TransferSpec,
};
use std::fmt;

/// Snapshot emitted while executing a synchronization plan.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SyncExecutionProgress {
    completed_actions: u64,
    total_actions: u64,
    current_path: String,
    current_bytes: u64,
    current_total_bytes: Option<u64>,
}

impl SyncExecutionProgress {
    /// Number of fully completed plan actions.
    #[must_use]
    pub const fn completed_actions(&self) -> u64 {
        self.completed_actions
    }

    /// Total number of actions in the immutable plan.
    #[must_use]
    pub const fn total_actions(&self) -> u64 {
        self.total_actions
    }

    /// Relative path currently being processed.
    #[must_use]
    pub fn current_path(&self) -> &str {
        &self.current_path
    }

    /// Bytes copied for the currently executing file.
    #[must_use]
    pub const fn current_bytes(&self) -> u64 {
        self.current_bytes
    }

    /// Total size of the current file when known.
    #[must_use]
    pub const fn current_total_bytes(&self) -> Option<u64> {
        self.current_total_bytes
    }

    /// Action-level progress in thousandths.
    #[must_use]
    pub fn permille(&self) -> Option<u16> {
        if self.total_actions == 0 {
            return Some(1000);
        }
        let scaled = self
            .completed_actions
            .min(self.total_actions)
            .saturating_mul(1000)
            / self.total_actions;
        u16::try_from(scaled).ok()
    }
}

/// Summary of work performed by a synchronization execution.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SyncExecutionReport {
    /// Files copied or replaced.
    pub files_copied: u64,
    /// Directories created.
    pub directories_created: u64,
    /// Destination entries removed.
    pub entries_removed: u64,
    /// Plan actions intentionally skipped.
    pub skipped: u64,
    /// Payload bytes copied when reported by the transfer layer.
    pub bytes_copied: u64,
}

/// Terminal execution result.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SyncExecutionOutcome {
    /// Every executable plan action completed.
    Completed(SyncExecutionReport),
    /// Cooperative cancellation was observed.
    Cancelled(SyncExecutionReport),
}

/// Policy used when an immutable sync plan contains type conflicts.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ConflictPolicy {
    /// Refuse execution until the caller resolves conflicts.
    #[default]
    Fail,
    /// Preserve the destination object and count the conflict as skipped.
    Skip,
    /// Replace the conflicting destination tree with the source object.
    Replace,
}

/// Synchronization execution failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SyncExecutionError {
    /// A backend operation failed.
    Backend(BackendError),
    /// A transfer operation failed.
    Transfer(ExecutionError),
    /// Safe staging or finalization failed.
    Reliability(ReliabilityError),
    /// The immutable plan contains a conflict that requires a decision.
    Conflict {
        /// Relative path requiring a decision.
        path: String,
    },
    /// A plan action was missing a source or destination endpoint.
    MissingEndpoint {
        /// Relative path of the malformed action.
        path: String,
        /// Endpoint that was required.
        endpoint: &'static str,
    },
    /// The plan contains more actions than the execution counter can represent.
    TooManyActions,
}

impl fmt::Display for SyncExecutionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Backend(error) => write!(formatter, "sync backend operation failed: {error}"),
            Self::Transfer(error) => write!(formatter, "sync transfer failed: {error}"),
            Self::Reliability(error) => write!(formatter, "sync reliable transfer failed: {error}"),
            Self::Conflict { path } => {
                write!(formatter, "sync plan contains unresolved conflict: {path}")
            }
            Self::MissingEndpoint { path, endpoint } => write!(
                formatter,
                "sync action {path} is missing its {endpoint} endpoint"
            ),
            Self::TooManyActions => formatter.write_str("sync plan contains too many actions"),
        }
    }
}

impl std::error::Error for SyncExecutionError {}
impl From<BackendError> for SyncExecutionError {
    fn from(value: BackendError) -> Self {
        Self::Backend(value)
    }
}
impl From<ExecutionError> for SyncExecutionError {
    fn from(value: ExecutionError) -> Self {
        Self::Transfer(value)
    }
}
impl From<ReliabilityError> for SyncExecutionError {
    fn from(value: ReliabilityError) -> Self {
        Self::Reliability(value)
    }
}

/// Executes an immutable synchronization plan.
///
/// Cancellation is checked between every plan action and throughout staged
/// file transfer. File copies and replacements use the reliability layer's
/// stage, verify, backup, atomic-finalize transaction.
///
/// # Errors
///
/// Returns [`SyncExecutionError`] for malformed plans, unresolved conflicts,
/// backend failures, or transfer failures.
pub fn execute_plan<F>(
    plan: &SyncPlan,
    source: &dyn Backend,
    destination: &dyn Backend,
    cancellation: &CancellationToken,
    on_progress: F,
) -> Result<SyncExecutionOutcome, SyncExecutionError>
where
    F: FnMut(SyncExecutionProgress),
{
    execute_plan_with_conflicts(
        plan,
        source,
        destination,
        cancellation,
        ConflictPolicy::Fail,
        on_progress,
    )
}

/// Executes an immutable plan with an explicit type-conflict policy.
///
/// # Errors
///
/// Returns [`SyncExecutionError`] for malformed plans, backend failures,
/// transfer failures, or conflicts when [`ConflictPolicy::Fail`] is selected.
pub fn execute_plan_with_conflicts<F>(
    plan: &SyncPlan,
    source: &dyn Backend,
    destination: &dyn Backend,
    cancellation: &CancellationToken,
    conflict_policy: ConflictPolicy,
    mut on_progress: F,
) -> Result<SyncExecutionOutcome, SyncExecutionError>
where
    F: FnMut(SyncExecutionProgress),
{
    let total_actions =
        u64::try_from(plan.actions().len()).map_err(|_| SyncExecutionError::TooManyActions)?;
    let mut report = SyncExecutionReport::default();

    for (index, action) in plan.actions().iter().enumerate() {
        if cancellation.is_cancelled() {
            return Ok(SyncExecutionOutcome::Cancelled(report));
        }

        let completed_actions =
            u64::try_from(index).map_err(|_| SyncExecutionError::TooManyActions)?;
        let outcome = execute_action(
            action,
            source,
            destination,
            cancellation,
            completed_actions,
            total_actions,
            conflict_policy,
            &mut report,
            &mut on_progress,
        )?;

        if outcome == ActionOutcome::Cancelled {
            return Ok(SyncExecutionOutcome::Cancelled(report));
        }

        on_progress(SyncExecutionProgress {
            completed_actions: completed_actions.saturating_add(1),
            total_actions,
            current_path: action.relative_path.clone(),
            current_bytes: 0,
            current_total_bytes: None,
        });
    }

    Ok(SyncExecutionOutcome::Completed(report))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ActionOutcome {
    Completed,
    Cancelled,
}

#[allow(clippy::too_many_arguments)]
fn execute_action<F>(
    action: &SyncAction,
    source: &dyn Backend,
    destination: &dyn Backend,
    cancellation: &CancellationToken,
    completed_actions: u64,
    total_actions: u64,
    conflict_policy: ConflictPolicy,
    report: &mut SyncExecutionReport,
    on_progress: &mut F,
) -> Result<ActionOutcome, SyncExecutionError>
where
    F: FnMut(SyncExecutionProgress),
{
    match action.kind {
        SyncActionKind::CreateDirectory => {
            destination.create_dir(required_destination(action)?)?;
            report.directories_created = report.directories_created.saturating_add(1);
            Ok(ActionOutcome::Completed)
        }
        SyncActionKind::CopyFile => execute_copy(
            action,
            source,
            destination,
            cancellation,
            completed_actions,
            total_actions,
            report,
            on_progress,
        ),
        SyncActionKind::RemoveFile | SyncActionKind::RemoveDirectory => {
            if remove_if_present(destination, required_destination(action)?)? {
                report.entries_removed = report.entries_removed.saturating_add(1);
            }
            Ok(ActionOutcome::Completed)
        }
        SyncActionKind::Conflict => execute_conflict(
            action,
            source,
            destination,
            cancellation,
            completed_actions,
            total_actions,
            conflict_policy,
            report,
            on_progress,
        ),
        SyncActionKind::Skip => {
            report.skipped = report.skipped.saturating_add(1);
            Ok(ActionOutcome::Completed)
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn execute_copy<F>(
    action: &SyncAction,
    source: &dyn Backend,
    destination: &dyn Backend,
    cancellation: &CancellationToken,
    completed_actions: u64,
    total_actions: u64,
    report: &mut SyncExecutionReport,
    on_progress: &mut F,
) -> Result<ActionOutcome, SyncExecutionError>
where
    F: FnMut(SyncExecutionProgress),
{
    let source_path = required_source(action)?.clone();
    let destination_path = required_destination(action)?.clone();
    let transfer_id =
        TransferId::new(completed_actions.saturating_add(1)).map_err(ExecutionError::from)?;
    let source_endpoint = Endpoint {
        backend: source.id().clone(),
        path: source_path,
    };
    let destination_endpoint = Endpoint {
        backend: destination.id().clone(),
        path: destination_path,
    };

    let outcome = execute_file_reliable(
        transfer_id,
        source_endpoint,
        destination_endpoint,
        source,
        destination,
        cancellation,
        |progress| {
            on_progress(SyncExecutionProgress {
                completed_actions,
                total_actions,
                current_path: action.relative_path.clone(),
                current_bytes: progress.bytes_copied(),
                current_total_bytes: progress.total_bytes(),
            });
        },
    )?;

    match outcome {
        ReliableTransferOutcome::Completed(file_report) => {
            report.files_copied = report.files_copied.saturating_add(1);
            report.bytes_copied = report
                .bytes_copied
                .saturating_add(file_report.bytes_copied());
            Ok(ActionOutcome::Completed)
        }
        ReliableTransferOutcome::Cancelled { bytes_copied } => {
            report.bytes_copied = report.bytes_copied.saturating_add(bytes_copied);
            Ok(ActionOutcome::Cancelled)
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn execute_conflict<F>(
    action: &SyncAction,
    source: &dyn Backend,
    destination: &dyn Backend,
    cancellation: &CancellationToken,
    completed_actions: u64,
    total_actions: u64,
    conflict_policy: ConflictPolicy,
    report: &mut SyncExecutionReport,
    on_progress: &mut F,
) -> Result<ActionOutcome, SyncExecutionError>
where
    F: FnMut(SyncExecutionProgress),
{
    match conflict_policy {
        ConflictPolicy::Fail => Err(SyncExecutionError::Conflict {
            path: action.relative_path.clone(),
        }),
        ConflictPolicy::Skip => {
            report.skipped = report.skipped.saturating_add(1);
            Ok(ActionOutcome::Completed)
        }
        ConflictPolicy::Replace => replace_conflict(
            action,
            source,
            destination,
            cancellation,
            completed_actions,
            total_actions,
            report,
            on_progress,
        ),
    }
}

#[allow(clippy::too_many_arguments)]
fn replace_conflict<F>(
    action: &SyncAction,
    source: &dyn Backend,
    destination: &dyn Backend,
    cancellation: &CancellationToken,
    completed_actions: u64,
    total_actions: u64,
    report: &mut SyncExecutionReport,
    on_progress: &mut F,
) -> Result<ActionOutcome, SyncExecutionError>
where
    F: FnMut(SyncExecutionProgress),
{
    let source_path = required_source(action)?.clone();
    let destination_path = required_destination(action)?.clone();
    let transfer_id =
        TransferId::new(completed_actions.saturating_add(1)).map_err(ExecutionError::from)?;
    let spec = TransferSpec {
        source: Endpoint {
            backend: source.id().clone(),
            path: source_path,
        },
        destination: Endpoint {
            backend: destination.id().clone(),
            path: destination_path,
        },
    };
    let outcome = execute_tree_reliable(
        transfer_id,
        &spec,
        source,
        destination,
        cancellation,
        true,
        |progress| {
            on_progress(SyncExecutionProgress {
                completed_actions,
                total_actions,
                current_path: action.relative_path.clone(),
                current_bytes: progress.bytes_copied(),
                current_total_bytes: progress.total_bytes(),
            });
        },
    )?;
    match outcome {
        ReliableTreeOutcome::Completed(tree_report) => {
            report.files_copied = report
                .files_copied
                .saturating_add(tree_report.transfer.files_copied());
            report.directories_created = report
                .directories_created
                .saturating_add(tree_report.transfer.directories_created());
            report.bytes_copied = report
                .bytes_copied
                .saturating_add(tree_report.transfer.bytes_copied());
            if tree_report.replaced_existing {
                report.entries_removed = report.entries_removed.saturating_add(1);
            }
            Ok(ActionOutcome::Completed)
        }
        ReliableTreeOutcome::Cancelled(tree_report) => {
            report.bytes_copied = report
                .bytes_copied
                .saturating_add(tree_report.bytes_copied());
            Ok(ActionOutcome::Cancelled)
        }
    }
}

fn remove_if_present(
    backend: &dyn Backend,
    path: &BackendPath,
) -> Result<bool, SyncExecutionError> {
    match backend.remove(path) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

fn required_source(action: &SyncAction) -> Result<&BackendPath, SyncExecutionError> {
    action
        .source
        .as_ref()
        .ok_or_else(|| SyncExecutionError::MissingEndpoint {
            path: action.relative_path.clone(),
            endpoint: "source",
        })
}

fn required_destination(action: &SyncAction) -> Result<&BackendPath, SyncExecutionError> {
    action
        .destination
        .as_ref()
        .ok_or_else(|| SyncExecutionError::MissingEndpoint {
            path: action.relative_path.clone(),
            endpoint: "destination",
        })
}

#[cfg(test)]
mod tests {
    use super::{SyncExecutionOutcome, execute_plan};
    use cyber_pumpkin_core::{BackendId, BackendPath};
    use cyber_pumpkin_local::LocalBackend;
    use cyber_pumpkin_sync_plan::{SyncOptions, plan_one_way};
    use cyber_pumpkin_transfer::CancellationToken;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(1);

    fn temp_root() -> PathBuf {
        let serial = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "cyber-pumpkin-sync-execute-{}-{serial}",
            std::process::id()
        ))
    }

    fn backend_path(path: &Path) -> Result<BackendPath, Box<dyn std::error::Error>> {
        Ok(BackendPath::new(path.to_string_lossy().into_owned())?)
    }

    #[test]
    fn executes_planned_local_sync() -> Result<(), Box<dyn std::error::Error>> {
        let root = temp_root();
        let source_root = root.join("source");
        let destination_root = root.join("destination");
        fs::create_dir_all(source_root.join("nested"))?;
        fs::create_dir_all(&destination_root)?;
        fs::write(source_root.join("nested/a.txt"), b"alpha")?;
        fs::write(destination_root.join("orphan.txt"), b"remove")?;

        let source = LocalBackend::new(BackendId::new("source")?);
        let destination = LocalBackend::new(BackendId::new("destination")?);
        let source_path = backend_path(&source_root)?;
        let destination_path = backend_path(&destination_root)?;
        let plan = plan_one_way(
            &source,
            &source_path,
            &destination,
            &destination_path,
            &SyncOptions {
                delete_orphans: true,
                ..SyncOptions::default()
            },
        )?;

        let outcome = execute_plan(
            &plan,
            &source,
            &destination,
            &CancellationToken::new(),
            |_| {},
        )?;
        assert!(matches!(outcome, SyncExecutionOutcome::Completed(_)));
        assert_eq!(fs::read(destination_root.join("nested/a.txt"))?, b"alpha");
        assert!(!destination_root.join("orphan.txt").exists());
        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn replaces_existing_file_through_reliability_layer() -> Result<(), Box<dyn std::error::Error>>
    {
        let root = temp_root();
        let source_root = root.join("source");
        let destination_root = root.join("destination");
        fs::create_dir_all(&source_root)?;
        fs::create_dir_all(&destination_root)?;
        fs::write(source_root.join("a.txt"), b"new content")?;
        fs::write(destination_root.join("a.txt"), b"old")?;

        let source = LocalBackend::new(BackendId::new("source")?);
        let destination = LocalBackend::new(BackendId::new("destination")?);
        let plan = plan_one_way(
            &source,
            &backend_path(&source_root)?,
            &destination,
            &backend_path(&destination_root)?,
            &SyncOptions::default(),
        )?;

        let outcome = execute_plan(
            &plan,
            &source,
            &destination,
            &CancellationToken::new(),
            |_| {},
        )?;
        assert!(matches!(outcome, SyncExecutionOutcome::Completed(_)));
        assert_eq!(fs::read(destination_root.join("a.txt"))?, b"new content");
        assert_eq!(fs::read_dir(&destination_root)?.count(), 1);

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn cancellation_before_execution_changes_nothing() -> Result<(), Box<dyn std::error::Error>> {
        let root = temp_root();
        let source_root = root.join("source");
        let destination_root = root.join("destination");
        fs::create_dir_all(&source_root)?;
        fs::create_dir_all(&destination_root)?;
        fs::write(source_root.join("a.txt"), b"alpha")?;

        let source = LocalBackend::new(BackendId::new("source")?);
        let destination = LocalBackend::new(BackendId::new("destination")?);
        let plan = plan_one_way(
            &source,
            &backend_path(&source_root)?,
            &destination,
            &backend_path(&destination_root)?,
            &SyncOptions::default(),
        )?;

        let cancellation = CancellationToken::new();
        cancellation.cancel();
        let outcome = execute_plan(&plan, &source, &destination, &cancellation, |_| {})?;
        assert!(matches!(outcome, SyncExecutionOutcome::Cancelled(_)));
        assert!(!destination_root.join("a.txt").exists());
        fs::remove_dir_all(root)?;
        Ok(())
    }
    #[test]
    fn replace_policy_resolves_file_over_directory_conflict()
    -> Result<(), Box<dyn std::error::Error>> {
        use super::{ConflictPolicy, execute_plan_with_conflicts};

        let root = temp_root();
        let source_root = root.join("source");
        let destination_root = root.join("destination");
        fs::create_dir_all(&source_root)?;
        fs::create_dir_all(destination_root.join("node/nested"))?;
        fs::write(source_root.join("node"), b"replacement-file")?;
        fs::write(destination_root.join("node/nested/old.txt"), b"old")?;

        let source = LocalBackend::new(BackendId::new("source")?);
        let destination = LocalBackend::new(BackendId::new("destination")?);
        let plan = plan_one_way(
            &source,
            &backend_path(&source_root)?,
            &destination,
            &backend_path(&destination_root)?,
            &SyncOptions::default(),
        )?;
        assert_eq!(plan.summary().conflicts, 1);

        let outcome = execute_plan_with_conflicts(
            &plan,
            &source,
            &destination,
            &CancellationToken::new(),
            ConflictPolicy::Replace,
            |_| {},
        )?;
        assert!(matches!(outcome, SyncExecutionOutcome::Completed(_)));
        assert_eq!(
            fs::read(destination_root.join("node"))?,
            b"replacement-file"
        );

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn skip_policy_preserves_conflicting_destination() -> Result<(), Box<dyn std::error::Error>> {
        use super::{ConflictPolicy, execute_plan_with_conflicts};

        let root = temp_root();
        let source_root = root.join("source");
        let destination_root = root.join("destination");
        fs::create_dir_all(&source_root)?;
        fs::create_dir_all(destination_root.join("node"))?;
        fs::write(source_root.join("node"), b"replacement-file")?;
        fs::write(destination_root.join("node/keep.txt"), b"keep")?;

        let source = LocalBackend::new(BackendId::new("source")?);
        let destination = LocalBackend::new(BackendId::new("destination")?);
        let plan = plan_one_way(
            &source,
            &backend_path(&source_root)?,
            &destination,
            &backend_path(&destination_root)?,
            &SyncOptions::default(),
        )?;

        let outcome = execute_plan_with_conflicts(
            &plan,
            &source,
            &destination,
            &CancellationToken::new(),
            ConflictPolicy::Skip,
            |_| {},
        )?;
        assert!(matches!(outcome, SyncExecutionOutcome::Completed(_)));
        assert_eq!(fs::read(destination_root.join("node/keep.txt"))?, b"keep");

        fs::remove_dir_all(root)?;
        Ok(())
    }
}
