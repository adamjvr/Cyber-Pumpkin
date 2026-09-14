use crate::browser::{PaneHandle, PaneSide};
use adw::prelude::*;
use cyber_pumpkin_application::{ConnectionProfiles, SavedConnection};
use gtk::Orientation;
use std::cell::Cell;
use std::path::Path;
use std::rc::Rc;

pub(crate) struct PumpkinPatch {
    pub(crate) root: gtk::Box,
    profiles: gtk::ListBox,
    left: PaneHandle,
    right: PaneHandle,
    active: Rc<Cell<PaneSide>>,
}

impl PumpkinPatch {
    pub(crate) fn new(
        home: &str,
        left: &PaneHandle,
        right: &PaneHandle,
        active: &Rc<Cell<PaneSide>>,
    ) -> Rc<Self> {
        let root = gtk::Box::new(Orientation::Vertical, 6);
        root.set_margin_top(8);
        root.set_margin_bottom(8);
        root.set_margin_start(8);
        root.set_margin_end(8);
        root.set_width_request(200);

        let title = gtk::Label::new(Some("Pumpkin Patch"));
        title.set_xalign(0.0);
        title.add_css_class("heading");
        root.append(&title);

        append_local_locations(&root, home, left, right, active);
        root.append(&gtk::Separator::new(Orientation::Horizontal));

        let connections_header = gtk::Box::new(Orientation::Horizontal, 4);
        let label = gtk::Label::new(Some("Connections"));
        label.set_xalign(0.0);
        label.set_hexpand(true);
        label.add_css_class("heading");
        let add = gtk::Button::from_icon_name("list-add-symbolic");
        add.set_tooltip_text(Some("Add saved connection"));
        connections_header.append(&label);
        connections_header.append(&add);
        root.append(&connections_header);

        let profiles = gtk::ListBox::new();
        profiles.set_selection_mode(gtk::SelectionMode::None);
        root.append(&profiles);

        root.append(&gtk::Separator::new(Orientation::Horizontal));
        let history_title = gtk::Label::new(Some("History"));
        history_title.set_xalign(0.0);
        history_title.add_css_class("heading");
        root.append(&history_title);

        let history = gtk::Label::new(Some("Recent connections will appear here."));
        history.set_wrap(true);
        history.set_xalign(0.0);
        history.add_css_class("dim-label");
        root.append(&history);

        let patch = Rc::new(Self {
            root,
            profiles,
            left: left.clone(),
            right: right.clone(),
            active: Rc::clone(active),
        });

        {
            let patch = Rc::clone(&patch);
            add.connect_clicked(move |_| patch.show_add_dialog());
        }

        patch.refresh();
        patch
    }

    fn refresh(self: &Rc<Self>) {
        while let Some(row) = self.profiles.row_at_index(0) {
            self.profiles.remove(&row);
        }

        let profiles = match ConnectionProfiles::load_default() {
            Ok(profiles) => profiles,
            Err(error) => {
                self.append_message(&format!("Could not load connections: {error}"));
                return;
            }
        };

        if profiles.profiles.is_empty() {
            self.append_message("No saved connections yet.");
            return;
        }

        for profile in profiles.profiles {
            self.append_profile(profile);
        }
    }

    fn append_message(&self, text: &str) {
        let label = gtk::Label::new(Some(text));
        label.set_wrap(true);
        label.set_xalign(0.0);
        label.add_css_class("dim-label");
        let row = gtk::ListBoxRow::new();
        row.set_selectable(false);
        row.set_child(Some(&label));
        self.profiles.append(&row);
    }

    fn append_profile(self: &Rc<Self>, profile: SavedConnection) {
        let content = gtk::Box::new(Orientation::Horizontal, 4);
        let connect = gtk::Button::new();
        connect.set_hexpand(true);
        connect.set_halign(gtk::Align::Fill);

        let labels = gtk::Box::new(Orientation::Vertical, 0);
        let name = gtk::Label::new(Some(&profile.name));
        name.set_xalign(0.0);
        let endpoint = gtk::Label::new(Some(&format!(
            "{}@{}:{}",
            profile.username, profile.host, profile.port
        )));
        endpoint.set_xalign(0.0);
        endpoint.add_css_class("dim-label");
        labels.append(&name);
        labels.append(&endpoint);
        connect.set_child(Some(&labels));

        let remove = gtk::Button::from_icon_name("list-remove-symbolic");
        remove.set_tooltip_text(Some("Remove saved connection"));

        content.append(&connect);
        content.append(&remove);

        let row = gtk::ListBoxRow::new();
        row.set_selectable(false);
        row.set_child(Some(&content));
        self.profiles.append(&row);

        {
            let patch = Rc::clone(self);
            let profile = profile.clone();
            connect.connect_clicked(move |_| {
                patch.connect_profile(&profile);
            });
        }

        {
            let patch = Rc::clone(self);
            let id = profile.id;
            remove.connect_clicked(move |_| {
                let mut profiles = ConnectionProfiles::load_default().unwrap_or_default();
                if profiles.remove(&id) {
                    if let Err(error) = profiles.save_default() {
                        eprintln!("failed to save Pumpkin Patch connections: {error}");
                    }
                }
                patch.refresh();
            });
        }
    }

