//! Native GTK4/libadwaita frontend for Cyber-Pumpkin on Linux.

use adw::prelude::*;
use cyber_pumpkin_application::PaneSession;
use cyber_pumpkin_backend::Backend;
use cyber_pumpkin_core::{BackendId, BackendPath, EntryKind, FileEntry};
use cyber_pumpkin_local::LocalBackend;
use cyber_pumpkin_transfer::{
    DestinationPolicy, Endpoint, TransferId, TransferJob, TransferSpec, execute_file_with_policy,
};
use gtk::Orientation;
use gtk::glib::{self, ControlFlow};
use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, TryRecvError};
use std::time::Duration;

static NEXT_TRANSFER_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone)]
struct PaneWidgets {
    path: gtk::Entry,
    list: gtk::ListBox,
    back: gtk::Button,
    forward: gtk::Button,
    up: gtk::Button,
}

#[derive(Clone)]
struct PaneHandle {
    root: gtk::Box,
    session: PaneSessionRef,
    entries: PaneEntriesRef,
    widgets: PaneWidgets,
}

#[derive(Clone)]
struct CopyBar {
    copy_right: gtk::Button,
    copy_left: gtk::Button,
    status: gtk::Label,
}

type PaneSessionRef = Rc<RefCell<PaneSession>>;
type PaneEntriesRef = Rc<RefCell<Vec<FileEntry>>>;

fn main() {
    let app = adw::Application::builder()
        .application_id("com.rothamplification.CyberPumpkin")
        .build();
    app.connect_activate(build_ui);
    app.run();
}

fn build_ui(app: &adw::Application) {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_owned());
    let left = build_pane("Local A", &home);
    let right = build_pane("Local B", &home);

    let header = adw::HeaderBar::new();
    header.set_title_widget(Some(&gtk::Label::new(Some("Cyber-Pumpkin"))));

    let copy_bar = create_copy_bar();
    connect_copy_bar(&copy_bar, &left, &right);

    let panes = gtk::Paned::new(Orientation::Horizontal);
    panes.set_hexpand(true);
    panes.set_vexpand(true);
    panes.set_position(600);
    panes.set_wide_handle(true);
    panes.set_start_child(Some(&left.root));
    panes.set_end_child(Some(&right.root));

    let root = gtk::Box::new(Orientation::Vertical, 0);
    root.append(&header);
    root.append(&copy_bar.root());
    root.append(&panes);

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("Cyber-Pumpkin")
        .default_width(1200)
        .default_height(760)
        .content(&root)
        .build();
    window.present();
}

impl CopyBar {
    fn root(&self) -> gtk::Box {
        let root = gtk::Box::new(Orientation::Horizontal, 8);
        root.set_margin_top(6);
        root.set_margin_bottom(6);
        root.set_margin_start(8);
        root.set_margin_end(8);
        root.append(&self.copy_left);
        root.append(&self.copy_right);
        root.append(&self.status);
        root
    }
}

fn create_copy_bar() -> CopyBar {
    let copy_left = gtk::Button::with_label("← Copy");
    let copy_right = gtk::Button::with_label("Copy →");
    let status = gtk::Label::new(Some("Ready"));
    status.set_xalign(0.0);
    status.set_hexpand(true);
    status.add_css_class("dim-label");
    CopyBar {
        copy_right,
        copy_left,
        status,
    }
}

fn connect_copy_bar(bar: &CopyBar, left: &PaneHandle, right: &PaneHandle) {
    connect_copy_button(&bar.copy_right, left, right, &bar.status);
    connect_copy_button(&bar.copy_left, right, left, &bar.status);
}

fn connect_copy_button(
    button: &gtk::Button,
    source: &PaneHandle,
    destination: &PaneHandle,
    status: &gtk::Label,
) {
    let source = source.clone();
    let destination = destination.clone();
    let status = status.clone();
    button.clone().connect_clicked(move |_| {
        start_copy(&source, &destination, &status);
    });
}

