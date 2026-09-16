#![allow(clippy::similar_names, clippy::too_many_lines)]

use crate::activity;
use crate::browser::{PaneHandle, format_size};
use crate::connection::PaneConnection;
use adw::prelude::*;
use cyber_pumpkin_application::{AppPreferences, ExistingItemAction};
use cyber_pumpkin_backend::ErrorKind;
use cyber_pumpkin_core::{BackendPath, EntryKind, FileEntry};
use cyber_pumpkin_decisions::{DecisionCenter, DecisionChoice, DecisionKind, DecisionScope};
use cyber_pumpkin_history::{HistoryKind, HistoryState};
use cyber_pumpkin_operations::{OperationId, OperationKind, OperationProgress, OperationQueue};
use cyber_pumpkin_scheduler::{OperationScheduler, SchedulerLimits};
use cyber_pumpkin_transfer::{CancellationToken, Endpoint, TransferId, TreeTransferProgress};
use cyber_pumpkin_transfer_runtime::{
    CopyConflictPolicy, CopyRequest, CopyRuntimeOutcome, execute_copy,
};
use gtk::Orientation;
use gtk::glib::{self, ControlFlow};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::mpsc::{self, TryRecvError};
use std::time::Duration;

#[derive(Clone)]
pub(crate) struct CopyBar {
    pub(crate) root: gtk::Box,
    copy_right: gtk::Button,
    copy_left: gtk::Button,
    cancel: gtk::Button,
    progress: gtk::ProgressBar,
    status: gtk::Label,
    active_cancels: Rc<RefCell<BTreeMap<OperationId, CancellationToken>>>,
    pending: Rc<RefCell<BTreeMap<OperationId, CopyJob>>>,
    operation_queue: Rc<RefCell<OperationQueue>>,
    scheduler: Rc<RefCell<OperationScheduler>>,
    decision_center: Rc<RefCell<DecisionCenter>>,
    activity_list: gtk::ListBox,
}

#[derive(Clone)]
struct CopyJob {
    operation_id: OperationId,
    transfer_id: TransferId,
    source_connection: PaneConnection,
    destination_connection: PaneConnection,
    source_path: BackendPath,
    destination_path: BackendPath,
    item_name: String,
    conflict_policy: CopyConflictPolicy,
    destination_pane: PaneHandle,
}

enum CopyWorkerEvent {
    Progress(TreeTransferProgress),
    Finished(Result<CopyRuntimeOutcome, String>),
}

pub(crate) fn build_copy_bar(
    left: &PaneHandle,
    right: &PaneHandle,
    activity_list: &gtk::ListBox,
    decision_center: Rc<RefCell<DecisionCenter>>,
) -> CopyBar {
    let copy_left = gtk::Button::with_label("← Copy");
    let copy_right = gtk::Button::with_label("Copy →");
    let cancel = gtk::Button::with_label("Cancel All");
    cancel.set_sensitive(false);

    let progress = gtk::ProgressBar::new();
    progress.set_hexpand(true);

    let status = gtk::Label::new(Some("Ready"));
    status.set_xalign(0.0);
    status.set_hexpand(true);
    status.add_css_class("dim-label");

    copy_left.set_visible(false);
    copy_right.set_visible(false);

    let root = gtk::Box::new(Orientation::Horizontal, 8);
    root.set_margin_top(5);
    root.set_margin_bottom(5);
    root.set_margin_start(8);
    root.set_margin_end(8);
    root.set_visible(false);
    root.append(&progress);
    root.append(&status);
    root.append(&cancel);

    let requested = AppPreferences::load_default()
        .map_or(5_u16, |preferences| {
            u16::from(preferences.transfers.simultaneous_transfers)
        })
        .clamp(1, 20);
    let limits = SchedulerLimits::new(requested).unwrap_or_default();

    let bar = CopyBar {
        root,
        copy_right,
        copy_left,
        cancel,
        progress,
        status,
        active_cancels: Rc::new(RefCell::new(BTreeMap::new())),
        pending: Rc::new(RefCell::new(BTreeMap::new())),
        operation_queue: Rc::new(RefCell::new(OperationQueue::new())),
        scheduler: Rc::new(RefCell::new(OperationScheduler::new(limits))),
        decision_center,
        activity_list: activity_list.clone(),
    };
    connect_bar(&bar, left, right);
    bar
}

