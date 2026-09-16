//! Cyber-Pumpkin command-line companion.

use cyber_pumpkin_application::{
    ConnectionProfiles, SavedConnection, application_support_directory,
};
use cyber_pumpkin_backend::Backend;
use cyber_pumpkin_core::{BackendId, BackendPath, FileEntry};
use cyber_pumpkin_history::HistoryLog;
use cyber_pumpkin_local::LocalBackend;
use cyber_pumpkin_reliability::{ReliableTransferOutcome, execute_file_reliable};
use cyber_pumpkin_sftp::{SftpAuth, SftpBackend, SftpConfig};
use cyber_pumpkin_sync::{ConflictPolicy, SyncExecutionOutcome, execute_plan_with_conflicts};
use cyber_pumpkin_sync_plan::{SyncOptions, plan_one_way};
use cyber_pumpkin_transfer::{
    CancellationToken, DestinationPolicy, Endpoint, TransferId, TransferJob, TransferSpec,
    TreeTransferOutcome, execute_file, execute_file_with_policy, execute_tree_controlled,
};
use std::error::Error;
use std::io;

const USAGE: &str = r"Cyber-Pumpkin cpk

USAGE:
  cpk about
  cpk local-ls <path>
  cpk local-stat <path>
  cpk local-copy <source> <destination>
  cpk local-copy-safe <source> <destination>
  cpk local-copy-tree-safe <source> <destination>
  cpk local-mkdir <path>
  cpk local-rename <source> <destination>
  cpk local-rm <path>
  cpk local-replace-safe <source> <destination>
  cpk sync-plan-local <source-dir> <destination-dir> [--delete-orphans]
  cpk sync-run-local <source-dir> <destination-dir> [--delete-orphans] [--conflicts=fail|skip|replace]
  cpk history-list
  cpk history-clear
  cpk profile-list
  cpk profile-add <id> <name> <host> <username> <port> <remote-path>
  cpk profile-rm <id>
  cpk sftp-ls <host> <username> <remote-path> [port]
  cpk sftp-put <local-source> <host> <username> <remote-destination> [port]
  cpk sftp-get <host> <username> <remote-source> <local-destination> [port]
  cpk sftp-copy-tree-put <local-source> <host> <username> <remote-destination> [port]
  cpk sftp-copy-tree-get <host> <username> <remote-source> <local-destination> [port]
  cpk sftp-mkdir <host> <username> <remote-path> [port]
  cpk sftp-rename <host> <username> <remote-source> <remote-destination> [port]
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
        Some("local-ls") => local_ls_command(&mut args)?,
        Some("local-stat") => local_stat_command(&mut args)?,
        Some("local-copy") => local_copy_command(&mut args, false)?,
        Some("local-copy-safe") => local_copy_command(&mut args, true)?,
        Some("local-copy-tree-safe") => local_copy_tree_command(&mut args)?,
        Some("local-mkdir") => local_mkdir_command(&mut args)?,
        Some("local-rename") => local_rename_command(&mut args)?,
        Some("local-rm") => local_remove_command(&mut args)?,
        Some("local-replace-safe") => local_replace_safe_command(&mut args)?,
        Some("sync-plan-local") => sync_plan_local_command(&mut args)?,
        Some("sync-run-local") => sync_run_local_command(&mut args)?,
        Some("history-list") => history_list_command(&mut args)?,
        Some("history-clear") => history_clear_command(&mut args)?,
        Some("profile-list") => profile_list_command(&mut args)?,
        Some("profile-add") => profile_add_command(&mut args)?,
        Some("profile-rm") => profile_remove_command(&mut args)?,
        Some("sftp-ls") => sftp_list_command(&mut args)?,
        Some("sftp-put") => sftp_put_command(&mut args)?,
        Some("sftp-get") => sftp_get_command(&mut args)?,
        Some("sftp-copy-tree-put") => sftp_copy_tree_put_command(&mut args)?,
        Some("sftp-copy-tree-get") => sftp_copy_tree_get_command(&mut args)?,
        Some("sftp-mkdir") => sftp_mkdir_command(&mut args)?,
        Some("sftp-rename") => sftp_rename_command(&mut args)?,
        Some("sftp-rm") => sftp_remove_command(&mut args)?,
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

