use crate::browser::{PaneHandle, format_size};
use adw::prelude::*;
use cyber_pumpkin_core::EntryKind;
use cyber_pumpkin_local::LocalBackend;
use cyber_pumpkin_transfer::{
    CancellationToken, ControlledTransferOutcome, Endpoint, TransferId, TransferJob,
    TransferProgress, TransferSpec, execute_file_controlled,
};
use gtk::Orientation;
use gtk::glib::{self, ControlFlow};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, TryRecvError};
use std::time::Duration;

static NEXT_TRANSFER_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone)]
pub(crate) struct CopyBar {
    pub(crate) root: gtk::Box,
    copy_right: gtk::Button,
    copy_left: gtk::Button,
    cancel: gtk::Button,
    progress: gtk::ProgressBar,
    status: gtk::Label,
    active_cancel: Rc<RefCell<Option<CancellationToken>>>,
    activity_list: gtk::ListBox,
}

enum CopyWorkerEvent {
    Progress(TransferProgress),
    Finished(Result<ControlledTransferOutcome, String>),
}

pub(crate) fn build_copy_bar(
    left: &PaneHandle,
    right: &PaneHandle,
    activity_list: &gtk::ListBox,
) -> CopyBar {
    let copy_left = gtk::Button::with_label("← Copy");
    let copy_right = gtk::Button::with_label("Copy →");
    let cancel = gtk::Button::with_label("Cancel");
    cancel.set_sensitive(false);

    let progress = gtk::ProgressBar::new();
    progress.set_hexpand(true);

    let status = gtk::Label::new(Some("Ready"));
    status.set_xalign(0.0);
    status.set_hexpand(true);
    status.add_css_class("dim-label");

    let root = gtk::Box::new(Orientation::Horizontal, 8);
    root.set_margin_top(6);
    root.set_margin_bottom(6);
    root.set_margin_start(8);
    root.set_margin_end(8);
    root.append(&copy_left);
    root.append(&copy_right);
    root.append(&cancel);
    root.append(&progress);
    root.append(&status);

    let bar = CopyBar {
        root,
        copy_right,
        copy_left,
        cancel,
        progress,
        status,
        active_cancel: Rc::new(RefCell::new(None)),
        activity_list: activity_list.clone(),
    };
    connect_bar(&bar, left, right);
    bar
}

fn connect_bar(bar: &CopyBar, left: &PaneHandle, right: &PaneHandle) {
    connect_copy_button(&bar.copy_right, left, right, bar);
    connect_copy_button(&bar.copy_left, right, left, bar);

    let active_cancel = Rc::clone(&bar.active_cancel);
    let status = bar.status.clone();
    bar.cancel.clone().connect_clicked(move |button| {
        let Some(token) = active_cancel.borrow().as_ref().cloned() else {
            return;
        };
        token.cancel();
        status.set_text("Cancelling transfer…");
        button.set_sensitive(false);
    });
}

fn connect_copy_button(
    button: &gtk::Button,
    source: &PaneHandle,
    destination: &PaneHandle,
    bar: &CopyBar,
) {
    let source = source.clone();
    let destination = destination.clone();
    let bar = bar.clone();
    button.clone().connect_clicked(move |_| {
        start_copy(&source, &destination, &bar);
    });
}

fn start_copy(source: &PaneHandle, destination: &PaneHandle, bar: &CopyBar) {
    if bar.active_cancel.borrow().is_some() {
        bar.status.set_text("A transfer is already active.");
        return;
    }

    let Some(entry) = source.selected_entry() else {
        bar.status.set_text("Select a file to copy.");
        return;
    };
    if entry.kind != EntryKind::File {
        bar.status
            .set_text("Folder copy lands in the recursive-transfer pass.");
        return;
    }

    let Ok(destination_path) = destination.destination_child(&entry.name) else {
        bar.status.set_text("Could not form destination path.");
        return;
    };
    let transfer_number = NEXT_TRANSFER_ID.fetch_add(1, Ordering::Relaxed);
    let Ok(transfer_id) = TransferId::new(transfer_number) else {
        bar.status.set_text("Could not allocate transfer id.");
        return;
    };

    let source_backend_id = source.backend_id();
    let destination_backend_id = destination.backend_id();
    let source_path = entry.path;
    let file_name = entry.name;
    let (sender, receiver) = mpsc::channel();
    let cancellation = CancellationToken::new();

    *bar.active_cancel.borrow_mut() = Some(cancellation.clone());
    bar.copy_left.set_sensitive(false);
    bar.copy_right.set_sensitive(false);
    bar.cancel.set_sensitive(true);
    bar.progress.set_fraction(0.0);
    bar.status.set_text(&format!("Copying {file_name}…"));

    let _worker = std::thread::spawn(move || {
        let source_backend = LocalBackend::new(source_backend_id.clone());
        let destination_backend = LocalBackend::new(destination_backend_id.clone());
        let spec = TransferSpec {
            source: Endpoint {
                backend: source_backend_id,
                path: source_path,
            },
            destination: Endpoint {
                backend: destination_backend_id,
                path: destination_path,
            },
        };
        let mut job = TransferJob::new(transfer_id, spec);
        let progress_sender = sender.clone();
        let result = execute_file_controlled(
            &mut job,
            &source_backend,
            &destination_backend,
            &cancellation,
            move |progress| {
                let _sent = progress_sender.send(CopyWorkerEvent::Progress(progress));
            },
        )
        .map_err(|error| error.to_string());
        let _sent = sender.send(CopyWorkerEvent::Finished(result));
    });

    watch_result(
        receiver,
        destination.clone(),
        bar.clone(),
        file_name,
        transfer_id,
    );
}

