//! Cyber-Pumpkin command-line companion.

use cyber_pumpkin_application::{
    AppPreferences, ConnectionProfiles, DoubleClickAction, ExistingItemAction, SavedConnection,
    application_support_directory,
};
use cyber_pumpkin_backend::{Backend, ErrorKind};
use cyber_pumpkin_core::{BackendId, BackendPath, FileEntry};
use cyber_pumpkin_file_ops::remove_tree;
use cyber_pumpkin_history::HistoryLog;
use cyber_pumpkin_local::LocalBackend;
use cyber_pumpkin_reliability::{ReliableTransferOutcome, execute_file_reliable};
use cyber_pumpkin_remote_edit::{create_session, download_initial, local_changed, upload_changed};
use cyber_pumpkin_sftp::{SftpAuth, SftpBackend, SftpConfig};
use cyber_pumpkin_sync::{ConflictPolicy, SyncExecutionOutcome, execute_plan_with_conflicts};
use cyber_pumpkin_sync_plan::{SyncOptions, plan_one_way};
use cyber_pumpkin_transfer::{
    CancellationToken, DestinationPolicy, Endpoint, TransferId, TransferJob, TransferSpec,
    TreeTransferOutcome, execute_file, execute_file_with_policy, execute_tree_controlled,
};
use cyber_pumpkin_transfer_runtime::{
    CopyConflictPolicy, CopyRequest, CopyRuntimeOutcome, execute_copy as execute_runtime_copy,
    execute_copy_with_recovery,
};
use serde::Deserialize;
use std::error::Error;
use std::fs;
use std::io;
use std::time::Duration;

const USAGE: &str = r"Cyber-Pumpkin cpk

USAGE:
  cpk about
  cpk local-ls <path>
  cpk local-stat <path>
  cpk local-mode <path>
  cpk local-chmod <octal-mode> <path>
  cpk local-copy <source> <destination>
  cpk local-copy-safe <source> <destination>
  cpk local-copy-tree-safe <source> <destination>
  cpk local-mkdir <path>
  cpk local-rename <source> <destination>
  cpk local-rm <path>
  cpk local-rm-tree <path>
  cpk local-replace-safe <source> <destination>
  cpk copy-runtime-local <source> <destination> [--conflicts=fail|replace|skip|keep-both]
  cpk sync-plan-local <source-dir> <destination-dir> [--delete-orphans]
  cpk sync-run-local <source-dir> <destination-dir> [--delete-orphans] [--conflicts=fail|skip|replace]
  cpk remote-edit-local-probe <remote-file> <workspace-dir>
  cpk history-list
  cpk history-clear
  cpk profile-list
  cpk profile-add <id> <name> <host> <username> <port> <remote-path>
  cpk profile-rm <id>
  cpk preferences-show
  cpk preferences-set <key> <value>
  cpk transfer-tree-preflight <request-json>
  cpk transfer-tree-request <request-json>
  cpk sftp-ls <host> <username> <remote-path> [port]
  cpk sftp-put <local-source> <host> <username> <remote-destination> [port]
  cpk sftp-get <host> <username> <remote-source> <local-destination> [port]
  cpk sftp-copy-tree-put <local-source> <host> <username> <remote-destination> [port]
  cpk sftp-copy-tree-get <host> <username> <remote-source> <local-destination> [port]
  cpk sftp-mkdir <host> <username> <remote-path> [port]
  cpk sftp-rename <host> <username> <remote-source> <remote-destination> [port]
  cpk sftp-rm <host> <username> <remote-path> [port]
  cpk sftp-rm-tree <host> <username> <remote-path> [port]

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
            println!(
                "Cyber-Pumpkin cpk {} — Local + SFTP release candidate",
                env!("CARGO_PKG_VERSION")
            );
        }
        Some("local-ls") => local_ls_command(&mut args)?,
        Some("local-stat") => local_stat_command(&mut args)?,
        Some("local-mode") => local_mode_command(&mut args)?,
        Some("local-chmod") => local_chmod_command(&mut args)?,
        Some("local-copy") => local_copy_command(&mut args, false)?,
        Some("local-copy-safe") => local_copy_command(&mut args, true)?,
        Some("local-copy-tree-safe") => local_copy_tree_command(&mut args)?,
        Some("local-mkdir") => local_mkdir_command(&mut args)?,
        Some("local-rename") => local_rename_command(&mut args)?,
        Some("local-rm") => local_remove_command(&mut args)?,
        Some("local-rm-tree") => local_remove_tree_command(&mut args)?,
        Some("local-replace-safe") => local_replace_safe_command(&mut args)?,
        Some("copy-runtime-local") => copy_runtime_local_command(&mut args)?,
        Some("sync-plan-local") => sync_plan_local_command(&mut args)?,
        Some("sync-run-local") => sync_run_local_command(&mut args)?,
        Some("remote-edit-local-probe") => remote_edit_local_probe_command(&mut args)?,
        Some("history-list") => history_list_command(&mut args)?,
        Some("history-clear") => history_clear_command(&mut args)?,
        Some("profile-list") => profile_list_command(&mut args)?,
        Some("profile-add") => profile_add_command(&mut args)?,
        Some("profile-rm") => profile_remove_command(&mut args)?,
        Some("preferences-show") => preferences_show_command(&mut args)?,
        Some("preferences-set") => preferences_set_command(&mut args)?,
        Some("transfer-tree-preflight") => transfer_tree_preflight_command(&mut args)?,
        Some("transfer-tree-request") => transfer_tree_request_command(&mut args)?,
        Some("sftp-ls") => sftp_list_command(&mut args)?,
        Some("sftp-put") => sftp_put_command(&mut args)?,
        Some("sftp-get") => sftp_get_command(&mut args)?,
        Some("sftp-copy-tree-put") => sftp_copy_tree_put_command(&mut args)?,
        Some("sftp-copy-tree-get") => sftp_copy_tree_get_command(&mut args)?,
        Some("sftp-mkdir") => sftp_mkdir_command(&mut args)?,
        Some("sftp-rename") => sftp_rename_command(&mut args)?,
        Some("sftp-rm") => sftp_remove_command(&mut args)?,
        Some("sftp-rm-tree") => sftp_remove_tree_command(&mut args)?,
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

