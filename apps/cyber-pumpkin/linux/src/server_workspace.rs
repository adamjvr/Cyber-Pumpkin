use crate::browser::PaneHandle;
use cyber_pumpkin_application::{ConnectionProfiles, SavedConnection};
use gtk::Orientation;
use gtk::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Clone)]
pub(crate) struct PumpkinPatchPanel {
    pub(crate) root: gtk::Box,
    list: gtk::ListBox,
    pane: PaneHandle,
    query: Rc<RefCell<String>>,
    on_browser: Rc<dyn Fn()>,
}

impl PumpkinPatchPanel {
    pub(crate) fn new(
        pane: &PaneHandle,
        on_browser: Rc<dyn Fn()>,
        on_quick_connect: &Rc<dyn Fn()>,
    ) -> Rc<Self> {
        let root = content_root();
        root.append(&page_title("Pumpkin Patch"));

        let subtitle = gtk::Label::new(Some(
            "Saved connections for this pane. Activate a connection to open it here.",
        ));
        subtitle.set_wrap(true);
        subtitle.set_xalign(0.0);
        subtitle.add_css_class("dim-label");
        root.append(&subtitle);

        let list = gtk::ListBox::new();
        list.set_selection_mode(gtk::SelectionMode::None);
        list.add_css_class("boxed-list");
        list.set_vexpand(true);
        root.append(&list);

        root.append(&section_row(
            "Shared Connections",
            "Shared connection discovery is not configured yet",
        ));
        root.append(&section_row(
            "History",
            "Connection history persistence lands next",
        ));

        let controls = gtk::Box::new(Orientation::Horizontal, 4);
        let add = gtk::Button::from_icon_name("list-add-symbolic");
        add.set_tooltip_text(Some("Quick Connect / Add to Pumpkin Patch"));
        let group = gtk::Button::from_icon_name("folder-new-symbolic");
        group.set_tooltip_text(Some("New connection group"));
        group.set_sensitive(false);
        let edit = gtk::Button::from_icon_name("edit-symbolic");
        edit.set_tooltip_text(Some("Edit selected connection"));
        edit.set_sensitive(false);
        controls.append(&add);
        controls.append(&group);
        controls.append(&edit);
        root.append(&controls);

        let panel = Rc::new(Self {
            root,
            list,
            pane: pane.clone(),
            query: Rc::new(RefCell::new(String::new())),
            on_browser,
        });

        {
            let on_quick_connect = Rc::clone(on_quick_connect);
            add.connect_clicked(move |_| on_quick_connect());
        }

        panel.refresh();
        panel
    }

    pub(crate) fn refresh(&self) {
        while let Some(row) = self.list.row_at_index(0) {
            self.list.remove(&row);
        }

        let profiles = match ConnectionProfiles::load_default() {
            Ok(profiles) => profiles,
            Err(error) => {
                self.list.append(&message_row(&format!(
                    "Could not load Pumpkin Patch: {error}"
                )));
                return;
            }
        };

        let query = self.query.borrow().clone();
        let mut visible = profiles.profiles;
        if !query.is_empty() {
            visible.retain(|profile| profile_matches(profile, &query));
        }

        if visible.is_empty() {
            let message = if query.is_empty() {
                "No saved connections yet."
            } else {
                "No saved connections match the current search."
            };
            self.list.append(&message_row(message));
            return;
        }

        for profile in visible {
            self.list.append(&self.server_row(profile));
        }
    }

    pub(crate) fn set_filter_query(&self, query: &str) {
        *self.query.borrow_mut() = query.trim().to_lowercase();
        self.refresh();
    }

    fn server_row(&self, profile: SavedConnection) -> gtk::ListBoxRow {
        let row = gtk::ListBoxRow::new();
        row.set_selectable(false);

        let button = gtk::Button::new();
        button.add_css_class("flat");
        button.set_halign(gtk::Align::Fill);

        let content = gtk::Box::new(Orientation::Horizontal, 10);
        content.set_margin_top(7);
        content.set_margin_bottom(7);
        content.set_margin_start(8);
        content.set_margin_end(8);

        let icon = gtk::Image::from_icon_name("network-server-symbolic");
        icon.set_pixel_size(22);
        let name = gtk::Label::new(Some(&profile.name));
        name.set_xalign(0.0);
        name.set_hexpand(true);
        name.add_css_class("heading");
        let address = gtk::Label::new(Some(&profile.host));
        address.set_xalign(1.0);
        address.add_css_class("dim-label");

        content.append(&icon);
        content.append(&name);
        content.append(&address);
        button.set_child(Some(&content));
        row.set_child(Some(&button));

        let pane = self.pane.clone();
        let on_browser = Rc::clone(&self.on_browser);
        button.connect_clicked(move |_| {
            match pane.connect_sftp(
                &profile.host,
                &profile.username,
                profile.port,
                &profile.initial_path,
            ) {
                Ok(()) => on_browser(),
                Err(error) => show_error("Pumpkin Patch connection failed", &error),
            }
        });

        row
    }
}

