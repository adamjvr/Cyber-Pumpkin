use crate::browser::{format_modified, format_size};
use crate::connection::PaneConnection;
use cyber_pumpkin_core::{EntryKind, FileEntry};
use gtk::Orientation;
use gtk::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::mpsc::{self, TryRecvError};
use std::time::Duration;

const PERMISSION_BITS: [u32; 9] = [
    0o400, 0o200, 0o100, 0o040, 0o020, 0o010, 0o004, 0o002, 0o001,
];

#[derive(Clone)]
struct InspectorSelection {
    entry: FileEntry,
    connection: PaneConnection,
}

struct InspectorMetadataResult {
    created: Result<Option<u64>, String>,
    unix_mode: Result<Option<u32>, String>,
    permissions_supported: bool,
}

struct PermissionApplyResult {
    unix_mode: Result<Option<u32>, String>,
    permissions_supported: bool,
}

#[derive(Clone)]
struct PermissionEditor {
    checks: Vec<gtk::CheckButton>,
    octal: gtk::Entry,
    load: gtk::Button,
    apply: gtk::Button,
    status: gtk::Label,
}

impl PermissionEditor {
    fn set_available(&self, available: bool) {
        self.load.set_sensitive(available);
        self.apply.set_sensitive(available);
        self.octal.set_sensitive(available);
        for check in &self.checks {
            check.set_sensitive(available);
        }
        if !available {
            self.octal.set_text("");
            for check in &self.checks {
                check.set_active(false);
            }
            self.status
                .set_text("Select a file or folder to inspect permissions.");
        }
    }

    fn set_mode(&self, mode: u32) {
        self.octal.set_text(&format!("{mode:04o}"));
        for (check, bit) in self.checks.iter().zip(PERMISSION_BITS) {
            check.set_active(mode & bit != 0);
        }
        self.status
            .set_text(&format!("Loaded POSIX mode {mode:04o}."));
    }

    fn permission_bits(&self) -> u32 {
        self.checks
            .iter()
            .zip(PERMISSION_BITS)
            .fold(
                0_u32,
                |mode, (check, bit)| {
                    if check.is_active() { mode | bit } else { mode }
                },
            )
    }
}

#[derive(Clone)]
pub(crate) struct InspectorPane {
    pub(crate) root: gtk::Box,
    icon: gtk::Image,
    name: gtk::Label,
    kind: gtk::Label,
    size: gtk::Label,
    modified: gtk::Label,
    created: gtk::Label,
    backend: gtk::Label,
    path: gtk::Label,
    permissions: PermissionEditor,
    selection: Rc<RefCell<Option<InspectorSelection>>>,
    generation: Rc<Cell<u64>>,
}

impl InspectorPane {
    pub(crate) fn new() -> Self {
        let root = gtk::Box::new(Orientation::Vertical, 0);
        root.set_width_request(300);

        let title_row = gtk::Box::new(Orientation::Horizontal, 6);
        title_row.set_margin_top(8);
        title_row.set_margin_bottom(8);
        title_row.set_margin_start(12);
        title_row.set_margin_end(12);
        let title = gtk::Label::new(Some("Inspector"));
        title.set_xalign(0.0);
        title.set_hexpand(true);
        title.add_css_class("heading");
        let collapse = gtk::Image::from_icon_name("pan-end-symbolic");
        collapse.add_css_class("dim-label");
        title_row.append(&title);
        title_row.append(&collapse);
        root.append(&title_row);
        root.append(&gtk::Separator::new(Orientation::Horizontal));

        let preview = gtk::Box::new(Orientation::Vertical, 8);
        preview.set_margin_top(18);
        preview.set_margin_bottom(16);
        preview.set_margin_start(14);
        preview.set_margin_end(14);
        let icon = gtk::Image::from_icon_name("folder-symbolic");
        icon.set_pixel_size(96);
        icon.set_halign(gtk::Align::Center);
        let name = value_label("No Selection");
        name.set_halign(gtk::Align::Center);
        name.set_xalign(0.5);
        name.add_css_class("heading");
        preview.append(&icon);
        preview.append(&name);
        root.append(&preview);
        root.append(&gtk::Separator::new(Orientation::Horizontal));

        let metadata = gtk::Box::new(Orientation::Vertical, 7);
        metadata.set_margin_top(12);
        metadata.set_margin_bottom(12);
        metadata.set_margin_start(14);
        metadata.set_margin_end(14);

        let kind = value_label("—");
        let size = value_label("—");
        let modified = value_label("—");
        let created = value_label("—");
        let backend = value_label("—");
        let path = value_label("—");
        path.set_wrap(true);
        path.set_selectable(true);

        metadata.append(&detail_row("Kind", &kind));
        metadata.append(&detail_row("Size", &size));
        metadata.append(&detail_row("Modified", &modified));
        metadata.append(&detail_row("Created", &created));
        metadata.append(&detail_row("Location", &backend));
        metadata.append(&gtk::Separator::new(Orientation::Horizontal));
        metadata.append(&section_label("Path"));
        metadata.append(&path);
        root.append(&metadata);
        root.append(&gtk::Separator::new(Orientation::Horizontal));

        let (permission_box, permissions) = build_permission_editor();
        root.append(&permission_box);

        let selection = Rc::new(RefCell::new(None));
        let generation = Rc::new(Cell::new(0));
        connect_permission_actions(&permissions, &created, &selection, &generation);
        permissions.set_available(false);

        Self {
            root,
            icon,
            name,
            kind,
            size,
            modified,
            created,
            backend,
            path,
            permissions,
            selection,
            generation,
        }
    }

