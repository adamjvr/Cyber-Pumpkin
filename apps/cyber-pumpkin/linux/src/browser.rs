use crate::connection::PaneConnection;
use adw::prelude::*;
use cyber_pumpkin_application::PaneSession;
use cyber_pumpkin_core::{BackendId, BackendPath, EntryKind, FileEntry};
use gtk::Orientation;
use gtk::gio;
use std::cell::{Cell, RefCell};
use std::cmp::Ordering;
use std::path::Path;
use std::rc::Rc;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PaneSide {
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SortMode {
    Name,
    Type,
    Size,
    Date,
}

type SelectionObserver = Rc<dyn Fn(Option<FileEntry>, String)>;

#[derive(Clone)]
struct PaneWidgets {
    heading: gtk::Label,
    path: gtk::Entry,
    list: gtk::ListBox,
    back: gtk::Button,
    forward: gtk::Button,
    up: gtk::Button,
    footer: gtk::Label,
}

#[derive(Clone)]
pub(crate) struct PaneHandle {
    pub(crate) root: gtk::Box,
    session: Rc<RefCell<PaneSession>>,
    connection: Rc<RefCell<PaneConnection>>,
    entries: Rc<RefCell<Vec<FileEntry>>>,
    show_hidden: Rc<Cell<bool>>,
    sort_mode: Rc<Cell<SortMode>>,
    sort_descending: Rc<Cell<bool>>,
    filter_query: Rc<RefCell<String>>,
    selection_observer: Rc<RefCell<Option<SelectionObserver>>>,
    widgets: PaneWidgets,
}

pub(crate) fn build_pane(
    title: &str,
    initial_path: &str,
    side: PaneSide,
    active: &Rc<Cell<PaneSide>>,
) -> PaneHandle {
    let backend_id = match side {
        PaneSide::Left => "pane-left",
        PaneSide::Right => "pane-right",
    };
    let connection = PaneConnection::local(backend_id).unwrap_or_else(|error| {
        eprintln!("failed to create local pane connection: {error}");
        std::process::exit(1);
    });
    let session = create_session(connection.backend_id(), initial_path);
    let connection = Rc::new(RefCell::new(connection));
    let entries = Rc::new(RefCell::new(Vec::new()));
    let show_hidden = Rc::new(Cell::new(false));
    let sort_mode = Rc::new(Cell::new(SortMode::Name));
    let sort_descending = Rc::new(Cell::new(false));
    let filter_query = Rc::new(RefCell::new(String::new()));
    let selection_observer = Rc::new(RefCell::new(None));
    let (root, widgets) = create_widgets(title);

    let pane = PaneHandle {
        root,
        session,
        connection,
        entries,
        show_hidden,
        sort_mode,
        sort_descending,
        filter_query,
        selection_observer,
        widgets,
    };
    pane.connect_navigation(side, active);
    pane.connect_context_menu(side, active);
    pane.refresh();
    pane
}

impl PaneHandle {
    pub(crate) fn refresh(&self) {
        let current = self.session.borrow().location().clone();
        let connection = self.connection.borrow().clone();
        match load_directory(&connection, &current) {
            Ok(loaded) => self.render(loaded),
            Err(error) => self
                .widgets
                .footer
                .set_text(&format!("Load failed: {error}")),
        }
    }

    pub(crate) fn navigate_text(&self, text: &str) {
        match BackendPath::new(text) {
            Ok(path) => self.navigate_to(path),
            Err(error) => self
                .widgets
                .footer
                .set_text(&format!("Invalid path: {error}")),
        }
    }

    pub(crate) fn set_show_hidden(&self, show: bool) {
        self.show_hidden.set(show);
        self.refresh();
    }

    pub(crate) fn set_sort_mode(&self, mode: SortMode) {
        self.sort_mode.set(mode);
        self.refresh();
    }

    pub(crate) fn set_sort_descending(&self, descending: bool) {
        self.sort_descending.set(descending);
        self.refresh();
    }

    pub(crate) fn set_filter_query(&self, query: &str) {
        *self.filter_query.borrow_mut() = query.trim().to_lowercase();
        self.refresh();
    }

    pub(crate) fn selected_entry(&self) -> Option<FileEntry> {
        let row = self.widgets.list.selected_row()?;
        let index = usize::try_from(row.index()).ok()?;
        self.entries.borrow().get(index).cloned()
    }

    pub(crate) fn backend_id(&self) -> BackendId {
        self.connection.borrow().backend_id()
    }

    pub(crate) fn connection(&self) -> PaneConnection {
        self.connection.borrow().clone()
    }

    pub(crate) fn connection_display_name(&self) -> String {
        self.connection.borrow().display_name()
    }

    pub(crate) fn set_selection_observer<F>(&self, observer: F)
    where
        F: Fn(Option<FileEntry>, String) + 'static,
    {
        *self.selection_observer.borrow_mut() = Some(Rc::new(observer));
        self.notify_selection(None);
    }

    fn notify_selection(&self, entry: Option<FileEntry>) {
        if let Some(observer) = self.selection_observer.borrow().as_ref() {
            observer(entry, self.connection_display_name());
        }
    }

    pub(crate) fn connect_sftp(
        &self,
        host: &str,
        username: &str,
        port: u16,
        path: &str,
    ) -> Result<(), String> {
        let id = self.backend_id();
        let connection = PaneConnection::sftp(id.as_str(), host, username, port)?;
        let target = BackendPath::new(path).map_err(|error| error.to_string())?;
        let loaded = load_directory(&connection, &target)?;
        *self.connection.borrow_mut() = connection;
        *self.session.borrow_mut() = PaneSession::new(self.backend_id(), target);
        self.widgets.heading.set_text(&format!(
            "SFTP — {}",
            self.connection.borrow().display_name()
        ));
        self.render(loaded);
        Ok(())
    }

    pub(crate) fn disconnect_to_local(&self, path: &str) -> Result<(), String> {
        let id = self.backend_id();
        let connection = PaneConnection::local(id.as_str())?;
        let target = BackendPath::new(path).map_err(|error| error.to_string())?;
        let loaded = load_directory(&connection, &target)?;
        *self.connection.borrow_mut() = connection;
        *self.session.borrow_mut() = PaneSession::new(self.backend_id(), target);
        self.widgets.heading.set_text("Local");
        self.render(loaded);
        Ok(())
    }

    pub(crate) fn location(&self) -> BackendPath {
        self.session.borrow().location().clone()
    }

    pub(crate) fn destination_child(&self, name: &str) -> Result<BackendPath, String> {
        child_path(&self.location(), name)
    }

    pub(crate) fn create_folder(&self, name: &str) {
        if invalid_name(name) {
            self.widgets.footer.set_text("Folder name is invalid.");
            return;
        }
        let Ok(path) = self.destination_child(name) else {
            self.widgets.footer.set_text("Could not form folder path.");
            return;
        };
        let backend = match self.connection.borrow().connect_backend() {
            Ok(backend) => backend,
            Err(error) => {
                self.widgets
                    .footer
                    .set_text(&format!("Connect failed: {error}"));
                return;
            }
        };
        match backend.create_dir(&path) {
            Ok(()) => {
                self.widgets.footer.set_text(&format!("Created {name}"));
                self.refresh();
            }
            Err(error) => self
                .widgets
                .footer
                .set_text(&format!("Create failed: {error}")),
        }
    }

    pub(crate) fn rename_selected(&self, new_name: &str) {
        if invalid_name(new_name) {
            self.widgets.footer.set_text("Name is invalid.");
            return;
        }
        let Some(entry) = self.selected_entry() else {
            self.widgets.footer.set_text("Select an item to rename.");
            return;
        };
        let Ok(destination) = self.destination_child(new_name) else {
            self.widgets
                .footer
                .set_text("Could not form destination path.");
            return;
        };
        let backend = match self.connection.borrow().connect_backend() {
            Ok(backend) => backend,
            Err(error) => {
                self.widgets
                    .footer
                    .set_text(&format!("Connect failed: {error}"));
                return;
            }
        };
        match backend.rename(&entry.path, &destination) {
            Ok(()) => {
                self.widgets
                    .footer
                    .set_text(&format!("Renamed {} → {new_name}", entry.name));
                self.refresh();
            }
            Err(error) => self
                .widgets
                .footer
                .set_text(&format!("Rename failed: {error}")),
        }
    }

    pub(crate) fn delete_selected(&self) {
        let Some(entry) = self.selected_entry() else {
            self.widgets.footer.set_text("Select an item to delete.");
            return;
        };
        let backend = match self.connection.borrow().connect_backend() {
            Ok(backend) => backend,
            Err(error) => {
                self.widgets
                    .footer
                    .set_text(&format!("Connect failed: {error}"));
                return;
            }
        };
        match backend.remove(&entry.path) {
            Ok(()) => {
                self.widgets
                    .footer
                    .set_text(&format!("Deleted {}", entry.name));
                self.refresh();
            }
            Err(error) => self
                .widgets
                .footer
                .set_text(&format!("Delete failed: {error}")),
        }
    }

    pub(crate) fn selected_name(&self) -> Option<String> {
        self.selected_entry().map(|entry| entry.name)
    }

    fn connect_navigation(&self, side: PaneSide, active: &Rc<Cell<PaneSide>>) {
        self.connect_row_activation(side, active);
        self.connect_path_entry(side, active);
        self.connect_back(side, active);
        self.connect_forward(side, active);
        self.connect_up(side, active);
        self.connect_selection(side, active);
    }

    fn connect_row_activation(&self, side: PaneSide, active: &Rc<Cell<PaneSide>>) {
        let pane = self.clone();
        let active = Rc::clone(active);
        self.widgets
            .list
            .clone()
            .connect_row_activated(move |_, row| {
                active.set(side);
                let Ok(index) = usize::try_from(row.index()) else {
                    return;
                };
                let Some(entry) = pane.entries.borrow().get(index).cloned() else {
                    return;
                };
                if entry.kind == EntryKind::Directory {
                    pane.navigate_to(entry.path);
                }
            });
    }

    fn connect_path_entry(&self, side: PaneSide, active: &Rc<Cell<PaneSide>>) {
        let pane = self.clone();
        let active = Rc::clone(active);
        self.widgets.path.clone().connect_activate(move |entry| {
            active.set(side);
            pane.navigate_text(entry.text().as_str());
        });
    }

    fn connect_back(&self, side: PaneSide, active: &Rc<Cell<PaneSide>>) {
        let pane = self.clone();
        let active = Rc::clone(active);
        self.widgets.back.clone().connect_clicked(move |_| {
            active.set(side);
            pane.go_back();
        });
    }

    fn connect_forward(&self, side: PaneSide, active: &Rc<Cell<PaneSide>>) {
        let pane = self.clone();
        let active = Rc::clone(active);
        self.widgets.forward.clone().connect_clicked(move |_| {
            active.set(side);
            pane.go_forward();
        });
    }

    fn connect_up(&self, side: PaneSide, active: &Rc<Cell<PaneSide>>) {
        let pane = self.clone();
        let active = Rc::clone(active);
        self.widgets.up.clone().connect_clicked(move |_| {
            active.set(side);
            pane.go_up();
        });
    }

    fn connect_selection(&self, side: PaneSide, active: &Rc<Cell<PaneSide>>) {
        let pane = self.clone();
        let active = Rc::clone(active);
        self.widgets
            .list
            .clone()
            .connect_row_selected(move |_, row| {
                active.set(side);
                let Some(row) = row else {
                    pane.update_footer();
                    pane.notify_selection(None);
                    return;
                };
                let Ok(index) = usize::try_from(row.index()) else {
                    return;
                };
                let Some(entry) = pane.entries.borrow().get(index).cloned() else {
                    return;
                };
                pane.widgets.footer.set_text(&format!(
                    "{} selected • {}",
                    entry.name,
                    format_size(entry.size)
                ));
                pane.notify_selection(Some(entry));
            });
    }

    #[allow(clippy::cast_possible_truncation)]
    fn connect_context_menu(&self, side: PaneSide, active: &Rc<Cell<PaneSide>>) {
        let menu = gio::Menu::new();
        menu.append(Some("Copy to Other Pane"), Some("app.copy-other"));
        menu.append(Some("Get Info"), Some("app.info"));
        menu.append(Some("Rename…"), Some("app.rename"));
        menu.append(Some("Delete…"), Some("app.delete"));

        let popover = gtk::PopoverMenu::from_model(Some(&menu));
        popover.set_parent(&self.widgets.list);

        let gesture = gtk::GestureClick::new();
        gesture.set_button(3);
        let active = Rc::clone(active);
        let list = self.widgets.list.clone();
        let popover_for_click = popover.clone();
        gesture.connect_pressed(move |_, _, _, y| {
            active.set(side);
            if let Some(row) = list.row_at_y(y as i32) {
                list.select_row(Some(&row));
            }
            popover_for_click.popup();
        });
        self.widgets.list.add_controller(gesture);
    }

    fn navigate_to(&self, target: BackendPath) {
        let connection = self.connection.borrow().clone();
        match load_directory(&connection, &target) {
            Ok(loaded) => {
                self.session.borrow_mut().navigate_to(target);
                self.render(loaded);
            }
            Err(error) => self
                .widgets
                .footer
                .set_text(&format!("Navigation failed: {error}")),
        }
    }

    fn go_back(&self) {
        let Some(target) = self.session.borrow().back_location().cloned() else {
            return;
        };
        let connection = self.connection.borrow().clone();
        let Ok(loaded) = load_directory(&connection, &target) else {
            return;
        };
        let _changed = self.session.borrow_mut().go_back();
        self.render(loaded);
    }

    fn go_forward(&self) {
        let Some(target) = self.session.borrow().forward_location().cloned() else {
            return;
        };
        let connection = self.connection.borrow().clone();
        let Ok(loaded) = load_directory(&connection, &target) else {
            return;
        };
        let _changed = self.session.borrow_mut().go_forward();
        self.render(loaded);
    }

    fn go_up(&self) {
        let current = self.location();
        let Some(parent) = Path::new(current.as_str()).parent() else {
            return;
        };
        let Some(parent) = parent.to_str() else {
            return;
        };
        self.navigate_text(parent);
    }

    fn render(&self, loaded: Vec<FileEntry>) {
        while let Some(row) = self.widgets.list.row_at_index(0) {
            self.widgets.list.remove(&row);
        }

        let show_hidden = self.show_hidden.get();
        let filter_query = self.filter_query.borrow().clone();
        let mut visible: Vec<FileEntry> = loaded
            .into_iter()
            .filter(|entry| show_hidden || !entry.name.starts_with('.'))
            .filter(|entry| {
                filter_query.is_empty() || entry.name.to_lowercase().contains(&filter_query)
            })
            .collect();
        sort_entries(
            &mut visible,
            self.sort_mode.get(),
            self.sort_descending.get(),
        );

        for entry in &visible {
            let row = gtk::ListBoxRow::new();
            row.set_child(Some(&entry_grid(entry)));
            self.widgets.list.append(&row);
        }

        *self.entries.borrow_mut() = visible;
        self.update_navigation_widgets();
        self.update_footer();
        self.notify_selection(None);
    }

    fn update_navigation_widgets(&self) {
        let session = self.session.borrow();
        self.widgets.path.set_text(session.location().as_str());
        self.widgets.back.set_sensitive(session.can_go_back());
        self.widgets.forward.set_sensitive(session.can_go_forward());
        self.widgets
            .up
            .set_sensitive(Path::new(session.location().as_str()).parent().is_some());
    }

    fn update_footer(&self) {
        let count = self.entries.borrow().len();
        let hidden = if self.show_hidden.get() {
            " • hidden shown"
        } else {
            ""
        };
        let order = if self.sort_descending.get() {
            "descending"
        } else {
            "ascending"
        };
        let filtered = if self.filter_query.borrow().is_empty() {
            ""
        } else {
            " • filtered"
        };
        self.widgets.footer.set_text(&format!(
            "{count} items{hidden}{filtered} • {} {order}",
            sort_mode_text(self.sort_mode.get())
        ));
    }
}

fn invalid_name(name: &str) -> bool {
    name.is_empty() || name.contains('/')
}

fn create_session(backend: BackendId, initial_path: &str) -> Rc<RefCell<PaneSession>> {
    let initial = BackendPath::new(initial_path).unwrap_or_else(|_| {
        BackendPath::new("/").unwrap_or_else(|error| {
            eprintln!("failed to create root backend path: {error}");
            std::process::exit(1);
        })
    });
    Rc::new(RefCell::new(PaneSession::new(backend, initial)))
}

fn create_widgets(title: &str) -> (gtk::Box, PaneWidgets) {
    let pane = gtk::Box::new(Orientation::Vertical, 0);

    let location_band = gtk::Box::new(Orientation::Horizontal, 6);
    location_band.set_margin_top(4);
    location_band.set_margin_bottom(4);
    location_band.set_margin_start(8);
    location_band.set_margin_end(8);

    let location_icon = gtk::Image::from_icon_name("drive-harddisk-symbolic");
    location_icon.set_pixel_size(16);
    let heading = gtk::Label::new(Some(title));
    heading.set_xalign(0.0);
    heading.set_hexpand(true);
    heading.add_css_class("heading");
    let view_icon = gtk::Image::from_icon_name("view-list-symbolic");
    view_icon.add_css_class("dim-label");
    location_band.append(&location_icon);
    location_band.append(&heading);
    location_band.append(&view_icon);
    pane.append(&location_band);

    let (nav, path, back, forward, up) = create_navigation_row();
    pane.append(&nav);
    pane.append(&gtk::Separator::new(Orientation::Horizontal));
    pane.append(&create_column_header());

    let list = gtk::ListBox::new();
    list.set_selection_mode(gtk::SelectionMode::Single);
    list.add_css_class("navigation-sidebar");
    let scroll = gtk::ScrolledWindow::builder()
        .hexpand(true)
        .vexpand(true)
        .child(&list)
        .build();
    pane.append(&scroll);

    let footer = gtk::Label::new(Some("0 items"));
    footer.set_xalign(0.0);
    footer.set_margin_top(3);
    footer.set_margin_bottom(4);
    footer.set_margin_start(8);
    footer.add_css_class("dim-label");
    pane.append(&footer);

    (
        pane,
        PaneWidgets {
            heading,
            path,
            list,
            back,
            forward,
            up,
            footer,
        },
    )
}

fn create_navigation_row() -> (gtk::Box, gtk::Entry, gtk::Button, gtk::Button, gtk::Button) {
    let nav = gtk::Box::new(Orientation::Horizontal, 3);
    nav.set_margin_top(3);
    nav.set_margin_bottom(4);
    nav.set_margin_start(8);
    nav.set_margin_end(8);

    let back = compact_icon_button("go-previous-symbolic", "Back");
    let forward = compact_icon_button("go-next-symbolic", "Forward");
    let up = compact_icon_button("go-up-symbolic", "Up");

    let path = gtk::Entry::new();
    path.set_hexpand(true);
    path.set_placeholder_text(Some("Location"));
    path.add_css_class("flat");
    nav.append(&back);
    nav.append(&forward);
    nav.append(&up);
    nav.append(&path);
    (nav, path, back, forward, up)
}

fn compact_icon_button(icon: &str, tooltip: &str) -> gtk::Button {
    let button = gtk::Button::from_icon_name(icon);
    button.set_tooltip_text(Some(tooltip));
    button.add_css_class("flat");
    button
}

fn create_column_header() -> gtk::Grid {
    let grid = gtk::Grid::builder().column_spacing(8).build();
    grid.set_margin_top(3);
    grid.set_margin_bottom(3);
    grid.set_margin_start(4);
    grid.set_margin_end(4);
    let name = column_label("Name", 0.0);
    let size = column_label("Size", 1.0);
    let date = column_label("Date", 0.0);
    name.set_hexpand(true);
    size.set_width_chars(10);
    date.set_width_chars(18);
    grid.attach(&name, 0, 0, 1, 1);
    grid.attach(&size, 1, 0, 1, 1);
    grid.attach(&date, 2, 0, 1, 1);
    grid.add_css_class("heading");
    grid
}

fn entry_grid(entry: &FileEntry) -> gtk::Grid {
    let grid = gtk::Grid::builder().column_spacing(8).build();
    grid.set_margin_start(4);
    grid.set_margin_end(4);
    grid.set_margin_top(1);
    grid.set_margin_bottom(1);

    let identity = gtk::Box::new(Orientation::Horizontal, 6);
    let icon = gtk::Image::from_icon_name(entry_icon_name(entry.kind));
    icon.set_pixel_size(16);
    let name = column_label(&entry.name, 0.0);
    name.set_hexpand(true);
    identity.append(&icon);
    identity.append(&name);
    identity.set_hexpand(true);

    let size = column_label(&format_size(entry.size), 1.0);
    size.set_width_chars(10);
    size.add_css_class("dim-label");
    let date = column_label(&format_modified(entry.modified), 0.0);
    date.set_width_chars(18);
    date.add_css_class("dim-label");

    grid.attach(&identity, 0, 0, 1, 1);
    grid.attach(&size, 1, 0, 1, 1);
    grid.attach(&date, 2, 0, 1, 1);
    grid
}

fn column_label(text: &str, xalign: f32) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    label.set_xalign(xalign);
    label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    label
}

