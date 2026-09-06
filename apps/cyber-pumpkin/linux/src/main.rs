//! Native GTK4/libadwaita frontend for Cyber-Pumpkin on Linux.
use adw::prelude::*;
use cyber_pumpkin_backend::Backend;
use cyber_pumpkin_core::{BackendId, BackendPath, EntryKind, FileEntry};
use cyber_pumpkin_local::LocalBackend;
use gtk::Orientation;

fn main() {
    let app = adw::Application::builder()
        .application_id("com.rothamplification.CyberPumpkin")
        .build();
    app.connect_activate(build_ui);
    app.run();
}

fn build_ui(app: &adw::Application) {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_owned());
    let entries = load_directory(&home).unwrap_or_default();

    let header = adw::HeaderBar::new();
    header.set_title_widget(Some(&gtk::Label::new(Some("Cyber-Pumpkin"))));

    let panes = gtk::Paned::new(Orientation::Horizontal);
    panes.set_hexpand(true);
    panes.set_vexpand(true);
    panes.set_position(600);
    panes.set_wide_handle(true);
    panes.set_start_child(Some(&build_pane("Local A", &home, &entries)));
    panes.set_end_child(Some(&build_pane("Local B", &home, &entries)));

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

fn load_directory(path: &str) -> Result<Vec<FileEntry>, String> {
    let id = BackendId::new("local-ui").map_err(|error| error.to_string())?;
    let path = BackendPath::new(path).map_err(|error| error.to_string())?;
    LocalBackend::new(id)
        .list(&path)
        .map_err(|error| error.to_string())
}

fn build_pane(title: &str, path: &str, entries: &[FileEntry]) -> gtk::Box {
    let pane = gtk::Box::new(Orientation::Vertical, 6);
    pane.set_margin_top(8);
    pane.set_margin_bottom(8);
    pane.set_margin_start(8);
    pane.set_margin_end(8);

    let heading = gtk::Label::new(Some(title));
    heading.set_xalign(0.0);
    heading.add_css_class("heading");
    pane.append(&heading);

    let location = gtk::Label::new(Some(path));
    location.set_xalign(0.0);
    location.add_css_class("dim-label");
    pane.append(&location);

    let list = gtk::ListBox::new();
    list.set_selection_mode(gtk::SelectionMode::Single);

    for entry in entries {
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
        list.append(&row);
    }

    let scroll = gtk::ScrolledWindow::builder()
        .hexpand(true)
        .vexpand(true)
        .child(&list)
        .build();
    pane.append(&scroll);
    pane
}
