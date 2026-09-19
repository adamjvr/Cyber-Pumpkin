//! Platform secret-storage contract for Cyber-Pumpkin.
//!
//! Saved connection profiles remain non-secret. Passwords and passphrases live
//! in the operating-system credential store, or in memory for deterministic tests.

use std::collections::BTreeMap;
use std::fmt;
#[cfg(target_os = "linux")]
use std::io::Write as _;
#[cfg(not(target_os = "macos"))]
use std::process::Command;
#[cfg(target_os = "linux")]
use std::process::Stdio;

#[cfg(target_os = "macos")]
use security_framework::passwords::{
    delete_generic_password, get_generic_password, set_generic_password,
};

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
        #[cfg(target_os = "macos")]
        {
            true
        }
        #[cfg(not(target_os = "macos"))]
        {
            helper_command()
                .arg("--help")
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .is_ok()
        }
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
const MACOS_KEYCHAIN_ITEM_NOT_FOUND: i32 = -25_300;

#[cfg(target_os = "macos")]
fn platform_set(key: &str, secret: &str) -> Result<(), SecretError> {
    set_generic_password(SERVICE, key, secret.as_bytes())
        .map_err(|error| SecretError::Command(error.to_string()))
}

#[cfg(target_os = "macos")]
fn platform_get(key: &str) -> Result<Option<String>, SecretError> {
    match get_generic_password(SERVICE, key) {
        Ok(bytes) => String::from_utf8(bytes)
            .map(Some)
            .map_err(|_| SecretError::InvalidUtf8),
        Err(error) if error.code() == MACOS_KEYCHAIN_ITEM_NOT_FOUND => Ok(None),
        Err(error) => Err(SecretError::Command(error.to_string())),
    }
}

#[cfg(target_os = "macos")]
fn platform_delete(key: &str) -> Result<(), SecretError> {
    match delete_generic_password(SERVICE, key) {
        Ok(()) => Ok(()),
        Err(error) if error.code() == MACOS_KEYCHAIN_ITEM_NOT_FOUND => Ok(()),
        Err(error) => Err(SecretError::Command(error.to_string())),
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