fn local_mode_command(args: &mut impl Iterator<Item = String>) -> Result<(), Box<dyn Error>> {
    let path = next_arg(args, "path")?;
    ensure_finished(args)?;
    let backend = local_backend()?;
    let path = BackendPath::new(path)?;
    match backend.unix_mode(&path)? {
        Some(mode) => println!("{mode:04o}"),
        None => println!("unsupported"),
    }
    Ok(())
}

fn local_chmod_command(args: &mut impl Iterator<Item = String>) -> Result<(), Box<dyn Error>> {
    let mode_text = next_arg(args, "octal-mode")?;
    let path = next_arg(args, "path")?;
    ensure_finished(args)?;
    let mode = u32::from_str_radix(mode_text.trim_start_matches("0o"), 8)?;
    let backend = local_backend()?;
    let path = BackendPath::new(path)?;
    backend.set_unix_mode(&path, mode)?;
    println!("{mode:04o} {}", path.as_str());
    Ok(())
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

fn local_remove_tree_command(
    args: &mut impl Iterator<Item = String>,
) -> Result<(), Box<dyn Error>> {
    let path = next_arg(args, "path")?;
    ensure_finished(args)?;
    let backend = local_backend()?;
    let backend_path = BackendPath::new(path)?;
    let report = remove_tree(&backend, &backend_path)?;
    println!(
        "removed files={} directories={} entries={}",
        report.files_removed,
        report.directories_removed,
        report.entries_removed(),
    );
    Ok(())
}

fn remote_edit_local_probe_command(
    args: &mut impl Iterator<Item = String>,
) -> Result<(), Box<dyn Error>> {
    let remote_file = next_arg(args, "remote-file")?;
    let workspace = next_arg(args, "workspace-dir")?;
    ensure_finished(args)?;

    let remote_id = BackendId::new("remote-edit-probe-remote")?;
    let local_id = BackendId::new("remote-edit-probe-local")?;
    let remote_backend = LocalBackend::new(remote_id.clone());
    let local_backend = LocalBackend::new(local_id.clone());
    let remote_path = BackendPath::new(remote_file)?;
    let entry = remote_backend.stat(&remote_path)?;

    let mut session = create_session(
        TransferId::new(1)?,
        Endpoint {
            backend: remote_id,
            path: remote_path,
        },
        local_id,
        std::path::Path::new(&workspace),
        &entry.name,
    )?;

    download_initial(
        &mut session,
        &remote_backend,
        &local_backend,
        &CancellationToken::new(),
    )?;

    let mut bytes = fs::read(session.local.path.as_str())?;
    bytes.extend_from_slice(b"\nCYBER-PUMPKIN REMOTE EDIT PROBE\n");
    fs::write(session.local.path.as_str(), bytes)?;
    if !local_changed(&session, &local_backend)? {
        return Err(io::Error::other("remote-edit probe failed to detect local change").into());
    }
    let uploaded = upload_changed(
        &mut session,
        &local_backend,
        &remote_backend,
        &CancellationToken::new(),
    )?;
    println!(
        "remote-edit uploaded={} local={}",
        uploaded,
        session.local.path.as_str()
    );
    Ok(())
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

fn copy_runtime_local_command(
    args: &mut impl Iterator<Item = String>,
) -> Result<(), Box<dyn Error>> {
    let source = next_arg(args, "source")?;
    let destination = next_arg(args, "destination")?;
    let policy = match args.next().as_deref() {
        None | Some("--conflicts=fail") => CopyConflictPolicy::Fail,
        Some("--conflicts=replace") => CopyConflictPolicy::Replace,
        Some("--conflicts=skip") => CopyConflictPolicy::Skip,
        Some("--conflicts=keep-both") => CopyConflictPolicy::KeepBoth,
        Some(other) => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("unexpected copy conflict option: {other}"),
            )
            .into());
        }
    };
    ensure_finished(args)?;

    let backend = local_backend()?;
    let request = CopyRequest {
        id: TransferId::new(1)?,
        source: Endpoint {
            backend: backend.id().clone(),
            path: BackendPath::new(source)?,
        },
        destination: Endpoint {
            backend: backend.id().clone(),
            path: BackendPath::new(destination)?,
        },
        conflict_policy: policy,
    };

    match execute_runtime_copy(
        &request,
        &backend,
        &backend,
        &CancellationToken::new(),
        |_| {},
    )? {
        CopyRuntimeOutcome::Completed(report) => println!(
            "completed destination={} files={} directories={} bytes={} replaced={}",
            report.destination.as_str(),
            report.transfer.files_copied(),
            report.transfer.directories_created(),
            report.transfer.bytes_copied(),
            report.replaced_existing,
        ),
        CopyRuntimeOutcome::Skipped { destination } => {
            println!("skipped destination={}", destination.as_str());
        }
        CopyRuntimeOutcome::Cancelled(report) => println!(
            "cancelled files={} directories={} bytes={}",
            report.files_copied(),
            report.directories_created(),
            report.bytes_copied(),
        ),
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

fn sftp_remove_tree_command(args: &mut impl Iterator<Item = String>) -> Result<(), Box<dyn Error>> {
    let host = next_arg(args, "host")?;
    let username = next_arg(args, "username")?;
    let path = next_arg(args, "remote-path")?;
    let port = optional_port(args.next())?;
    ensure_finished(args)?;
    let backend = remote_backend(&host, &username, port)?;
    let path = BackendPath::new(path)?;
    let report = remove_tree(&backend, &path)?;
    println!(
        "removed files={} directories={} entries={}",
        report.files_removed,
        report.directories_removed,
        report.entries_removed(),
    );
    Ok(())
}

fn preferences_show_command(args: &mut impl Iterator<Item = String>) -> Result<(), Box<dyn Error>> {
    ensure_finished(args)?;
    println!(
        "{}",
        serde_json::to_string(&AppPreferences::load_default()?)?
    );
    Ok(())
}

fn preferences_set_command(args: &mut impl Iterator<Item = String>) -> Result<(), Box<dyn Error>> {
    let key = next_arg(args, "key")?;
    let value = next_arg(args, "value")?;
    ensure_finished(args)?;
    let mut preferences = AppPreferences::load_default()?;

    match key.as_str() {
        "files.confirm-delete" => preferences.files.confirm_delete = parse_bool(&value)?,
        "files.double-click-action" => {
            preferences.files.double_click_action = parse_double_click_action(&value)?;
        }
        "transfers.downloading-files" => {
            preferences.transfers.downloading_files = parse_existing_item_action(&value)?;
        }
        "transfers.downloading-folders" => {
            preferences.transfers.downloading_folders = parse_existing_item_action(&value)?;
        }
        "transfers.uploading-files" => {
            preferences.transfers.uploading_files = parse_existing_item_action(&value)?;
        }
        "transfers.uploading-folders" => {
            preferences.transfers.uploading_folders = parse_existing_item_action(&value)?;
        }
        "transfers.simultaneous-transfers" => {
            let count = value.parse::<u8>()?;
            if !(1..=20).contains(&count) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "simultaneous transfers must be in 1..=20",
                )
                .into());
            }
            preferences.transfers.simultaneous_transfers = count;
        }
        "transfers.keep-activity" => preferences.transfers.keep_activity = parse_bool(&value)?,
        "advanced.keep-connections-alive" => {
            preferences.advanced.keep_connections_alive = parse_bool(&value)?;
        }
        "advanced.verbose-logging" => {
            preferences.advanced.verbose_logging = parse_bool(&value)?;
        }
        other => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("unknown preference key: {other}"),
            )
            .into());
        }
    }

    preferences.save_default()?;
    println!("saved {key}");
    Ok(())
}