    pub(crate) fn update(
        &self,
        entry: Option<FileEntry>,
        backend: &str,
        connection: PaneConnection,
    ) {
        self.generation.set(self.generation.get().wrapping_add(1));
        let Some(entry) = entry else {
            self.icon.set_icon_name(Some("folder-symbolic"));
            self.name.set_text("No Selection");
            self.kind.set_text("—");
            self.size.set_text("—");
            self.modified.set_text("—");
            self.created.set_text("—");
            self.backend.set_text(backend);
            self.path.set_text("—");
            *self.selection.borrow_mut() = None;
            self.permissions.set_available(false);
            return;
        };

        self.icon.set_icon_name(Some(icon_name(entry.kind)));
        self.name.set_text(&entry.name);
        self.kind.set_text(kind_text(entry.kind));
        self.size.set_text(&format_size(entry.size));
        self.modified.set_text(&format_modified(entry.modified));
        self.created.set_text("Loading…");
        self.backend.set_text(backend);
        self.path.set_text(entry.path.as_str());
        *self.selection.borrow_mut() = Some(InspectorSelection { entry, connection });
        self.permissions.set_available(true);
        self.permissions.octal.set_text("");
        self.permissions.status.set_text("Loading metadata…");
        load_metadata_async(
            &self.permissions,
            &self.created,
            &self.selection,
            &self.generation,
        );
    }
}

fn connect_permission_actions(
    editor: &PermissionEditor,
    created: &gtk::Label,
    selection: &Rc<RefCell<Option<InspectorSelection>>>,
    generation: &Rc<Cell<u64>>,
) {
    for check in &editor.checks {
        let editor = editor.clone();
        check.connect_toggled(move |_| {
            let special = parse_mode(editor.octal.text().as_str()).map_or(0, |mode| mode & 0o7000);
            let mode = special | editor.permission_bits();
            editor.octal.set_text(&format!("{mode:04o}"));
        });
    }

    {
        let editor = editor.clone();
        let created = created.clone();
        let selection = Rc::clone(selection);
        let generation = Rc::clone(generation);
        editor.load.clone().connect_clicked(move |_| {
            load_metadata_async(&editor, &created, &selection, &generation);
        });
    }
    {
        let editor = editor.clone();
        let selection = Rc::clone(selection);
        let generation = Rc::clone(generation);
        editor.apply.clone().connect_clicked(move |_| {
            apply_permissions_async(&editor, &selection, &generation);
        });
    }
}

fn load_metadata_async(
    editor: &PermissionEditor,
    created: &gtk::Label,
    selection: &Rc<RefCell<Option<InspectorSelection>>>,
    generation: &Rc<Cell<u64>>,
) {
    let Some(selection) = selection.borrow().clone() else {
        editor.status.set_text("No selected item.");
        return;
    };
    let request_generation = generation.get();
    editor.load.set_sensitive(false);
    editor.apply.set_sensitive(false);
    editor.status.set_text("Loading metadata…");

    let (sender, receiver) = mpsc::channel();
    let _worker = std::thread::spawn(move || {
        let result = load_metadata_worker(selection);
        let _send_result = sender.send(result);
    });

    let editor = editor.clone();
    let created = created.clone();
    let generation = Rc::clone(generation);
    let _poll = gtk::glib::timeout_add_local(Duration::from_millis(25), move || {
        match receiver.try_recv() {
            Ok(result) => {
                if generation.get() == request_generation {
                    apply_metadata_result(&editor, &created, result);
                }
                gtk::glib::ControlFlow::Break
            }
            Err(TryRecvError::Empty) => gtk::glib::ControlFlow::Continue,
            Err(TryRecvError::Disconnected) => {
                if generation.get() == request_generation {
                    editor.set_available(true);
                    editor
                        .status
                        .set_text("Metadata worker stopped unexpectedly.");
                }
                gtk::glib::ControlFlow::Break
            }
        }
    });
}

