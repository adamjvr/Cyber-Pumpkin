use adw::prelude::*;
use cyber_pumpkin_application::PaneSession;
use cyber_pumpkin_backend::Backend;
use cyber_pumpkin_core::{BackendId, BackendPath, EntryKind, FileEntry};
use cyber_pumpkin_local::LocalBackend;
use gtk::Orientation;
use std::cell::{Cell, RefCell};
use std::path::Path;
use std::rc::Rc;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PaneSide {
    Left,
    Right,
}

#[derive(Clone)]
struct PaneWidgets {
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
    entries: Rc<RefCell<Vec<FileEntry>>>,
    show_hidden: Rc<Cell<bool>>,
    widgets: PaneWidgets,
}

pub(crate) fn build_pane(
    title: &str,
    initial_path: &str,
    side: PaneSide,
    active: &Rc<Cell<PaneSide>>,
) -> PaneHandle {
    let session = create_session(initial_path);
    let entries = Rc::new(RefCell::new(Vec::new()));
    let show_hidden = Rc::new(Cell::new(false));
    let (root, widgets) = create_widgets(title);

    let pane = PaneHandle {
        root,
        session,
        entries,
        show_hidden,
        widgets,
    };
    pane.connect_navigation(side, active);
    pane.refresh();
    pane
}

impl PaneHandle {
    pub(crate) fn refresh(&self) {
        let current = self.session.borrow().location().clone();
        match load_directory(&current) {
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

    pub(crate) fn selected_entry(&self) -> Option<FileEntry> {
        let row = self.widgets.list.selected_row()?;
        let index = usize::try_from(row.index()).ok()?;
        self.entries.borrow().get(index).cloned()
    }

    pub(crate) fn backend_id(&self) -> BackendId {
        self.session.borrow().backend().clone()
    }

    pub(crate) fn location(&self) -> BackendPath {
        self.session.borrow().location().clone()
    }

    pub(crate) fn destination_child(&self, name: &str) -> Result<BackendPath, String> {
        local_child_path(&self.location(), name)
    }

    pub(crate) fn create_folder(&self, name: &str) {
        if name.is_empty() || name.contains('/') {
            self.widgets.footer.set_text("Folder name is invalid.");
            return;
        }
        let Ok(path) = self.destination_child(name) else {
            self.widgets.footer.set_text("Could not form folder path.");
            return;
        };
        let backend = LocalBackend::new(self.backend_id());
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
            });
    }

