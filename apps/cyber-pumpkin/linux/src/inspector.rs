use crate::browser::{format_modified, format_size};
use crate::connection::PaneConnection;
use cyber_pumpkin_core::{EntryKind, FileEntry};
use gtk::Orientation;
use gtk::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

const PERMISSION_BITS: [u32; 9] = [
    0o400, 0o200, 0o100, 0o040, 0o020, 0o010, 0o004, 0o002, 0o001,
];

#[derive(Clone)]
struct InspectorSelection {
    entry: FileEntry,
    connection: PaneConnection,
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
    backend: gtk::Label,
    path: gtk::Label,
    permissions: PermissionEditor,
    selection: Rc<RefCell<Option<InspectorSelection>>>,
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
        let backend = value_label("—");
        let path = value_label("—");
        path.set_wrap(true);
        path.set_selectable(true);

        metadata.append(&detail_row("Kind", &kind));
        metadata.append(&detail_row("Size", &size));
        metadata.append(&detail_row("Modified", &modified));
        metadata.append(&detail_row("Location", &backend));
        metadata.append(&gtk::Separator::new(Orientation::Horizontal));
        metadata.append(&section_label("Path"));
        metadata.append(&path);
        root.append(&metadata);
        root.append(&gtk::Separator::new(Orientation::Horizontal));

        let (permission_box, permissions) = build_permission_editor();
        root.append(&permission_box);

        let selection = Rc::new(RefCell::new(None));
        connect_permission_actions(&permissions, &selection);
        permissions.set_available(false);

        Self {
            root,
            icon,
            name,
            kind,
            size,
            modified,
            backend,
            path,
            permissions,
            selection,
        }
    }

    pub(crate) fn update(
        &self,
        entry: Option<FileEntry>,
        backend: &str,
        connection: PaneConnection,
    ) {
        let Some(entry) = entry else {
            self.icon.set_icon_name(Some("folder-symbolic"));
            self.name.set_text("No Selection");
            self.kind.set_text("—");
            self.size.set_text("—");
            self.modified.set_text("—");
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
        self.backend.set_text(backend);
        self.path.set_text(entry.path.as_str());
        *self.selection.borrow_mut() = Some(InspectorSelection { entry, connection });
        self.permissions.set_available(true);
        self.permissions.octal.set_text("");
        self.permissions
            .status
            .set_text("Load permissions to inspect or edit the selected item.");
    }
}

fn connect_permission_actions(
    editor: &PermissionEditor,
    selection: &Rc<RefCell<Option<InspectorSelection>>>,
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
        let selection = Rc::clone(selection);
        editor.load.clone().connect_clicked(move |_| {
            load_permissions(&editor, &selection);
        });
    }
    {
        let editor = editor.clone();
        let selection = Rc::clone(selection);
        editor.apply.clone().connect_clicked(move |_| {
            apply_permissions(&editor, &selection);
        });
    }
}

fn load_permissions(
    editor: &PermissionEditor,
    selection: &Rc<RefCell<Option<InspectorSelection>>>,
) {
    let Some(selection) = selection.borrow().clone() else {
        editor.status.set_text("No selected item.");
        return;
    };
    let backend = match selection.connection.connect_backend() {
        Ok(backend) => backend,
        Err(error) => {
            editor.status.set_text(&format!("Connect failed: {error}"));
            return;
        }
    };
    if !backend.capabilities().unix_permissions.is_supported() {
        editor
            .status
            .set_text("This backend does not expose Unix permissions.");
        return;
    }
    match backend.unix_mode(&selection.entry.path) {
        Ok(Some(mode)) => editor.set_mode(mode),
        Ok(None) => editor
            .status
            .set_text("Unix mode is unavailable for this item."),
        Err(error) => editor
            .status
            .set_text(&format!("Permission read failed: {error}")),
    }
}

fn apply_permissions(
    editor: &PermissionEditor,
    selection: &Rc<RefCell<Option<InspectorSelection>>>,
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
    let backend = match selection.connection.connect_backend() {
        Ok(backend) => backend,
        Err(error) => {
            editor.status.set_text(&format!("Connect failed: {error}"));
            return;
        }
    };
    if !backend.capabilities().unix_permissions.is_supported() {
        editor
            .status
            .set_text("This backend does not support Unix permission writes.");
        return;
    }
    if let Err(error) = backend.set_unix_mode(&selection.entry.path, mode) {
        editor
            .status
            .set_text(&format!("Permission update failed: {error}"));
        return;
    }
    match backend.unix_mode(&selection.entry.path) {
        Ok(Some(applied)) => {
            editor.set_mode(applied);
            editor
                .status
                .set_text(&format!("Applied and verified POSIX mode {applied:04o}."));
        }
        Ok(None) => editor.status.set_text(&format!(
            "Applied POSIX mode {mode:04o}; verification unavailable."
        )),
        Err(error) => editor.status.set_text(&format!(
            "Applied POSIX mode {mode:04o}, but verification failed: {error}"
        )),
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