#[allow(clippy::similar_names, clippy::too_many_lines)]
pub(crate) fn build_quick_connect_panel(
    pane: &PaneHandle,
    pumpkin_patch: &Rc<PumpkinPatchPanel>,
    on_browser: Rc<dyn Fn()>,
) -> gtk::Box {
    let root = content_root();
    root.append(&page_title("Quick Connect"));

    let subtitle = gtk::Label::new(Some(
        "Connect this pane directly. SFTP uses your SSH agent and strict known_hosts verification.",
    ));
    subtitle.set_wrap(true);
    subtitle.set_xalign(0.0);
    subtitle.add_css_class("dim-label");
    root.append(&subtitle);

    let form = gtk::Grid::builder()
        .column_spacing(12)
        .row_spacing(9)
        .build();

    let protocol = gtk::ComboBoxText::new();
    protocol.append_text("SFTP");
    protocol.set_active(Some(0));
    protocol.set_sensitive(false);
    form_control(&form, 0, "Protocol", &protocol);

    let display_name = form_entry(&form, 1, "Name", "Optional Pumpkin Patch name", "");
    let host = form_entry(&form, 2, "Server", "hostname or address", "");
    let port = form_entry(&form, 3, "Port", "22", "22");
    let username = form_entry(&form, 4, "User", "username", "");
    let path = form_entry(&form, 5, "Remote Path", "/", "/");

    let authentication = gtk::Label::new(Some("SSH Agent / OpenSSH keys"));
    authentication.set_xalign(0.0);
    authentication.add_css_class("dim-label");
    form_control(&form, 6, "Authentication", &authentication);

    root.append(&form);

    let save = gtk::CheckButton::with_label("Add to Pumpkin Patch");
    save.set_halign(gtk::Align::End);
    root.append(&save);

    let actions = gtk::Box::new(Orientation::Horizontal, 8);
    actions.set_halign(gtk::Align::End);
    let cancel = gtk::Button::with_label("Cancel");
    let connect = gtk::Button::with_label("Connect");
    connect.add_css_class("suggested-action");
    actions.append(&cancel);
    actions.append(&connect);
    root.append(&actions);

    {
        let on_browser = Rc::clone(&on_browser);
        cancel.connect_clicked(move |_| on_browser());
    }

    {
        let pane = pane.clone();
        let pumpkin_patch = Rc::clone(pumpkin_patch);
        connect.connect_clicked(move |_| {
            let port_number = port.text().parse::<u16>().unwrap_or(22);
            let host_text = host.text().to_string();
            let username_text = username.text().to_string();
            let path_text = path.text().to_string();

            match pane.connect_sftp(&host_text, &username_text, port_number, &path_text) {
                Ok(()) => {
                    if save.is_active() {
                        let name = if display_name.text().trim().is_empty() {
                            host_text.clone()
                        } else {
                            display_name.text().to_string()
                        };
                        let profile = SavedConnection::new(
                            profile_id(&name, &host_text, &username_text),
                            name,
                            host_text,
                            username_text,
                            port_number,
                            path_text,
                        );
                        match profile {
                            Ok(profile) => {
                                let mut profiles =
                                    ConnectionProfiles::load_default().unwrap_or_default();
                                profiles.upsert(profile);
                                if let Err(error) = profiles.save_default() {
                                    show_error(
                                        "Could not save Pumpkin Patch connection",
                                        &error.to_string(),
                                    );
                                } else {
                                    pumpkin_patch.refresh();
                                }
                            }
                            Err(error) => show_error("Invalid saved connection", &error),
                        }
                    }
                    on_browser();
                }
                Err(error) => show_error("Quick Connect failed", &error),
            }
        });
    }

    root
}

fn content_root() -> gtk::Box {
    let root = gtk::Box::new(Orientation::Vertical, 12);
    root.set_margin_top(14);
    root.set_margin_bottom(14);
    root.set_margin_start(14);
    root.set_margin_end(14);
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
    row.set_margin_top(5);
    row.set_margin_bottom(5);
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

fn message_row(text: &str) -> gtk::ListBoxRow {
    let row = gtk::ListBoxRow::new();
    row.set_selectable(false);
    let label = gtk::Label::new(Some(text));
    label.set_xalign(0.0);
    label.set_margin_top(8);
    label.set_margin_bottom(8);
    label.set_margin_start(8);
    label.set_margin_end(8);
    label.add_css_class("dim-label");
    row.set_child(Some(&label));
    row
}

fn form_entry(
    grid: &gtk::Grid,
    row: i32,
    label: &str,
    placeholder: &str,
    initial: &str,
) -> gtk::Entry {
    let entry = gtk::Entry::new();
    entry.set_hexpand(true);
    entry.set_placeholder_text(Some(placeholder));
    entry.set_text(initial);
    form_control(grid, row, label, &entry);
    entry
}

fn form_control<W: IsA<gtk::Widget>>(grid: &gtk::Grid, row: i32, label: &str, control: &W) {
    let title = gtk::Label::new(Some(label));
    title.set_xalign(1.0);
    grid.attach(&title, 0, row, 1, 1);
    grid.attach(control, 1, row, 1, 1);
}

fn profile_matches(profile: &SavedConnection, query: &str) -> bool {
    profile.name.to_lowercase().contains(query)
        || profile.host.to_lowercase().contains(query)
        || profile.username.to_lowercase().contains(query)
}

fn profile_id(name: &str, host: &str, username: &str) -> String {
    format!("{name}-{username}-{host}")
        .to_lowercase()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '-' {
                character
            } else {
                '-'
            }
        })
        .collect()
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
