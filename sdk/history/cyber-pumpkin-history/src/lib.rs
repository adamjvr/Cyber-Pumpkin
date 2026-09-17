//! Versioned persistent operation/activity history.
//!
//! History contains no credentials or authentication secrets.

use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::Path;

const HISTORY_VERSION: u32 = 1;

/// Activity category stored in history.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum HistoryKind {
    /// File or tree copy/transfer.
    Copy,
    /// Synchronization run.
    Sync,
    /// Connection attempt or session event.
    Connection,
    /// Delete operation.
    Delete,
    /// Rename/move operation.
    Rename,
    /// Remote-edit watcher, upload, conflict, or stop event.
    RemoteEdit,
    /// Other operation not covered by a stable category.
    Other,
}

/// Observable state stored in activity history.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum HistoryState {
    /// Operation or session is currently active.
    Active,
    /// Operation completed successfully.
    Completed,
    /// Operation was cancelled.
    Cancelled,
    /// Operation failed.
    Failed,
}

/// One persisted activity record.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct HistoryEntry {
    /// Optional runtime operation identifier.
    pub operation_id: Option<u64>,
    /// Activity category.
    pub kind: HistoryKind,
    /// Activity state.
    pub state: HistoryState,
    /// Short user-facing label.
    pub label: String,
    /// Additional detail.
    pub detail: String,
    /// Unix timestamp in seconds.
    pub timestamp: u64,
}

impl HistoryEntry {
    /// Creates a history entry with an explicit timestamp.
    #[must_use]
    pub fn new(
        operation_id: Option<u64>,
        kind: HistoryKind,
        state: HistoryState,
        label: impl Into<String>,
        detail: impl Into<String>,
        timestamp: u64,
    ) -> Self {
        Self {
            operation_id,
            kind,
            state,
            label: label.into(),
            detail: detail.into(),
            timestamp,
        }
    }
}

/// Versioned history collection.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct HistoryLog {
    /// On-disk schema version.
    pub version: u32,
    /// Oldest-to-newest entries.
    pub entries: Vec<HistoryEntry>,
}

impl Default for HistoryLog {
    fn default() -> Self {
        Self {
            version: HISTORY_VERSION,
            entries: Vec::new(),
        }
    }
}

impl HistoryLog {
    /// Loads history from a JSON file. Missing files produce an empty log.
    ///
    /// # Errors
    ///
    /// Returns [`io::Error`] for I/O failures or malformed JSON.
    pub fn load(path: &Path) -> Result<Self, io::Error> {
        match fs::read(path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(error),
        }
    }

    /// Saves history atomically.
    ///
    /// # Errors
    ///
    /// Returns [`io::Error`] when serialization or filesystem operations fail.
    pub fn save(&self, path: &Path) -> Result<(), io::Error> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let payload = serde_json::to_vec_pretty(self)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        let temporary = path.with_extension("json.tmp");
        fs::write(&temporary, payload)?;
        fs::rename(temporary, path)
    }

    /// Appends an entry and bounds retained history to `maximum_entries`.
    ///
    /// A zero maximum keeps no historical entries.
    pub fn append(&mut self, entry: HistoryEntry, maximum_entries: usize) {
        self.entries.push(entry);
        if self.entries.len() > maximum_entries {
            let remove = self.entries.len() - maximum_entries;
            self.entries.drain(0..remove);
        }
    }

    /// Removes all retained history entries.
    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::{HistoryEntry, HistoryKind, HistoryLog, HistoryState};
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(1);

    #[test]
    fn bounded_history_keeps_newest_entries() {
        let mut log = HistoryLog::default();
        for id in 1..=4 {
            log.append(
                HistoryEntry::new(
                    Some(id),
                    HistoryKind::Copy,
                    HistoryState::Completed,
                    format!("copy {id}"),
                    "ok",
                    id,
                ),
                2,
            );
        }
        assert_eq!(log.entries.len(), 2);
        assert_eq!(log.entries[0].operation_id, Some(3));
        assert_eq!(log.entries[1].operation_id, Some(4));
    }

    #[test]
    fn save_and_load_round_trip() -> Result<(), Box<dyn std::error::Error>> {
        let serial = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "cyber-pumpkin-history-{}-{serial}.json",
            std::process::id()
        ));
        let mut log = HistoryLog::default();
        log.append(
            HistoryEntry::new(
                Some(9),
                HistoryKind::Sync,
                HistoryState::Failed,
                "Sync",
                "network failed",
                123,
            ),
            200,
        );
        log.append(
            HistoryEntry::new(
                Some(10),
                HistoryKind::RemoteEdit,
                HistoryState::Active,
                "Remote Edit Session",
                "/srv/patch.txt",
                124,
            ),
            200,
        );
        log.save(&path)?;
        assert_eq!(HistoryLog::load(&path)?, log);
        fs::remove_file(path)?;
        Ok(())
    }
}