#[derive(Debug, Deserialize)]
struct TransferEndpointRequest {
    kind: String,
    path: String,
    host: Option<String>,
    username: Option<String>,
    port: Option<u16>,
}

#[derive(Debug, Deserialize)]
struct TransferTreeRequest {
    operation_id: u64,
    source: TransferEndpointRequest,
    destination: TransferEndpointRequest,
    conflict_policy: String,
}

fn transfer_tree_preflight_command(
    args: &mut impl Iterator<Item = String>,
) -> Result<(), Box<dyn Error>> {
    let request_path = next_arg(args, "request-json")?;
    ensure_finished(args)?;
    let request = read_transfer_tree_request(&request_path)?;
    let (backend, endpoint) =
        build_transfer_endpoint(&request.destination, "preflight-destination")?;
    let exists = match backend.stat(&endpoint.path) {
        Ok(_) => true,
        Err(error) if error.kind() == ErrorKind::NotFound => false,
        Err(error) => return Err(error.into()),
    };
    println!("{}", serde_json::json!({ "exists": exists }));
    Ok(())
}

fn transfer_tree_request_command(
    args: &mut impl Iterator<Item = String>,
) -> Result<(), Box<dyn Error>> {
    let request_path = next_arg(args, "request-json")?;
    ensure_finished(args)?;
    let request = read_transfer_tree_request(&request_path)?;
    let (source_backend, source_endpoint) =
        build_transfer_endpoint(&request.source, "tree-source")?;
    let (destination_backend, destination_endpoint) =
        build_transfer_endpoint(&request.destination, "tree-destination")?;
    let policy = parse_runtime_conflict_policy(&request.conflict_policy)?;
    let transfer_id = TransferId::new(request.operation_id)?;
    let runtime_request = CopyRequest {
        id: transfer_id,
        source: source_endpoint,
        destination: destination_endpoint,
        conflict_policy: policy,
    };
    let family = if request.destination.kind == "local" {
        "local"
    } else {
        "remote"
    };
    let journal_path = application_support_directory()
        .join("recovery")
        .join(family)
        .join(format!("cpk-tree-{}.json", request.operation_id));

    match execute_copy_with_recovery(
        &runtime_request,
        source_backend.as_ref(),
        destination_backend.as_ref(),
        &CancellationToken::new(),
        Some(&journal_path),
        |_| {},
    )? {
        CopyRuntimeOutcome::Completed(report) => println!(
            "{}",
            serde_json::json!({
                "state": "completed",
                "destination": report.destination.as_str(),
                "files": report.transfer.files_copied(),
                "directories": report.transfer.directories_created(),
                "bytes": report.transfer.bytes_copied(),
                "replaced": report.replaced_existing,
            })
        ),
        CopyRuntimeOutcome::Skipped { destination } => println!(
            "{}",
            serde_json::json!({
                "state": "skipped",
                "destination": destination.as_str(),
            })
        ),
        CopyRuntimeOutcome::Cancelled(report) => println!(
            "{}",
            serde_json::json!({
                "state": "cancelled",
                "files": report.files_copied(),
                "directories": report.directories_created(),
                "bytes": report.bytes_copied(),
            })
        ),
    }
    Ok(())
}

