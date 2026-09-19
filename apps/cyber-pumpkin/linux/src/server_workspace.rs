use crate::browser::PaneHandle;
use crate::connection::PaneSftpAuth;
use cyber_pumpkin_application::{
    ConnectionProfiles, SavedAuthentication, SavedConnection, TrustedHosts,
};
use cyber_pumpkin_secrets::{PlatformSecretStore, SecretStore};
use cyber_pumpkin_sftp::{HostKeyStatus, SftpConfig, probe_host_key};
use gtk::Orientation;
use gtk::glib::{self, ControlFlow};
use gtk::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc::{self, TryRecvError};
use std::time::Duration;

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
            "Saved pane-local SFTP connections. Secrets stay in the OS credential store.",
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

        let controls = gtk::Box::new(Orientation::Horizontal, 4);
        let add = gtk::Button::from_icon_name("list-add-symbolic");
        add.set_tooltip_text(Some("Quick Connect / Add to Pumpkin Patch"));
        controls.append(&add);
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

        let open = gtk::Button::new();
        open.add_css_class("flat");
        open.set_hexpand(true);
        open.set_halign(gtk::Align::Fill);

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
        let address = gtk::Label::new(Some(&format!(
            "{}@{}:{}",
            profile.username, profile.host, profile.port
        )));
        address.set_xalign(1.0);
        address.add_css_class("dim-label");
        content.append(&icon);
        content.append(&name);
        content.append(&address);
        open.set_child(Some(&content));

        let remove = gtk::Button::from_icon_name("user-trash-symbolic");
        remove.add_css_class("flat");
        remove.set_tooltip_text(Some("Remove from Pumpkin Patch"));

        let actions = gtk::Box::new(Orientation::Horizontal, 4);
        actions.append(&open);
        actions.append(&remove);
        row.set_child(Some(&actions));

        let pane = self.pane.clone();
        let on_browser = Rc::clone(&self.on_browser);
        let open_profile = profile.clone();
        open.connect_clicked(move |_| match auth_from_profile(&open_profile) {
            Ok(auth) => connect_with_trust(
                &pane,
                &open_profile.host,
                &open_profile.username,
                open_profile.port,
                &open_profile.initial_path,
                auth,
                Rc::clone(&on_browser),
            ),
            Err(error) => show_error("Pumpkin Patch authentication failed", &error),
        });

        let list = self.list.clone();
        let row_for_remove = row.clone();
        remove.connect_clicked(move |_| match remove_profile(&profile) {
            Ok(()) => list.remove(&row_for_remove),
            Err(error) => show_error("Could not remove Pumpkin Patch connection", &error),
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
        "SFTP with strict host verification, SSH agent, password, or private key authentication.",
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

    let authentication = gtk::ComboBoxText::new();
    authentication.append_text("SSH Agent");
    authentication.append_text("Password");
    authentication.append_text("Private Key");
    authentication.set_active(Some(0));
    form_control(&form, 6, "Authentication", &authentication);

    let password = password_entry(&form, 7, "Password", "password");
    let private_key = form_entry(&form, 8, "Private Key", "~/.ssh/id_ed25519", "");
    let passphrase = password_entry(&form, 9, "Key Passphrase", "optional passphrase");
    password.set_sensitive(false);
    private_key.set_sensitive(false);
    passphrase.set_sensitive(false);

    {
        let password = password.clone();
        let private_key = private_key.clone();
        let passphrase = passphrase.clone();
        authentication.connect_changed(move |combo| {
            let active = combo.active().unwrap_or(0);
            password.set_sensitive(active == 1);
            private_key.set_sensitive(active == 2);
            passphrase.set_sensitive(active == 2);
        });
    }

    root.append(&form);

    let save = gtk::CheckButton::with_label("Add to Pumpkin Patch");
    save.set_halign(gtk::Align::End);
    root.append(&save);

    let save_secret =
        gtk::CheckButton::with_label("Store password/passphrase in OS credential store");
    save_secret.set_halign(gtk::Align::End);
    save_secret.set_active(true);
    root.append(&save_secret);

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
            let host_text = host.text().trim().to_owned();
            let username_text = username.text().trim().to_owned();
            let path_text = path.text().to_string();
            let name = if display_name.text().trim().is_empty() {
                host_text.clone()
            } else {
                display_name.text().to_string()
            };

            let auth = match authentication.active().unwrap_or(0) {
                1 if password.text().is_empty() => {
                    show_error("Password required", "Enter an SFTP password.");
                    return;
                }
                1 => PaneSftpAuth::Password(password.text().to_string()),
                2 if private_key.text().trim().is_empty() => {
                    show_error(
                        "Private key required",
                        "Choose or enter a private-key path.",
                    );
                    return;
                }
                2 => PaneSftpAuth::PrivateKey {
                    private_key: expand_home(private_key.text().as_str()),
                    passphrase: (!passphrase.text().is_empty())
                        .then(|| passphrase.text().to_string()),
                },
                _ => PaneSftpAuth::Agent,
            };

            let save_requested = save.is_active();
            let save_secret_requested = save_secret.is_active();
            let pane_for_success = pane.clone();
            let pumpkin_patch = Rc::clone(&pumpkin_patch);
            let on_browser = Rc::clone(&on_browser);
            let auth_for_save = auth.clone();
            let profile_id = profile_id(&name, &host_text, &username_text);
            let host_for_save = host_text.clone();
            let username_for_save = username_text.clone();
            let path_for_save = path_text.clone();
            let name_for_save = name.clone();

            let success: Rc<dyn Fn()> = Rc::new(move || {
                if save_requested {
                    match save_profile(
                        &profile_id,
                        &name_for_save,
                        &host_for_save,
                        &username_for_save,
                        port_number,
                        &path_for_save,
                        &auth_for_save,
                        save_secret_requested,
                    ) {
                        Ok(()) => pumpkin_patch.refresh(),
                        Err(error) => show_error("Could not save Pumpkin Patch connection", &error),
                    }
                }
                let _ = &pane_for_success;
                on_browser();
            });

            connect_with_trust(
                &pane,
                &host_text,
                &username_text,
                port_number,
                &path_text,
                auth,
                success,
            );
        });
    }

    root
}