fn local_ls_command(args: &mut impl Iterator<Item = String>) -> Result<(), Box<dyn Error>> {
    let path = next_arg(args, "path")?;
    ensure_finished(args)?;
    local_list(&path)
}

fn local_stat_command(args: &mut impl Iterator<Item = String>) -> Result<(), Box<dyn Error>> {
    let path = next_arg(args, "path")?;
    ensure_finished(args)?;
    local_stat(&path)
}

fn local_copy_command(
    args: &mut impl Iterator<Item = String>,
    safe: bool,
) -> Result<(), Box<dyn Error>> {
    let source = next_arg(args, "source")?;
    let destination = next_arg(args, "destination")?;
    ensure_finished(args)?;
    if safe {
        local_copy_safe(&source, &destination)
    } else {
        local_copy(&source, &destination)
    }
}

fn local_copy_tree_command(args: &mut impl Iterator<Item = String>) -> Result<(), Box<dyn Error>> {
    let source = next_arg(args, "source")?;
    let destination = next_arg(args, "destination")?;
    ensure_finished(args)?;
    local_copy_tree_safe(&source, &destination)
}

fn local_mkdir_command(args: &mut impl Iterator<Item = String>) -> Result<(), Box<dyn Error>> {
    let path = next_arg(args, "path")?;
    ensure_finished(args)?;
    local_mkdir(&path)
}

fn local_rename_command(args: &mut impl Iterator<Item = String>) -> Result<(), Box<dyn Error>> {
    let source = next_arg(args, "source")?;
    let destination = next_arg(args, "destination")?;
    ensure_finished(args)?;
    local_rename(&source, &destination)
}

fn local_remove_command(args: &mut impl Iterator<Item = String>) -> Result<(), Box<dyn Error>> {
    let path = next_arg(args, "path")?;
    ensure_finished(args)?;
    local_remove(&path)
}

fn local_replace_safe_command(
    args: &mut impl Iterator<Item = String>,
) -> Result<(), Box<dyn Error>> {
    let source = next_arg(args, "source")?;
    let destination = next_arg(args, "destination")?;
    ensure_finished(args)?;

    let backend_id = BackendId::new("local-reliable")?;
    let backend = LocalBackend::new(backend_id.clone());
    let outcome = execute_file_reliable(
        TransferId::new(1)?,
        Endpoint {
            backend: backend_id.clone(),
            path: BackendPath::new(source)?,
        },
        Endpoint {
            backend: backend_id,
            path: BackendPath::new(destination)?,
        },
        &backend,
        &backend,
        &CancellationToken::new(),
        |_| {},
    )?;

    match outcome {
        ReliableTransferOutcome::Completed(report) => println!(
            "completed bytes={} replaced_existing={}",
            report.bytes_copied(),
            report.replaced_existing(),
        ),
        ReliableTransferOutcome::Cancelled { bytes_copied } => {
            println!("cancelled bytes={bytes_copied}");
        }
    }
    Ok(())
}

fn sync_plan_local_command(args: &mut impl Iterator<Item = String>) -> Result<(), Box<dyn Error>> {
    let source = next_arg(args, "source-dir")?;
    let destination = next_arg(args, "destination-dir")?;
    let options = sync_options(args)?;
    let (_, _, plan) = local_sync_plan(&source, &destination, &options)?;
    let summary = plan.summary();
    println!(
        "create_dirs={} copy_files={} removals={} conflicts={} skipped={}",
        summary.directories_to_create,
        summary.files_to_copy,
        summary.entries_to_remove,
        summary.conflicts,
        summary.skipped,
    );
    for action in plan.actions() {
        println!(
            "{:?}\t{}\t{}",
            action.kind, action.relative_path, action.reason
        );
    }
    Ok(())
}

fn sync_run_local_command(args: &mut impl Iterator<Item = String>) -> Result<(), Box<dyn Error>> {
    let source = next_arg(args, "source-dir")?;
    let destination = next_arg(args, "destination-dir")?;
    let (options, conflict_policy) = sync_run_options(args)?;
    let (source_backend, destination_backend, plan) =
        local_sync_plan(&source, &destination, &options)?;
    let outcome = execute_plan_with_conflicts(
        &plan,
        &source_backend,
        &destination_backend,
        &CancellationToken::new(),
        conflict_policy,
        |_| {},
    )?;
    match outcome {
        SyncExecutionOutcome::Completed(report) => println!(
            "completed files={} directories={} removed={} skipped={} bytes={}",
            report.files_copied,
            report.directories_created,
            report.entries_removed,
            report.skipped,
            report.bytes_copied,
        ),
        SyncExecutionOutcome::Cancelled(report) => println!(
            "cancelled files={} directories={} removed={} skipped={} bytes={}",
            report.files_copied,
            report.directories_created,
            report.entries_removed,
            report.skipped,
            report.bytes_copied,
        ),
    }
    Ok(())
}

