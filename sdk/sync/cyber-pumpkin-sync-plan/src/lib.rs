//! Deterministic backend-neutral synchronization planning.

use cyber_pumpkin_backend::{Backend, BackendError};
use cyber_pumpkin_core::{BackendPath, EntryKind, FileEntry};
use cyber_pumpkin_rules::{RuleDecision, RuleSet, RuleTarget, evaluate};
use std::collections::BTreeMap;
use std::fmt;

/// Planner options.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SyncOptions {
    /// Remove destination-only entries.
    pub delete_orphans: bool,
    /// Request symbolic-link traversal.
    pub follow_symlinks: bool,
    /// Ordered rules.
    pub rules: Vec<RuleSet>,
}

/// Planned action.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SyncActionKind {
    /// Create a destination directory.
    CreateDirectory,
    /// Copy a source file to the destination.
    CopyFile,
    /// Remove a destination file or symbolic link.
    RemoveFile,
    /// Remove a destination directory after its children.
    RemoveDirectory,
    /// Source and destination entry kinds disagree.
    Conflict,
    /// Entry is unsupported or intentionally skipped.
    Skip,
}

/// One sync action.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SyncAction {
    /// Action category.
    pub kind: SyncActionKind,
    /// Slash-separated path relative to both sync roots.
    pub relative_path: String,
    /// Source path when the action reads a source object.
    pub source: Option<BackendPath>,
    /// Destination path when the action targets a destination object.
    pub destination: Option<BackendPath>,
    /// Human-readable explanation used by preview UIs.
    pub reason: String,
}

/// Plan summary.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SyncSummary {
    /// Directories to create.
    pub directories_to_create: u64,
    /// Files to copy.
    pub files_to_copy: u64,
    /// Destination entries to remove.
    pub entries_to_remove: u64,
    /// Conflicts requiring a decision.
    pub conflicts: u64,
    /// Unsupported or intentionally skipped entries.
    pub skipped: u64,
}

/// Immutable plan.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SyncPlan {
    actions: Vec<SyncAction>,
    summary: SyncSummary,
}

impl SyncPlan {
    /// Returns actions in safe deterministic order.
    #[must_use]
    pub fn actions(&self) -> &[SyncAction] {
        &self.actions
    }

    /// Returns summary.
    #[must_use]
    pub const fn summary(&self) -> SyncSummary {
        self.summary
    }
}

/// Planning error.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SyncPlanError {
    /// Backend enumeration or metadata failed.
    Backend(BackendError),
    /// A sync root is not a directory.
    RootNotDirectory {
        /// Invalid root path.
        path: BackendPath,
        /// Reported entry kind.
        kind: EntryKind,
    },
    /// A child path could not be represented relative to its root.
    PathOutsideRoot {
        /// Sync root.
        root: BackendPath,
        /// Child path.
        child: BackendPath,
    },
}

impl fmt::Display for SyncPlanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Backend(error) => write!(f, "backend failed during sync planning: {error}"),
            Self::RootNotDirectory { path, kind } => write!(
                f,
                "sync root {} is not a directory: {kind:?}",
                path.as_str()
            ),
            Self::PathOutsideRoot { root, child } => write!(
                f,
                "path {} is outside sync root {}",
                child.as_str(),
                root.as_str()
            ),
        }
    }
}
impl std::error::Error for SyncPlanError {}
impl From<BackendError> for SyncPlanError {
    fn from(value: BackendError) -> Self {
        Self::Backend(value)
    }
}

/// Plans a one-way source-to-destination sync.
///
/// # Errors
///
/// Returns [`SyncPlanError`] when roots or enumeration fail.
pub fn plan_one_way(
    source: &dyn Backend,
    source_root: &BackendPath,
    destination: &dyn Backend,
    destination_root: &BackendPath,
    options: &SyncOptions,
) -> Result<SyncPlan, SyncPlanError> {
    ensure_directory(source, source_root)?;
    ensure_directory(destination, destination_root)?;

    let source_snapshot = snapshot(source, source_root, options)?;
    let destination_snapshot = snapshot(destination, destination_root, &SyncOptions::default())?;

    let mut buckets =
        plan_source_actions(&source_snapshot, &destination_snapshot, destination_root)?;
    sort_source_actions(&mut buckets);

    let removals = plan_removals(
        &source_snapshot,
        &destination_snapshot,
        options.delete_orphans,
    );

    let actions = combine_actions(buckets, removals);
    let summary = summarize(&actions);

    Ok(SyncPlan { actions, summary })
}

#[derive(Default)]
struct ActionBuckets {
    create_dirs: Vec<SyncAction>,
    copy_files: Vec<SyncAction>,
    conflicts: Vec<SyncAction>,
    skipped: Vec<SyncAction>,
}