fn auth_from_profile(profile: &SavedConnection) -> Result<PaneSftpAuth, String> {
    match profile.authentication {
        SavedAuthentication::Agent => Ok(PaneSftpAuth::Agent),
        SavedAuthentication::Password => {
            let key = profile
                .secret_key
                .as_deref()
                .ok_or_else(|| "saved password reference is missing".to_owned())?;
            let secret = PlatformSecretStore
                .get(key)
                .map_err(|error| error.to_string())?
                .ok_or_else(|| format!("password is missing from OS credential store: {key}"))?;
            Ok(PaneSftpAuth::Password(secret))
        }
        SavedAuthentication::PrivateKey => {
            let private_key = profile
                .private_key
                .clone()
                .ok_or_else(|| "saved private-key path is missing".to_owned())?;
            let passphrase = match profile.secret_key.as_deref() {
                Some(key) => PlatformSecretStore
                    .get(key)
                    .map_err(|error| error.to_string())?,
                None => None,
            };
            Ok(PaneSftpAuth::PrivateKey {
                private_key,
                passphrase,
            })
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn connect_with_trust(
    pane: &PaneHandle,
    host: &str,
    username: &str,
    port: u16,
    path: &str,
    auth: PaneSftpAuth,
    on_success: Rc<dyn Fn()>,
) {
    let config = match SftpConfig::new(pane.backend_id(), host, username) {
        Ok(config) => config.with_port(port),
        Err(error) => {
            show_error("Invalid SFTP connection", &error.to_string());
            return;
        }
    };

    let (sender, receiver) = mpsc::channel();
    let _probe = std::thread::spawn(move || {
        let result = probe_host_key(&config).map_err(|error| error.to_string());
        let _sent = sender.send(result);
    });

    let pane = pane.clone();
    let host = host.to_owned();
    let username = username.to_owned();
    let path = path.to_owned();
    glib::timeout_add_local(Duration::from_millis(25), move || {
        match receiver.try_recv() {
            Ok(Ok(probe)) => {
                match probe.status() {
                    HostKeyStatus::Match => connect_now(
                        &pane,
                        &host,
                        &username,
                        port,
                        &path,
                        auth.clone(),
                        None,
                        Rc::clone(&on_success),
                    ),
                    HostKeyStatus::Mismatch => show_error(
                        "SSH host key changed",
                        &format!(
                            "The server key does not match OpenSSH known_hosts.\n\nPresented SHA-256 fingerprint:\n{}",
                            probe.fingerprint()
                        ),
                    ),
                    HostKeyStatus::Failure => show_error(
                        "SSH host verification failed",
                        &format!("Fingerprint: {}", probe.fingerprint()),
                    ),
                    HostKeyStatus::Unknown => {
                        let fingerprint = probe.fingerprint().to_owned();
                        let trusted = TrustedHosts::load_default()
                            .ok()
                            .and_then(|hosts| hosts.fingerprint(&host, port).map(str::to_owned));
                        if trusted.as_deref() == Some(fingerprint.as_str()) {
                            connect_now(
                                &pane,
                                &host,
                                &username,
                                port,
                                &path,
                                auth.clone(),
                                Some(fingerprint),
                                Rc::clone(&on_success),
                            );
                        } else {
                            show_unknown_host_dialog(
                                &pane,
                                &host,
                                &username,
                                port,
                                &path,
                                auth.clone(),
                                fingerprint,
                                Rc::clone(&on_success),
                            );
                        }
                    }
                }
                ControlFlow::Break
            }
            Ok(Err(error)) => {
                show_error("SSH host-key probe failed", &error);
                ControlFlow::Break
            }
            Err(TryRecvError::Empty) => ControlFlow::Continue,
            Err(TryRecvError::Disconnected) => {
                show_error("SSH host-key probe failed", "Host-key worker disconnected.");
                ControlFlow::Break
            }
        }
    });
}

#[allow(clippy::too_many_arguments)]
fn show_unknown_host_dialog(
    pane: &PaneHandle,
    host: &str,
    username: &str,
    port: u16,
    path: &str,
    auth: PaneSftpAuth,
    fingerprint: String,
    on_success: Rc<dyn Fn()>,
) {
    let dialog = gtk::Dialog::builder()
        .title("Unknown SSH Host")
        .modal(true)
        .build();
    dialog.add_button("Cancel", gtk::ResponseType::Cancel);
    dialog.add_button("Trust Once", gtk::ResponseType::Other(1));
    dialog.add_button("Trust Always", gtk::ResponseType::Accept);

    let label = gtk::Label::new(Some(&format!(
        "The host is not in OpenSSH known_hosts.\n\n{host}:{port}\nSHA-256 fingerprint:\n{fingerprint}\n\nVerify this fingerprint before trusting it."
    )));
    label.set_wrap(true);
    label.set_selectable(true);
    label.set_margin_top(16);
    label.set_margin_bottom(16);
    label.set_margin_start(16);
    label.set_margin_end(16);
    dialog.content_area().append(&label);

    let pane = pane.clone();
    let host = host.to_owned();
    let username = username.to_owned();
    let path = path.to_owned();
    dialog.connect_response(move |dialog, response| {
        if response == gtk::ResponseType::Accept || response == gtk::ResponseType::Other(1) {
            if response == gtk::ResponseType::Accept {
                let save = (|| {
                    let mut trust =
                        TrustedHosts::load_default().map_err(|error| error.to_string())?;
                    trust.trust(&host, port, &fingerprint);
                    trust.save_default().map_err(|error| error.to_string())
                })();
                if let Err(error) = save {
                    show_error("Could not save trusted host", &error);
                    dialog.close();
                    return;
                }
            }
            connect_now(
                &pane,
                &host,
                &username,
                port,
                &path,
                auth.clone(),
                Some(fingerprint.clone()),
                Rc::clone(&on_success),
            );
        }
        dialog.close();
    });
    dialog.present();
}

#[allow(clippy::too_many_arguments)]
fn connect_now(
    pane: &PaneHandle,
    host: &str,
    username: &str,
    port: u16,
    path: &str,
    auth: PaneSftpAuth,
    trusted_fingerprint: Option<String>,
    on_success: Rc<dyn Fn()>,
) {
    pane.connect_sftp_with_auth_async(
        host,
        username,
        port,
        path,
        auth,
        trusted_fingerprint,
        Rc::new(move |result| match result {
            Ok(()) => on_success(),
            Err(error) => show_error("SFTP connection failed", &error),
        }),
    );
}

fn remove_profile(profile: &SavedConnection) -> Result<(), String> {
    let mut profiles = ConnectionProfiles::load_default().map_err(|error| error.to_string())?;
    if !profiles.remove(&profile.id) {
        return Err("saved connection no longer exists".to_owned());
    }
    profiles.save_default().map_err(|error| error.to_string())?;

    if let Some(secret_key) = profile.secret_key.as_deref() {
        let mut store = PlatformSecretStore;
        if let Err(error) = store.delete(secret_key) {
            return Err(format!(
                "connection removed, but credential cleanup failed: {error}"
            ));
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn save_profile(
    id: &str,
    name: &str,
    host: &str,
    username: &str,
    port: u16,
    path: &str,
    auth: &PaneSftpAuth,
    save_secret: bool,
) -> Result<(), String> {
    let mut profile = SavedConnection::new(id, name, host, username, port, path)?;
    match auth {
        PaneSftpAuth::Agent => {}
        PaneSftpAuth::Password(password) => {
            if !save_secret {
                return Err(
                    "Password connections can only be saved when the password is stored in the OS credential store."
                        .to_owned(),
                );
            }
            let key = format!("connection:{id}:password");
            let mut store = PlatformSecretStore;
            store
                .set(&key, password)
                .map_err(|error| error.to_string())?;
            profile = profile.with_password_secret(key);
        }
        PaneSftpAuth::PrivateKey {
            private_key,
            passphrase,
        } => {
            let secret_key = if let Some(passphrase) = passphrase {
                if !save_secret {
                    return Err(
                        "A passphrase-protected key can only be saved when its passphrase is stored in the OS credential store."
                            .to_owned(),
                    );
                }
                let key = format!("connection:{id}:key-passphrase");
                let mut store = PlatformSecretStore;
                store
                    .set(&key, passphrase)
                    .map_err(|error| error.to_string())?;
                Some(key)
            } else {
                None
            };
            profile = profile.with_private_key(private_key.clone(), secret_key);
        }
    }
    profile.validate()?;
    let mut profiles = ConnectionProfiles::load_default().map_err(|error| error.to_string())?;
    profiles.upsert(profile);
    profiles.save_default().map_err(|error| error.to_string())
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

fn password_entry(grid: &gtk::Grid, row: i32, label: &str, placeholder: &str) -> gtk::Entry {
    let entry = form_entry(grid, row, label, placeholder, "");
    entry.set_visibility(false);
    entry.set_input_purpose(gtk::InputPurpose::Password);
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

fn expand_home(path: &str) -> String {
    if path == "~" {
        return std::env::var("HOME").unwrap_or_else(|_| path.to_owned());
    }
    if let Some(rest) = path.strip_prefix("~/")
        && let Ok(home) = std::env::var("HOME")
    {
        return format!("{home}/{rest}");
    }
    path.to_owned()
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
