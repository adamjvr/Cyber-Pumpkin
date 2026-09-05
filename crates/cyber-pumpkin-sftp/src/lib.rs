//! SFTP backend built on libssh2 with strict host-key verification.
//!
//! The first milestone intentionally supports SSH-agent and private-key
//! authentication only. Passwords are not accepted on the command line.

use cyber_pumpkin_backend::{Backend, BackendError, ErrorKind, ReadStream, WriteStream};
use cyber_pumpkin_core::{
    BackendCapabilities, BackendId, BackendPath, CapabilitySupport, EntryKind, FileEntry,
};
use ssh2::{CheckResult, FileStat, KnownHostFileKind, Session, Sftp};
use std::env;
use std::io;
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

const DEFAULT_PORT: u16 = 22;
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
const DEFAULT_TIMEOUT_MS: u32 = 30_000;

/// Non-secret SFTP connection configuration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SftpConfig {
    id: BackendId,
    host: String,
    port: u16,
    username: String,
    known_hosts_file: PathBuf,
}

impl SftpConfig {
    /// Creates configuration using port 22 and `~/.ssh/known_hosts`.
    ///
    /// # Errors
    ///
    /// Returns [`BackendError`] when the host or username is blank, or when
    /// the home directory cannot be resolved.
    pub fn new(
        id: BackendId,
        host: impl Into<String>,
        username: impl Into<String>,
    ) -> Result<Self, BackendError> {
        let host = host.into();
        let username = username.into();
        if host.trim().is_empty() {
            return Err(BackendError::new(
                ErrorKind::InvalidInput,
                "configure SFTP",
                None,
                "host must not be blank",
            ));
        }
        if username.trim().is_empty() {
            return Err(BackendError::new(
                ErrorKind::InvalidInput,
                "configure SFTP",
                None,
                "username must not be blank",
            ));
        }

        let home = env::var_os("HOME").ok_or_else(|| {
            BackendError::new(
                ErrorKind::InvalidInput,
                "resolve known_hosts",
                None,
                "HOME is not set",
            )
        })?;

        Ok(Self {
            id,
            host,
            port: DEFAULT_PORT,
            username,
            known_hosts_file: PathBuf::from(home).join(".ssh/known_hosts"),
        })
    }

    /// Overrides the TCP port.
    #[must_use]
    pub const fn with_port(mut self, port: u16) -> Self {
        self.port = port;
        self
    }

    /// Overrides the OpenSSH known-hosts file.
    #[must_use]
    pub fn with_known_hosts_file(mut self, path: impl Into<PathBuf>) -> Self {
        self.known_hosts_file = path.into();
        self
    }

    /// Returns the configured backend identifier.
    #[must_use]
    pub const fn id(&self) -> &BackendId {
        &self.id
    }

    /// Returns the remote hostname.
    #[must_use]
    pub fn host(&self) -> &str {
        &self.host
    }

    /// Returns the remote TCP port.
    #[must_use]
    pub const fn port(&self) -> u16 {
        self.port
    }

    /// Returns the SSH username.
    #[must_use]
    pub fn username(&self) -> &str {
        &self.username
    }

    /// Returns the known-hosts file used for strict verification.
    #[must_use]
    pub fn known_hosts_file(&self) -> &Path {
        &self.known_hosts_file
    }
}

/// Private-key authentication material supplied by a trusted caller.
///
/// The optional passphrase is intentionally not exposed through the Phase 1A
/// command-line interface.
pub struct PrivateKeyAuth {
    private_key: PathBuf,
    public_key: Option<PathBuf>,
    passphrase: Option<String>,
}

impl PrivateKeyAuth {
    /// Creates private-key authentication without a passphrase.
    #[must_use]
    pub fn new(private_key: impl Into<PathBuf>) -> Self {
        Self {
            private_key: private_key.into(),
            public_key: None,
            passphrase: None,
        }
    }

    /// Supplies an explicit public-key path.
    #[must_use]
    pub fn with_public_key(mut self, public_key: impl Into<PathBuf>) -> Self {
        self.public_key = Some(public_key.into());
        self
    }

    /// Supplies a passphrase obtained through a trusted secret path.
    #[must_use]
    pub fn with_passphrase(mut self, passphrase: impl Into<String>) -> Self {
        self.passphrase = Some(passphrase.into());
        self
    }
}

/// Authentication method used after host identity has been verified.
pub enum SftpAuth {
    /// Authenticate through the user's SSH agent.
    Agent,
    /// Authenticate using a private key stored on disk.
    PrivateKey(Box<PrivateKeyAuth>),
}

/// Connected SFTP filesystem backend.
pub struct SftpBackend {
    id: BackendId,
    sftp: Sftp,
}