fn plan_source_actions(
    source_snapshot: &BTreeMap<String, FileEntry>,
    destination_snapshot: &BTreeMap<String, FileEntry>,
    destination_root: &BackendPath,
) -> Result<ActionBuckets, SyncPlanError> {
    let mut buckets = ActionBuckets::default();

    for (relative, source_entry) in source_snapshot {
        let destination_path = join(destination_root, relative)?;

        match destination_snapshot.get(relative) {
            None => {
                push_missing_source_action(&mut buckets, relative, source_entry, destination_path);
            }
            Some(destination_entry) if destination_entry.kind != source_entry.kind => {
                buckets.conflicts.push(action(
                    SyncActionKind::Conflict,
                    relative,
                    Some(source_entry.path.clone()),
                    Some(destination_entry.path.clone()),
                    "source and destination entry kinds differ",
                ));
            }
            Some(destination_entry)
                if source_entry.kind == EntryKind::File
                    && file_differs(source_entry, destination_entry) =>
            {
                buckets.copy_files.push(action(
                    SyncActionKind::CopyFile,
                    relative,
                    Some(source_entry.path.clone()),
                    Some(destination_entry.path.clone()),
                    "source and destination metadata differ",
                ));
            }
            Some(_) if matches!(source_entry.kind, EntryKind::Symlink | EntryKind::Other) => {
                buckets.skipped.push(action(
                    SyncActionKind::Skip,
                    relative,
                    Some(source_entry.path.clone()),
                    Some(destination_path),
                    "entry type is not representable by the current backend contract",
                ));
            }
            Some(_) => {}
        }
    }

    Ok(buckets)
}

fn push_missing_source_action(
    buckets: &mut ActionBuckets,
    relative: &str,
    source_entry: &FileEntry,
    destination_path: BackendPath,
) {
    let target = match source_entry.kind {
        EntryKind::Directory => &mut buckets.create_dirs,
        EntryKind::File => &mut buckets.copy_files,
        EntryKind::Symlink | EntryKind::Other => &mut buckets.skipped,
    };

    let (kind, reason) = match source_entry.kind {
        EntryKind::Directory => (
            SyncActionKind::CreateDirectory,
            "destination directory is missing",
        ),
        EntryKind::File => (SyncActionKind::CopyFile, "destination file is missing"),
        EntryKind::Symlink | EntryKind::Other => (
            SyncActionKind::Skip,
            "entry type is not representable by the current backend contract",
        ),
    };

    target.push(action(
        kind,
        relative,
        Some(source_entry.path.clone()),
        Some(destination_path),
        reason,
    ));
}

fn sort_source_actions(buckets: &mut ActionBuckets) {
    buckets
        .create_dirs
        .sort_by_key(|item| depth(&item.relative_path));
    buckets
        .copy_files
        .sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    buckets
        .conflicts
        .sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    buckets
        .skipped
        .sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
}

fn plan_removals(
    source_snapshot: &BTreeMap<String, FileEntry>,
    destination_snapshot: &BTreeMap<String, FileEntry>,
    delete_orphans: bool,
) -> Vec<SyncAction> {
    if !delete_orphans {
        return Vec::new();
    }

    let mut removals = destination_snapshot
        .iter()
        .filter(|(relative, _)| !source_snapshot.contains_key(*relative))
        .map(|(relative, destination_entry)| {
            let kind = if destination_entry.kind == EntryKind::Directory {
                SyncActionKind::RemoveDirectory
            } else {
                SyncActionKind::RemoveFile
            };

            action(
                kind,
                relative,
                None,
                Some(destination_entry.path.clone()),
                "destination entry is absent from source",
            )
        })
        .collect::<Vec<_>>();

    removals.sort_by(|left, right| {
        depth(&right.relative_path)
            .cmp(&depth(&left.relative_path))
            .then_with(|| left.relative_path.cmp(&right.relative_path))
    });
    removals
}

fn combine_actions(buckets: ActionBuckets, removals: Vec<SyncAction>) -> Vec<SyncAction> {
    let mut actions = Vec::with_capacity(
        buckets.create_dirs.len()
            + buckets.copy_files.len()
            + buckets.conflicts.len()
            + buckets.skipped.len()
            + removals.len(),
    );
    actions.extend(buckets.create_dirs);
    actions.extend(buckets.copy_files);
    actions.extend(buckets.conflicts);
    actions.extend(buckets.skipped);
    actions.extend(removals);
    actions
}

fn summarize(actions: &[SyncAction]) -> SyncSummary {
    let mut summary = SyncSummary::default();

    for item in actions {
        match item.kind {
            SyncActionKind::CreateDirectory => {
                summary.directories_to_create += 1;
            }
            SyncActionKind::CopyFile => {
                summary.files_to_copy += 1;
            }
            SyncActionKind::RemoveFile | SyncActionKind::RemoveDirectory => {
                summary.entries_to_remove += 1;
            }
            SyncActionKind::Conflict => {
                summary.conflicts += 1;
            }
            SyncActionKind::Skip => {
                summary.skipped += 1;
            }
        }
    }

    summary
}

