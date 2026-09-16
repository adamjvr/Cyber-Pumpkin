use crate::browser::{PaneHandle, format_size};
use adw::prelude::*;
use cyber_pumpkin_operations::{OperationId, OperationKind, OperationProgress, OperationQueue};
use cyber_pumpkin_sync::{
    SyncExecutionOutcome, SyncExecutionProgress, SyncExecutionReport, execute_plan,
};
use cyber_pumpkin_sync_plan::{SyncActionKind, SyncOptions, SyncPlan, plan_one_way};
use cyber_pumpkin_transfer::CancellationToken;
use gtk::Orientation;
use gtk::glib::{self, ControlFlow};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc::{self, TryRecvError};
use std::time::Duration;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SyncDirection {
    LeftToRight,
    RightToLeft,
}

#[derive(Clone)]
struct PreparedPlan {
    direction: SyncDirection,
    plan: SyncPlan,
}

#[derive(Clone)]
pub(crate) struct SyncPanel {
    pub(crate) root: gtk::Box,
    left_path: gtk::Label,
    right_path: gtk::Label,
    direction: gtk::ComboBoxText,
    delete_orphans: gtk::CheckButton,
    follow_symlinks: gtk::CheckButton,
    summary: gtk::Label,
    preview_list: gtk::ListBox,
    simulate: gtk::Button,
    synchronize: gtk::Button,
    back: gtk::Button,
    cancel_run: gtk::Button,
    prepared: Rc<RefCell<Option<PreparedPlan>>>,
    execution_cancel: Rc<RefCell<Option<CancellationToken>>>,
    operation_queue: Rc<RefCell<OperationQueue>>,
    left: PaneHandle,
    right: PaneHandle,
    main_stack: gtk::Stack,
    title: gtk::Label,
    activity_list: gtk::ListBox,
}

enum PreviewEvent {
    Finished(Result<SyncPlan, String>),
}

enum ExecutionEvent {
    Progress(SyncExecutionProgress),
    Finished(Result<SyncExecutionOutcome, String>),
}

impl SyncPanel {
    #[allow(clippy::too_many_lines)]
    pub(crate) fn new(
        left: &PaneHandle,
        right: &PaneHandle,
        main_stack: &gtk::Stack,
        title: &gtk::Label,
        activity_list: &gtk::ListBox,
    ) -> Self {
        let root = gtk::Box::new(Orientation::Vertical, 14);
        root.set_margin_top(22);
        root.set_margin_bottom(22);
        root.set_margin_start(54);
        root.set_margin_end(54);

        let heading = gtk::Label::new(Some("Sync Files"));
        heading.add_css_class("title-1");
        root.append(&heading);

        let endpoints = gtk::Box::new(Orientation::Horizontal, 18);
        endpoints.set_halign(gtk::Align::Center);
        let left_endpoint = sync_endpoint("Left Pane");
        let right_endpoint = sync_endpoint("Right Pane");
        let direction = gtk::ComboBoxText::new();
        direction.append_text("Left → Right");
        direction.append_text("Right → Left");
        direction.set_active(Some(0));
        endpoints.append(&left_endpoint.0);
        endpoints.append(&direction);
        endpoints.append(&right_endpoint.0);
        root.append(&endpoints);
        root.append(&gtk::Separator::new(Orientation::Horizontal));

        let options = gtk::Box::new(Orientation::Horizontal, 14);
        options.set_halign(gtk::Align::Center);
        let delete_orphans = gtk::CheckButton::with_label("Delete destination-only items");
        let follow_symlinks = gtk::CheckButton::with_label("Follow symbolic links");
        follow_symlinks.set_sensitive(false);
        follow_symlinks.set_tooltip_text(Some(
            "Symlink execution waits for the backend readlink/symlink contract",
        ));
        options.append(&delete_orphans);
        options.append(&follow_symlinks);
        root.append(&options);

        let summary = gtk::Label::new(Some("Simulate to build an immutable sync plan."));
        summary.set_xalign(0.0);
        summary.set_wrap(true);
        summary.add_css_class("dim-label");
        root.append(&summary);

        let preview_list = gtk::ListBox::new();
        preview_list.set_selection_mode(gtk::SelectionMode::None);
        let preview_scroll = gtk::ScrolledWindow::new();
        preview_scroll.set_vexpand(true);
        preview_scroll.set_min_content_height(320);
        preview_scroll.set_child(Some(&preview_list));
        root.append(&preview_scroll);

        let actions = gtk::Box::new(Orientation::Horizontal, 8);
        actions.set_halign(gtk::Align::End);
        let back = gtk::Button::with_label("Back");
        let cancel_run = gtk::Button::with_label("Cancel Run");
        cancel_run.set_sensitive(false);
        let simulate = gtk::Button::with_label("Simulate");
        let synchronize = gtk::Button::with_label("Synchronize");
        synchronize.add_css_class("suggested-action");
        synchronize.set_sensitive(false);
        actions.append(&back);
        actions.append(&cancel_run);
        actions.append(&simulate);
        actions.append(&synchronize);
        root.append(&actions);

        let panel = Self {
            root,
            left_path: left_endpoint.1,
            right_path: right_endpoint.1,
            direction,
            delete_orphans,
            follow_symlinks,
            summary,
            preview_list,
            simulate,
            synchronize,
            back,
            cancel_run,
            prepared: Rc::new(RefCell::new(None)),
            execution_cancel: Rc::new(RefCell::new(None)),
            operation_queue: Rc::new(RefCell::new(OperationQueue::new())),
            left: left.clone(),
            right: right.clone(),
            main_stack: main_stack.clone(),
            title: title.clone(),
            activity_list: activity_list.clone(),
        };
        panel.connect_actions();
        panel
    }

