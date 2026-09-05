//! Cyber-Pumpkin command-line companion.

use cyber_pumpkin_backend::Backend;
use cyber_pumpkin_core::{BackendId, BackendPath, FileEntry};
use cyber_pumpkin_local::LocalBackend;
use cyber_pumpkin_sftp::{SftpAuth, SftpBackend, SftpConfig};
use cyber_pumpkin_transfer::{Endpoint, TransferId, TransferJob, TransferSpec, execute_file};
use std::error::Error;
use std::io;

const USAGE: &str = r"Cyber-Pumpkin cpk

USAGE:
  cpk about
  cpk local-ls <path>
  cpk local-stat <path>
  cpk local-copy <source> <destination>
  cpk sftp-ls <host> <username> <remote-path> [port]
  cpk sftp-put <local-source> <host> <username> <remote-destination> [port]
  cpk sftp-get <host> <username> <remote-source> <local-destination> [port]
  cpk sftp-rm <host> <username> <remote-path> [port]

SFTP authentication uses the SSH agent. Host keys must already match
~/.ssh/known_hosts. Passwords are intentionally not accepted as CLI arguments.";

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args().skip(1);
    let command = args.next();

    match command.as_deref() {
        Some("about") => {
            ensure_finished(&mut args)?;
            println!("Cyber-Pumpkin cpk 0.1.0 — Phase 1 Local + SFTP vertical slice");
        }
        Some("local-ls") => {
            let path = next_arg(&mut args, "path")?;
            ensure_finished(&mut args)?;
            local_list(&path)?;
        }
        Some("local-stat") => {
            let path = next_arg(&mut args, "path")?;
            ensure_finished(&mut args)?;
            local_stat(&path)?;
        }
        Some("local-copy") => {
            let source = next_arg(&mut args, "source")?;
            let destination = next_arg(&mut args, "destination")?;
            ensure_finished(&mut args)?;
            local_copy(&source, &destination)?;
        }
        Some("sftp-ls") => {
            let host = next_arg(&mut args, "host")?;
            let username = next_arg(&mut args, "username")?;
            let path = next_arg(&mut args, "remote-path")?;
            let port = optional_port(args.next())?;
            ensure_finished(&mut args)?;
            sftp_list(&host, &username, &path, port)?;
        }
        Some("sftp-put") => {
            let source = next_arg(&mut args, "local-source")?;
            let host = next_arg(&mut args, "host")?;
            let username = next_arg(&mut args, "username")?;
            let destination = next_arg(&mut args, "remote-destination")?;
            let port = optional_port(args.next())?;
            ensure_finished(&mut args)?;
            sftp_put(&source, &host, &username, &destination, port)?;
        }
        Some("sftp-get") => {
            let host = next_arg(&mut args, "host")?;
            let username = next_arg(&mut args, "username")?;
            let source = next_arg(&mut args, "remote-source")?;
            let destination = next_arg(&mut args, "local-destination")?;
            let port = optional_port(args.next())?;
            ensure_finished(&mut args)?;
            sftp_get(&host, &username, &source, &destination, port)?;
        }
        Some("sftp-rm") => {
            let host = next_arg(&mut args, "host")?;
            let username = next_arg(&mut args, "username")?;
            let path = next_arg(&mut args, "remote-path")?;
            let port = optional_port(args.next())?;
            ensure_finished(&mut args)?;
            sftp_remove(&host, &username, &path, port)?;
        }
        Some("help" | "--help" | "-h") | None => println!("{USAGE}"),
        Some(other) => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("unknown command: {other}\n\n{USAGE}"),
            )
            .into());
        }
    }

    Ok(())
}

fn next_arg(
    args: &mut impl Iterator<Item = String>,
    name: &'static str,
) -> Result<String, io::Error> {
    args.next().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("missing required argument: {name}"),
        )
    })
}

fn ensure_finished(args: &mut impl Iterator<Item = String>) -> Result<(), io::Error> {
    if let Some(extra) = args.next() {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("unexpected argument: {extra}"),
        ))
    } else {
        Ok(())
    }
}

