//! Shared SSH transport, host-trust, and authentication layer.

use ssh2::{CheckResult, KnownHostFileKind, Session};
use std::fmt;
use std::io;
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

const DEFAULT_PORT: u16 = 22;
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
const DEFAULT_TIMEOUT_MS: u32 = 30_000;

/// Stable category for SSH failures.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SshErrorKind {
    /// Invalid configuration.
    InvalidInput,
    /// TCP transport failure.
    Transport,
    /// SSH protocol failure.
    Protocol,
    /// Host identity failure.
    HostKey,
    /// Authentication failure.
    Authentication,
}

/// Stable SSH error.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SshError {
    kind: SshErrorKind,
    operation: &'static str,
    message: String,
}

impl SshError {
    /// Creates an SSH error.
    #[must_use]
    pub fn new(kind: SshErrorKind, operation: &'static str, message: impl Into<String>) -> Self {
        Self {
            kind,
            operation,
            message: message.into(),
        }
    }

    /// Returns the error category.
    #[must_use]
    pub const fn kind(&self) -> SshErrorKind {
        self.kind
    }

    /// Returns the failed operation.
    #[must_use]
    pub const fn operation(&self) -> &'static str {
        self.operation
    }

    /// Returns diagnostic text.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for SshError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} failed: {}", self.operation, self.message)
    }
}

impl std::error::Error for SshError {}

/// Non-secret SSH connection configuration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SshConfig {
    host: String,
    port: u16,
    username: String,
    known_hosts_file: PathBuf,
    timeout: Duration,
}

impl SshConfig {
    /// Creates configuration using port 22 and `~/.ssh/known_hosts`.
    ///
    /// # Errors
    ///
    /// Returns [`SshError`] when host or username is blank, or HOME is absent.
    pub fn new(host: impl Into<String>, username: impl Into<String>) -> Result<Self, SshError> {
        let host = host.into();
        let username = username.into();
        if host.trim().is_empty() {
            return Err(SshError::new(
                SshErrorKind::InvalidInput,
                "configure SSH",
                "host must not be blank",
            ));
        }
        if username.trim().is_empty() {
            return Err(SshError::new(
                SshErrorKind::InvalidInput,
                "configure SSH",
                "username must not be blank",
            ));
        }
        let home = std::env::var_os("HOME").ok_or_else(|| {
            SshError::new(
                SshErrorKind::InvalidInput,
                "resolve known_hosts",
                "HOME is not set",
            )
        })?;
        Ok(Self {
            host,
            port: DEFAULT_PORT,
            username,
            known_hosts_file: PathBuf::from(home).join(".ssh/known_hosts"),
            timeout: DEFAULT_TIMEOUT,
        })
    }

    /// Overrides the TCP port.
    #[must_use]
    pub const fn with_port(mut self, port: u16) -> Self {
        self.port = port;
        self
    }

    /// Overrides the known-hosts file.
    #[must_use]
    pub fn with_known_hosts_file(mut self, path: impl Into<PathBuf>) -> Self {
        self.known_hosts_file = path.into();
        self
    }

    /// Overrides the timeout.
    #[must_use]
    pub const fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Returns host.
    #[must_use]
    pub fn host(&self) -> &str {
        &self.host
    }

    /// Returns port.
    #[must_use]
    pub const fn port(&self) -> u16 {
        self.port
    }

    /// Returns username.
    #[must_use]
    pub fn username(&self) -> &str {
        &self.username
    }

    /// Returns known-hosts path.
    #[must_use]
    pub fn known_hosts_file(&self) -> &Path {
        &self.known_hosts_file
    }

    /// Returns timeout.
    #[must_use]
    pub const fn timeout(&self) -> Duration {
        self.timeout
    }
}

/// Private-key authentication material.
pub struct PrivateKeyAuth {
    private_key: PathBuf,
    public_key: Option<PathBuf>,
    passphrase: Option<String>,
}

impl PrivateKeyAuth {
    /// Creates private-key auth.
    #[must_use]
    pub fn new(private_key: impl Into<PathBuf>) -> Self {
        Self {
            private_key: private_key.into(),
            public_key: None,
            passphrase: None,
        }
    }

    /// Supplies public key.
    #[must_use]
    pub fn with_public_key(mut self, public_key: impl Into<PathBuf>) -> Self {
        self.public_key = Some(public_key.into());
        self
    }

    /// Supplies passphrase from a trusted secret path.
    #[must_use]
    pub fn with_passphrase(mut self, passphrase: impl Into<String>) -> Self {
        self.passphrase = Some(passphrase.into());
        self
    }
}

/// SSH authentication mode.
pub enum SshAuth {
    /// SSH agent.
    Agent,
    /// Private key.
    PrivateKey(Box<PrivateKeyAuth>),
}