fn read_transfer_tree_request(path: &str) -> Result<TransferTreeRequest, Box<dyn Error>> {
    let bytes = fs::read(path)?;
    Ok(serde_json::from_slice(&bytes)?)
}

fn build_transfer_endpoint(
    request: &TransferEndpointRequest,
    backend_id: &str,
) -> Result<(Box<dyn Backend>, Endpoint), Box<dyn Error>> {
    let id = BackendId::new(backend_id)?;
    let backend: Box<dyn Backend> = match request.kind.as_str() {
        "local" => Box::new(LocalBackend::new(id.clone())),
        "sftp" => {
            let host = request.host.as_deref().ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "SFTP endpoint requires host")
            })?;
            let username = request.username.as_deref().ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "SFTP endpoint requires username",
                )
            })?;
            let port = request.port.unwrap_or(22);
            let mut config = SftpConfig::new(id.clone(), host, username)?.with_port(port);
            let preferences = AppPreferences::load_default().unwrap_or_default();
            config = if preferences.advanced.keep_connections_alive {
                config.with_keepalive_interval(Some(Duration::from_secs(60)))
            } else {
                config.with_keepalive_interval(None)
            };
            Box::new(SftpBackend::connect(&config, &SftpAuth::Agent)?)
        }
        other => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("unsupported endpoint kind: {other}"),
            )
            .into());
        }
    };
    let endpoint = Endpoint {
        backend: id,
        path: BackendPath::new(request.path.clone())?,
    };
    Ok((backend, endpoint))
}

