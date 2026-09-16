//! SFTP filesystem backend built on Cyber-Pumpkin's shared SSH layer.

use cyber_pumpkin_backend::{Backend, BackendError, ErrorKind, ReadStream, WriteStream};
use cyber_pumpkin_core::{
    BackendCapabilities, BackendId, BackendPath, CapabilitySupport, EntryKind, FileEntry,
};
pub use cyber_pumpkin_ssh::{HostKeyProbe, HostKeyStatus, PrivateKeyAuth, SshAuth as SftpAuth};
use cyber_pumpkin_ssh::{
    SshConfig, SshConnection, SshError, SshErrorKind, probe_host_key as probe_ssh_host_key,
};
use ssh2::{ErrorCode, FileStat, Sftp};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Non-secret SFTP configuration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SftpConfig {
    id: BackendId,
    ssh: SshConfig,
}

impl SftpConfig {
    /// Creates SFTP configuration using shared SSH defaults.
    ///
    /// # Errors
    ///
    /// Returns [`BackendError`] when configuration is invalid.
    pub fn new(
        id: BackendId,
        host: impl Into<String>,
        username: impl Into<String>,
    ) -> Result<Self, BackendError> {
        let ssh = SshConfig::new(host, username).map_err(|error| map_ssh_error(&error))?;
        Ok(Self { id, ssh })
    }

    /// Overrides SSH port.
    #[must_use]
    pub fn with_port(mut self, port: u16) -> Self {
        self.ssh = self.ssh.with_port(port);
        self
    }

    /// Overrides known-hosts path.
    #[must_use]
    pub fn with_known_hosts_file(mut self, path: impl Into<PathBuf>) -> Self {
        self.ssh = self.ssh.with_known_hosts_file(path);
        self
    }

    /// Overrides timeout.
    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.ssh = self.ssh.with_timeout(timeout);
        self
    }

    /// Overrides SSH keepalive behavior.
    #[must_use]
    pub fn with_keepalive_interval(mut self, interval: Option<Duration>) -> Self {
        self.ssh = self.ssh.with_keepalive_interval(interval);
        self
    }

    /// Overrides bounded connection redials.
    #[must_use]
    pub fn with_max_redials(mut self, max_redials: u8) -> Self {
        self.ssh = self.ssh.with_max_redials(max_redials);
        self
    }

    /// Allows one exact application-trusted host fingerprint.
    #[must_use]
    pub fn with_trusted_host_fingerprint(mut self, fingerprint: impl Into<String>) -> Self {
        self.ssh = self.ssh.with_trusted_host_fingerprint(fingerprint);
        self
    }

    /// Backend id.
    #[must_use]
    pub const fn id(&self) -> &BackendId {
        &self.id
    }

    /// Host.
    #[must_use]
    pub fn host(&self) -> &str {
        self.ssh.host()
    }

    /// Port.
    #[must_use]
    pub const fn port(&self) -> u16 {
        self.ssh.port()
    }

    /// Username.
    #[must_use]
    pub fn username(&self) -> &str {
        self.ssh.username()
    }

    /// Known-hosts file.
    #[must_use]
    pub fn known_hosts_file(&self) -> &Path {
        self.ssh.known_hosts_file()
    }
}

/// Probes a server SSH host key before authentication.
///
/// # Errors
///
/// Returns [`BackendError`] for transport, protocol, or host-key failures.
pub fn probe_host_key(config: &SftpConfig) -> Result<HostKeyProbe, BackendError> {
    probe_ssh_host_key(&config.ssh).map_err(|error| map_ssh_error(&error))
}

/// Connected SFTP backend.
pub struct SftpBackend {
    id: BackendId,
    _ssh: SshConnection,
    sftp: Sftp,
}

impl SftpBackend {
    /// Connects through shared SSH and opens SFTP.
    ///
    /// # Errors
    ///
    /// Returns [`BackendError`] for SSH or SFTP failures.
    pub fn connect(config: &SftpConfig, auth: &SftpAuth) -> Result<Self, BackendError> {
        let ssh =
            SshConnection::connect(&config.ssh, auth).map_err(|error| map_ssh_error(&error))?;
        let sftp = ssh.session().sftp().map_err(|error| {
            BackendError::new(
                ErrorKind::Protocol,
                "open SFTP subsystem",
                None,
                error.to_string(),
            )
        })?;
        Ok(Self {
            id: config.id().clone(),
            _ssh: ssh,
            sftp,
        })
    }

    fn path(path: &BackendPath) -> &Path {
        Path::new(path.as_str())
    }

