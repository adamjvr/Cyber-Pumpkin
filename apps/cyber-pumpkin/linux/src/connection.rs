use cyber_pumpkin_application::AppPreferences;
use cyber_pumpkin_backend::Backend;
use cyber_pumpkin_core::BackendId;
use cyber_pumpkin_local::LocalBackend;
use cyber_pumpkin_sftp::{PrivateKeyAuth, SftpAuth, SftpBackend, SftpConfig};
use std::time::Duration;

/// Authentication held only in the live pane connection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum PaneSftpAuth {
    Agent,
    Password(String),
    PrivateKey {
        private_key: String,
        passphrase: Option<String>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum PaneConnection {
    Local {
        id: BackendId,
    },
    Sftp {
        id: BackendId,
        host: String,
        username: String,
        port: u16,
        auth: PaneSftpAuth,
        trusted_fingerprint: Option<String>,
    },
}

impl PaneConnection {
    pub(crate) fn local(id: &str) -> Result<Self, String> {
        Ok(Self::Local {
            id: BackendId::new(id).map_err(|error| error.to_string())?,
        })
    }

    pub(crate) fn sftp(
        id: &str,
        host: &str,
        username: &str,
        port: u16,
        auth: PaneSftpAuth,
        trusted_fingerprint: Option<String>,
    ) -> Result<Self, String> {
        if host.trim().is_empty() {
            return Err("SFTP host must not be blank.".to_owned());
        }
        if username.trim().is_empty() {
            return Err("SFTP username must not be blank.".to_owned());
        }
        if port == 0 {
            return Err("SFTP port must be non-zero.".to_owned());
        }
        Ok(Self::Sftp {
            id: BackendId::new(id).map_err(|error| error.to_string())?,
            host: host.trim().to_owned(),
            username: username.trim().to_owned(),
            port,
            auth,
            trusted_fingerprint,
        })
    }

    pub(crate) fn backend_id(&self) -> BackendId {
        match self {
            Self::Local { id } | Self::Sftp { id, .. } => id.clone(),
        }
    }

    pub(crate) const fn is_local(&self) -> bool {
        matches!(self, Self::Local { .. })
    }

    pub(crate) fn display_name(&self) -> String {
        match self {
            Self::Local { .. } => "Local".to_owned(),
            Self::Sftp {
                host,
                username,
                port,
                ..
            } => format!("{username}@{host}:{port}"),
        }
    }

    pub(crate) fn connect_backend(&self) -> Result<Box<dyn Backend>, String> {
        match self {
            Self::Local { id } => Ok(Box::new(LocalBackend::new(id.clone()))),
            Self::Sftp {
                id,
                host,
                username,
                port,
                auth,
                trusted_fingerprint,
            } => {
                let preferences = AppPreferences::load_default().unwrap_or_default();
                let mut config = SftpConfig::new(id.clone(), host, username)
                    .map_err(|error| error.to_string())?
                    .with_port(*port)
                    .with_max_redials(2);

                if preferences.advanced.keep_connections_alive {
                    config = config.with_keepalive_interval(Some(Duration::from_secs(60)));
                } else {
                    config = config.with_keepalive_interval(None);
                }

                if let Some(fingerprint) = trusted_fingerprint {
                    config = config.with_trusted_host_fingerprint(fingerprint.clone());
                }

                let auth = match auth {
                    PaneSftpAuth::Agent => SftpAuth::Agent,
                    PaneSftpAuth::Password(password) => SftpAuth::Password(password.clone()),
                    PaneSftpAuth::PrivateKey {
                        private_key,
                        passphrase,
                    } => {
                        let mut key = PrivateKeyAuth::new(private_key);
                        if let Some(passphrase) = passphrase {
                            key = key.with_passphrase(passphrase.clone());
                        }
                        SftpAuth::PrivateKey(Box::new(key))
                    }
                };

                let backend =
                    SftpBackend::connect(&config, &auth).map_err(|error| error.to_string())?;
                Ok(Box::new(backend))
            }
        }
    }
}