fn load_metadata_worker(selection: InspectorSelection) -> InspectorMetadataResult {
    let InspectorSelection { entry, connection } = selection;
    match connection.connect_backend() {
        Ok(backend) => InspectorMetadataResult {
            created: backend
                .created_time(&entry.path)
                .map_err(|error| error.to_string()),
            unix_mode: backend
                .unix_mode(&entry.path)
                .map_err(|error| error.to_string()),
            permissions_supported: backend.capabilities().unix_permissions.is_supported(),
        },
        Err(error) => InspectorMetadataResult {
            created: Err(format!("Connect failed: {error}")),
            unix_mode: Err(format!("Connect failed: {error}")),
            permissions_supported: false,
        },
    }
}

fn apply_metadata_result(
    editor: &PermissionEditor,
    created: &gtk::Label,
    result: InspectorMetadataResult,
) {
    match result.created {
        Ok(value) => created.set_text(&format_modified(value)),
        Err(error) => created.set_text(&format!("Unavailable ({error})")),
    }

    if !result.permissions_supported {
        editor.set_available(false);
        editor
            .status
            .set_text("This backend does not expose Unix permissions.");
        return;
    }

    editor.set_available(true);
    match result.unix_mode {
        Ok(Some(mode)) => editor.set_mode(mode),
        Ok(None) => editor
            .status
            .set_text("Unix mode is unavailable for this item."),
        Err(error) => editor
            .status
            .set_text(&format!("Permission read failed: {error}")),
    }
}

fn apply_permissions_async(
    editor: &PermissionEditor,
    selection: &Rc<RefCell<Option<InspectorSelection>>>,
    generation: &Rc<Cell<u64>>,
) {
    let Some(selection) = selection.borrow().clone() else {
        editor.status.set_text("No selected item.");
        return;
    };
    let mode = match parse_mode(editor.octal.text().as_str()) {
        Ok(mode) => mode,
        Err(error) => {
            editor.status.set_text(&error);
            return;
        }
    };
    let request_generation = generation.get();
    editor.set_available(false);
    editor.status.set_text("Applying permissions…");

    let (sender, receiver) = mpsc::channel();
    let _worker = std::thread::spawn(move || {
        let result = apply_permissions_worker(selection, mode);
        let _send_result = sender.send(result);
    });

    let editor = editor.clone();
    let generation = Rc::clone(generation);
    let _poll = gtk::glib::timeout_add_local(Duration::from_millis(25), move || {
        match receiver.try_recv() {
            Ok(result) => {
                if generation.get() == request_generation {
                    apply_permission_result(&editor, mode, result);
                }
                gtk::glib::ControlFlow::Break
            }
            Err(TryRecvError::Empty) => gtk::glib::ControlFlow::Continue,
            Err(TryRecvError::Disconnected) => {
                if generation.get() == request_generation {
                    editor.set_available(true);
                    editor
                        .status
                        .set_text("Permission worker stopped unexpectedly.");
                }
                gtk::glib::ControlFlow::Break
            }
        }
    });
}

fn apply_permissions_worker(selection: InspectorSelection, mode: u32) -> PermissionApplyResult {
    let InspectorSelection { entry, connection } = selection;
    let backend = match connection.connect_backend() {
        Ok(backend) => backend,
        Err(error) => {
            return PermissionApplyResult {
                unix_mode: Err(format!("Connect failed: {error}")),
                permissions_supported: false,
            };
        }
    };
    if !backend.capabilities().unix_permissions.is_supported() {
        return PermissionApplyResult {
            unix_mode: Ok(None),
            permissions_supported: false,
        };
    }
    let unix_mode = backend
        .set_unix_mode(&entry.path, mode)
        .and_then(|()| backend.unix_mode(&entry.path))
        .map_err(|error| error.to_string());
    PermissionApplyResult {
        unix_mode,
        permissions_supported: true,
    }
}

fn apply_permission_result(
    editor: &PermissionEditor,
    requested: u32,
    result: PermissionApplyResult,
) {
    if !result.permissions_supported {
        editor.set_available(false);
        editor
            .status
            .set_text("This backend does not support Unix permission writes.");
        return;
    }
    editor.set_available(true);
    match result.unix_mode {
        Ok(Some(applied)) => {
            editor.set_mode(applied);
            editor
                .status
                .set_text(&format!("Applied and verified POSIX mode {applied:04o}."));
        }
        Ok(None) => editor.status.set_text(&format!(
            "Applied POSIX mode {requested:04o}; verification unavailable."
        )),
        Err(error) => editor
            .status
            .set_text(&format!("Permission update failed: {error}")),
    }
}

