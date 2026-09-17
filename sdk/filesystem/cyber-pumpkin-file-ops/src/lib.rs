//! Backend-neutral higher-level filesystem operations.

use cyber_pumpkin_backend::{Backend, BackendError, ErrorKind};
use cyber_pumpkin_core::{BackendPath, EntryKind};

/// Recursive delete report.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RemoveTreeReport {
    /// Files, links, or non-directory entries removed.
    pub files_removed: u64,
    /// Directories removed, including the requested root when it is a directory.
    pub directories_removed: u64,
}

impl RemoveTreeReport {
    /// Returns total removed entries.
    #[must_use]
    pub const fn entries_removed(self) -> u64 {
        self.files_removed.saturating_add(self.directories_removed)
    }
}

/// Removes one file or a complete directory tree.
///
/// Children are removed depth-first so the backend only needs an empty-directory
/// remove primitive.
///
/// # Errors
///
/// Returns [`BackendError`] if metadata, enumeration, or removal fails.
pub fn remove_tree(
    backend: &dyn Backend,
    path: &BackendPath,
) -> Result<RemoveTreeReport, BackendError> {
    let entry = backend.stat(path)?;
    if entry.kind != EntryKind::Directory {
        backend.remove(path)?;
        return Ok(RemoveTreeReport {
            files_removed: 1,
            directories_removed: 0,
        });
    }

    let mut report = RemoveTreeReport::default();
    for child in backend.list(path)? {
        let child_report = remove_tree(backend, &child.path)?;
        report.files_removed = report
            .files_removed
            .saturating_add(child_report.files_removed);
        report.directories_removed = report
            .directories_removed
            .saturating_add(child_report.directories_removed);
    }
    backend.remove(path)?;
    report.directories_removed = report.directories_removed.saturating_add(1);
    Ok(report)
}

/// Removes a tree if present.
///
/// # Errors
///
/// Returns [`BackendError`] for failures other than a missing root.
pub fn remove_tree_if_present(
    backend: &dyn Backend,
    path: &BackendPath,
) -> Result<RemoveTreeReport, BackendError> {
    match backend.stat(path) {
        Ok(_) => remove_tree(backend, path),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(RemoveTreeReport::default()),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::remove_tree;
    use cyber_pumpkin_core::{BackendId, BackendPath};
    use cyber_pumpkin_local::LocalBackend;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(1);

    fn root() -> PathBuf {
        let serial = NEXT.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "cyber-pumpkin-file-ops-{}-{serial}",
            std::process::id()
        ))
    }

    #[test]
    fn recursively_removes_non_empty_tree() -> Result<(), Box<dyn std::error::Error>> {
        let root = root();
        let target = root.join("target");
        fs::create_dir_all(target.join("nested/deeper"))?;
        fs::write(target.join("a.txt"), b"a")?;
        fs::write(target.join("nested/deeper/b.txt"), b"b")?;

        let backend = LocalBackend::new(BackendId::new("local")?);
        let path = BackendPath::new(target.to_string_lossy().into_owned())?;
        let report = remove_tree(&backend, &path)?;

        assert_eq!(report.files_removed, 2);
        assert_eq!(report.directories_removed, 3);
        assert_eq!(report.entries_removed(), 5);
        assert!(!target.exists());

        fs::remove_dir_all(root)?;
        Ok(())
    }
}