impl CopyBar {
    pub(crate) fn copy_between(&self, source: &PaneHandle, destination: &PaneHandle) {
        request_copy(source, destination, self);
    }
}

fn connect_bar(bar: &CopyBar, left: &PaneHandle, right: &PaneHandle) {
    connect_copy_button(&bar.copy_right, left, right, bar);
    connect_copy_button(&bar.copy_left, right, left, bar);

    let bar = bar.clone();
    bar.cancel
        .clone()
        .connect_clicked(move |_| cancel_all(&bar));
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
        request_copy(&source, &destination, &bar);
    });
}

fn request_copy(source: &PaneHandle, destination: &PaneHandle, bar: &CopyBar) {
    let Some(entry) = source.selected_entry() else {
        bar.status.set_text("Select a file or folder to copy.");
        return;
    };
    if matches!(entry.kind, EntryKind::Symlink | EntryKind::Other) {
        bar.status
            .set_text("Links and special entries are not supported yet.");
        return;
    }

    let Ok(destination_path) = destination.destination_child(&entry.name) else {
        bar.status.set_text("Could not form destination path.");
        return;
    };

    let destination_connection = destination.connection();
    let destination_exists = match destination_connection.connect_backend() {
        Ok(backend) => match backend.stat(&destination_path) {
            Ok(_) => true,
            Err(error) if error.kind() == ErrorKind::NotFound => false,
            Err(error) => {
                bar.status
                    .set_text(&format!("Destination preflight failed: {error}"));
                return;
            }
        },
        Err(error) => {
            bar.status
                .set_text(&format!("Destination connection failed: {error}"));
            return;
        }
    };

    if !destination_exists {
        enqueue_copy(
            source,
            destination,
            entry,
            destination_path,
            CopyConflictPolicy::Fail,
            bar,
        );
        return;
    }

    match preferred_existing_action(source, destination, entry.kind) {
        ExistingItemAction::Replace => enqueue_copy(
            source,
            destination,
            entry,
            destination_path,
            CopyConflictPolicy::Replace,
            bar,
        ),
        ExistingItemAction::Skip => enqueue_copy(
            source,
            destination,
            entry,
            destination_path,
            CopyConflictPolicy::Skip,
            bar,
        ),
        ExistingItemAction::Ask => {
            request_existing_item_decision(source, destination, entry, destination_path, bar);
        }
    }
}

fn preferred_existing_action(
    source: &PaneHandle,
    destination: &PaneHandle,
    kind: EntryKind,
) -> ExistingItemAction {
    let preferences = AppPreferences::load_default().unwrap_or_default();
    let source_connection = source.connection();
    let destination_connection = destination.connection();
    let downloading = !source_connection.is_local() && destination_connection.is_local();

    match (downloading, kind) {
        (true, EntryKind::Directory) => preferences.transfers.downloading_folders,
        (true, _) => preferences.transfers.downloading_files,
        (false, EntryKind::Directory) => preferences.transfers.uploading_folders,
        (false, _) => preferences.transfers.uploading_files,
    }
}

