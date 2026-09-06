//! Native GTK4/libadwaita frontend for Cyber-Pumpkin on Linux.

use adw::prelude::*;
use cyber_pumpkin_application::PaneSession;
use cyber_pumpkin_backend::Backend;
use cyber_pumpkin_core::{BackendId, BackendPath, EntryKind, FileEntry};
use cyber_pumpkin_local::LocalBackend;
use gtk::Orientation;
use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

#[derive(Clone)]
struct PaneWidgets {
    path: gtk::Entry,
    list: gtk::ListBox,
    back: gtk::Button,
    forward: gtk::Button,
    up: gtk::Button,
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

    let header = adw::HeaderBar::new();
    header.set_title_widget(Some(&gtk::Label::new(Some("Cyber-Pumpkin"))));

    let panes = gtk::Paned::new(Orientation::Horizontal);
    panes.set_hexpand(true);
    panes.set_vexpand(true);
    panes.set_position(600);
    panes.set_wide_handle(true);
    panes.set_start_child(Some(&build_pane("Local A", &home)));
    panes.set_end_child(Some(&build_pane("Local B", &home)));

    let root = gtk::Box::new(Orientation::Vertical, 0);
    root.append(&header);
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

fn build_pane(title: &str, initial_path: &str) -> gtk::Box {
    let session = create_session(initial_path);
    let entries = Rc::new(RefCell::new(Vec::<FileEntry>::new()));
    let (pane, widgets) = create_pane_widgets(title);

    refresh_current(&session, &entries, &widgets);
    connect_pane_navigation(&session, &entries, &widgets);

    let scroll = gtk::ScrolledWindow::builder()
        .hexpand(true)
        .vexpand(true)
        .child(&widgets.list)
        .build();
    pane.append(&scroll);
    pane
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
    pane.append(&nav);

    let list = gtk::ListBox::new();
    list.set_selection_mode(gtk::SelectionMode::Single);

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
        Err(error) => eprintln!("initial directory load failed: {error}"),
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
        let display = match entry.kind {
            EntryKind::Directory => format!("{}/", entry.name),
            EntryKind::Symlink => format!("{} →", entry.name),
            EntryKind::File | EntryKind::Other => entry.name.clone(),
        };

        let row = gtk::ListBoxRow::new();
        let label = gtk::Label::new(Some(&display));
        label.set_xalign(0.0);
        label.set_margin_top(5);
        label.set_margin_bottom(5);
        label.set_margin_start(8);
        label.set_margin_end(8);
        row.set_child(Some(&label));
        widgets.list.append(&row);
    }

    *entries.borrow_mut() = loaded;

    let session = session.borrow();
    widgets.path.set_text(session.location().as_str());
    widgets.back.set_sensitive(session.can_go_back());
    widgets.forward.set_sensitive(session.can_go_forward());

    let current = session.location().as_str();
    widgets
        .up
        .set_sensitive(Path::new(current).parent().is_some());
}

fn load_directory(path: &BackendPath) -> Result<Vec<FileEntry>, String> {
    let id = BackendId::new("local-ui").map_err(|error| error.to_string())?;
    LocalBackend::new(id)
        .list(path)
        .map_err(|error| error.to_string())
}
