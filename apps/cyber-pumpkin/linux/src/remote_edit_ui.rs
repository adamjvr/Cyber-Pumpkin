use crate::activity;
use crate::browser::PaneHandle;
use crate::connection::PaneConnection;
use cyber_pumpkin_application::application_support_directory;
use cyber_pumpkin_core::{BackendId, EntryKind};
use cyber_pumpkin_history::{HistoryKind, HistoryState};
use cyber_pumpkin_local::LocalBackend;
use cyber_pumpkin_remote_edit::{RemoteEditError, RemoteEditSession};
use cyber_pumpkin_remote_edit::{create_session, download_initial, local_changed, upload_changed};
use cyber_pumpkin_transfer::{CancellationToken, Endpoint, TransferId};
use std::collections::{BTreeMap, VecDeque};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::Duration;

static NEXT_REMOTE_EDIT_ID: AtomicU64 = AtomicU64::new(1_000_000);
static ACTIVE_REMOTE_EDITS: OnceLock<Mutex<BTreeMap<u64, ActiveRemoteEdit>>> = OnceLock::new();
static REMOTE_EDIT_EVENTS: OnceLock<Mutex<VecDeque<RemoteEditActivityEvent>>> = OnceLock::new();
const MAX_PENDING_REMOTE_EDIT_EVENTS: usize = 256;

#[derive(Clone)]
struct ActiveRemoteEdit {
    id: u64,
    path: String,
    cancellation: CancellationToken,
}

#[derive(Clone, Debug)]
struct RemoteEditActivityEvent {
    operation_id: u64,
    state: HistoryState,
    label: &'static str,
    detail: String,
}

fn remote_edit_event_queue() -> &'static Mutex<VecDeque<RemoteEditActivityEvent>> {
    REMOTE_EDIT_EVENTS.get_or_init(|| Mutex::new(VecDeque::new()))
}

fn remote_edit_event_queue_guard() -> MutexGuard<'static, VecDeque<RemoteEditActivityEvent>> {
    match remote_edit_event_queue().lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

fn emit_remote_edit_event(
    operation_id: u64,
    state: HistoryState,
    label: &'static str,
    detail: impl Into<String>,
) {
    let mut events = remote_edit_event_queue_guard();
    if events.len() >= MAX_PENDING_REMOTE_EDIT_EVENTS {
        events.pop_front();
    }
    events.push_back(RemoteEditActivityEvent {
        operation_id,
        state,
        label,
        detail: detail.into(),
    });
}

fn drain_remote_edit_events() -> Vec<RemoteEditActivityEvent> {
    remote_edit_event_queue_guard().drain(..).collect()
}

/// Installs the GTK-main-thread bridge for Remote Edit worker events.
///
/// Watcher threads only enqueue plain data. This timer is the sole boundary
/// that touches GTK widgets and persistent Activity history.
pub(crate) fn install_activity_bridge(activity_list: &gtk::ListBox) {
    let activity_list = activity_list.clone();
    let _activity_source = gtk::glib::timeout_add_local(Duration::from_millis(200), move || {
        for event in drain_remote_edit_events() {
            if event.state == HistoryState::Active {
                activity::record_transient_keyed(
                    &activity_list,
                    event.operation_id,
                    HistoryKind::RemoteEdit,
                    event.state,
                    event.label,
                    &event.detail,
                );
                continue;
            }

            let upload_completed =
                event.state == HistoryState::Completed && event.label == "Remote Edit Upload";
            if !upload_completed {
                activity::remove_transient(&activity_list, event.operation_id);
            }
            activity::record(
                &activity_list,
                Some(event.operation_id),
                HistoryKind::RemoteEdit,
                event.state,
                event.label,
                &event.detail,
            );
        }
        gtk::glib::ControlFlow::Continue
    });
}

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

fn remote_edit_registry() -> &'static Mutex<BTreeMap<u64, ActiveRemoteEdit>> {
    ACTIVE_REMOTE_EDITS.get_or_init(|| Mutex::new(BTreeMap::new()))
}

fn remote_edit_registry_guard() -> MutexGuard<'static, BTreeMap<u64, ActiveRemoteEdit>> {
    match remote_edit_registry().lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

fn register_remote_edit(id: u64, path: &str) -> CancellationToken {
    let cancellation = CancellationToken::new();
    remote_edit_registry_guard().insert(
        id,
        ActiveRemoteEdit {
            id,
            path: path.to_owned(),
            cancellation: cancellation.clone(),
        },
    );
    cancellation
}

fn unregister_remote_edit(id: u64) {
    remote_edit_registry_guard().remove(&id);
}

fn stop_remote_edit(id: u64) -> bool {
    let Some(active) = remote_edit_registry_guard().remove(&id) else {
        return false;
    };
    active.cancellation.cancel();
    emit_remote_edit_event(
        id,
        HistoryState::Cancelled,
        "Remote Edit Session",
        format!(
            "Stopped watcher for {}; local working copy retained.",
            active.path
        ),
    );
    true
}

fn stop_all_remote_edits() -> usize {
    let mut sessions = remote_edit_registry_guard();
    let count = sessions.len();
    for session in sessions.values() {
        session.cancellation.cancel();
        emit_remote_edit_event(
            session.id,
            HistoryState::Cancelled,
            "Remote Edit Session",
            format!(
                "Stopped watcher for {}; local working copy retained.",
                session.path
            ),
        );
    }
    sessions.clear();
    count
}