fn request_existing_item_decision(
    source: &PaneHandle,
    destination: &PaneHandle,
    entry: FileEntry,
    destination_path: BackendPath,
    bar: &CopyBar,
) {
    let kind = if entry.kind == EntryKind::Directory {
        DecisionKind::ExistingDirectory
    } else {
        DecisionKind::ExistingFile
    };
    let request = bar.decision_center.borrow_mut().request(
        kind,
        entry.name.clone(),
        format!("The destination already contains “{}”.", entry.name),
        vec![
            DecisionChoice::Replace,
            DecisionChoice::Skip,
            DecisionChoice::KeepBoth,
            DecisionChoice::Cancel,
        ],
    );
    let Ok((decision_id, immediate)) = request else {
        bar.status
            .set_text("Could not create copy conflict decision.");
        return;
    };

    if let Some(resolution) = immediate {
        apply_copy_decision(
            source,
            destination,
            entry,
            destination_path,
            resolution.choice,
            bar,
        );
        return;
    }

    let dialog = gtk::Dialog::builder()
        .title("Destination Already Exists")
        .modal(true)
        .build();
    dialog.add_button("Cancel", gtk::ResponseType::Cancel);
    dialog.add_button("Skip", gtk::ResponseType::Other(1));
    dialog.add_button("Keep Both", gtk::ResponseType::Other(2));
    dialog.add_button("Replace", gtk::ResponseType::Accept);

    let content = gtk::Box::new(Orientation::Vertical, 8);
    content.set_margin_top(16);
    content.set_margin_bottom(16);
    content.set_margin_start(16);
    content.set_margin_end(16);

    let label = gtk::Label::new(Some(&format!(
        "“{}” already exists in the destination.",
        entry.name
    )));
    label.set_xalign(0.0);
    label.set_wrap(true);
    content.append(&label);

    let remember =
        gtk::CheckButton::with_label("Use this choice for matching conflicts this session");
    content.append(&remember);
    dialog.content_area().append(&content);

    let source = source.clone();
    let destination = destination.clone();
    let bar = bar.clone();
    dialog.connect_response(move |dialog, response| {
        let choice = match response {
            gtk::ResponseType::Accept => DecisionChoice::Replace,
            gtk::ResponseType::Other(1) => DecisionChoice::Skip,
            gtk::ResponseType::Other(2) => DecisionChoice::KeepBoth,
            _ => DecisionChoice::Cancel,
        };
        let scope = if remember.is_active() {
            DecisionScope::AllMatching
        } else {
            DecisionScope::ThisItem
        };

        match bar
            .decision_center
            .borrow_mut()
            .resolve(decision_id, choice, scope)
        {
            Ok(_) => apply_copy_decision(
                &source,
                &destination,
                entry.clone(),
                destination_path.clone(),
                choice,
                &bar,
            ),
            Err(error) => bar
                .status
                .set_text(&format!("Could not resolve copy conflict: {error}")),
        }
        dialog.close();
    });
    dialog.present();
}

fn apply_copy_decision(
    source: &PaneHandle,
    destination: &PaneHandle,
    entry: FileEntry,
    destination_path: BackendPath,
    choice: DecisionChoice,
    bar: &CopyBar,
) {
    let policy = match choice {
        DecisionChoice::Replace => CopyConflictPolicy::Replace,
        DecisionChoice::Skip => CopyConflictPolicy::Skip,
        DecisionChoice::KeepBoth => CopyConflictPolicy::KeepBoth,
        _ => {
            bar.status.set_text("Copy cancelled before execution.");
            return;
        }
    };
    enqueue_copy(source, destination, entry, destination_path, policy, bar);
}

fn enqueue_copy(
    source: &PaneHandle,
    destination: &PaneHandle,
    entry: FileEntry,
    destination_path: BackendPath,
    conflict_policy: CopyConflictPolicy,
    bar: &CopyBar,
) {
    let operation_id = match bar.operation_queue.borrow_mut().enqueue(
        OperationKind::Copy,
        format!("Copy {}", entry.name),
        [],
    ) {
        Ok(id) => id,
        Err(error) => {
            bar.status
                .set_text(&format!("Could not queue copy: {error}"));
            return;
        }
    };
    let transfer_id = match TransferId::new(operation_id.get()) {
        Ok(id) => id,
        Err(error) => {
            if let Err(queue_error) = bar
                .operation_queue
                .borrow_mut()
                .fail(operation_id, error.to_string())
            {
                eprintln!("failed to fail copy operation after transfer-id error: {queue_error}");
            }
            bar.status
                .set_text(&format!("Could not allocate transfer id: {error}"));
            return;
        }
    };

    bar.pending.borrow_mut().insert(
        operation_id,
        CopyJob {
            operation_id,
            transfer_id,
            source_connection: source.connection(),
            destination_connection: destination.connection(),
            source_path: entry.path,
            destination_path,
            item_name: entry.name,
            conflict_policy,
            destination_pane: destination.clone(),
        },
    );

    bar.root.set_visible(true);
    bar.status.set_text(&format!(
        "Queued copy • {} active • {} waiting",
        bar.scheduler.borrow().active_count(),
        bar.pending.borrow().len()
    ));
    dispatch_pending(bar);
}

