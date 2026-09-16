//! Platform secret-storage contract for Cyber-Pumpkin.
//!
//! Saved connection profiles remain non-secret. Passwords and passphrases live
//! in the operating-system credential store, or in memory for deterministic tests.

use std::collections::BTreeMap;
use std::fmt;
use std::io::Write as _;
use std::process::{Command, Stdio};

#[cfg(target_os = "macos")]
const SERVICE: &str = "Cyber-Pumpkin";

/// Secret-store failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SecretError {
    /// Native credential helper is unavailable.
    Unavailable(String),
    /// Native credential helper failed.
    Command(String),
    /// Stored secret could not be decoded as UTF-8.
    InvalidUtf8,
}

impl fmt::Display for SecretError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable(message) => write!(formatter, "secret store unavailable: {message}"),
            Self::Command(message) => write!(formatter, "secret store command failed: {message}"),
            Self::InvalidUtf8 => formatter.write_str("secret store returned invalid UTF-8"),
        }
    }
}

impl std::error::Error for SecretError {}

/// Backend-neutral secret storage.
pub trait SecretStore {
    /// Stores or replaces one secret.
    ///
    /// # Errors
    ///
    /// Returns [`SecretError`] when platform storage fails.
    fn set(&mut self, key: &str, secret: &str) -> Result<(), SecretError>;

    /// Retrieves one secret.
    ///
    /// # Errors
    ///
    /// Returns [`SecretError`] when platform storage fails.
    fn get(&self, key: &str) -> Result<Option<String>, SecretError>;

    /// Deletes one secret when present.
    ///
    /// # Errors
    ///
    /// Returns [`SecretError`] when platform storage fails.
    fn delete(&mut self, key: &str) -> Result<(), SecretError>;
}

/// Deterministic in-memory store for tests and temporary sessions.
#[derive(Default, Debug)]
pub struct MemorySecretStore {
    values: BTreeMap<String, String>,
}

impl SecretStore for MemorySecretStore {
    fn set(&mut self, key: &str, secret: &str) -> Result<(), SecretError> {
        self.values.insert(key.to_owned(), secret.to_owned());
        Ok(())
    }

    fn get(&self, key: &str) -> Result<Option<String>, SecretError> {
        Ok(self.values.get(key).cloned())
    }

    fn delete(&mut self, key: &str) -> Result<(), SecretError> {
        self.values.remove(key);
        Ok(())
    }
}

/// Native OS credential-store adapter.
///
/// Linux uses Secret Service through `secret-tool`; macOS uses Keychain through
/// the system `security` command. Cyber-Pumpkin never writes returned secret
/// values into its JSON profile files.
#[derive(Clone, Copy, Debug, Default)]
pub struct PlatformSecretStore;

impl PlatformSecretStore {
    /// Returns whether the native helper executable can be launched.
    #[must_use]
    pub fn is_available() -> bool {
        helper_command()
            .arg("--help")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok()
    }
}

impl SecretStore for PlatformSecretStore {
    fn set(&mut self, key: &str, secret: &str) -> Result<(), SecretError> {
        platform_set(key, secret)
    }

    fn get(&self, key: &str) -> Result<Option<String>, SecretError> {
        platform_get(key)
    }

    fn delete(&mut self, key: &str) -> Result<(), SecretError> {
        platform_delete(key)
    }
}

#[cfg(target_os = "linux")]
fn helper_command() -> Command {
    Command::new("secret-tool")
}