    pub(crate) fn refresh(&self) {
        self.left_path.set_text(self.left.location().as_str());
        self.right_path.set_text(self.right.location().as_str());
        self.clear_preview();
        self.summary
            .set_text("Simulate to build an immutable sync plan.");
        self.synchronize.set_sensitive(false);
        *self.prepared.borrow_mut() = None;
    }

    fn connect_actions(&self) {
        {
            let panel = self.clone();
            self.simulate
                .connect_clicked(move |_| panel.start_preview());
        }
        {
            let panel = self.clone();
            self.synchronize
                .connect_clicked(move |_| panel.start_execution());
        }
        {
            let cancellation = Rc::clone(&self.execution_cancel);
            let summary = self.summary.clone();
            self.cancel_run.connect_clicked(move |button| {
                if let Some(token) = cancellation.borrow().as_ref() {
                    token.cancel();
                    summary.set_text("Cancelling sync…");
                    button.set_sensitive(false);
                }
            });
        }
        {
            let panel = self.clone();
            self.back.connect_clicked(move |_| {
                if panel.execution_cancel.borrow().is_none() {
                    panel.main_stack.set_visible_child_name("browser");
                    panel.title.set_text("Cyber-Pumpkin");
                }
            });
        }
    }

    fn start_preview(&self) {
        if self.execution_cancel.borrow().is_some() {
            self.summary
                .set_text("A synchronization is currently running.");
            return;
        }

        self.clear_preview();
        self.summary.set_text("Building sync plan…");
        self.simulate.set_sensitive(false);
        self.synchronize.set_sensitive(false);
        *self.prepared.borrow_mut() = None;

        let direction = self.selected_direction();
        let (source, destination) = self.panes_for_direction(direction);
        let source_connection = source.connection();
        let destination_connection = destination.connection();
        let source_root = source.location();
        let destination_root = destination.location();
        let options = SyncOptions {
            delete_orphans: self.delete_orphans.is_active(),
            follow_symlinks: self.follow_symlinks.is_active(),
            rules: Vec::new(),
        };
        let (sender, receiver) = mpsc::channel();

        let _worker = std::thread::spawn(move || {
            let result = (|| {
                let source_backend = source_connection.connect_backend()?;
                let destination_backend = destination_connection.connect_backend()?;
                plan_one_way(
                    source_backend.as_ref(),
                    &source_root,
                    destination_backend.as_ref(),
                    &destination_root,
                    &options,
                )
                .map_err(|error| error.to_string())
            })();
            let _sent = sender.send(PreviewEvent::Finished(result));
        });

        self.watch_preview(receiver, direction);
    }