fn dispatch_pending(bar: &CopyBar) {
    let admitted = {
        let mut queue = bar.operation_queue.borrow_mut();
        match bar.scheduler.borrow_mut().admit_ready(&mut queue) {
            Ok(ids) => ids,
            Err(error) => {
                bar.status
                    .set_text(&format!("Transfer scheduler failed: {error}"));
                return;
            }
        }
    };

    for operation_id in admitted {
        let Some(job) = bar.pending.borrow_mut().remove(&operation_id) else {
            continue;
        };
        start_worker(job, bar);
    }

    update_cancel_state(bar);
}

fn start_worker(job: CopyJob, bar: &CopyBar) {
    let cancellation = CancellationToken::new();
    bar.active_cancels
        .borrow_mut()
        .insert(job.operation_id, cancellation.clone());
    bar.progress.set_fraction(0.0);
    bar.status.set_text(&format!(
        "Copying {} • {} active",
        job.item_name,
        bar.scheduler.borrow().active_count()
    ));

    let request = CopyRequest {
        id: job.transfer_id,
        source: Endpoint {
            backend: job.source_connection.backend_id(),
            path: job.source_path.clone(),
        },
        destination: Endpoint {
            backend: job.destination_connection.backend_id(),
            path: job.destination_path.clone(),
        },
        conflict_policy: job.conflict_policy,
    };
    let source_connection = job.source_connection.clone();
    let destination_connection = job.destination_connection.clone();
    let (sender, receiver) = mpsc::channel();

    let _worker = std::thread::spawn(move || {
        let result = (|| {
            let source_backend = source_connection.connect_backend()?;
            let destination_backend = destination_connection.connect_backend()?;
            let progress_sender = sender.clone();
            execute_copy(
                &request,
                source_backend.as_ref(),
                destination_backend.as_ref(),
                &cancellation,
                move |progress| {
                    let _sent = progress_sender.send(CopyWorkerEvent::Progress(progress));
                },
            )
            .map_err(|error| error.to_string())
        })();
        let _sent = sender.send(CopyWorkerEvent::Finished(result));
    });

    watch_result(receiver, job, bar.clone());
}

fn watch_result(receiver: mpsc::Receiver<CopyWorkerEvent>, job: CopyJob, bar: CopyBar) {
    glib::timeout_add_local(Duration::from_millis(50), move || {
        loop {
            match receiver.try_recv() {
                Ok(CopyWorkerEvent::Progress(progress)) => {
                    update_progress(&bar, &job, progress);
                }
                Ok(CopyWorkerEvent::Finished(result)) => {
                    finish_result(&bar, &job, result);
                    return ControlFlow::Break;
                }
                Err(TryRecvError::Empty) => return ControlFlow::Continue,
                Err(TryRecvError::Disconnected) => {
                    finish_result(&bar, &job, Err("copy worker disconnected".to_owned()));
                    return ControlFlow::Break;
                }
            }
        }
    });
}

fn update_progress(bar: &CopyBar, job: &CopyJob, progress: TreeTransferProgress) {
    if let Some(permille) = progress.permille() {
        bar.progress.set_fraction(f64::from(permille) / 1000.0);
    } else {
        bar.progress.pulse();
    }

    let operation_progress = OperationProgress {
        completed_units: progress.files_copied(),
        total_units: Some(progress.total_files()),
        completed_bytes: progress.bytes_copied(),
        total_bytes: progress.total_bytes(),
    };
    if let Err(error) = bar
        .operation_queue
        .borrow_mut()
        .update_progress(job.operation_id, operation_progress)
    {
        eprintln!("failed to update copy operation progress: {error}");
    }

    bar.status.set_text(&format!(
        "Copying {} — {}/{} files • {} active",
        job.item_name,
        progress.files_copied(),
        progress.total_files(),
        bar.scheduler.borrow().active_count()
    ));
}

