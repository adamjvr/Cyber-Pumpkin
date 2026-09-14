//! Persistent shared application preferences.

#![allow(clippy::missing_errors_doc, clippy::must_use_candidate)]

use crate::profiles::application_support_directory;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Policy used when a destination item already exists.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub enum ExistingItemAction {
    /// Ask the user which action to take.
    #[default]
    Ask,
    /// Replace the existing destination item.
    Replace,
    /// Leave the existing item untouched and skip the incoming item.
    Skip,
}

/// Action performed when a file row is double-clicked.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub enum DoubleClickAction {
    /// Open or enter the selected item.
    #[default]
    Open,
    /// Transfer the selected item to the opposite pane.
    Transfer,
    /// Show metadata for the selected item.
    Inspect,
}

/// Preferences controlling file-browser interaction.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct FilePreferences {
    /// Whether deletion requires confirmation.
    pub confirm_delete: bool,
    /// Action assigned to double-click.
    pub double_click_action: DoubleClickAction,
}

impl Default for FilePreferences {
    fn default() -> Self {
        Self {
            confirm_delete: true,
            double_click_action: DoubleClickAction::Open,
        }
    }
}

/// Preferences controlling transfer conflict and queue behavior.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TransferPreferences {
    /// Conflict policy for downloaded files.
    pub downloading_files: ExistingItemAction,
    /// Conflict policy for downloaded folders.
    pub downloading_folders: ExistingItemAction,
    /// Conflict policy for uploaded files.
    pub uploading_files: ExistingItemAction,
    /// Conflict policy for uploaded folders.
    pub uploading_folders: ExistingItemAction,
    /// Maximum number of simultaneous transfers requested by the user.
    pub simultaneous_transfers: u8,
    /// Whether completed activity entries remain visible.
    pub keep_activity: bool,
}

impl Default for TransferPreferences {
    fn default() -> Self {
        Self {
            downloading_files: ExistingItemAction::Ask,
            downloading_folders: ExistingItemAction::Ask,
            uploading_files: ExistingItemAction::Ask,
            uploading_folders: ExistingItemAction::Ask,
            simultaneous_transfers: 5,
            keep_activity: true,
        }
    }
}

/// Preferences for connection and diagnostic behavior.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AdvancedPreferences {
    /// Whether idle connections should use keepalive behavior.
    pub keep_connections_alive: bool,
    /// Whether verbose diagnostic logging is enabled.
    pub verbose_logging: bool,
}

impl Default for AdvancedPreferences {
    fn default() -> Self {
        Self {
            keep_connections_alive: true,
            verbose_logging: false,
        }
    }
}

/// Complete persisted Cyber-Pumpkin preference set.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct AppPreferences {
    /// File-browser preferences.
    pub files: FilePreferences,
    /// Transfer preferences.
    pub transfers: TransferPreferences,
    /// Advanced preferences.
    pub advanced: AdvancedPreferences,
}

impl AppPreferences {
    /// Loads preferences from a specific JSON path.
    pub fn load(path: &Path) -> Result<Self, io::Error> {
        match fs::read(path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(error),
        }
    }

    /// Saves preferences atomically to a specific JSON path.
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

    /// Loads preferences from Cyber-Pumpkin's default path.
    pub fn load_default() -> Result<Self, io::Error> {
        Self::load(&default_preferences_path())
    }

    /// Saves preferences to Cyber-Pumpkin's default path.
    pub fn save_default(&self) -> Result<(), io::Error> {
        self.save(&default_preferences_path())
    }
}

/// Returns the default JSON path for Cyber-Pumpkin preferences.
#[must_use]
pub fn default_preferences_path() -> PathBuf {
    application_support_directory().join("preferences.json")
}

#[cfg(test)]
mod tests {
    use super::{AppPreferences, ExistingItemAction};

    #[test]
    fn transfer_defaults_are_conservative() {
        let preferences = AppPreferences::default();
        assert_eq!(
            preferences.transfers.uploading_files,
            ExistingItemAction::Ask
        );
        assert_eq!(preferences.transfers.simultaneous_transfers, 5);
        assert!(preferences.files.confirm_delete);
    }
}