    fn watch_preview(&self, receiver: mpsc::Receiver<PreviewEvent>, direction: SyncDirection) {
        let panel = self.clone();
        glib::timeout_add_local(Duration::from_millis(50), move || {
            match receiver.try_recv() {
                Ok(PreviewEvent::Finished(result)) => {
                    panel.simulate.set_sensitive(true);
                    panel.finish_preview(direction, result);
                    ControlFlow::Break
                }
                Err(TryRecvError::Empty) => ControlFlow::Continue,
                Err(TryRecvError::Disconnected) => {
                    panel.simulate.set_sensitive(true);
                    panel.summary.set_text("Sync planner worker disconnected.");
                    ControlFlow::Break
                }
            }
        });
    }

    fn finish_preview(&self, direction: SyncDirection, result: Result<SyncPlan, String>) {
        match result {
            Ok(plan) => {
                self.render_plan(&plan);
                let conflicts = plan.summary().conflicts;
                self.synchronize.set_sensitive(conflicts == 0);
                *self.prepared.borrow_mut() = Some(PreparedPlan { direction, plan });
            }
            Err(error) => {
                self.summary.set_text(&format!("Plan failed: {error}"));
                self.synchronize.set_sensitive(false);
            }
        }
    }

    fn render_plan(&self, plan: &SyncPlan) {
        self.clear_preview();
        let summary = plan.summary();
        self.summary.set_text(&format!(
            "{} directories • {} files • {} removals • {} conflicts • {} skipped",
            summary.directories_to_create,
            summary.files_to_copy,
            summary.entries_to_remove,
            summary.conflicts,
            summary.skipped,
        ));

        if plan.actions().is_empty() {
            self.append_preview_row("No changes required.", true);
            return;
        }

        for action in plan.actions() {
            let verb = match action.kind {
                SyncActionKind::CreateDirectory => "Create folder",
                SyncActionKind::CopyFile => "Copy",
                SyncActionKind::RemoveFile => "Remove file",
                SyncActionKind::RemoveDirectory => "Remove folder",
                SyncActionKind::Conflict => "CONFLICT",
                SyncActionKind::Skip => "Skip",
            };
            self.append_preview_row(
                &format!("{verb}  {}  —  {}", action.relative_path, action.reason),
                action.kind != SyncActionKind::Conflict,
            );
        }
    }

    fn start_execution(&self) {
        let Some(prepared) = self.prepared.borrow().clone() else {
            self.summary.set_text("Simulate before synchronizing.");
            return;
        };
        if prepared.plan.summary().conflicts > 0 {
            self.summary
                .set_text("Resolve plan conflicts before synchronizing.");
            return;
        }

        let operation_id = match self.operation_queue.borrow_mut().enqueue(
            OperationKind::Sync,
            "Synchronize panes",
            [],
        ) {
            Ok(id) => id,
            Err(error) => {
                self.summary
                    .set_text(&format!("Could not queue sync: {error}"));
                return;
            }
        };
        if let Err(error) = self.operation_queue.borrow_mut().start(operation_id) {
            self.summary
                .set_text(&format!("Could not start sync: {error}"));
            return;
        }

        let (source, destination) = self.panes_for_direction(prepared.direction);
        let source_connection = source.connection();
        let destination_connection = destination.connection();
        let destination_pane = destination.clone();
        let plan = prepared.plan;
        let cancellation = CancellationToken::new();
        *self.execution_cancel.borrow_mut() = Some(cancellation.clone());
        self.cancel_run.set_sensitive(true);
        self.synchronize.set_sensitive(false);
        self.simulate.set_sensitive(false);
        self.back.set_sensitive(false);
        self.summary.set_text("Synchronizing…");

        let (sender, receiver) = mpsc::channel();
        let _worker = std::thread::spawn(move || {
            let result = (|| {
                let source_backend = source_connection.connect_backend()?;
                let destination_backend = destination_connection.connect_backend()?;
                let progress_sender = sender.clone();
                execute_plan(
                    &plan,
                    source_backend.as_ref(),
                    destination_backend.as_ref(),
                    &cancellation,
                    move |progress| {
                        let _sent = progress_sender.send(ExecutionEvent::Progress(progress));
                    },
                )
                .map_err(|error| error.to_string())
            })();
            let _sent = sender.send(ExecutionEvent::Finished(result));
        });

        self.watch_execution(receiver, operation_id, destination_pane);
    }

