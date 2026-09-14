use adw::prelude::*;
use cyber_pumpkin_application::{AppPreferences, DoubleClickAction, ExistingItemAction};
use gtk::Orientation;
use std::cell::RefCell;
use std::rc::Rc;

pub(crate) fn show_preferences(app: &adw::Application) {
    let preferences = Rc::new(RefCell::new(
        AppPreferences::load_default().unwrap_or_default(),
    ));

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("Cyber-Pumpkin Preferences")
        .default_width(760)
        .default_height(560)
        .build();

    let stack = gtk::Stack::new();
    stack.set_hexpand(true);
    stack.set_vexpand(true);
    stack.set_transition_type(gtk::StackTransitionType::Crossfade);

    stack.add_titled(&general_page(), Some("general"), "General");
    stack.add_titled(&files_page(&preferences), Some("files"), "Files");
    stack.add_titled(
        &transfers_page(&preferences),
        Some("transfers"),
        "Transfers",
    );
    stack.add_titled(&rules_page(), Some("rules"), "Rules");
    stack.add_titled(&keys_page(), Some("keys"), "Keys");
    stack.add_titled(
        &advanced_page(Rc::clone(&preferences)),
        Some("advanced"),
        "Advanced",
    );

    let switcher = gtk::StackSwitcher::new();
    switcher.set_stack(Some(&stack));
    switcher.set_halign(gtk::Align::Center);

    let root = gtk::Box::new(Orientation::Vertical, 0);
    let header = adw::HeaderBar::new();
    header.set_title_widget(Some(&switcher));
    root.append(&header);
    root.append(&stack);

    window.set_content(Some(&root));
    window.present();
}

fn general_page() -> gtk::Widget {
    let page = page_box();
    page.append(&section_title("General"));
    page.append(&body_label(
        "Startup locations, tab behavior, terminal integration, and update policy will live here.",
    ));
    page.append(&body_label(
        "Cyber-Pumpkin keeps platform-specific presentation native while shared application behavior stays in Rust.",
    ));
    page.upcast()
}

fn files_page(preferences: &Rc<RefCell<AppPreferences>>) -> gtk::Widget {
    let page = page_box();
    page.append(&section_title("Files"));

    let confirm_delete = gtk::CheckButton::with_label("Ask before deleting items");
    confirm_delete.set_active(preferences.borrow().files.confirm_delete);
    {
        let preferences = Rc::clone(preferences);
        confirm_delete.connect_toggled(move |button| {
            preferences.borrow_mut().files.confirm_delete = button.is_active();
            save_preferences(&preferences.borrow());
        });
    }
    page.append(&confirm_delete);

    page.append(&field_label("Double-click action"));
    let double_click = gtk::ComboBoxText::new();
    double_click.append_text("Open");
    double_click.append_text("Transfer");
    double_click.append_text("Inspect");
    double_click.set_active(Some(match preferences.borrow().files.double_click_action {
        DoubleClickAction::Open => 0,
        DoubleClickAction::Transfer => 1,
        DoubleClickAction::Inspect => 2,
    }));
    {
        let preferences = Rc::clone(preferences);
        double_click.connect_changed(move |combo| {
            preferences.borrow_mut().files.double_click_action = match combo.active() {
                Some(1) => DoubleClickAction::Transfer,
                Some(2) => DoubleClickAction::Inspect,
                _ => DoubleClickAction::Open,
            };
            save_preferences(&preferences.borrow());
        });
    }
    page.append(&double_click);

    page.append(&body_label(
        "Custom editor mappings will be added after the remote-edit workspace lands.",
    ));
    page.upcast()
}

fn transfers_page(preferences: &Rc<RefCell<AppPreferences>>) -> gtk::Widget {
    let page = page_box();
    page.append(&section_title("Transfers"));
    page.append(&body_label(
        "Choose how Cyber-Pumpkin handles existing destination items.",
    ));

    append_conflict_row(
        &page,
        "Downloading files",
        Rc::clone(preferences),
        ConflictField::DownloadingFiles,
    );
    append_conflict_row(
        &page,
        "Downloading folders",
        Rc::clone(preferences),
        ConflictField::DownloadingFolders,
    );
    append_conflict_row(
        &page,
        "Uploading files",
        Rc::clone(preferences),
        ConflictField::UploadingFiles,
    );
    append_conflict_row(
        &page,
        "Uploading folders",
        Rc::clone(preferences),
        ConflictField::UploadingFolders,
    );

    page.append(&field_label("Simultaneous transfers"));
    let adjustment = gtk::Adjustment::new(
        f64::from(preferences.borrow().transfers.simultaneous_transfers),
        1.0,
        20.0,
        1.0,
        1.0,
        0.0,
    );
    let simultaneous = gtk::SpinButton::new(Some(&adjustment), 1.0, 0);
    {
        let preferences = Rc::clone(preferences);
        simultaneous.connect_value_changed(move |spin| {
            if let Ok(value) = u8::try_from(spin.value_as_int()) {
                preferences.borrow_mut().transfers.simultaneous_transfers = value;
                save_preferences(&preferences.borrow());
            }
        });
    }
    page.append(&simultaneous);

    let keep_activity = gtk::CheckButton::with_label("Keep completed activity items");
    keep_activity.set_active(preferences.borrow().transfers.keep_activity);
    {
        let preferences = Rc::clone(preferences);
        keep_activity.connect_toggled(move |button| {
            preferences.borrow_mut().transfers.keep_activity = button.is_active();
            save_preferences(&preferences.borrow());
        });
    }
    page.append(&keep_activity);
    page.upcast()
}