    fn entry(path: &Path, stat: &FileStat) -> Result<FileEntry, BackendError> {
        let path_text = path.to_str().ok_or_else(|| {
            BackendError::new(
                ErrorKind::Protocol,
                "decode SFTP path",
                None,
                "server returned a non-UTF-8 path",
            )
        })?;
        let name = match path.file_name() {
            Some(value) => value
                .to_str()
                .ok_or_else(|| {
                    BackendError::new(
                        ErrorKind::Protocol,
                        "decode SFTP file name",
                        None,
                        "server returned a non-UTF-8 file name",
                    )
                })?
                .to_owned(),
            None => path_text.to_owned(),
        };

        let remote_type = stat.file_type();
        let kind = if remote_type.is_file() {
            EntryKind::File
        } else if remote_type.is_dir() {
            EntryKind::Directory
        } else if remote_type.is_symlink() {
            EntryKind::Symlink
        } else {
            EntryKind::Other
        };
        let backend_path = BackendPath::new(path_text).map_err(|error| {
            BackendError::new(
                ErrorKind::Protocol,
                "convert SFTP path",
                None,
                error.to_string(),
            )
        })?;

        Ok(FileEntry {
            path: backend_path,
            name,
            kind,
            size: remote_type.is_file().then_some(stat.size).flatten(),
            modified: stat.mtime,
        })
    }

    fn ssh_error(operation: &'static str, path: &BackendPath, error: &ssh2::Error) -> BackendError {
        let kind = match error.code() {
            ErrorCode::SFTP(2) => ErrorKind::NotFound,
            ErrorCode::SFTP(3) => ErrorKind::PermissionDenied,
            _ => ErrorKind::Protocol,
        };
        BackendError::new(kind, operation, Some(path.clone()), error.to_string())
    }
}

impl Backend for SftpBackend {
    fn id(&self) -> &BackendId {
        &self.id
    }

    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities {
            resumable_write: CapabilitySupport::Unsupported,
            atomic_rename: CapabilitySupport::Supported,
            unix_permissions: CapabilitySupport::Supported,
            server_checksum: CapabilitySupport::Unsupported,
        }
    }

    fn list(&self, path: &BackendPath) -> Result<Vec<FileEntry>, BackendError> {
        let mut entries = self
            .sftp
            .readdir(Self::path(path))
            .map_err(|error| Self::ssh_error("list SFTP directory", path, &error))?
            .into_iter()
            .map(|(remote_path, stat)| Self::entry(&remote_path, &stat))
            .collect::<Result<Vec<_>, _>>()?;
        entries.sort_by(|left, right| left.name.cmp(&right.name));
        Ok(entries)
    }

    fn stat(&self, path: &BackendPath) -> Result<FileEntry, BackendError> {
        let stat = self
            .sftp
            .lstat(Self::path(path))
            .map_err(|error| Self::ssh_error("stat SFTP path", path, &error))?;
        Self::entry(Self::path(path), &stat)
    }

    fn open_read(&self, path: &BackendPath) -> Result<ReadStream, BackendError> {
        let file = self
            .sftp
            .open(Self::path(path))
            .map_err(|error| Self::ssh_error("open SFTP file for read", path, &error))?;
        Ok(Box::new(file))
    }

    fn open_write(&self, path: &BackendPath) -> Result<WriteStream, BackendError> {
        let file = self
            .sftp
            .create(Self::path(path))
            .map_err(|error| Self::ssh_error("open SFTP file for write", path, &error))?;
        Ok(Box::new(file))
    }

    fn create_dir(&self, path: &BackendPath) -> Result<(), BackendError> {
        self.sftp
            .mkdir(Self::path(path), 0o755)
            .map_err(|error| Self::ssh_error("create SFTP directory", path, &error))
    }

    fn rename(&self, source: &BackendPath, destination: &BackendPath) -> Result<(), BackendError> {
        self.sftp
            .rename(Self::path(source), Self::path(destination), None)
            .map_err(|error| Self::ssh_error("rename SFTP path", source, &error))
    }

    fn remove(&self, path: &BackendPath) -> Result<(), BackendError> {
        let stat = self
            .sftp
            .lstat(Self::path(path))
            .map_err(|error| Self::ssh_error("stat SFTP path before remove", path, &error))?;
        if stat.is_dir() {
            self.sftp
                .rmdir(Self::path(path))
                .map_err(|error| Self::ssh_error("remove SFTP directory", path, &error))
        } else {
            self.sftp
                .unlink(Self::path(path))
                .map_err(|error| Self::ssh_error("remove SFTP file", path, &error))
        }
    }
}

fn map_ssh_error(error: &SshError) -> BackendError {
    let kind = match error.kind() {
        SshErrorKind::InvalidInput => ErrorKind::InvalidInput,
        SshErrorKind::Transport => ErrorKind::Transport,
        SshErrorKind::Protocol => ErrorKind::Protocol,
        SshErrorKind::HostKey => ErrorKind::HostKey,
        SshErrorKind::Authentication => ErrorKind::Authentication,
    };
    BackendError::new(kind, error.operation(), None, error.message())
}

#[cfg(test)]
mod tests {
    use super::SftpConfig;
    use cyber_pumpkin_core::BackendId;

    #[test]
    fn rejects_blank_host() -> Result<(), Box<dyn std::error::Error>> {
        let id = BackendId::new("remote")?;
        assert!(SftpConfig::new(id, "   ", "adam").is_err());
        Ok(())
    }

    #[test]
    fn custom_port_is_preserved() -> Result<(), Box<dyn std::error::Error>> {
        let id = BackendId::new("remote")?;
        let config = SftpConfig::new(id, "example.com", "adam")?.with_port(2222);
        assert_eq!(config.port(), 2222);
        Ok(())
    }
}