    fn watch_execution(
        &self,
        receiver: mpsc::Receiver<ExecutionEvent>,
        operation_id: OperationId,
        destination: PaneHandle,
    ) {
        let panel = self.clone();
        glib::timeout_add_local(Duration::from_millis(50), move || {
            loop {
                match receiver.try_recv() {
                    Ok(ExecutionEvent::Progress(progress)) => {
                        panel.update_execution_progress(operation_id, &progress);
                    }
                    Ok(ExecutionEvent::Finished(result)) => {
                        panel.finish_execution(operation_id, &destination, result);
                        return ControlFlow::Break;
                    }
                    Err(TryRecvError::Empty) => return ControlFlow::Continue,
                    Err(TryRecvError::Disconnected) => {
                        panel.finish_execution(
                            operation_id,
                            &destination,
                            Err("sync worker disconnected".to_owned()),
                        );
                        return ControlFlow::Break;
                    }
                }
            }
        });
    }

    fn update_execution_progress(
        &self,
        operation_id: OperationId,
        progress: &SyncExecutionProgress,
    ) {
        let operation_progress = OperationProgress {
            completed_units: progress.completed_actions(),
            total_units: Some(progress.total_actions()),
            completed_bytes: progress.current_bytes(),
            total_bytes: progress.current_total_bytes(),
        };
        if let Err(error) = self
            .operation_queue
            .borrow_mut()
            .update_progress(operation_id, operation_progress)
        {
            eprintln!("failed to update sync operation progress: {error}");
        }

        self.summary.set_text(&format!(
            "Synchronizing {} — {}/{} actions • {}",
            progress.current_path(),
            progress.completed_actions(),
            progress.total_actions(),
            progress.current_total_bytes().map_or_else(
                || format_size(Some(progress.current_bytes())),
                |total| format!(
                    "{} / {}",
                    format_size(Some(progress.current_bytes())),
                    format_size(Some(total)),
                ),
            ),
        ));
    }

    fn finish_execution(
        &self,
        operation_id: OperationId,
        destination: &PaneHandle,
        result: Result<SyncExecutionOutcome, String>,
    ) {
        *self.execution_cancel.borrow_mut() = None;
        self.cancel_run.set_sensitive(false);
        self.simulate.set_sensitive(true);
        self.back.set_sensitive(true);

        match result {
            Ok(SyncExecutionOutcome::Completed(report)) => {
                if let Err(error) = self.operation_queue.borrow_mut().complete(operation_id) {
                    eprintln!("failed to complete sync operation: {error}");
                }
                self.summary.set_text(&completion_text("Completed", report));
                self.add_activity(operation_id, "Completed", report);
                destination.refresh();
                self.synchronize.set_sensitive(false);
                *self.prepared.borrow_mut() = None;
            }
            Ok(SyncExecutionOutcome::Cancelled(report)) => {
                if let Err(error) = self.operation_queue.borrow_mut().cancel(operation_id) {
                    eprintln!("failed to cancel sync operation: {error}");
                }
                self.summary.set_text(&completion_text("Cancelled", report));
                self.add_activity(operation_id, "Cancelled", report);
                destination.refresh();
                self.synchronize.set_sensitive(true);
            }
            Err(error) => {
                if let Err(queue_error) = self
                    .operation_queue
                    .borrow_mut()
                    .fail(operation_id, error.clone())
                {
                    eprintln!("failed to fail sync operation: {queue_error}");
                }
                self.summary
                    .set_text(&format!("Synchronization failed: {error}"));
                self.add_activity_text(operation_id, "Failed", &error);
                self.synchronize.set_sensitive(true);
            }
        }
    }