fn parse_mode(text: &str) -> Result<u32, String> {
    let trimmed = text.trim();
    let digits = trimmed.strip_prefix("0o").unwrap_or(trimmed);
    if digits.is_empty()
        || digits.len() > 4
        || !digits.bytes().all(|byte| (b'0'..=b'7').contains(&byte))
    {
        return Err("Enter an octal mode from 0000 through 7777.".to_owned());
    }
    let mode =
        u32::from_str_radix(digits, 8).map_err(|error| format!("Invalid octal mode: {error}"))?;
    if mode > 0o7777 {
        return Err("Unix mode must not exceed 7777.".to_owned());
    }
    Ok(mode)
}

fn build_permission_editor() -> (gtk::Box, PermissionEditor) {
    let permissions = gtk::Box::new(Orientation::Vertical, 8);
    permissions.set_margin_top(12);
    permissions.set_margin_bottom(12);
    permissions.set_margin_start(14);
    permissions.set_margin_end(14);
    permissions.append(&section_label("Permissions"));

    let grid = gtk::Grid::builder()
        .column_spacing(12)
        .row_spacing(6)
        .build();
    for (column, title) in ["Read", "Write", "Execute"].iter().enumerate() {
        let label = gtk::Label::new(Some(title));
        label.add_css_class("dim-label");
        grid.attach(&label, i32::try_from(column + 1).unwrap_or(1), 0, 1, 1);
    }

    let mut checks = Vec::with_capacity(9);
    for (row, title) in ["User", "Group", "World"].iter().enumerate() {
        let label = gtk::Label::new(Some(title));
        label.set_xalign(0.0);
        label.add_css_class("dim-label");
        let row_index = i32::try_from(row + 1).unwrap_or(1);
        grid.attach(&label, 0, row_index, 1, 1);
        for column in 1..=3 {
            let check = gtk::CheckButton::new();
            grid.attach(&check, column, row_index, 1, 1);
            checks.push(check);
        }
    }
    permissions.append(&grid);

    let mode_row = gtk::Box::new(Orientation::Horizontal, 8);
    let mode_label = gtk::Label::new(Some("Octal"));
    mode_label.set_xalign(0.0);
    mode_label.add_css_class("dim-label");
    let octal = gtk::Entry::new();
    octal.set_width_chars(6);
    octal.set_placeholder_text(Some("0644"));
    octal.set_tooltip_text(Some(
        "POSIX mode, including optional setuid/setgid/sticky bits",
    ));
    mode_row.append(&mode_label);
    mode_row.append(&octal);
    permissions.append(&mode_row);

    let buttons = gtk::Box::new(Orientation::Horizontal, 8);
    let load = gtk::Button::with_label("Load Mode");
    let apply = gtk::Button::with_label("Apply Mode");
    apply.add_css_class("suggested-action");
    buttons.append(&load);
    buttons.append(&apply);
    permissions.append(&buttons);

    let status = gtk::Label::builder()
        .label("Select a file or folder to inspect permissions.")
        .wrap(true)
        .xalign(0.0)
        .css_classes(vec!["dim-label".to_owned()])
        .build();
    permissions.append(&status);

    (
        permissions,
        PermissionEditor {
            checks,
            octal,
            load,
            apply,
            status,
        },
    )
}

fn detail_row(label: &str, value: &gtk::Label) -> gtk::Box {
    let row = gtk::Box::new(Orientation::Horizontal, 8);
    let label = gtk::Label::new(Some(label));
    label.set_xalign(0.0);
    label.set_width_chars(10);
    label.add_css_class("dim-label");
    value.set_hexpand(true);
    row.append(&label);
    row.append(value);
    row
}

fn section_label(text: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    label.set_xalign(0.0);
    label.add_css_class("heading");
    label
}

fn value_label(text: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    label.set_xalign(0.0);
    label
}

const fn kind_text(kind: EntryKind) -> &'static str {
    match kind {
        EntryKind::File => "File",
        EntryKind::Directory => "Folder",
        EntryKind::Symlink => "Symbolic Link",
        EntryKind::Other => "Other",
    }
}

const fn icon_name(kind: EntryKind) -> &'static str {
    match kind {
        EntryKind::File => "text-x-generic-symbolic",
        EntryKind::Directory => "folder-symbolic",
        EntryKind::Symlink => "emblem-symbolic-link-symbolic",
        EntryKind::Other => "unknown-symbolic",
    }
}