fn start_copy(source: &PaneHandle, destination: &PaneHandle, status: &gtk::Label) {
    let Some(entry) = selected_entry(source) else {
        status.set_text("Select a file to copy.");
        return;
    };
    if entry.kind != EntryKind::File {
        status.set_text("Directory copy lands in the recursive-transfer milestone.");
        return;
    }

    let destination_directory = destination.session.borrow().location().clone();
    let Ok(destination_path) = local_child_path(&destination_directory, &entry.name) else {
        status.set_text("Could not form destination path.");
        return;
    };
    let transfer_number = NEXT_TRANSFER_ID.fetch_add(1, Ordering::Relaxed);
    let Ok(transfer_id) = TransferId::new(transfer_number) else {
        status.set_text("Could not allocate transfer id.");
        return;
    };

    let source_backend_id = source.session.borrow().backend().clone();
    let destination_backend_id = destination.session.borrow().backend().clone();
    let source_path = entry.path;
    let file_name = entry.name;
    let (sender, receiver) = mpsc::channel();

    status.set_text(&format!("Copying {file_name}…"));
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
        let result = execute_file_with_policy(
            &mut job,
            &source_backend,
            &destination_backend,
            DestinationPolicy::FailIfExists,
        )
        .map(cyber_pumpkin_transfer::TransferReport::bytes_copied)
        .map_err(|error| error.to_string());
        let _sent = sender.send(result);
    });

    watch_copy_result(receiver, destination.clone(), status.clone(), file_name);
}

fn watch_copy_result(
    receiver: mpsc::Receiver<Result<u64, String>>,
    destination: PaneHandle,
    status: gtk::Label,
    file_name: String,
) {
    glib::timeout_add_local(Duration::from_millis(50), move || {
        match receiver.try_recv() {
            Ok(Ok(bytes)) => {
                status.set_text(&format!(
                    "Copied {file_name} — {}",
                    format_size(Some(bytes))
                ));
                refresh_current(
                    &destination.session,
                    &destination.entries,
                    &destination.widgets,
                );
                ControlFlow::Break
            }
            Ok(Err(error)) => {
                status.set_text(&format!("Copy failed: {error}"));
                ControlFlow::Break
            }
            Err(TryRecvError::Empty) => ControlFlow::Continue,
            Err(TryRecvError::Disconnected) => {
                status.set_text("Copy worker disconnected.");
                ControlFlow::Break
            }
        }
    });
}

fn selected_entry(pane: &PaneHandle) -> Option<FileEntry> {
    let row = pane.widgets.list.selected_row()?;
    let index = usize::try_from(row.index()).ok()?;
    pane.entries.borrow().get(index).cloned()
}

fn local_child_path(directory: &BackendPath, name: &str) -> Result<BackendPath, String> {
    let child = Path::new(directory.as_str()).join(name);
    let text = child
        .to_str()
        .ok_or_else(|| "local path is not valid UTF-8".to_owned())?;
    BackendPath::new(text).map_err(|error| error.to_string())
}

fn build_pane(title: &str, initial_path: &str) -> PaneHandle {
    let session = create_session(initial_path);
    let entries = Rc::new(RefCell::new(Vec::<FileEntry>::new()));
    let (root, widgets) = create_pane_widgets(title);

    refresh_current(&session, &entries, &widgets);
    connect_pane_navigation(&session, &entries, &widgets);

    PaneHandle {
        root,
        session,
        entries,
        widgets,
    }
}