fn finish_result(bar: &CopyBar, job: &CopyJob, result: Result<CopyRuntimeOutcome, String>) {
    bar.active_cancels.borrow_mut().remove(&job.operation_id);

    match result {
        Ok(CopyRuntimeOutcome::Completed(report)) => {
            let scheduler_result = {
                let mut queue = bar.operation_queue.borrow_mut();
                bar.scheduler
                    .borrow_mut()
                    .complete(&mut queue, job.operation_id)
            };
            if let Err(error) = scheduler_result {
                eprintln!("failed to complete copy operation: {error}");
            }
            bar.progress.set_fraction(1.0);
            bar.status.set_text(&format!(
                "Copied {} — {} files • {}",
                job.item_name,
                report.transfer.files_copied(),
                format_size(Some(report.transfer.bytes_copied()))
            ));
            activity::record(
                &bar.activity_list,
                Some(job.operation_id.get()),
                HistoryKind::Copy,
                HistoryState::Completed,
                &format!("Copy {}", job.item_name),
                &format!(
                    "{} files • {}{}",
                    report.transfer.files_copied(),
                    format_size(Some(report.transfer.bytes_copied())),
                    if report.replaced_existing {
                        " • replaced existing"
                    } else {
                        ""
                    }
                ),
            );
            job.destination_pane.refresh();
        }
        Ok(CopyRuntimeOutcome::Skipped { .. }) => {
            let scheduler_result = {
                let mut queue = bar.operation_queue.borrow_mut();
                bar.scheduler
                    .borrow_mut()
                    .complete(&mut queue, job.operation_id)
            };
            if let Err(error) = scheduler_result {
                eprintln!("failed to complete skipped copy operation: {error}");
            }
            bar.progress.set_fraction(0.0);
            bar.status
                .set_text(&format!("Skipped existing {}", job.item_name));
            activity::record(
                &bar.activity_list,
                Some(job.operation_id.get()),
                HistoryKind::Copy,
                HistoryState::Completed,
                &format!("Copy {}", job.item_name),
                "Skipped existing destination",
            );
        }
        Ok(CopyRuntimeOutcome::Cancelled(report)) => {
            let scheduler_result = {
                let mut queue = bar.operation_queue.borrow_mut();
                bar.scheduler
                    .borrow_mut()
                    .cancel(&mut queue, job.operation_id)
            };
            if let Err(error) = scheduler_result {
                eprintln!("failed to cancel copy operation: {error}");
            }
            bar.progress.set_fraction(0.0);
            bar.status.set_text(&format!(
                "Cancelled {} after {} files",
                job.item_name,
                report.files_copied()
            ));
            activity::record(
                &bar.activity_list,
                Some(job.operation_id.get()),
                HistoryKind::Copy,
                HistoryState::Cancelled,
                &format!("Copy {}", job.item_name),
                &format!(
                    "{} files • {}",
                    report.files_copied(),
                    format_size(Some(report.bytes_copied()))
                ),
            );
            job.destination_pane.refresh();
        }
        Err(error) => {
            let scheduler_result = {
                let mut queue = bar.operation_queue.borrow_mut();
                bar.scheduler
                    .borrow_mut()
                    .fail(&mut queue, job.operation_id, error.clone())
            };
            if let Err(queue_error) = scheduler_result {
                eprintln!("failed to fail copy operation: {queue_error}");
            }
            bar.progress.set_fraction(0.0);
            bar.status.set_text(&format!("Copy failed: {error}"));
            activity::record(
                &bar.activity_list,
                Some(job.operation_id.get()),
                HistoryKind::Copy,
                HistoryState::Failed,
                &format!("Copy {}", job.item_name),
                &error,
            );
        }
    }

    dispatch_pending(bar);
    update_cancel_state(bar);
}

fn cancel_all(bar: &CopyBar) {
    for token in bar.active_cancels.borrow().values() {
        token.cancel();
    }

    let queued = bar.pending.borrow().keys().copied().collect::<Vec<_>>();
    for id in queued {
        if let Err(error) = bar.operation_queue.borrow_mut().cancel(id) {
            eprintln!("failed to cancel queued copy operation: {error}");
        }
        bar.pending.borrow_mut().remove(&id);
    }

    bar.status.set_text("Cancelling transfers…");
    update_cancel_state(bar);
}

fn update_cancel_state(bar: &CopyBar) {
    let active = bar.active_cancels.borrow().len();
    let pending = bar.pending.borrow().len();
    bar.cancel.set_sensitive(active > 0 || pending > 0);
    if active == 0 && pending == 0 && bar.status.text() == "Cancelling transfers…" {
        bar.status.set_text("Ready");
    }
}