#[derive(Clone, Copy)]
enum ConflictField {
    DownloadingFiles,
    DownloadingFolders,
    UploadingFiles,
    UploadingFolders,
}

fn append_conflict_row(
    page: &gtk::Box,
    label: &str,
    preferences: Rc<RefCell<AppPreferences>>,
    field: ConflictField,
) {
    let row = gtk::Box::new(Orientation::Horizontal, 8);
    let title = field_label(label);
    title.set_width_chars(20);
    let combo = gtk::ComboBoxText::new();
    combo.append_text("Ask");
    combo.append_text("Replace");
    combo.append_text("Skip");
    combo.set_hexpand(true);

    let current = match field {
        ConflictField::DownloadingFiles => preferences.borrow().transfers.downloading_files,
        ConflictField::DownloadingFolders => preferences.borrow().transfers.downloading_folders,
        ConflictField::UploadingFiles => preferences.borrow().transfers.uploading_files,
        ConflictField::UploadingFolders => preferences.borrow().transfers.uploading_folders,
    };
    combo.set_active(Some(conflict_index(current)));

    combo.connect_changed(move |combo| {
        let action = match combo.active() {
            Some(1) => ExistingItemAction::Replace,
            Some(2) => ExistingItemAction::Skip,
            _ => ExistingItemAction::Ask,
        };
        let mut preferences_mut = preferences.borrow_mut();
        match field {
            ConflictField::DownloadingFiles => {
                preferences_mut.transfers.downloading_files = action;
            }
            ConflictField::DownloadingFolders => {
                preferences_mut.transfers.downloading_folders = action;
            }
            ConflictField::UploadingFiles => {
                preferences_mut.transfers.uploading_files = action;
            }
            ConflictField::UploadingFolders => {
                preferences_mut.transfers.uploading_folders = action;
            }
        }
        save_preferences(&preferences_mut);
    });

    row.append(&title);
    row.append(&combo);
    page.append(&row);
}

const fn conflict_index(action: ExistingItemAction) -> u32 {
    match action {
        ExistingItemAction::Ask => 0,
        ExistingItemAction::Replace => 1,
        ExistingItemAction::Skip => 2,
    }
}

fn rules_page() -> gtk::Widget {
    let page = page_box();
    page.append(&section_title("Rules"));
    page.append(&body_label(
        "Reusable include/skip rules will be shared by recursive transfer and sync planning.",
    ));

    let example = gtk::Frame::new(Some("Rule editor foundation"));
    let text = body_label(
        "Match: name / extension / type / path\nOperators: is / contains / starts with\nActions: include / skip",
    );
    text.set_margin_top(12);
    text.set_margin_bottom(12);
    text.set_margin_start(12);
    text.set_margin_end(12);
    example.set_child(Some(&text));
    page.append(&example);
    page.upcast()
}

fn keys_page() -> gtk::Widget {
    let page = page_box();
    page.append(&section_title("Keys"));
    page.append(&body_label(
        "SFTP currently authenticates through your SSH agent and honors keys configured by OpenSSH.",
    ));
    page.append(&body_label(
        "Secret material is intentionally kept out of saved connection profiles.",
    ));
    page.upcast()
}

fn advanced_page(preferences: Rc<RefCell<AppPreferences>>) -> gtk::Widget {
    let page = page_box();
    page.append(&section_title("Advanced"));

    let keepalive = gtk::CheckButton::with_label("Keep idle connections alive");
    keepalive.set_active(preferences.borrow().advanced.keep_connections_alive);
    {
        let preferences = Rc::clone(&preferences);
        keepalive.connect_toggled(move |button| {
            preferences.borrow_mut().advanced.keep_connections_alive = button.is_active();
            save_preferences(&preferences.borrow());
        });
    }
    page.append(&keepalive);

    let verbose = gtk::CheckButton::with_label("Verbose logging");
    verbose.set_active(preferences.borrow().advanced.verbose_logging);
    verbose.connect_toggled(move |button| {
        preferences.borrow_mut().advanced.verbose_logging = button.is_active();
        save_preferences(&preferences.borrow());
    });
    page.append(&verbose);

    page.append(&body_label(
        "Proxy configuration, protocol-specific tuning, and connection-pool controls will build on this shared settings model.",
    ));
    page.upcast()
}

fn page_box() -> gtk::Box {
    let page = gtk::Box::new(Orientation::Vertical, 12);
    page.set_margin_top(24);
    page.set_margin_bottom(24);
    page.set_margin_start(32);
    page.set_margin_end(32);
    page
}

fn section_title(text: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    label.set_xalign(0.0);
    label.add_css_class("title-2");
    label
}

fn field_label(text: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    label.set_xalign(0.0);
    label
}

fn body_label(text: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    label.set_xalign(0.0);
    label.set_wrap(true);
    label
}

fn save_preferences(preferences: &AppPreferences) {
    if let Err(error) = preferences.save_default() {
        eprintln!("failed to save Cyber-Pumpkin preferences: {error}");
    }
}