fn create_session(initial_path: &str) -> PaneSessionRef {
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

fn create_pane_widgets(title: &str) -> (gtk::Box, PaneWidgets) {
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

    (
        pane,
        PaneWidgets {
            path,
            list,
            back,
            forward,
            up,
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

fn column_label(text: &str, xalign: f32) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    label.set_xalign(xalign);
    label.set_margin_start(8);
    label.set_margin_end(8);
    label
}

fn connect_pane_navigation(
    session: &PaneSessionRef,
    entries: &PaneEntriesRef,
    widgets: &PaneWidgets,
) {
    connect_row_activation(session, entries, widgets);
    connect_path_entry(session, entries, widgets);
    connect_back(session, entries, widgets);
    connect_forward(session, entries, widgets);
    connect_up(session, entries, widgets);
}

fn connect_row_activation(
    session: &PaneSessionRef,
    entries: &PaneEntriesRef,
    widgets: &PaneWidgets,
) {
    let session = Rc::clone(session);
    let entries = Rc::clone(entries);
    let widgets = widgets.clone();

    widgets.list.clone().connect_row_activated(move |_, row| {
        let Ok(index) = usize::try_from(row.index()) else {
            return;
        };
        let Some(entry) = entries.borrow().get(index).cloned() else {
            return;
        };
        if entry.kind == EntryKind::Directory {
            navigate_to(&session, &entries, &widgets, entry.path);
        }
    });
}

fn connect_path_entry(session: &PaneSessionRef, entries: &PaneEntriesRef, widgets: &PaneWidgets) {
    let session = Rc::clone(session);
    let entries = Rc::clone(entries);
    let widgets = widgets.clone();

    widgets.path.clone().connect_activate(move |entry| {
        let text = entry.text().to_string();
        match BackendPath::new(text) {
            Ok(target) => navigate_to(&session, &entries, &widgets, target),
            Err(error) => eprintln!("invalid path: {error}"),
        }
    });
}

fn connect_back(session: &PaneSessionRef, entries: &PaneEntriesRef, widgets: &PaneWidgets) {
    let session = Rc::clone(session);
    let entries = Rc::clone(entries);
    let widgets = widgets.clone();

    widgets.back.clone().connect_clicked(move |_| {
        let Some(target) = session.borrow().back_location().cloned() else {
            return;
        };
        let Ok(loaded) = load_directory(&target) else {
            return;
        };
        let _changed = session.borrow_mut().go_back();
        render_entries(&session, &entries, &widgets, loaded);
    });
}

fn connect_forward(session: &PaneSessionRef, entries: &PaneEntriesRef, widgets: &PaneWidgets) {
    let session = Rc::clone(session);
    let entries = Rc::clone(entries);
    let widgets = widgets.clone();

    widgets.forward.clone().connect_clicked(move |_| {
        let Some(target) = session.borrow().forward_location().cloned() else {
            return;
        };
        let Ok(loaded) = load_directory(&target) else {
            return;
        };
        let _changed = session.borrow_mut().go_forward();
        render_entries(&session, &entries, &widgets, loaded);
    });
}

fn connect_up(session: &PaneSessionRef, entries: &PaneEntriesRef, widgets: &PaneWidgets) {
    let session = Rc::clone(session);
    let entries = Rc::clone(entries);
    let widgets = widgets.clone();

    widgets.up.clone().connect_clicked(move |_| {
        let current = session.borrow().location().as_str().to_owned();
        let Some(parent) = Path::new(&current).parent() else {
            return;
        };
        let Some(parent) = parent.to_str() else {
            return;
        };
        let Ok(target) = BackendPath::new(parent) else {
            return;
        };
        navigate_to(&session, &entries, &widgets, target);
    });
}

fn navigate_to(
    session: &PaneSessionRef,
    entries: &PaneEntriesRef,
    widgets: &PaneWidgets,
    target: BackendPath,
) {
    match load_directory(&target) {
        Ok(loaded) => {
            session.borrow_mut().navigate_to(target);
            render_entries(session, entries, widgets, loaded);
        }
        Err(error) => eprintln!("navigation failed: {error}"),
    }
}

fn refresh_current(session: &PaneSessionRef, entries: &PaneEntriesRef, widgets: &PaneWidgets) {
    let current = session.borrow().location().clone();
    match load_directory(&current) {
        Ok(loaded) => render_entries(session, entries, widgets, loaded),
        Err(error) => eprintln!("directory load failed: {error}"),
    }
}

fn render_entries(
    session: &PaneSessionRef,
    entries: &PaneEntriesRef,
    widgets: &PaneWidgets,
    loaded: Vec<FileEntry>,
) {
    while let Some(child) = widgets.list.first_child() {
        widgets.list.remove(&child);
    }
    for entry in &loaded {
        let row = gtk::ListBoxRow::new();
        row.set_child(Some(&entry_grid(entry)));
        widgets.list.append(&row);
    }
    *entries.borrow_mut() = loaded;

    let session = session.borrow();
    widgets.path.set_text(session.location().as_str());
    widgets.back.set_sensitive(session.can_go_back());
    widgets.forward.set_sensitive(session.can_go_forward());
    widgets
        .up
        .set_sensitive(Path::new(session.location().as_str()).parent().is_some());
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

const fn entry_kind_text(kind: EntryKind) -> &'static str {
    match kind {
        EntryKind::File => "File",
        EntryKind::Directory => "Folder",
        EntryKind::Symlink => "Link",
        EntryKind::Other => "Other",
    }
}

fn format_size(size: Option<u64>) -> String {
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

fn load_directory(path: &BackendPath) -> Result<Vec<FileEntry>, String> {
    let id = BackendId::new("local-ui").map_err(|error| error.to_string())?;
    LocalBackend::new(id)
        .list(path)
        .map_err(|error| error.to_string())
}