pub(crate) fn shutdown_remote_edits() {
    let _stopped = stop_all_remote_edits();
}

pub(crate) fn show_remote_edit_sessions(app: &adw::Application) {
    use gtk::prelude::*;

    let sessions = remote_edit_registry_guard()
        .values()
        .map(|session| (session.id, session.path.clone()))
        .collect::<Vec<_>>();

    let window = gtk::ApplicationWindow::builder()
        .application(app)
        .title("Remote Edit Sessions")
        .default_width(640)
        .default_height(320)
        .build();
    let root = gtk::Box::new(gtk::Orientation::Vertical, 10);
    root.set_margin_top(14);
    root.set_margin_bottom(14);
    root.set_margin_start(14);
    root.set_margin_end(14);

    let title = gtk::Label::new(Some("Remote Edit Sessions"));
    title.set_xalign(0.0);
    title.add_css_class("title-3");
    root.append(&title);

    if sessions.is_empty() {
        let empty = gtk::Label::new(Some("No active remote-edit watchers."));
        empty.set_xalign(0.0);
        empty.add_css_class("dim-label");
        root.append(&empty);
    } else {
        let list = gtk::Box::new(gtk::Orientation::Vertical, 6);
        for (id, path) in sessions {
            let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            let label = gtk::Label::new(Some(&format!("#{id}  {path}")));
            label.set_xalign(0.0);
            label.set_hexpand(true);
            label.set_wrap(true);
            let stop = gtk::Button::with_label("Stop");
            stop.add_css_class("destructive-action");
            stop.connect_clicked(move |button| {
                if stop_remote_edit(id) {
                    button.set_label("Stopping…");
                    button.set_sensitive(false);
                } else {
                    button.set_label("Stopped");
                    button.set_sensitive(false);
                }
            });
            row.append(&label);
            row.append(&stop);
            list.append(&row);
        }
        root.append(&list);

        let stop_all = gtk::Button::with_label("Stop All Remote Edits");
        stop_all.add_css_class("destructive-action");
        stop_all.connect_clicked(|button| {
            let count = stop_all_remote_edits();
            button.set_label(&format!("Stopped {count}"));
            button.set_sensitive(false);
        });
        root.append(&stop_all);
    }

    let note = gtk::Label::builder()
        .label("Stopping a session cancels its watcher and prevents further automatic uploads. The local working copy is left on disk.")
        .wrap(true)
        .xalign(0.0)
        .css_classes(vec!["dim-label".to_owned()])
        .build();
    root.append(&note);

    window.set_child(Some(&root));
    window.present();
}

fn start_remote_edit_watcher(
    connection: PaneConnection,
    local_id: BackendId,
    session: RemoteEditSession,
) {
    let cancellation = register_remote_edit(session.id.get(), session.remote.path.as_str());
    emit_remote_edit_event(
        session.id.get(),
        HistoryState::Active,
        "Remote Edit Session",
        format!(
            "Watching {} (working copy: {}).",
            session.remote.path.as_str(),
            session.local.path.as_str()
        ),
    );
    let _watcher = std::thread::spawn(move || {
        let local_backend = LocalBackend::new(local_id);
        let mut session = session;
        loop {
            if cancellation.is_cancelled() {
                break;
            }
            std::thread::sleep(Duration::from_secs(1));
            if cancellation.is_cancelled() {
                break;
            }

            match local_changed(&session, &local_backend) {
                Ok(false) => continue,
                Ok(true) => {}
                Err(error) => {
                    emit_remote_edit_event(
                        session.id.get(),
                        HistoryState::Failed,
                        "Remote Edit Watcher Failed",
                        format!("{}: {error}", session.remote.path.as_str()),
                    );
                    eprintln!("remote-edit watcher stopped: {error}");
                    break;
                }
            }

            std::thread::sleep(Duration::from_secs(1));
            if cancellation.is_cancelled() {
                break;
            }

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
                &cancellation,
            ) {
                Ok(true) => {
                    emit_remote_edit_event(
                        session.id.get(),
                        HistoryState::Completed,
                        "Remote Edit Upload",
                        format!("Uploaded {}.", session.remote.path.as_str()),
                    );
                    eprintln!("remote-edit uploaded {}", session.remote.path.as_str());
                }
                Ok(false) => {}
                Err(RemoteEditError::RemoteChanged) => {
                    emit_remote_edit_event(
                        session.id.get(),
                        HistoryState::Failed,
                        "Remote Edit Conflict",
                        format!(
                            "Upload refused because {} changed remotely. Local working copy retained at {}.",
                            session.remote.path.as_str(),
                            session.local.path.as_str()
                        ),
                    );
                    eprintln!(
                        "remote-edit conflict for {}: remote content changed",
                        session.remote.path.as_str()
                    );
                    break;
                }
                Err(error) => {
                    if cancellation.is_cancelled() {
                        break;
                    }
                    emit_remote_edit_event(
                        session.id.get(),
                        HistoryState::Failed,
                        "Remote Edit Upload Failed",
                        format!("{}: {error}", session.remote.path.as_str()),
                    );
                    eprintln!("remote-edit upload failed: {error}");
                    break;
                }
            }
        }
        unregister_remote_edit(session.id.get());
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