    fn selected_direction(&self) -> SyncDirection {
        if self.direction.active() == Some(1) {
            SyncDirection::RightToLeft
        } else {
            SyncDirection::LeftToRight
        }
    }

    fn panes_for_direction(&self, direction: SyncDirection) -> (PaneHandle, PaneHandle) {
        match direction {
            SyncDirection::LeftToRight => (self.left.clone(), self.right.clone()),
            SyncDirection::RightToLeft => (self.right.clone(), self.left.clone()),
        }
    }

    fn clear_preview(&self) {
        while let Some(row) = self.preview_list.row_at_index(0) {
            self.preview_list.remove(&row);
        }
    }

    fn append_preview_row(&self, text: &str, normal: bool) {
        let label = gtk::Label::new(Some(text));
        label.set_xalign(0.0);
        label.set_wrap(true);
        label.set_margin_top(4);
        label.set_margin_bottom(4);
        label.set_margin_start(8);
        label.set_margin_end(8);
        if normal {
            label.add_css_class("dim-label");
        } else {
            label.add_css_class("error");
        }
        let row = gtk::ListBoxRow::new();
        row.set_selectable(false);
        row.set_child(Some(&label));
        self.preview_list.append(&row);
    }

    fn add_activity(&self, id: OperationId, state: &str, report: SyncExecutionReport) {
        self.add_activity_text(
            id,
            state,
            &format!(
                "{} files • {} folders • {} removals • {}",
                report.files_copied,
                report.directories_created,
                report.entries_removed,
                format_size(Some(report.bytes_copied)),
            ),
        );
    }

    fn add_activity_text(&self, id: OperationId, state: &str, detail: &str) {
        remove_empty_activity_row(&self.activity_list);
        let label = gtk::Label::new(Some(&format!("#{} • Sync • {state} • {detail}", id.get())));
        label.set_xalign(0.0);
        label.set_wrap(true);
        label.set_margin_top(4);
        label.set_margin_bottom(4);
        label.set_margin_start(8);
        label.set_margin_end(8);
        let row = gtk::ListBoxRow::new();
        row.set_selectable(false);
        row.set_child(Some(&label));
        self.activity_list.prepend(&row);
    }
}

fn sync_endpoint(title: &str) -> (gtk::Box, gtk::Label) {
    let root = gtk::Box::new(Orientation::Vertical, 6);
    root.set_width_request(330);
    let icon = gtk::Image::from_icon_name("folder-symbolic");
    icon.set_pixel_size(64);
    icon.set_halign(gtk::Align::Center);
    let title = gtk::Label::new(Some(title));
    title.add_css_class("heading");
    title.set_halign(gtk::Align::Center);
    let path = gtk::Label::new(Some("—"));
    path.set_halign(gtk::Align::Center);
    path.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
    path.add_css_class("dim-label");
    root.append(&icon);
    root.append(&title);
    root.append(&path);
    (root, path)
}

fn completion_text(state: &str, report: SyncExecutionReport) -> String {
    format!(
        "{state}: {} files • {} folders • {} removals • {} skipped • {}",
        report.files_copied,
        report.directories_created,
        report.entries_removed,
        report.skipped,
        format_size(Some(report.bytes_copied)),
    )
}

fn remove_empty_activity_row(list: &gtk::ListBox) {
    let Some(row) = list.row_at_index(0) else {
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