fn sync_run_options(
    args: &mut impl Iterator<Item = String>,
) -> Result<(SyncOptions, ConflictPolicy), Box<dyn Error>> {
    let mut delete_orphans = false;
    let mut conflict_policy = ConflictPolicy::Fail;

    for option in args {
        match option.as_str() {
            "--delete-orphans" => delete_orphans = true,
            "--conflicts=fail" => conflict_policy = ConflictPolicy::Fail,
            "--conflicts=skip" => conflict_policy = ConflictPolicy::Skip,
            "--conflicts=replace" => conflict_policy = ConflictPolicy::Replace,
            other => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("unexpected sync option: {other}"),
                )
                .into());
            }
        }
    }

    Ok((
        SyncOptions {
            delete_orphans,
            ..SyncOptions::default()
        },
        conflict_policy,
    ))
}

fn sync_options(args: &mut impl Iterator<Item = String>) -> Result<SyncOptions, Box<dyn Error>> {
    let delete_orphans = match args.next().as_deref() {
        None => false,
        Some("--delete-orphans") => true,
        Some(other) => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("unexpected sync option: {other}"),
            )
            .into());
        }
    };
    ensure_finished(args)?;
    Ok(SyncOptions {
        delete_orphans,
        ..SyncOptions::default()
    })
}

fn local_sync_plan(
    source: &str,
    destination: &str,
    options: &SyncOptions,
) -> Result<
    (
        LocalBackend,
        LocalBackend,
        cyber_pumpkin_sync_plan::SyncPlan,
    ),
    Box<dyn Error>,
> {
    let source_backend = LocalBackend::new(BackendId::new("sync-source")?);
    let destination_backend = LocalBackend::new(BackendId::new("sync-destination")?);
    let plan = plan_one_way(
        &source_backend,
        &BackendPath::new(source)?,
        &destination_backend,
        &BackendPath::new(destination)?,
        options,
    )?;
    Ok((source_backend, destination_backend, plan))
}

fn history_list_command(args: &mut impl Iterator<Item = String>) -> Result<(), Box<dyn Error>> {
    ensure_finished(args)?;
    let history = HistoryLog::load(&application_support_directory().join("history.json"))?;
    for entry in history.entries.iter().rev() {
        println!(
            "{}\t{:?}\t{:?}\t{}\t{}",
            entry.timestamp, entry.kind, entry.state, entry.label, entry.detail,
        );
    }
    Ok(())
}

fn history_clear_command(args: &mut impl Iterator<Item = String>) -> Result<(), Box<dyn Error>> {
    ensure_finished(args)?;
    let history = HistoryLog::default();
    history.save(&application_support_directory().join("history.json"))?;
    println!("activity history cleared");
    Ok(())
}

fn profile_list_command(args: &mut impl Iterator<Item = String>) -> Result<(), Box<dyn Error>> {
    ensure_finished(args)?;
    for profile in ConnectionProfiles::load_default()?.profiles {
        println!(
            "{}\t{}\t{}\t{}\t{}\t{}",
            profile.id,
            profile.name,
            profile.host,
            profile.username,
            profile.port,
            profile.initial_path
        );
    }
    Ok(())
}

fn profile_add_command(args: &mut impl Iterator<Item = String>) -> Result<(), Box<dyn Error>> {
    let id = next_arg(args, "id")?;
    let name = next_arg(args, "name")?;
    let host = next_arg(args, "host")?;
    let username = next_arg(args, "username")?;
    let port = next_arg(args, "port")?.parse::<u16>()?;
    let path = next_arg(args, "remote-path")?;
    ensure_finished(args)?;

    let profile = SavedConnection::new(id, name, host, username, port, path)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
    let mut profiles = ConnectionProfiles::load_default()?;
    profiles.upsert(profile);
    profiles.save_default()?;
    println!("saved connection");
    Ok(())
}

