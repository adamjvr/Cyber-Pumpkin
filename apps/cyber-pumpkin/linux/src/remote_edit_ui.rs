use crate::browser::PaneHandle;
use crate::connection::PaneConnection;
use cyber_pumpkin_application::application_support_directory;
use cyber_pumpkin_core::{BackendId, EntryKind};
use cyber_pumpkin_local::LocalBackend;
use cyber_pumpkin_remote_edit::RemoteEditSession;
use cyber_pumpkin_remote_edit::{create_session, download_initial, local_changed, upload_changed};
use cyber_pumpkin_transfer::{CancellationToken, Endpoint, TransferId};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

static NEXT_REMOTE_EDIT_ID: AtomicU64 = AtomicU64::new(1_000_000);

pub(crate) fn edit_selected_remote_file(pane: &PaneHandle) {
    let connection = pane.connection();
    if connection.is_local() {
        show_error(
            "Remote Edit",
            "Select a file in an SFTP pane. Local files can be opened directly.",
        );
        return;
    }

    let Some(entry) = pane.selected_entry() else {
        show_error("Remote Edit", "Select a remote file to edit.");
        return;
    };
    if entry.kind != EntryKind::File {
        show_error(
            "Remote Edit",
            "Remote Edit currently supports regular files.",
        );
        return;
    }

    let serial = NEXT_REMOTE_EDIT_ID.fetch_add(1, Ordering::Relaxed);
    let Ok(transfer_id) = TransferId::new(serial) else {
        show_error(
            "Remote Edit",
            "Could not allocate a remote-edit operation id.",
        );
        return;
    };
    let Ok(local_id) = BackendId::new(format!("remote-edit-local-{serial}")) else {
        show_error(
            "Remote Edit",
            "Could not allocate the local workspace backend.",
        );
        return;
    };

    let workspace = application_support_directory().join("remote-edit");
    let remote = Endpoint {
        backend: connection.backend_id(),
        path: entry.path.clone(),
    };
    let mut session = match create_session(
        transfer_id,
        remote,
        local_id.clone(),
        &workspace,
        &entry.name,
    ) {
        Ok(session) => session,
        Err(error) => {
            show_error("Remote Edit", &error.to_string());
            return;
        }
    };

    let remote_backend = match connection.connect_backend() {
        Ok(backend) => backend,
        Err(error) => {
            show_error("Remote Edit connection failed", &error);
            return;
        }
    };
    let local_backend = LocalBackend::new(local_id.clone());
    if let Err(error) = download_initial(
        &mut session,
        remote_backend.as_ref(),
        &local_backend,
        &CancellationToken::new(),
    ) {
        show_error("Remote Edit download failed", &error.to_string());
        return;
    }

    if let Err(error) = Command::new("xdg-open")
        .arg(session.local.path.as_str())
        .spawn()
    {
        show_error("Could not open editor", &error.to_string());
        return;
    }

    start_remote_edit_watcher(connection, local_id, session);
}

fn start_remote_edit_watcher(
    connection: PaneConnection,
    local_id: BackendId,
    session: RemoteEditSession,
) {
    let _watcher = std::thread::spawn(move || {
        let local_backend = LocalBackend::new(local_id);
        let mut session = session;
        loop {
            std::thread::sleep(Duration::from_secs(1));

            match local_changed(&session, &local_backend) {
                Ok(false) => continue,
                Ok(true) => {}
                Err(error) => {
                    eprintln!("remote-edit watcher stopped: {error}");
                    break;
                }
            }

            std::thread::sleep(Duration::from_secs(1));

            let remote_backend = match connection.connect_backend() {
                Ok(backend) => backend,
                Err(error) => {
                    eprintln!("remote-edit reconnect failed: {error}");
                    continue;
                }
            };

            match upload_changed(
                &mut session,
                &local_backend,
                remote_backend.as_ref(),
                &CancellationToken::new(),
            ) {
                Ok(true) => {
                    eprintln!("remote-edit uploaded {}", session.remote.path.as_str());
                }
                Ok(false) => {}
                Err(error) => {
                    eprintln!("remote-edit upload failed: {error}");
                }
            }
        }
    });
}

fn show_error(title: &str, message: &str) {
    use gtk::prelude::*;

    let dialog = gtk::MessageDialog::builder()
        .modal(true)
        .message_type(gtk::MessageType::Error)
        .buttons(gtk::ButtonsType::Close)
        .text(title)
        .secondary_text(message)
        .build();
    dialog.connect_response(|dialog, _| dialog.close());
    dialog.present();
}