fn snapshot(
    backend: &dyn Backend,
    root: &BackendPath,
    options: &SyncOptions,
) -> Result<BTreeMap<String, FileEntry>, SyncPlanError> {
    let mut entries = BTreeMap::new();
    visit_directory(backend, root, root, options, &mut entries)?;
    Ok(entries)
}

fn visit_directory(
    backend: &dyn Backend,
    root: &BackendPath,
    directory: &BackendPath,
    options: &SyncOptions,
    entries: &mut BTreeMap<String, FileEntry>,
) -> Result<(), SyncPlanError> {
    for entry in backend.list(directory)? {
        let relative = relative_path(root, &entry.path)?;
        if evaluate(
            &options.rules,
            RuleTarget {
                name: &entry.name,
                path: &relative,
                kind: entry.kind,
            },
        ) == RuleDecision::Skip
        {
            continue;
        }
        let recurse = entry.kind == EntryKind::Directory;
        let path = entry.path.clone();
        entries.insert(relative, entry);
        if recurse {
            visit_directory(backend, root, &path, options, entries)?;
        }
    }
    Ok(())
}

fn ensure_directory(backend: &dyn Backend, root: &BackendPath) -> Result<(), SyncPlanError> {
    let entry = backend.stat(root)?;
    if entry.kind == EntryKind::Directory {
        Ok(())
    } else {
        Err(SyncPlanError::RootNotDirectory {
            path: root.clone(),
            kind: entry.kind,
        })
    }
}

fn file_differs(source: &FileEntry, destination: &FileEntry) -> bool {
    source.size != destination.size || source.modified != destination.modified
}

fn relative_path(root: &BackendPath, child: &BackendPath) -> Result<String, SyncPlanError> {
    let root_text = root.as_str().trim_end_matches('/');
    let child_text = child.as_str();
    if child_text == root_text {
        return Ok(String::new());
    }
    let prefix = format!("{root_text}/");
    child_text
        .strip_prefix(&prefix)
        .map(str::to_owned)
        .ok_or_else(|| SyncPlanError::PathOutsideRoot {
            root: root.clone(),
            child: child.clone(),
        })
}

fn join(root: &BackendPath, relative: &str) -> Result<BackendPath, SyncPlanError> {
    let root_text = root.as_str().trim_end_matches('/');
    let text = if root_text.is_empty() {
        format!("/{relative}")
    } else {
        format!("{root_text}/{relative}")
    };
    BackendPath::new(text).map_err(|_| SyncPlanError::PathOutsideRoot {
        root: root.clone(),
        child: root.clone(),
    })
}

fn action(
    kind: SyncActionKind,
    relative_path: &str,
    source: Option<BackendPath>,
    destination: Option<BackendPath>,
    reason: &str,
) -> SyncAction {
    SyncAction {
        kind,
        relative_path: relative_path.to_owned(),
        source,
        destination,
        reason: reason.to_owned(),
    }
}

fn depth(path: &str) -> usize {
    path.split('/').filter(|part| !part.is_empty()).count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use cyber_pumpkin_core::{BackendId, BackendPath};
    use cyber_pumpkin_local::LocalBackend;
    use std::{
        fs,
        path::{Path, PathBuf},
        sync::atomic::{AtomicU64, Ordering},
    };
    static NEXT: AtomicU64 = AtomicU64::new(1);

    fn temp_root() -> PathBuf {
        let serial = NEXT.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "cyber-pumpkin-sync-plan-{}-{serial}",
            std::process::id()
        ))
    }
    fn bp(path: &Path) -> Result<BackendPath, Box<dyn std::error::Error>> {
        Ok(BackendPath::new(path.to_string_lossy().into_owned())?)
    }

    #[test]
    fn plans_missing_tree_and_orphan_removal() -> Result<(), Box<dyn std::error::Error>> {
        let root = temp_root();
        let source = root.join("source");
        let destination = root.join("destination");
        fs::create_dir_all(source.join("nested"))?;
        fs::create_dir_all(&destination)?;
        fs::write(source.join("nested/file.txt"), b"source")?;
        fs::write(destination.join("orphan.txt"), b"orphan")?;

        let source_backend = LocalBackend::new(BackendId::new("source")?);
        let destination_backend = LocalBackend::new(BackendId::new("destination")?);
        let plan = plan_one_way(
            &source_backend,
            &bp(&source)?,
            &destination_backend,
            &bp(&destination)?,
            &SyncOptions {
                delete_orphans: true,
                ..SyncOptions::default()
            },
        )?;
        assert!(
            plan.actions()
                .iter()
                .any(|a| a.kind == SyncActionKind::CreateDirectory && a.relative_path == "nested")
        );
        assert!(plan.actions().iter().any(|a| a.kind == SyncActionKind::CopyFile && a.relative_path == "nested/file.txt"));
        assert!(
            plan.actions()
                .iter()
                .any(|a| a.kind == SyncActionKind::RemoveFile && a.relative_path == "orphan.txt")
        );
        fs::remove_dir_all(root)?;
        Ok(())
    }
}