fn profile_remove_command(args: &mut impl Iterator<Item = String>) -> Result<(), Box<dyn Error>> {
    let id = next_arg(args, "id")?;
    ensure_finished(args)?;
    let mut profiles = ConnectionProfiles::load_default()?;
    if profiles.remove(&id) {
        profiles.save_default()?;
        println!("removed {id}");
    } else {
        println!("connection not found: {id}");
    }
    Ok(())
}

fn sftp_list_command(args: &mut impl Iterator<Item = String>) -> Result<(), Box<dyn Error>> {
    let host = next_arg(args, "host")?;
    let username = next_arg(args, "username")?;
    let path = next_arg(args, "remote-path")?;
    let port = optional_port(args.next())?;
    ensure_finished(args)?;
    sftp_list(&host, &username, &path, port)
}

fn sftp_put_command(args: &mut impl Iterator<Item = String>) -> Result<(), Box<dyn Error>> {
    let source = next_arg(args, "local-source")?;
    let host = next_arg(args, "host")?;
    let username = next_arg(args, "username")?;
    let destination = next_arg(args, "remote-destination")?;
    let port = optional_port(args.next())?;
    ensure_finished(args)?;
    sftp_put(&source, &host, &username, &destination, port)
}

fn sftp_get_command(args: &mut impl Iterator<Item = String>) -> Result<(), Box<dyn Error>> {
    let host = next_arg(args, "host")?;
    let username = next_arg(args, "username")?;
    let source = next_arg(args, "remote-source")?;
    let destination = next_arg(args, "local-destination")?;
    let port = optional_port(args.next())?;
    ensure_finished(args)?;
    sftp_get(&host, &username, &source, &destination, port)
}

fn sftp_copy_tree_put_command(
    args: &mut impl Iterator<Item = String>,
) -> Result<(), Box<dyn Error>> {
    let source = next_arg(args, "local-source")?;
    let host = next_arg(args, "host")?;
    let username = next_arg(args, "username")?;
    let destination = next_arg(args, "remote-destination")?;
    let port = optional_port(args.next())?;
    ensure_finished(args)?;
    sftp_copy_tree_put(&source, &host, &username, &destination, port)
}

fn sftp_copy_tree_get_command(
    args: &mut impl Iterator<Item = String>,
) -> Result<(), Box<dyn Error>> {
    let host = next_arg(args, "host")?;
    let username = next_arg(args, "username")?;
    let source = next_arg(args, "remote-source")?;
    let destination = next_arg(args, "local-destination")?;
    let port = optional_port(args.next())?;
    ensure_finished(args)?;
    sftp_copy_tree_get(&host, &username, &source, &destination, port)
}

fn sftp_mkdir_command(args: &mut impl Iterator<Item = String>) -> Result<(), Box<dyn Error>> {
    let host = next_arg(args, "host")?;
    let username = next_arg(args, "username")?;
    let path = next_arg(args, "remote-path")?;
    let port = optional_port(args.next())?;
    ensure_finished(args)?;
    sftp_mkdir(&host, &username, &path, port)
}

fn sftp_rename_command(args: &mut impl Iterator<Item = String>) -> Result<(), Box<dyn Error>> {
    let host = next_arg(args, "host")?;
    let username = next_arg(args, "username")?;
    let source = next_arg(args, "remote-source")?;
    let destination = next_arg(args, "remote-destination")?;
    let port = optional_port(args.next())?;
    ensure_finished(args)?;
    sftp_rename(&host, &username, &source, &destination, port)
}

fn sftp_remove_command(args: &mut impl Iterator<Item = String>) -> Result<(), Box<dyn Error>> {
    let host = next_arg(args, "host")?;
    let username = next_arg(args, "username")?;
    let path = next_arg(args, "remote-path")?;
    let port = optional_port(args.next())?;
    ensure_finished(args)?;
    sftp_remove(&host, &username, &path, port)
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
    let spec = local_transfer_spec(&backend, source, destination)?;
    let mut job = TransferJob::new(TransferId::new(1)?, spec);
    let report = execute_file(&mut job, &backend, &backend)?;
    println!("copied {} bytes", report.bytes_copied());
    Ok(())
}

