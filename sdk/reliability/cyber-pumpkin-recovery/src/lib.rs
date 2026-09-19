//! Persistent recovery journal for interrupted replacement operations.

use cyber_pumpkin_backend::{Backend, BackendError, ErrorKind};
use cyber_pumpkin_core::BackendPath;
use cyber_pumpkin_file_ops::remove_tree_if_present;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const MAX_DISCOVERED_JOURNALS: usize = 1024;

/// Recovery state for one replacement transaction.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum RecoveryPhase {
    /// Temporary stage may exist; original destination has not moved.
    Staging,
    /// Original destination has moved to backup.
    BackupMoved,
    /// New destination finalized; backup may remain.
    Finalized,
}

/// Persisted recovery entry.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RecoveryEntry {
    /// Stable operation identifier.
    pub operation_id: u64,
    /// Destination path.
    pub destination: String,
    /// Optional stage sidecar.
    pub stage: Option<String>,
    /// Optional backup sidecar.
    pub backup: Option<String>,
    /// Last durable phase.
    pub phase: RecoveryPhase,
}

/// Versioned journal.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RecoveryJournal {
    /// On-disk format version.
    pub version: u32,
    /// Active recovery entries.
    pub entries: Vec<RecoveryEntry>,
}

impl Default for RecoveryJournal {
    fn default() -> Self {
        Self {
            version: 1,
            entries: Vec::new(),
        }
    }
}

impl RecoveryJournal {
    /// Loads a journal. A missing file produces an empty journal.
    ///
    /// # Errors
    ///
    /// Returns [`io::Error`] for malformed JSON or I/O failures.
    pub fn load(path: &Path) -> Result<Self, io::Error> {
        match fs::read(path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(error),
        }
    }

    /// Saves atomically through a sibling temporary file.
    ///
    /// # Errors
    ///
    /// Returns [`io::Error`] for serialization or filesystem failures.
    pub fn save(&self, path: &Path) -> Result<(), io::Error> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let bytes = serde_json::to_vec_pretty(self)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        let temporary = path.with_extension("json.tmp");
        fs::write(&temporary, bytes)?;
        fs::rename(temporary, path)
    }

    /// Saves a non-empty journal, or removes an empty journal file.
    ///
    /// # Errors
    ///
    /// Returns [`io::Error`] for filesystem or serialization failures.
    pub fn save_or_remove(&self, path: &Path) -> Result<(), io::Error> {
        if self.entries.is_empty() {
            return match fs::remove_file(path) {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
                Err(error) => Err(error),
            };
        }
        self.save(path)
    }

    /// Inserts or replaces one operation entry.
    pub fn upsert(&mut self, entry: RecoveryEntry) {
        if let Some(existing) = self
            .entries
            .iter_mut()
            .find(|existing| existing.operation_id == entry.operation_id)
        {
            *existing = entry;
        } else {
            self.entries.push(entry);
            self.entries.sort_by_key(|entry| entry.operation_id);
        }
    }

    /// Removes a completed operation.
    pub fn remove(&mut self, operation_id: u64) {
        self.entries
            .retain(|entry| entry.operation_id != operation_id);
    }
}

/// Recursively discovers regular JSON recovery journals below `root`.
///
/// Symlinks are not followed and discovery is bounded.
///
/// # Errors
///
/// Returns [`io::Error`] for enumeration failures or if the safety bound is exceeded.
pub fn discover_journals(root: &Path) -> Result<Vec<PathBuf>, io::Error> {
    let mut journals = Vec::new();
    discover_journals_inner(root, &mut journals)?;
    journals.sort();
    Ok(journals)
}

fn discover_journals_inner(root: &Path, journals: &mut Vec<PathBuf>) -> Result<(), io::Error> {
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    for entry in entries {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let path = entry.path();
        if file_type.is_dir() {
            discover_journals_inner(&path, journals)?;
            continue;
        }
        if !file_type.is_file() || path.extension().and_then(|value| value.to_str()) != Some("json")
        {
            continue;
        }
        journals.push(path);
        if journals.len() > MAX_DISCOVERED_JOURNALS {
            return Err(io::Error::other(format!(
                "recovery journal discovery exceeded {MAX_DISCOVERED_JOURNALS} files"
            )));
        }
    }
    Ok(())
}

/// Recovery action taken for one journal entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecoveryAction {
    /// Stage/backup sidecars were cleaned.
    Cleaned,
    /// Backup was restored to the missing destination.
    Restored,
    /// Entry required no backend mutation.
    Noop,
}

/// Aggregate startup recovery result.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RecoverySweepReport {
    /// Entries examined successfully.
    pub entries_processed: u64,
    /// Entries that restored an original destination.
    pub restored: u64,
    /// Entries that cleaned stage/backup sidecars.
    pub cleaned: u64,
    /// Entries requiring no backend mutation.
    pub noop: u64,
}

