//! Persistent non-secret connection-profile storage.

#![allow(clippy::missing_errors_doc, clippy::must_use_candidate)]

use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const PROFILE_VERSION: u32 = 1;

/// A saved, non-secret SFTP connection profile.
///
/// Authentication secrets are intentionally excluded from this structure.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SavedConnection {
    /// Stable profile identifier used for replacement and deletion.
    pub id: String,
    /// User-facing display name.
    pub name: String,
    /// Remote hostname or address.
    pub host: String,
    /// Remote login username.
    pub username: String,
    /// Remote SSH port.
    pub port: u16,
    /// Initial remote path opened after connection.
    pub initial_path: String,
}

impl SavedConnection {
    /// Creates and validates a saved connection profile.
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        host: impl Into<String>,
        username: impl Into<String>,
        port: u16,
        initial_path: impl Into<String>,
    ) -> Result<Self, String> {
        let profile = Self {
            id: id.into(),
            name: name.into(),
            host: host.into(),
            username: username.into(),
            port,
            initial_path: initial_path.into(),
        };
        profile.validate()?;
        Ok(profile)
    }

    /// Validates profile fields required for a usable connection.
    pub fn validate(&self) -> Result<(), String> {
        if self.id.trim().is_empty() {
            return Err("connection id must not be blank".to_owned());
        }
        if self.name.trim().is_empty() {
            return Err("connection name must not be blank".to_owned());
        }
        if self.host.trim().is_empty() {
            return Err("connection host must not be blank".to_owned());
        }
        if self.username.trim().is_empty() {
            return Err("connection username must not be blank".to_owned());
        }
        if self.port == 0 {
            return Err("connection port must be non-zero".to_owned());
        }
        if self.initial_path.is_empty() {
            return Err("connection path must not be empty".to_owned());
        }
        Ok(())
    }
}

/// Versioned collection of saved connection profiles.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ConnectionProfiles {
    /// On-disk profile-format version.
    pub version: u32,
    /// Saved connection entries.
    pub profiles: Vec<SavedConnection>,
}

impl Default for ConnectionProfiles {
    fn default() -> Self {
        Self {
            version: PROFILE_VERSION,
            profiles: Vec::new(),
        }
    }
}

impl ConnectionProfiles {
    /// Inserts a profile or replaces an existing profile with the same stable id.
    pub fn upsert(&mut self, profile: SavedConnection) {
        if let Some(existing) = self
            .profiles
            .iter_mut()
            .find(|existing| existing.id == profile.id)
        {
            *existing = profile;
        } else {
            self.profiles.push(profile);
        }
        self.profiles
            .sort_by_key(|profile| profile.name.to_lowercase());
    }

    /// Removes a profile by stable id and reports whether one existed.
    pub fn remove(&mut self, id: &str) -> bool {
        let before = self.profiles.len();
        self.profiles.retain(|profile| profile.id != id);
        self.profiles.len() != before
    }

    /// Finds a saved profile by stable id.
    #[must_use]
    pub fn find(&self, id: &str) -> Option<&SavedConnection> {
        self.profiles.iter().find(|profile| profile.id == id)
    }

    /// Loads profiles from a specific JSON path.
    pub fn load(path: &Path) -> Result<Self, io::Error> {
        match fs::read(path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(error),
        }
    }

    /// Saves profiles atomically to a specific JSON path.
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

    /// Loads profiles from Cyber-Pumpkin's default application-support path.
    pub fn load_default() -> Result<Self, io::Error> {
        Self::load(&default_profiles_path())
    }

    /// Saves profiles to Cyber-Pumpkin's default application-support path.
    pub fn save_default(&self) -> Result<(), io::Error> {
        self.save(&default_profiles_path())
    }
}

/// Returns the default JSON path for saved connection profiles.
#[must_use]
pub fn default_profiles_path() -> PathBuf {
    application_support_directory().join("connections.json")
}

/// Returns Cyber-Pumpkin's platform-appropriate application-support directory.
#[must_use]
pub fn application_support_directory() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home)
                .join("Library")
                .join("Application Support")
                .join("Cyber-Pumpkin");
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
            return PathBuf::from(xdg).join("cyber-pumpkin");
        }
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join(".config").join("cyber-pumpkin");
        }
    }

    PathBuf::from(".cyber-pumpkin")
}

#[cfg(test)]
mod tests {
    use super::{ConnectionProfiles, SavedConnection};

    fn profile(id: &str, name: &str) -> Result<SavedConnection, String> {
        SavedConnection::new(id, name, "example.test", "adam", 22, "/")
    }

    #[test]
    fn upsert_replaces_by_stable_id() -> Result<(), String> {
        let mut profiles = ConnectionProfiles::default();
        profiles.upsert(profile("studio", "Studio")?);
        profiles.upsert(profile("studio", "Studio Updated")?);
        assert_eq!(profiles.profiles.len(), 1);
        assert_eq!(profiles.profiles[0].name, "Studio Updated");
        Ok(())
    }

    #[test]
    fn remove_reports_whether_profile_existed() -> Result<(), String> {
        let mut profiles = ConnectionProfiles::default();
        profiles.upsert(profile("studio", "Studio")?);
        assert!(profiles.remove("studio"));
        assert!(!profiles.remove("studio"));
        Ok(())
    }
}
