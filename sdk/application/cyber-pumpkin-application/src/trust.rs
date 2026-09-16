//! Persistent non-secret SSH host-trust records.

use crate::profiles::application_support_directory;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const TRUST_VERSION: u32 = 1;

/// One application-scoped trusted SSH host fingerprint.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TrustedHost {
    /// Hostname or address.
    pub host: String,
    /// TCP port.
    pub port: u16,
    /// Host-key fingerprint text supplied by the SSH layer.
    pub fingerprint: String,
}

/// Versioned host-trust collection.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TrustedHosts {
    /// On-disk format version.
    pub version: u32,
    /// Trusted host entries.
    pub hosts: Vec<TrustedHost>,
}

impl Default for TrustedHosts {
    fn default() -> Self {
        Self {
            version: TRUST_VERSION,
            hosts: Vec::new(),
        }
    }
}

impl TrustedHosts {
    /// Returns a stored fingerprint for a host and port.
    #[must_use]
    pub fn fingerprint(&self, host: &str, port: u16) -> Option<&str> {
        self.hosts
            .iter()
            .find(|entry| entry.host == host && entry.port == port)
            .map(|entry| entry.fingerprint.as_str())
    }

    /// Inserts or replaces one trusted host.
    pub fn trust(&mut self, host: impl Into<String>, port: u16, fingerprint: impl Into<String>) {
        let host = host.into();
        let fingerprint = fingerprint.into();
        if let Some(existing) = self
            .hosts
            .iter_mut()
            .find(|entry| entry.host == host && entry.port == port)
        {
            existing.fingerprint = fingerprint;
        } else {
            self.hosts.push(TrustedHost {
                host,
                port,
                fingerprint,
            });
        }
        self.hosts
            .sort_by(|left, right| (&left.host, left.port).cmp(&(&right.host, right.port)));
    }

    /// Loads trust records from a JSON file.
    ///
    /// # Errors
    ///
    /// Returns [`io::Error`] for I/O or malformed JSON.
    pub fn load(path: &Path) -> Result<Self, io::Error> {
        match fs::read(path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(error),
        }
    }

    /// Saves trust records atomically.
    ///
    /// # Errors
    ///
    /// Returns [`io::Error`] for serialization or filesystem failures.
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

    /// Loads the default application trust database.
    ///
    /// # Errors
    ///
    /// Returns [`io::Error`] for I/O or malformed JSON.
    pub fn load_default() -> Result<Self, io::Error> {
        Self::load(&default_trusted_hosts_path())
    }

    /// Saves the default application trust database.
    ///
    /// # Errors
    ///
    /// Returns [`io::Error`] for I/O or serialization failures.
    pub fn save_default(&self) -> Result<(), io::Error> {
        self.save(&default_trusted_hosts_path())
    }
}

/// Returns the default application trust-database path.
#[must_use]
pub fn default_trusted_hosts_path() -> PathBuf {
    application_support_directory().join("trusted-hosts.json")
}

#[cfg(test)]
mod tests {
    use super::TrustedHosts;

    #[test]
    fn trust_replaces_same_host_and_port() {
        let mut trust = TrustedHosts::default();
        trust.trust("example.test", 22, "AA");
        trust.trust("example.test", 22, "BB");
        assert_eq!(trust.hosts.len(), 1);
        assert_eq!(trust.fingerprint("example.test", 22), Some("BB"));
    }
}