/// Failure while loading, applying, or persisting a recovery sweep.
#[derive(Debug)]
pub enum RecoverySweepError {
    /// Journal I/O or decoding failure.
    Io(io::Error),
    /// Backend recovery action failed.
    Backend(BackendError),
}

impl std::fmt::Display for RecoverySweepError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "recovery journal I/O failed: {error}"),
            Self::Backend(error) => write!(formatter, "recovery backend action failed: {error}"),
        }
    }
}
impl std::error::Error for RecoverySweepError {}
impl From<io::Error> for RecoverySweepError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}
impl From<BackendError> for RecoverySweepError {
    fn from(value: BackendError) -> Self {
        Self::Backend(value)
    }
}

/// Loads and drains a recovery journal one entry at a time.
///
/// # Errors
///
/// Returns [`RecoverySweepError`] when journal I/O or backend recovery fails.
pub fn recover_journal(
    backend: &dyn Backend,
    path: &Path,
) -> Result<RecoverySweepReport, RecoverySweepError> {
    let mut journal = RecoveryJournal::load(path)?;
    let entries = journal.entries.clone();
    let mut report = RecoverySweepReport::default();
    if entries.is_empty() {
        journal.save_or_remove(path)?;
        return Ok(report);
    }
    for entry in entries {
        let action = recover_entry(backend, &entry)?;
        report.entries_processed = report.entries_processed.saturating_add(1);
        match action {
            RecoveryAction::Restored => report.restored = report.restored.saturating_add(1),
            RecoveryAction::Cleaned => report.cleaned = report.cleaned.saturating_add(1),
            RecoveryAction::Noop => report.noop = report.noop.saturating_add(1),
        }
        journal.remove(entry.operation_id);
        journal.save_or_remove(path)?;
    }
    Ok(report)
}

/// Recovers one interrupted replacement conservatively.
///
/// # Errors
///
/// Returns [`BackendError`] when cleanup or restore operations fail.
pub fn recover_entry(
    backend: &dyn Backend,
    entry: &RecoveryEntry,
) -> Result<RecoveryAction, BackendError> {
    let destination = BackendPath::new(entry.destination.clone()).map_err(|error| {
        BackendError::new(
            ErrorKind::InvalidInput,
            "decode recovery destination",
            None,
            error.to_string(),
        )
    })?;

    let stage = decode_optional("decode recovery stage", entry.stage.as_deref())?;
    let backup = decode_optional("decode recovery backup", entry.backup.as_deref())?;

    match entry.phase {
        RecoveryPhase::Staging => {
            if let Some(stage) = stage {
                let removed = remove_tree_if_present(backend, &stage)?.entries_removed() > 0;
                Ok(if removed {
                    RecoveryAction::Cleaned
                } else {
                    RecoveryAction::Noop
                })
            } else {
                Ok(RecoveryAction::Noop)
            }
        }
        RecoveryPhase::BackupMoved => {
            let Some(backup) = backup else {
                let cleaned = if let Some(stage) = stage.as_ref() {
                    remove_tree_if_present(backend, stage)?.entries_removed() > 0
                } else {
                    false
                };
                return Ok(if cleaned {
                    RecoveryAction::Cleaned
                } else {
                    RecoveryAction::Noop
                });
            };

            let destination_exists = exists(backend, &destination)?;
            let backup_exists = exists(backend, &backup)?;
            if !backup_exists {
                let cleaned = if let Some(stage) = stage.as_ref() {
                    remove_tree_if_present(backend, stage)?.entries_removed() > 0
                } else {
                    false
                };
                return Ok(if cleaned {
                    RecoveryAction::Cleaned
                } else {
                    RecoveryAction::Noop
                });
            }

            // BackupMoved is deliberately conservative: until Finalized is
            // durably recorded, the original object is authoritative. A crash
            // may have left a partial tree at the destination or may have
            // completed the final rename without persisting the next phase.
            // Rolling back in both cases guarantees that recovery never keeps
            // an incompletely committed replacement.
            if destination_exists {
                let _entries_removed =
                    remove_tree_if_present(backend, &destination)?.entries_removed();
            }
            backend.rename(&backup, &destination)?;
            if let Some(stage) = stage.as_ref() {
                let _entries_removed = remove_tree_if_present(backend, stage)?.entries_removed();
            }
            Ok(RecoveryAction::Restored)
        }
        RecoveryPhase::Finalized => {
            let mut mutated = false;
            if let Some(stage) = stage {
                mutated |= remove_tree_if_present(backend, &stage)?.entries_removed() > 0;
            }
            if let Some(backup) = backup {
                mutated |= remove_tree_if_present(backend, &backup)?.entries_removed() > 0;
            }
            Ok(if mutated {
                RecoveryAction::Cleaned
            } else {
                RecoveryAction::Noop
            })
        }
    }
}