fn entry_icon_name(kind: EntryKind) -> &'static str {
    match kind {
        EntryKind::File => "text-x-generic-symbolic",
        EntryKind::Directory => "folder-symbolic",
        EntryKind::Symlink => "emblem-symbolic-link-symbolic",
        EntryKind::Other => "unknown-symbolic",
    }
}

pub(crate) fn format_modified(modified: Option<u64>) -> String {
    let Some(seconds) = modified else {
        return "—".to_owned();
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(seconds, |duration| duration.as_secs());
    if seconds >= now {
        return "now".to_owned();
    }
    let age = now - seconds;
    if age < 3_600 {
        format!("{}m ago", age / 60)
    } else if age < 86_400 {
        format!("{}h ago", age / 3_600)
    } else if age < 2_592_000 {
        format!("{}d ago", age / 86_400)
    } else if age < 31_536_000 {
        format!("{}mo ago", age / 2_592_000)
    } else {
        format!("{}y ago", age / 31_536_000)
    }
}

fn entry_kind_text(kind: EntryKind) -> &'static str {
    match kind {
        EntryKind::File => "File",
        EntryKind::Directory => "Folder",
        EntryKind::Symlink => "Link",
        EntryKind::Other => "Other",
    }
}

fn sort_entries(entries: &mut [FileEntry], mode: SortMode, descending: bool) {
    entries.sort_by(|left, right| compare_entries(left, right, mode));
    if descending {
        entries.reverse();
    }
}

