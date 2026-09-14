use crate::browser::PaneHandle;
use cyber_pumpkin_application::{ConnectionProfiles, SavedConnection};
use gtk::Orientation;
use gtk::prelude::*;

pub(crate) fn build_servers_panel(right: &PaneHandle, stack: &gtk::Stack) -> gtk::Box {
    let root = content_root();
    root.append(&page_title("Servers"));

    let profiles = ConnectionProfiles::load_default().unwrap_or_default();
    if profiles.profiles.is_empty() {
        let empty = gtk::Label::new(Some("No saved servers yet."));
        empty.set_xalign(0.0);
        empty.add_css_class("dim-label");
        root.append(&empty);
    } else {
        let list = gtk::ListBox::new();
        list.set_selection_mode(gtk::SelectionMode::None);
        list.add_css_class("boxed-list");
        for profile in profiles.profiles {
            list.append(&server_row(profile, right, stack));
        }
        root.append(&list);
    }

    root.append(&section_row("Shared Servers", "Not configured"));
    root.append(&section_row(
        "History",
        "Recent connection history coming next",
    ));
    root
}

pub(crate) fn build_quick_connect_panel(right: &PaneHandle, stack: &gtk::Stack) -> gtk::Box {
    let root = content_root();
    root.append(&page_title("Quick Connect"));

    let subtitle = gtk::Label::new(Some(
        "Connect the right pane directly. SSH-agent authentication and strict known_hosts verification remain enforced.",
    ));
    subtitle.set_wrap(true);
    subtitle.set_xalign(0.0);
    subtitle.add_css_class("dim-label");
    root.append(&subtitle);

    let form = gtk::Grid::builder()
        .column_spacing(12)
        .row_spacing(10)
        .build();
    let host = form_entry(&form, 0, "Server", "hostname or address", "");
    let username = form_entry(&form, 1, "User", "username", "");
    let port = form_entry(&form, 2, "Port", "22", "22");
    let path = form_entry(&form, 3, "Path", "/", "/");
    root.append(&form);

    let actions = gtk::Box::new(Orientation::Horizontal, 8);
    actions.set_halign(gtk::Align::End);
    let cancel = gtk::Button::with_label("Cancel");
    let connect = gtk::Button::with_label("Connect");
    connect.add_css_class("suggested-action");
    actions.append(&cancel);
    actions.append(&connect);
    root.append(&actions);

    {
        let stack = stack.clone();
        cancel.connect_clicked(move |_| stack.set_visible_child_name("browser"));
    }
    {
        let right = right.clone();
        let stack = stack.clone();
        connect.connect_clicked(move |_| {
            let port = port.text().parse::<u16>().unwrap_or(22);
            match right.connect_sftp(
                host.text().as_str(),
                username.text().as_str(),
                port,
                path.text().as_str(),
            ) {
                Ok(()) => stack.set_visible_child_name("browser"),
                Err(error) => show_error("Quick Connect failed", &error),
            }
        });
    }

    root
}

fn server_row(profile: SavedConnection, right: &PaneHandle, stack: &gtk::Stack) -> gtk::ListBoxRow {
    let row = gtk::ListBoxRow::new();
    row.set_activatable(false);
    row.set_selectable(false);

    let content = gtk::Box::new(Orientation::Horizontal, 10);
    content.set_margin_top(8);
    content.set_margin_bottom(8);
    content.set_margin_start(10);
    content.set_margin_end(10);

    let icon = gtk::Image::from_icon_name("network-server-symbolic");
    icon.set_pixel_size(28);
    let labels = gtk::Box::new(Orientation::Vertical, 1);
    labels.set_hexpand(true);
    let name = gtk::Label::new(Some(&profile.name));
    name.set_xalign(0.0);
    name.add_css_class("heading");
    let endpoint = gtk::Label::new(Some(&format!(
        "{}@{}:{}  {}",
        profile.username, profile.host, profile.port, profile.initial_path
    )));
    endpoint.set_xalign(0.0);
    endpoint.add_css_class("dim-label");
    labels.append(&name);
    labels.append(&endpoint);

    let open = gtk::Button::with_label("Open");
    {
        let right = right.clone();
        let stack = stack.clone();
        open.connect_clicked(move |_| {
            match right.connect_sftp(
                &profile.host,
                &profile.username,
                profile.port,
                &profile.initial_path,
            ) {
                Ok(()) => stack.set_visible_child_name("browser"),
                Err(error) => show_error("Saved server failed", &error),
            }
        });
    }

    content.append(&icon);
    content.append(&labels);
    content.append(&open);
    row.set_child(Some(&content));
    row
}

fn content_root() -> gtk::Box {
    let root = gtk::Box::new(Orientation::Vertical, 14);
    root.set_margin_top(18);
    root.set_margin_bottom(18);
    root.set_margin_start(18);
    root.set_margin_end(18);
    root
}

fn page_title(text: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    label.set_xalign(0.0);
    label.add_css_class("title-2");
    label
}

fn section_row(title: &str, detail: &str) -> gtk::Box {
    let row = gtk::Box::new(Orientation::Horizontal, 10);
    row.set_margin_top(8);
    let icon = gtk::Image::from_icon_name("folder-remote-symbolic");
    let label = gtk::Label::new(Some(title));
    label.set_xalign(0.0);
    label.set_hexpand(true);
    label.add_css_class("heading");
    let detail = gtk::Label::new(Some(detail));
    detail.add_css_class("dim-label");
    row.append(&icon);
    row.append(&label);
    row.append(&detail);
    row
}

fn form_entry(
    grid: &gtk::Grid,
    row: i32,
    label: &str,
    placeholder: &str,
    initial: &str,
) -> gtk::Entry {
    let title = gtk::Label::new(Some(label));
    title.set_xalign(1.0);
    let entry = gtk::Entry::new();
    entry.set_hexpand(true);
    entry.set_placeholder_text(Some(placeholder));
    entry.set_text(initial);
    grid.attach(&title, 0, row, 1, 1);
    grid.attach(&entry, 1, row, 1, 1);
    entry
}

fn show_error(title: &str, message: &str) {
    let dialog = gtk::MessageDialog::builder()
        .modal(true)
        .message_type(gtk::MessageType::Error)
        .buttons(gtk::ButtonsType::Close)
        .text(title)
        .secondary_text(message)
        .build();
    dialog.connect_response(|dialog, _| dialog.close());
    dialog.present();
}