fn decode_optional(
    operation: &'static str,
    value: Option<&str>,
) -> Result<Option<BackendPath>, BackendError> {
    value
        .map(|value| {
            BackendPath::new(value.to_owned()).map_err(|error| {
                BackendError::new(ErrorKind::InvalidInput, operation, None, error.to_string())
            })
        })
        .transpose()
}

fn exists(backend: &dyn Backend, path: &BackendPath) -> Result<bool, BackendError> {
    match backend.stat(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        RecoveryAction, RecoveryEntry, RecoveryJournal, RecoveryPhase, discover_journals,
        recover_entry,
    };
    use cyber_pumpkin_core::BackendId;
    use cyber_pumpkin_local::LocalBackend;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(1);

    fn root() -> PathBuf {
        let serial = NEXT.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "cyber-pumpkin-recovery-{}-{serial}",
            std::process::id()
        ))
    }

    #[test]
    fn journal_round_trips() -> Result<(), Box<dyn std::error::Error>> {
        let root = root();
        fs::create_dir_all(&root)?;
        let path = root.join("journal.json");

        let mut journal = RecoveryJournal::default();
        journal.upsert(RecoveryEntry {
            operation_id: 9,
            destination: "/tmp/destination".to_owned(),
            stage: Some("/tmp/stage".to_owned()),
            backup: None,
            phase: RecoveryPhase::Staging,
        });
        journal.save(&path)?;
        assert_eq!(RecoveryJournal::load(&path)?, journal);

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn discovers_nested_json_journals_only() -> Result<(), Box<dyn std::error::Error>> {
        let root = root();
        fs::create_dir_all(root.join("nested"))?;
        fs::write(root.join("one.json"), b"{}")?;
        fs::write(root.join("nested/two.json"), b"{}")?;
        fs::write(root.join("nested/ignore.txt"), b"x")?;
        let journals = discover_journals(&root)?;
        assert_eq!(journals.len(), 2);
        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn staging_recovery_keeps_committed_destination_when_stage_is_absent()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = root();
        fs::create_dir_all(&root)?;
        let destination = root.join("destination.txt");
        let vanished_stage = root.join("stage.txt");
        fs::write(&destination, b"committed")?;
        let backend = LocalBackend::new(BackendId::new("local")?);
        let action = recover_entry(
            &backend,
            &RecoveryEntry {
                operation_id: 12,
                destination: destination.to_string_lossy().into_owned(),
                stage: Some(vanished_stage.to_string_lossy().into_owned()),
                backup: None,
                phase: RecoveryPhase::Staging,
            },
        )?;
        assert_eq!(action, RecoveryAction::Noop);
        assert_eq!(fs::read(&destination)?, b"committed");
        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn backup_moved_rolls_back_partial_destination() -> Result<(), Box<dyn std::error::Error>> {
        let root = root();
        fs::create_dir_all(&root)?;
        let destination = root.join("destination.txt");
        let backup = root.join("backup.txt");
        fs::write(&destination, b"partial replacement")?;
        fs::write(&backup, b"original")?;

        let backend = LocalBackend::new(BackendId::new("local")?);
        let action = recover_entry(
            &backend,
            &RecoveryEntry {
                operation_id: 11,
                destination: destination.to_string_lossy().into_owned(),
                stage: None,
                backup: Some(backup.to_string_lossy().into_owned()),
                phase: RecoveryPhase::BackupMoved,
            },
        )?;

        assert_eq!(action, RecoveryAction::Restored);
        assert_eq!(fs::read(&destination)?, b"original");
        assert!(!backup.exists());
        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn backup_moved_restores_missing_destination() -> Result<(), Box<dyn std::error::Error>> {
        let root = root();
        fs::create_dir_all(&root)?;
        let destination = root.join("destination.txt");
        let backup = root.join("backup.txt");
        fs::write(&backup, b"original")?;

        let backend = LocalBackend::new(BackendId::new("local")?);
        let action = recover_entry(
            &backend,
            &RecoveryEntry {
                operation_id: 10,
                destination: destination.to_string_lossy().into_owned(),
                stage: None,
                backup: Some(backup.to_string_lossy().into_owned()),
                phase: RecoveryPhase::BackupMoved,
            },
        )?;

        assert_eq!(action, RecoveryAction::Restored);
        assert_eq!(fs::read(destination)?, b"original");
        fs::remove_dir_all(root)?;
        Ok(())
    }
}
