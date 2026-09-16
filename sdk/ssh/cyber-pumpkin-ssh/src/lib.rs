//! Shared SSH transport, host-trust, and authentication layer.

use ssh2::{CheckResult, HashType, KnownHostFileKind, Session};
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
    trusted_host_fingerprint: Option<String>,
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
            trusted_host_fingerprint: None,
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

    /// Allows one exact application-trusted fingerprint for an otherwise
    /// unknown host. A `known_hosts` mismatch is never bypassed.
    #[must_use]
    pub fn with_trusted_host_fingerprint(mut self, fingerprint: impl Into<String>) -> Self {
        self.trusted_host_fingerprint = Some(fingerprint.into());
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

    /// Returns the explicit application trust override.
    #[must_use]
    pub fn trusted_host_fingerprint(&self) -> Option<&str> {
        self.trusted_host_fingerprint.as_deref()
    }
}

/// Private-key authentication material.
#[derive(Clone, Debug, Eq, PartialEq)]
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
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SshAuth {
    /// SSH agent.
    Agent,
    /// Username/password authentication. Password remains in memory only.
    Password(String),
    /// Private key.
    PrivateKey(Box<PrivateKeyAuth>),
}

/// Host-key relationship to OpenSSH `known_hosts`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HostKeyStatus {
    /// Host key matches `known_hosts`.
    Match,
    /// Host is absent from `known_hosts`.
    Unknown,
    /// Host exists but the presented key differs.
    Mismatch,
    /// `known_hosts` could not classify the key.
    Failure,
}

/// Host-key probe result produced before authentication.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HostKeyProbe {
    fingerprint: String,
    status: HostKeyStatus,
}

impl HostKeyProbe {
    /// Returns the SHA-256 host-key fingerprint.
    #[must_use]
    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
    }

    /// Returns known-hosts status.
    #[must_use]
    pub const fn status(&self) -> HostKeyStatus {
        self.status
    }
}

/// Performs TCP + SSH handshake and returns host identity without authenticating.
///
/// # Errors
///
/// Returns [`SshError`] for transport, protocol, or host-key failures.
pub fn probe_host_key(config: &SshConfig) -> Result<HostKeyProbe, SshError> {
    validate_config(config)?;
    let session = handshake_session(config)?;
    let (host_key, _) = session.host_key().ok_or_else(|| {
        SshError::new(
            SshErrorKind::HostKey,
            "read SSH host key",
            "server did not provide a host key",
        )
    })?;
    let fingerprint = host_key_fingerprint(&session)?;
    let mut known_hosts = session.known_hosts().map_err(|error| {
        SshError::new(
            SshErrorKind::HostKey,
            "initialize known_hosts",
            error.to_string(),
        )
    })?;
    load_known_hosts(&mut known_hosts, config)?;
    let status = match known_hosts.check_port(config.host(), config.port(), host_key) {
        CheckResult::Match => HostKeyStatus::Match,
        CheckResult::NotFound => HostKeyStatus::Unknown,
        CheckResult::Mismatch => HostKeyStatus::Mismatch,
        CheckResult::Failure => HostKeyStatus::Failure,
    };
    Ok(HostKeyProbe {
        fingerprint,
        status,
    })
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
        validate_config(config)?;
        let session = handshake_session(config)?;
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

fn validate_config(config: &SshConfig) -> Result<(), SshError> {
    if config.port() == 0 {
        Err(SshError::new(
            SshErrorKind::InvalidInput,
            "configure SSH",
            "port must be non-zero",
        ))
    } else {
        Ok(())
    }
}

fn handshake_session(config: &SshConfig) -> Result<Session, SshError> {
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
    Ok(session)
}

fn load_known_hosts(
    known_hosts: &mut ssh2::KnownHosts,
    config: &SshConfig,
) -> Result<(), SshError> {
    if !config.known_hosts_file().exists() {
        return Ok(());
    }
    known_hosts
        .read_file(config.known_hosts_file(), KnownHostFileKind::OpenSSH)
        .map_err(|error| {
            SshError::new(
                SshErrorKind::HostKey,
                "read known_hosts",
                format!("{}: {error}", config.known_hosts_file().display()),
            )
        })?;
    Ok(())
}

fn host_key_fingerprint(session: &Session) -> Result<String, SshError> {
    let hash = session.host_key_hash(HashType::Sha256).ok_or_else(|| {
        SshError::new(
            SshErrorKind::HostKey,
            "hash SSH host key",
            "SHA-256 host-key fingerprint unavailable",
        )
    })?;
    Ok(hash
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect::<Vec<_>>()
        .join(":"))
}

fn verify_host_key(session: &Session, config: &SshConfig) -> Result<(), SshError> {
    let (host_key, _) = session.host_key().ok_or_else(|| {
        SshError::new(
            SshErrorKind::HostKey,
            "read SSH host key",
            "server did not provide a host key",
        )
    })?;
    let fingerprint = host_key_fingerprint(session)?;
    let mut known_hosts = session.known_hosts().map_err(|error| {
        SshError::new(
            SshErrorKind::HostKey,
            "initialize known_hosts",
            error.to_string(),
        )
    })?;
    load_known_hosts(&mut known_hosts, config)?;

    match known_hosts.check_port(config.host(), config.port(), host_key) {
        CheckResult::Match => Ok(()),
        CheckResult::Mismatch => Err(SshError::new(
            SshErrorKind::HostKey,
            "verify SSH host key",
            format!("host key does not match known_hosts; fingerprint {fingerprint}"),
        )),
        CheckResult::NotFound
            if config.trusted_host_fingerprint() == Some(fingerprint.as_str()) =>
        {
            Ok(())
        }
        CheckResult::NotFound => Err(SshError::new(
            SshErrorKind::HostKey,
            "verify SSH host key",
            format!("host is not present in known_hosts; fingerprint {fingerprint}"),
        )),
        CheckResult::Failure => Err(SshError::new(
            SshErrorKind::HostKey,
            "verify SSH host key",
            format!("known_hosts verification failed; fingerprint {fingerprint}"),
        )),
    }
}

fn authenticate(session: &Session, config: &SshConfig, auth: &SshAuth) -> Result<(), SshError> {
    let result = match auth {
        SshAuth::Agent => session.userauth_agent(config.username()),
        SshAuth::Password(password) => session.userauth_password(config.username(), password),
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
        assert_eq!(config.trusted_host_fingerprint(), None);

        let trusted = config.with_trusted_host_fingerprint("AA:BB");
        assert_eq!(trusted.trusted_host_fingerprint(), Some("AA:BB"));
        Ok(())
    }
}