impl SftpBackend {
    /// Connects, verifies the server host key, authenticates, and opens SFTP.
    ///
    /// # Errors
    ///
    /// Returns [`BackendError`] for TCP, SSH handshake, host-key,
    /// authentication, or SFTP-subsystem failures. Unknown and mismatched host
    /// keys are rejected; this function never performs trust-on-first-use.
    pub fn connect(config: &SftpConfig, auth: &SftpAuth) -> Result<Self, BackendError> {
        let tcp = TcpStream::connect((config.host(), config.port()))
            .map_err(|error| Self::io_error(ErrorKind::Transport, "connect TCP", None, &error))?;
        tcp.set_read_timeout(Some(DEFAULT_TIMEOUT))
            .map_err(|error| {
                Self::io_error(ErrorKind::Transport, "set TCP read timeout", None, &error)
            })?;
        tcp.set_write_timeout(Some(DEFAULT_TIMEOUT))
            .map_err(|error| {
                Self::io_error(ErrorKind::Transport, "set TCP write timeout", None, &error)
            })?;

        let mut session = Session::new().map_err(|error| {
            BackendError::new(
                ErrorKind::Protocol,
                "create SSH session",
                None,
                error.to_string(),
            )
        })?;
        session.set_tcp_stream(tcp);
        session.set_timeout(DEFAULT_TIMEOUT_MS);
        session.handshake().map_err(|error| {
            BackendError::new(
                ErrorKind::Protocol,
                "SSH handshake",
                None,
                error.to_string(),
            )
        })?;

        Self::verify_host_key(&session, config)?;
        Self::authenticate(&session, config, auth)?;

        let sftp = session.sftp().map_err(|error| {
            BackendError::new(
                ErrorKind::Protocol,
                "open SFTP subsystem",
                None,
                error.to_string(),
            )
        })?;

        Ok(Self {
            id: config.id().clone(),
            sftp,
        })
    }

    fn verify_host_key(session: &Session, config: &SftpConfig) -> Result<(), BackendError> {
        let (host_key, _) = session.host_key().ok_or_else(|| {
            BackendError::new(
                ErrorKind::HostKey,
                "read SSH host key",
                None,
                "server did not provide a host key",
            )
        })?;

        let mut known_hosts = session.known_hosts().map_err(|error| {
            BackendError::new(
                ErrorKind::HostKey,
                "initialize known_hosts",
                None,
                error.to_string(),
            )
        })?;
        known_hosts
            .read_file(config.known_hosts_file(), KnownHostFileKind::OpenSSH)
            .map_err(|error| {
                BackendError::new(
                    ErrorKind::HostKey,
                    "read known_hosts",
                    None,
                    format!("{}: {error}", config.known_hosts_file().display()),
                )
            })?;

        match known_hosts.check_port(config.host(), config.port(), host_key) {
            CheckResult::Match => Ok(()),
            CheckResult::Mismatch => Err(BackendError::new(
                ErrorKind::HostKey,
                "verify SSH host key",
                None,
                "host key does not match known_hosts",
            )),
            CheckResult::NotFound => Err(BackendError::new(
                ErrorKind::HostKey,
                "verify SSH host key",
                None,
                "host is not present in known_hosts",
            )),
            CheckResult::Failure => Err(BackendError::new(
                ErrorKind::HostKey,
                "verify SSH host key",
                None,
                "known_hosts verification failed",
            )),
        }
    }

    fn authenticate(
        session: &Session,
        config: &SftpConfig,
        auth: &SftpAuth,
    ) -> Result<(), BackendError> {
        let result = match auth {
            SftpAuth::Agent => session.userauth_agent(config.username()),
            SftpAuth::PrivateKey(key) => session.userauth_pubkey_file(
                config.username(),
                key.public_key.as_deref(),
                key.private_key.as_path(),
                key.passphrase.as_deref(),
            ),
        };

        result.map_err(|error| {
            BackendError::new(
                ErrorKind::Authentication,
                "SSH authentication",
                None,
                error.to_string(),
            )
        })?;

        if session.authenticated() {
            Ok(())
        } else {
            Err(BackendError::new(
                ErrorKind::Authentication,
                "SSH authentication",
                None,
                "server did not accept authentication",
            ))
        }
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
        })
    }

    fn ssh_error(operation: &'static str, path: &BackendPath, error: &ssh2::Error) -> BackendError {
        BackendError::new(
            ErrorKind::Protocol,
            operation,
            Some(path.clone()),
            error.to_string(),
        )
    }

    fn io_error(
        kind: ErrorKind,
        operation: &'static str,
        path: Option<BackendPath>,
        error: &io::Error,
    ) -> BackendError {
        BackendError::new(kind, operation, path, error.to_string())
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