fn optional_port(value: Option<String>) -> Result<u16, Box<dyn Error>> {
    let port = match value {
        Some(text) => text.parse::<u16>()?,
        None => 22,
    };
    if port == 0 {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "port must be non-zero").into());
    }
    Ok(port)
}

fn local_backend() -> Result<LocalBackend, Box<dyn Error>> {
    Ok(LocalBackend::new(BackendId::new("local")?))
}

fn remote_backend(host: &str, username: &str, port: u16) -> Result<SftpBackend, Box<dyn Error>> {
    let config = SftpConfig::new(BackendId::new("sftp")?, host, username)?.with_port(port);
    Ok(SftpBackend::connect(&config, &SftpAuth::Agent)?)
}

fn local_list(path: &str) -> Result<(), Box<dyn Error>> {
    let backend = local_backend()?;
    for entry in backend.list(&BackendPath::new(path)?)? {
        print_entry(&entry);
    }
    Ok(())
}

fn local_stat(path: &str) -> Result<(), Box<dyn Error>> {
    let backend = local_backend()?;
    let entry = backend.stat(&BackendPath::new(path)?)?;
    print_entry(&entry);
    Ok(())
}

fn local_copy(source: &str, destination: &str) -> Result<(), Box<dyn Error>> {
    let backend = local_backend()?;
    let backend_id = backend.id().clone();
    let spec = TransferSpec {
        source: Endpoint {
            backend: backend_id.clone(),
            path: BackendPath::new(source)?,
        },
        destination: Endpoint {
            backend: backend_id,
            path: BackendPath::new(destination)?,
        },
    };
    let mut job = TransferJob::new(TransferId::new(1)?, spec);
    let report = execute_file(&mut job, &backend, &backend)?;
    println!("copied {} bytes", report.bytes_copied());
    Ok(())
}

fn sftp_list(host: &str, username: &str, path: &str, port: u16) -> Result<(), Box<dyn Error>> {
    let backend = remote_backend(host, username, port)?;
    for entry in backend.list(&BackendPath::new(path)?)? {
        print_entry(&entry);
    }
    Ok(())
}

fn sftp_put(
    source: &str,
    host: &str,
    username: &str,
    destination: &str,
    port: u16,
) -> Result<(), Box<dyn Error>> {
    let local = local_backend()?;
    let remote = remote_backend(host, username, port)?;
    let spec = TransferSpec {
        source: Endpoint {
            backend: local.id().clone(),
            path: BackendPath::new(source)?,
        },
        destination: Endpoint {
            backend: remote.id().clone(),
            path: BackendPath::new(destination)?,
        },
    };
    let mut job = TransferJob::new(TransferId::new(1)?, spec);
    let report = execute_file(&mut job, &local, &remote)?;
    println!("uploaded {} bytes", report.bytes_copied());
    Ok(())
}

fn sftp_get(
    host: &str,
    username: &str,
    source: &str,
    destination: &str,
    port: u16,
) -> Result<(), Box<dyn Error>> {
    let remote = remote_backend(host, username, port)?;
    let local = local_backend()?;
    let spec = TransferSpec {
        source: Endpoint {
            backend: remote.id().clone(),
            path: BackendPath::new(source)?,
        },
        destination: Endpoint {
            backend: local.id().clone(),
            path: BackendPath::new(destination)?,
        },
    };
    let mut job = TransferJob::new(TransferId::new(1)?, spec);
    let report = execute_file(&mut job, &remote, &local)?;
    println!("downloaded {} bytes", report.bytes_copied());
    Ok(())
}

fn print_entry(entry: &FileEntry) {
    let size = entry
        .size
        .map_or_else(|| "-".to_owned(), |value| value.to_string());
    println!("{:?}\t{size}\t{}", entry.kind, entry.path.as_str());
}

fn sftp_remove(host: &str, username: &str, path: &str, port: u16) -> Result<(), Box<dyn Error>> {
    let backend = remote_backend(host, username, port)?;
    backend.remove(&BackendPath::new(path)?)?;
    println!("removed {path}");
    Ok(())
}