fn compare_entries(left: &FileEntry, right: &FileEntry, mode: SortMode) -> Ordering {
    let folder_order = folder_rank(left.kind).cmp(&folder_rank(right.kind));
    if folder_order != Ordering::Equal {
        return folder_order;
    }

    match mode {
        SortMode::Name => compare_names(left, right),
        SortMode::Type => entry_kind_text(left.kind)
            .cmp(entry_kind_text(right.kind))
            .then_with(|| compare_names(left, right)),
        SortMode::Size => left
            .size
            .unwrap_or_default()
            .cmp(&right.size.unwrap_or_default())
            .then_with(|| compare_names(left, right)),
        SortMode::Date => left
            .modified
            .unwrap_or_default()
            .cmp(&right.modified.unwrap_or_default())
            .then_with(|| compare_names(left, right)),
    }
}

fn compare_names(left: &FileEntry, right: &FileEntry) -> Ordering {
    left.name
        .to_lowercase()
        .cmp(&right.name.to_lowercase())
        .then_with(|| left.name.cmp(&right.name))
}

const fn folder_rank(kind: EntryKind) -> u8 {
    if matches!(kind, EntryKind::Directory) {
        0
    } else {
        1
    }
}

const fn sort_mode_text(mode: SortMode) -> &'static str {
    match mode {
        SortMode::Name => "name",
        SortMode::Type => "type",
        SortMode::Size => "size",
        SortMode::Date => "date",
    }
}

pub(crate) fn format_size(size: Option<u64>) -> String {
    let Some(bytes) = size else {
        return "—".to_owned();
    };
    if bytes < 1_024 {
        format!("{bytes} B")
    } else if bytes < 1_048_576 {
        format!("{} KiB", bytes / 1_024)
    } else if bytes < 1_073_741_824 {
        format!("{} MiB", bytes / 1_048_576)
    } else {
        format!("{} GiB", bytes / 1_073_741_824)
    }
}

fn child_path(directory: &BackendPath, name: &str) -> Result<BackendPath, String> {
    let child = Path::new(directory.as_str()).join(name);
    let text = child
        .to_str()
        .ok_or_else(|| "local path is not valid UTF-8".to_owned())?;
    BackendPath::new(text).map_err(|error| error.to_string())
}

fn load_directory(
    connection: &PaneConnection,
    path: &BackendPath,
) -> Result<Vec<FileEntry>, String> {
    connection
        .connect_backend()?
        .list(path)
        .map_err(|error| error.to_string())
}