    fn connect_profile(&self, profile: &SavedConnection) {
        let pane = active_pane(&self.left, &self.right, self.active.get());
        if let Err(error) = pane.connect_sftp(
            &profile.host,
            &profile.username,
            profile.port,
            &profile.initial_path,
        ) {
            show_error("Saved connection failed", &error);
        }
    }

    fn show_add_dialog(self: &Rc<Self>) {
        let dialog = gtk::Dialog::builder()
            .title("Add Pumpkin Patch Connection")
            .modal(true)
            .build();
        dialog.add_button("Cancel", gtk::ResponseType::Cancel);
        dialog.add_button("Save & Connect", gtk::ResponseType::Accept);

        let fields = gtk::Box::new(Orientation::Vertical, 6);
        fields.set_margin_top(12);
        fields.set_margin_bottom(12);
        fields.set_margin_start(12);
        fields.set_margin_end(12);

        let name = entry("Display name", "");
        let host = entry("Host", "");
        let username = entry("Username", "");
        let port = entry("Port", "22");
        let path = entry("Remote path", "/");

        for field in [&name, &host, &username, &port, &path] {
            fields.append(field);
        }
        dialog.content_area().append(&fields);

        let patch_controller = Rc::clone(self);
        dialog.connect_response(move |dialog, response| {
            if response == gtk::ResponseType::Accept {
                let port_number = port.text().parse::<u16>().unwrap_or(22);
                let id = profile_id(
                    name.text().as_str(),
                    host.text().as_str(),
                    username.text().as_str(),
                );

                match SavedConnection::new(
                    id,
                    name.text().as_str(),
                    host.text().as_str(),
                    username.text().as_str(),
                    port_number,
                    path.text().as_str(),
                ) {
                    Ok(profile) => {
                        let mut profiles = ConnectionProfiles::load_default().unwrap_or_default();
                        profiles.upsert(profile.clone());
                        match profiles.save_default() {
                            Ok(()) => {
                                patch_controller.refresh();
                                patch_controller.connect_profile(&profile);
                            }
                            Err(error) => {
                                show_error("Could not save connection", &error.to_string());
                            }
                        }
                    }
                    Err(error) => show_error("Invalid connection", &error),
                }
            }
            dialog.close();
        });
        dialog.present();
    }
}

fn append_local_locations(
    root: &gtk::Box,
    home: &str,
    left: &PaneHandle,
    right: &PaneHandle,
    active: &Rc<Cell<PaneSide>>,
) {
    let local_title = gtk::Label::new(Some("Local Locations"));
    local_title.set_xalign(0.0);
    local_title.add_css_class("heading");
    root.append(&local_title);

    let home_path = Path::new(home);
    let locations = [
        ("Home", home_path.to_path_buf()),
        ("Desktop", home_path.join("Desktop")),
        ("Documents", home_path.join("Documents")),
        ("Downloads", home_path.join("Downloads")),
        ("Root", Path::new("/").to_path_buf()),
    ];

    for (label, path) in locations {
        let button = gtk::Button::with_label(label);
        button.set_halign(gtk::Align::Fill);
        let left = left.clone();
        let right = right.clone();
        let active = Rc::clone(active);
        let text = path.to_string_lossy().into_owned();
        button.connect_clicked(move |_| {
            active_pane(&left, &right, active.get()).navigate_text(&text);
        });
        root.append(&button);
    }
}

fn entry(placeholder: &str, initial: &str) -> gtk::Entry {
    let entry = gtk::Entry::new();
    entry.set_placeholder_text(Some(placeholder));
    entry.set_text(initial);
    entry
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

fn active_pane<'a>(left: &'a PaneHandle, right: &'a PaneHandle, side: PaneSide) -> &'a PaneHandle {
    match side {
        PaneSide::Left => left,
        PaneSide::Right => right,
    }
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
