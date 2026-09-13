use cyber_pumpkin_backend::Backend;
use cyber_pumpkin_core::BackendId;
use cyber_pumpkin_local::LocalBackend;
use cyber_pumpkin_sftp::{SftpAuth, SftpBackend, SftpConfig};

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
    },
}

impl PaneConnection {
    pub(crate) fn local(id: &str) -> Result<Self, String> {
        Ok(Self::Local {
            id: BackendId::new(id).map_err(|error| error.to_string())?,
        })
    }

    pub(crate) fn sftp(id: &str, host: &str, username: &str, port: u16) -> Result<Self, String> {
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
        })
    }

    pub(crate) fn backend_id(&self) -> BackendId {
        match self {
            Self::Local { id } | Self::Sftp { id, .. } => id.clone(),
        }
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
            } => {
                let config = SftpConfig::new(id.clone(), host, username)
                    .map_err(|error| error.to_string())?
                    .with_port(*port);
                let backend = SftpBackend::connect(&config, &SftpAuth::Agent)
                    .map_err(|error| error.to_string())?;
                Ok(Box::new(backend))
            }
        }
    }
}
