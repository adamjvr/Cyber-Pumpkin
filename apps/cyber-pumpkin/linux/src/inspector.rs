use crate::browser::{format_modified, format_size};
use cyber_pumpkin_core::{EntryKind, FileEntry};
use gtk::Orientation;
use gtk::prelude::*;

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
}

impl InspectorPane {
    pub(crate) fn new() -> Self {
        let root = gtk::Box::new(Orientation::Vertical, 0);
        root.set_width_request(270);

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

        let permissions = gtk::Box::new(Orientation::Vertical, 8);
        permissions.set_margin_top(12);
        permissions.set_margin_bottom(12);
        permissions.set_margin_start(14);
        permissions.set_margin_end(14);
        permissions.append(&section_label("Permissions"));
        permissions.append(&permission_grid());
        permissions.append(
            &gtk::Label::builder()
                .label(
                    "Permission editing will activate when backend metadata write support lands.",
                )
                .wrap(true)
                .xalign(0.0)
                .css_classes(vec!["dim-label".to_owned()])
                .build(),
        );
        root.append(&permissions);

        Self {
            root,
            icon,
            name,
            kind,
            size,
            modified,
            backend,
            path,
        }
    }

    pub(crate) fn update(&self, entry: Option<FileEntry>, backend: &str) {
        let Some(entry) = entry else {
            self.icon.set_icon_name(Some("folder-symbolic"));
            self.name.set_text("No Selection");
            self.kind.set_text("—");
            self.size.set_text("—");
            self.modified.set_text("—");
            self.backend.set_text(backend);
            self.path.set_text("—");
            return;
        };

        self.icon.set_icon_name(Some(icon_name(entry.kind)));
        self.name.set_text(&entry.name);
        self.kind.set_text(kind_text(entry.kind));
        self.size.set_text(&format_size(entry.size));
        self.modified.set_text(&format_modified(entry.modified));
        self.backend.set_text(backend);
        self.path.set_text(entry.path.as_str());
    }
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

fn permission_grid() -> gtk::Grid {
    let grid = gtk::Grid::builder()
        .column_spacing(12)
        .row_spacing(6)
        .build();
    for (column, title) in ["Read", "Write", "Execute"].iter().enumerate() {
        let label = gtk::Label::new(Some(title));
        label.add_css_class("dim-label");
        grid.attach(&label, i32::try_from(column + 1).unwrap_or(1), 0, 1, 1);
    }
    for (row, title) in ["User", "Group", "World"].iter().enumerate() {
        let label = gtk::Label::new(Some(title));
        label.set_xalign(0.0);
        label.add_css_class("dim-label");
        grid.attach(&label, 0, i32::try_from(row + 1).unwrap_or(1), 1, 1);
        for column in 1..=3 {
            let check = gtk::CheckButton::new();
            check.set_sensitive(false);
            grid.attach(&check, column, i32::try_from(row + 1).unwrap_or(1), 1, 1);
        }
    }
    grid
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