#[cfg(target_os = "macos")]
fn helper_command() -> Command {
    Command::new("security")
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn helper_command() -> Command {
    Command::new("__cyber_pumpkin_secret_store_unavailable__")
}

#[cfg(target_os = "linux")]
fn platform_set(key: &str, secret: &str) -> Result<(), SecretError> {
    let mut child = Command::new("secret-tool")
        .args([
            "store",
            "--label=Cyber-Pumpkin",
            "application",
            "cyber-pumpkin",
            "key",
            key,
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| SecretError::Unavailable(error.to_string()))?;

    child
        .stdin
        .as_mut()
        .ok_or_else(|| SecretError::Command("secret-tool stdin unavailable".to_owned()))?
        .write_all(secret.as_bytes())
        .map_err(|error| SecretError::Command(error.to_string()))?;

    let output = child
        .wait_with_output()
        .map_err(|error| SecretError::Command(error.to_string()))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(SecretError::Command(
            String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        ))
    }
}

#[cfg(target_os = "linux")]
fn platform_get(key: &str) -> Result<Option<String>, SecretError> {
    let output = Command::new("secret-tool")
        .args(["lookup", "application", "cyber-pumpkin", "key", key])
        .output()
        .map_err(|error| SecretError::Unavailable(error.to_string()))?;
    if !output.status.success() {
        return Ok(None);
    }
    let value = String::from_utf8(output.stdout).map_err(|_| SecretError::InvalidUtf8)?;
    Ok(Some(value.trim_end_matches(&['\r', '\n'][..]).to_owned()))
}

#[cfg(target_os = "linux")]
fn platform_delete(key: &str) -> Result<(), SecretError> {
    let output = Command::new("secret-tool")
        .args(["clear", "application", "cyber-pumpkin", "key", key])
        .output()
        .map_err(|error| SecretError::Unavailable(error.to_string()))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(SecretError::Command(
            String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        ))
    }
}

#[cfg(target_os = "macos")]
fn platform_set(key: &str, secret: &str) -> Result<(), SecretError> {
    let output = Command::new("security")
        .args([
            "add-generic-password",
            "-U",
            "-a",
            key,
            "-s",
            SERVICE,
            "-w",
            secret,
        ])
        .output()
        .map_err(|error| SecretError::Unavailable(error.to_string()))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(SecretError::Command(
            String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        ))
    }
}

#[cfg(target_os = "macos")]
fn platform_get(key: &str) -> Result<Option<String>, SecretError> {
    let output = Command::new("security")
        .args(["find-generic-password", "-a", key, "-s", SERVICE, "-w"])
        .output()
        .map_err(|error| SecretError::Unavailable(error.to_string()))?;
    if !output.status.success() {
        return Ok(None);
    }
    let value = String::from_utf8(output.stdout).map_err(|_| SecretError::InvalidUtf8)?;
    Ok(Some(value.trim_end_matches(&['\r', '\n'][..]).to_owned()))
}

#[cfg(target_os = "macos")]
fn platform_delete(key: &str) -> Result<(), SecretError> {
    let output = Command::new("security")
        .args(["delete-generic-password", "-a", key, "-s", SERVICE])
        .output()
        .map_err(|error| SecretError::Unavailable(error.to_string()))?;
    if output.status.success() || output.status.code() == Some(44) {
        Ok(())
    } else {
        Err(SecretError::Command(
            String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        ))
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn platform_set(_key: &str, _secret: &str) -> Result<(), SecretError> {
    Err(SecretError::Unavailable("unsupported platform".to_owned()))
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn platform_get(_key: &str) -> Result<Option<String>, SecretError> {
    Err(SecretError::Unavailable("unsupported platform".to_owned()))
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn platform_delete(_key: &str) -> Result<(), SecretError> {
    Err(SecretError::Unavailable("unsupported platform".to_owned()))
}

#[cfg(test)]
mod tests {
    use super::{MemorySecretStore, SecretStore};

    #[test]
    fn memory_store_round_trips_and_deletes() -> Result<(), Box<dyn std::error::Error>> {
        let mut store = MemorySecretStore::default();
        store.set("studio", "hunter2")?;
        assert_eq!(store.get("studio")?.as_deref(), Some("hunter2"));
        store.delete("studio")?;
        assert_eq!(store.get("studio")?, None);
        Ok(())
    }
}