fn local_copy_safe(source: &str, destination: &str) -> Result<(), Box<dyn Error>> {
    let backend = local_backend()?;
    let spec = local_transfer_spec(&backend, source, destination)?;
    let mut job = TransferJob::new(TransferId::new(1)?, spec);
    let report = execute_file_with_policy(
        &mut job,
        &backend,
        &backend,
        DestinationPolicy::FailIfExists,
    )?;
    println!("copied {} bytes", report.bytes_copied());
    Ok(())
}

fn local_copy_tree_safe(source: &str, destination: &str) -> Result<(), Box<dyn Error>> {
    let backend = local_backend()?;
    let spec = local_transfer_spec(&backend, source, destination)?;
    let cancellation = CancellationToken::new();

    let outcome = execute_tree_controlled(&spec, &backend, &backend, &cancellation, |_| {})?;

    match outcome {
        TreeTransferOutcome::Completed(report) => {
            println!(
                "copied {} files, {} directories, {} bytes",
                report.files_copied(),
                report.directories_created(),
                report.bytes_copied()
            );
            Ok(())
        }
        TreeTransferOutcome::Cancelled(_) => {
            Err(io::Error::new(io::ErrorKind::Interrupted, "tree copy was cancelled").into())
        }
    }
}

fn local_transfer_spec(
    backend: &LocalBackend,
    source: &str,
    destination: &str,
) -> Result<TransferSpec, Box<dyn Error>> {
    let backend_id = backend.id().clone();
    Ok(TransferSpec {
        source: Endpoint {
            backend: backend_id.clone(),
            path: BackendPath::new(source)?,
        },
        destination: Endpoint {
            backend: backend_id,
            path: BackendPath::new(destination)?,
        },
    })
}

fn local_mkdir(path: &str) -> Result<(), Box<dyn Error>> {
    let backend = local_backend()?;
    backend.create_dir(&BackendPath::new(path)?)?;
    println!("created {path}");
    Ok(())
}

fn local_rename(source: &str, destination: &str) -> Result<(), Box<dyn Error>> {
    let backend = local_backend()?;
    backend.rename(&BackendPath::new(source)?, &BackendPath::new(destination)?)?;
    println!("renamed {source} -> {destination}");
    Ok(())
}

fn local_remove(path: &str) -> Result<(), Box<dyn Error>> {
    let backend = local_backend()?;
    backend.remove(&BackendPath::new(path)?)?;
    println!("removed {path}");
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
    let modified = entry
        .modified
        .map_or_else(|| "-".to_owned(), |value| value.to_string());
    println!(
        "{:?}\t{size}\t{modified}\t{}",
        entry.kind,
        entry.path.as_str()
    );
}

fn sftp_copy_tree_put(
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
    run_tree_copy(&spec, &local, &remote)
}

fn sftp_copy_tree_get(
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
    run_tree_copy(&spec, &remote, &local)
}

fn run_tree_copy(
    spec: &TransferSpec,
    source: &dyn Backend,
    destination: &dyn Backend,
) -> Result<(), Box<dyn Error>> {
    let cancellation = CancellationToken::new();
    match execute_tree_controlled(spec, source, destination, &cancellation, |_| {})? {
        TreeTransferOutcome::Completed(report) => {
            println!(
                "copied {} files, {} directories, {} bytes",
                report.files_copied(),
                report.directories_created(),
                report.bytes_copied()
            );
            Ok(())
        }
        TreeTransferOutcome::Cancelled(_) => {
            Err(io::Error::new(io::ErrorKind::Interrupted, "tree copy was cancelled").into())
        }
    }
}

fn sftp_mkdir(host: &str, username: &str, path: &str, port: u16) -> Result<(), Box<dyn Error>> {
    let backend = remote_backend(host, username, port)?;
    backend.create_dir(&BackendPath::new(path)?)?;
    println!("created {path}");
    Ok(())
}

fn sftp_rename(
    host: &str,
    username: &str,
    source: &str,
    destination: &str,
    port: u16,
) -> Result<(), Box<dyn Error>> {
    let backend = remote_backend(host, username, port)?;
    backend.rename(&BackendPath::new(source)?, &BackendPath::new(destination)?)?;
    println!("renamed {source} -> {destination}");
    Ok(())
}

fn sftp_remove(host: &str, username: &str, path: &str, port: u16) -> Result<(), Box<dyn Error>> {
    let backend = remote_backend(host, username, port)?;
    backend.remove(&BackendPath::new(path)?)?;
    println!("removed {path}");
    Ok(())
}