/// Verified and authenticated SSH connection.
pub struct SshConnection {
    session: Session,
}

impl SshConnection {
    /// Connects, verifies host identity, and authenticates.
    ///
    /// # Errors
    ///
    /// Returns [`SshError`] on transport, protocol, trust, or auth failure.
    pub fn connect(config: &SshConfig, auth: &SshAuth) -> Result<Self, SshError> {
        if config.port() == 0 {
            return Err(SshError::new(
                SshErrorKind::InvalidInput,
                "configure SSH",
                "port must be non-zero",
            ));
        }

        let tcp = TcpStream::connect((config.host(), config.port()))
            .map_err(|error| io_error("connect TCP", &error))?;
        tcp.set_read_timeout(Some(config.timeout()))
            .map_err(|error| io_error("set TCP read timeout", &error))?;
        tcp.set_write_timeout(Some(config.timeout()))
            .map_err(|error| io_error("set TCP write timeout", &error))?;

        let mut session = Session::new().map_err(|error| {
            SshError::new(
                SshErrorKind::Protocol,
                "create SSH session",
                error.to_string(),
            )
        })?;
        session.set_tcp_stream(tcp);
        let timeout_ms = u32::try_from(config.timeout().as_millis()).unwrap_or(DEFAULT_TIMEOUT_MS);
        session.set_timeout(timeout_ms);
        session.handshake().map_err(|error| {
            SshError::new(SshErrorKind::Protocol, "SSH handshake", error.to_string())
        })?;

        verify_host_key(&session, config)?;
        authenticate(&session, config, auth)?;
        Ok(Self { session })
    }

    /// Returns underlying authenticated session.
    #[must_use]
    pub const fn session(&self) -> &Session {
        &self.session
    }
}

fn verify_host_key(session: &Session, config: &SshConfig) -> Result<(), SshError> {
    let (host_key, _) = session.host_key().ok_or_else(|| {
        SshError::new(
            SshErrorKind::HostKey,
            "read SSH host key",
            "server did not provide a host key",
        )
    })?;
    let mut known_hosts = session.known_hosts().map_err(|error| {
        SshError::new(
            SshErrorKind::HostKey,
            "initialize known_hosts",
            error.to_string(),
        )
    })?;
    known_hosts
        .read_file(config.known_hosts_file(), KnownHostFileKind::OpenSSH)
        .map_err(|error| {
            SshError::new(
                SshErrorKind::HostKey,
                "read known_hosts",
                format!("{}: {error}", config.known_hosts_file().display()),
            )
        })?;

    match known_hosts.check_port(config.host(), config.port(), host_key) {
        CheckResult::Match => Ok(()),
        CheckResult::Mismatch => Err(SshError::new(
            SshErrorKind::HostKey,
            "verify SSH host key",
            "host key does not match known_hosts",
        )),
        CheckResult::NotFound => Err(SshError::new(
            SshErrorKind::HostKey,
            "verify SSH host key",
            "host is not present in known_hosts",
        )),
        CheckResult::Failure => Err(SshError::new(
            SshErrorKind::HostKey,
            "verify SSH host key",
            "known_hosts verification failed",
        )),
    }
}

fn authenticate(session: &Session, config: &SshConfig, auth: &SshAuth) -> Result<(), SshError> {
    let result = match auth {
        SshAuth::Agent => session.userauth_agent(config.username()),
        SshAuth::PrivateKey(key) => session.userauth_pubkey_file(
            config.username(),
            key.public_key.as_deref(),
            key.private_key.as_path(),
            key.passphrase.as_deref(),
        ),
    };
    result.map_err(|error| {
        SshError::new(
            SshErrorKind::Authentication,
            "SSH authentication",
            error.to_string(),
        )
    })?;
    if session.authenticated() {
        Ok(())
    } else {
        Err(SshError::new(
            SshErrorKind::Authentication,
            "SSH authentication",
            "server did not accept authentication",
        ))
    }
}

fn io_error(operation: &'static str, error: &io::Error) -> SshError {
    SshError::new(SshErrorKind::Transport, operation, error.to_string())
}

#[cfg(test)]
mod tests {
    use super::SshConfig;
    use std::time::Duration;

    #[test]
    fn rejects_blank_host() {
        assert!(SshConfig::new("   ", "adam").is_err());
    }

    #[test]
    fn preserves_overrides() -> Result<(), Box<dyn std::error::Error>> {
        let config = SshConfig::new("example.test", "adam")?
            .with_port(2222)
            .with_timeout(Duration::from_secs(45));
        assert_eq!(config.port(), 2222);
        assert_eq!(config.timeout(), Duration::from_secs(45));
        Ok(())
    }
}