fn watch_result(
    receiver: mpsc::Receiver<CopyWorkerEvent>,
    destination: PaneHandle,
    bar: CopyBar,
    file_name: String,
    transfer_id: TransferId,
) {
    glib::timeout_add_local(Duration::from_millis(50), move || {
        loop {
            match receiver.try_recv() {
                Ok(CopyWorkerEvent::Progress(progress)) => {
                    update_progress(&bar, &file_name, progress);
                }
                Ok(CopyWorkerEvent::Finished(result)) => {
                    finish_result(&destination, &bar, &file_name, transfer_id, result);
                    return ControlFlow::Break;
                }
                Err(TryRecvError::Empty) => return ControlFlow::Continue,
                Err(TryRecvError::Disconnected) => {
                    clear_active(&bar);
                    bar.progress.set_fraction(0.0);
                    bar.status.set_text("Copy worker disconnected.");
                    add_activity(&bar.activity_list, transfer_id, &file_name, "Failed", None);
                    return ControlFlow::Break;
                }
            }
        }
    });
}

fn update_progress(bar: &CopyBar, file_name: &str, progress: TransferProgress) {
    if let Some(permille) = progress.permille() {
        bar.progress.set_fraction(f64::from(permille) / 1000.0);
    } else {
        bar.progress.pulse();
    }

    let copied = format_size(Some(progress.bytes_copied()));
    let detail = match progress.total_bytes() {
        Some(total) => format!("{copied} / {}", format_size(Some(total))),
        None => copied,
    };
    bar.status
        .set_text(&format!("Copying {file_name} — {detail}"));
}

fn finish_result(
    destination: &PaneHandle,
    bar: &CopyBar,
    file_name: &str,
    transfer_id: TransferId,
    result: Result<ControlledTransferOutcome, String>,
) {
    clear_active(bar);

    match result {
        Ok(ControlledTransferOutcome::Completed(report)) => {
            let bytes = report.bytes_copied();
            bar.progress.set_fraction(1.0);
            bar.status.set_text(&format!(
                "Copied {file_name} — {}",
                format_size(Some(bytes))
            ));
            add_activity(
                &bar.activity_list,
                transfer_id,
                file_name,
                "Completed",
                Some(bytes),
            );
            destination.refresh();
        }
        Ok(ControlledTransferOutcome::Cancelled { bytes_copied }) => {
            bar.progress.set_fraction(0.0);
            bar.status.set_text(&format!(
                "Cancelled {file_name} after {}",
                format_size(Some(bytes_copied))
            ));
            add_activity(
                &bar.activity_list,
                transfer_id,
                file_name,
                "Cancelled",
                Some(bytes_copied),
            );
            destination.refresh();
        }
        Err(error) => {
            bar.progress.set_fraction(0.0);
            bar.status.set_text(&format!("Copy failed: {error}"));
            add_activity(&bar.activity_list, transfer_id, file_name, "Failed", None);
        }
    }
}

fn clear_active(bar: &CopyBar) {
    *bar.active_cancel.borrow_mut() = None;
    bar.copy_left.set_sensitive(true);
    bar.copy_right.set_sensitive(true);
    bar.cancel.set_sensitive(false);
}

fn add_activity(
    list: &gtk::ListBox,
    transfer_id: TransferId,
    file_name: &str,
    state: &str,
    bytes: Option<u64>,
) {
    remove_empty_activity_row(list);

    let label = gtk::Label::new(Some(&format!(
        "#{} • Copy {file_name} • {state} • {}",
        transfer_id.get(),
        format_size(bytes)
    )));
    label.set_xalign(0.0);
    label.set_margin_top(4);
    label.set_margin_bottom(4);
    label.set_margin_start(8);
    label.set_margin_end(8);

    let row = gtk::ListBoxRow::new();
    row.set_selectable(false);
    row.set_child(Some(&label));
    list.prepend(&row);
}

fn remove_empty_activity_row(list: &gtk::ListBox) {
    let Some(row) = list.first_child() else {
        return;
    };
    let Ok(row) = row.downcast::<gtk::ListBoxRow>() else {
        return;
    };
    let Some(child) = row.child() else {
        return;
    };
    let Ok(label) = child.downcast::<gtk::Label>() else {
        return;
    };
    if label.text() == "No transfer activity yet." {
        list.remove(&row);
    }
}