    fn navigate_to(&self, target: BackendPath) {
        match load_directory(&target) {
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
        let Ok(loaded) = load_directory(&target) else {
            return;
        };
        let _changed = self.session.borrow_mut().go_back();
        self.render(loaded);
    }

    fn go_forward(&self) {
        let Some(target) = self.session.borrow().forward_location().cloned() else {
            return;
        };
        let Ok(loaded) = load_directory(&target) else {
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
        while let Some(child) = self.widgets.list.first_child() {
            self.widgets.list.remove(&child);
        }

        let show_hidden = self.show_hidden.get();
        let visible: Vec<FileEntry> = loaded
            .into_iter()
            .filter(|entry| show_hidden || !entry.name.starts_with('.'))
            .collect();

        for entry in &visible {
            let row = gtk::ListBoxRow::new();
            row.set_child(Some(&entry_grid(entry)));
            self.widgets.list.append(&row);
        }

        *self.entries.borrow_mut() = visible;
        let session = self.session.borrow();
        self.widgets.path.set_text(session.location().as_str());
        self.widgets.back.set_sensitive(session.can_go_back());
        self.widgets.forward.set_sensitive(session.can_go_forward());
        self.widgets
            .up
            .set_sensitive(Path::new(session.location().as_str()).parent().is_some());
        drop(session);
        self.update_footer();
    }

    fn update_footer(&self) {
        let count = self.entries.borrow().len();
        let hidden = if self.show_hidden.get() {
            " • hidden shown"
        } else {
            ""
        };
        self.widgets
            .footer
            .set_text(&format!("{count} items{hidden}"));
    }
}

fn create_session(initial_path: &str) -> Rc<RefCell<PaneSession>> {
    let initial = BackendPath::new(initial_path).unwrap_or_else(|_| {
        BackendPath::new("/").unwrap_or_else(|error| {
            eprintln!("failed to create root backend path: {error}");
            std::process::exit(1);
        })
    });
    let backend = BackendId::new("local-ui").unwrap_or_else(|error| {
        eprintln!("failed to create local backend id: {error}");
        std::process::exit(1);
    });
    Rc::new(RefCell::new(PaneSession::new(backend, initial)))
}

fn create_widgets(title: &str) -> (gtk::Box, PaneWidgets) {
    let pane = gtk::Box::new(Orientation::Vertical, 6);
    pane.set_margin_top(8);
    pane.set_margin_bottom(8);
    pane.set_margin_start(8);
    pane.set_margin_end(8);

    let heading = gtk::Label::new(Some(title));
    heading.set_xalign(0.0);
    heading.add_css_class("heading");
    pane.append(&heading);

    let (nav, path, back, forward, up) = create_navigation_row();
    pane.append(&nav);
    pane.append(&create_column_header());

    let list = gtk::ListBox::new();
    list.set_selection_mode(gtk::SelectionMode::Single);
    let scroll = gtk::ScrolledWindow::builder()
        .hexpand(true)
        .vexpand(true)
        .child(&list)
        .build();
    pane.append(&scroll);

    let footer = gtk::Label::new(Some("0 items"));
    footer.set_xalign(0.0);
    footer.add_css_class("dim-label");
    pane.append(&footer);

    (
        pane,
        PaneWidgets {
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
    let nav = gtk::Box::new(Orientation::Horizontal, 4);
    let back = gtk::Button::from_icon_name("go-previous-symbolic");
    let forward = gtk::Button::from_icon_name("go-next-symbolic");
    let up = gtk::Button::from_icon_name("go-up-symbolic");
    back.set_tooltip_text(Some("Back"));
    forward.set_tooltip_text(Some("Forward"));
    up.set_tooltip_text(Some("Up"));

    let path = gtk::Entry::new();
    path.set_hexpand(true);
    nav.append(&back);
    nav.append(&forward);
    nav.append(&up);
    nav.append(&path);
    (nav, path, back, forward, up)
}

fn create_column_header() -> gtk::Grid {
    let grid = gtk::Grid::builder().column_spacing(12).build();
    let name = column_label("Name", 0.0);
    let kind = column_label("Type", 0.0);
    let size = column_label("Size", 1.0);
    name.set_hexpand(true);
    kind.set_width_chars(10);
    size.set_width_chars(12);
    grid.attach(&name, 0, 0, 1, 1);
    grid.attach(&kind, 1, 0, 1, 1);
    grid.attach(&size, 2, 0, 1, 1);
    grid.add_css_class("heading");
    grid
}

fn entry_grid(entry: &FileEntry) -> gtk::Grid {
    let grid = gtk::Grid::builder().column_spacing(12).build();
    let name = column_label(&entry.name, 0.0);
    let kind = column_label(entry_kind_text(entry.kind), 0.0);
    let size = column_label(&format_size(entry.size), 1.0);
    name.set_hexpand(true);
    kind.set_width_chars(10);
    size.set_width_chars(12);
    grid.attach(&name, 0, 0, 1, 1);
    grid.attach(&kind, 1, 0, 1, 1);
    grid.attach(&size, 2, 0, 1, 1);
    grid.set_margin_top(4);
    grid.set_margin_bottom(4);
    grid
}

fn column_label(text: &str, xalign: f32) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    label.set_xalign(xalign);
    label.set_margin_start(8);
    label.set_margin_end(8);
    label
}

const fn entry_kind_text(kind: EntryKind) -> &'static str {
    match kind {
        EntryKind::File => "File",
        EntryKind::Directory => "Folder",
        EntryKind::Symlink => "Link",
        EntryKind::Other => "Other",
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

fn local_child_path(directory: &BackendPath, name: &str) -> Result<BackendPath, String> {
    let child = Path::new(directory.as_str()).join(name);
    let text = child
        .to_str()
        .ok_or_else(|| "local path is not valid UTF-8".to_owned())?;
    BackendPath::new(text).map_err(|error| error.to_string())
}

fn load_directory(path: &BackendPath) -> Result<Vec<FileEntry>, String> {
    let id = BackendId::new("local-ui").map_err(|error| error.to_string())?;
    LocalBackend::new(id)
        .list(path)
        .map_err(|error| error.to_string())
}