fn parse_runtime_conflict_policy(value: &str) -> Result<CopyConflictPolicy, io::Error> {
    match value {
        "fail" => Ok(CopyConflictPolicy::Fail),
        "replace" => Ok(CopyConflictPolicy::Replace),
        "skip" => Ok(CopyConflictPolicy::Skip),
        "keep-both" => Ok(CopyConflictPolicy::KeepBoth),
        other => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("unsupported conflict policy: {other}"),
        )),
    }
}

fn parse_bool(value: &str) -> Result<bool, io::Error> {
    match value {
        "true" | "1" | "yes" => Ok(true),
        "false" | "0" | "no" => Ok(false),
        other => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("expected boolean value, got {other}"),
        )),
    }
}

fn parse_existing_item_action(value: &str) -> Result<ExistingItemAction, io::Error> {
    match value {
        "ask" => Ok(ExistingItemAction::Ask),
        "replace" => Ok(ExistingItemAction::Replace),
        "skip" => Ok(ExistingItemAction::Skip),
        other => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("expected ask|replace|skip, got {other}"),
        )),
    }
}

fn parse_double_click_action(value: &str) -> Result<DoubleClickAction, io::Error> {
    match value {
        "open" => Ok(DoubleClickAction::Open),
        "transfer" => Ok(DoubleClickAction::Transfer),
        "inspect" => Ok(DoubleClickAction::Inspect),
        other => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("expected open|transfer|inspect, got {other}"),
        )),
    }
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
    let mut config = SftpConfig::new(BackendId::new("sftp")?, host, username)?.with_port(port);
    let preferences = AppPreferences::load_default().unwrap_or_default();
    config = if preferences.advanced.keep_connections_alive {
        config.with_keepalive_interval(Some(Duration::from_secs(60)))
    } else {
        config.with_keepalive_interval(None)
    };
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
#[cfg(test)]
mod completion_tests {
    use super::{
        CopyConflictPolicy, TransferTreeRequest, parse_bool, parse_runtime_conflict_policy,
    };

    #[test]
    fn completion_conflict_policy_accepts_keep_both() -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(
            parse_runtime_conflict_policy("keep-both")?,
            CopyConflictPolicy::KeepBoth
        );
        Ok(())
    }

    #[test]
    fn completion_boolean_parser_is_strict() -> Result<(), Box<dyn std::error::Error>> {
        assert!(parse_bool("true")?);
        assert!(!parse_bool("false")?);
        assert!(parse_bool("maybe").is_err());
        Ok(())
    }

    #[test]
    fn completion_transfer_request_deserializes() -> Result<(), Box<dyn std::error::Error>> {
        let request: TransferTreeRequest = serde_json::from_str(
            r#"{
                "operation_id": 7,
                "source": {"kind":"local","path":"/tmp/a","host":null,"username":null,"port":null},
                "destination": {"kind":"sftp","path":"/tmp/b","host":"example.test","username":"adam","port":22},
                "conflict_policy":"replace"
            }"#,
        )?;
        assert_eq!(request.operation_id, 7);
        assert_eq!(request.source.kind, "local");
        assert_eq!(request.destination.kind, "sftp");
        assert_eq!(request.conflict_policy, "replace");
        Ok(())
    }
}
