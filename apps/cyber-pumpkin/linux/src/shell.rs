use crate::browser::{PaneHandle, PaneSide, build_pane};
use crate::transfer_ui::build_copy_bar;
use adw::prelude::*;
use gtk::Orientation;
use gtk::gio;
use std::cell::Cell;
use std::path::Path;
use std::rc::Rc;

pub(crate) fn build_ui(app: &adw::Application) {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_owned());
    let active = Rc::new(Cell::new(PaneSide::Left));
    let left = build_pane("Local A", &home, PaneSide::Left, &active);
    let right = build_pane("Local B", &home, PaneSide::Right, &active);

    let activity_list = create_activity_list();
    let activity_revealer = create_activity_revealer(&activity_list);
    let copy_bar = build_copy_bar(&left, &right, &activity_list);

    let panes = gtk::Paned::new(Orientation::Horizontal);
    panes.set_hexpand(true);
    panes.set_vexpand(true);
    panes.set_position(560);
    panes.set_wide_handle(true);
    panes.set_start_child(Some(&left.root));
    panes.set_end_child(Some(&right.root));

    let browser = gtk::Box::new(Orientation::Vertical, 0);
    browser.append(&build_toolbar(&left, &right, &active, &activity_revealer));
    browser.append(&copy_bar.root);
    browser.append(&panes);
    browser.append(&activity_revealer);

    let body = gtk::Paned::new(Orientation::Horizontal);
    body.set_position(190);
    body.set_start_child(Some(&build_pumpkin_patch(&home, &left, &right, &active)));
    body.set_end_child(Some(&browser));

    let header = adw::HeaderBar::new();
    header.set_title_widget(Some(&gtk::Label::new(Some("Cyber-Pumpkin"))));
    header.pack_end(&build_menu_button());

    install_actions(app, &home, &left, &right, &active, &activity_revealer);
    install_accelerators(app);

    let root = gtk::Box::new(Orientation::Vertical, 0);
    root.append(&header);
    root.append(&body);

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("Cyber-Pumpkin")
        .default_width(1320)
        .default_height(820)
        .content(&root)
        .build();
    window.present();
}

fn build_menu_button() -> gtk::MenuButton {
    let menu = gio::Menu::new();

    let file = gio::Menu::new();
    file.append(Some("New Folder…"), Some("app.new-folder"));
    file.append(Some("Rename…"), Some("app.rename"));
    file.append(Some("Delete…"), Some("app.delete"));
    menu.append_submenu(Some("File"), &file);

    let view = gio::Menu::new();
    view.append(Some("Refresh"), Some("app.refresh"));
    view.append(Some("Show Hidden Files"), Some("app.hidden"));
    view.append(Some("Show Activity"), Some("app.activity"));
    menu.append_submenu(Some("View"), &view);

    let go = gio::Menu::new();
    go.append(Some("Home"), Some("app.go-home"));
    go.append(Some("Downloads"), Some("app.go-downloads"));
    go.append(Some("Root"), Some("app.go-root"));
    menu.append_submenu(Some("Go"), &go);

    let help = gio::Menu::new();
    help.append(Some("About Cyber-Pumpkin"), Some("app.about"));
    menu.append_submenu(Some("Help"), &help);

    gtk::MenuButton::builder()
        .icon_name("open-menu-symbolic")
        .tooltip_text("Cyber-Pumpkin Menu")
        .menu_model(&menu)
        .build()
}

fn install_actions(
    app: &adw::Application,
    home: &str,
    left: &PaneHandle,
    right: &PaneHandle,
    active: &Rc<Cell<PaneSide>>,
    activity: &gtk::Revealer,
) {
    install_simple_action(app, "refresh", {
        let left = left.clone();
        let right = right.clone();
        let active = Rc::clone(active);
        move || active_pane(&left, &right, active.get()).refresh()
    });

    install_simple_action(app, "new-folder", {
        let left = left.clone();
        let right = right.clone();
        let active = Rc::clone(active);
        move || show_new_folder_dialog(active_pane(&left, &right, active.get()))
    });

    install_simple_action(app, "rename", {
        let left = left.clone();
        let right = right.clone();
        let active = Rc::clone(active);
        move || show_rename_dialog(active_pane(&left, &right, active.get()))
    });

    install_simple_action(app, "delete", {
        let left = left.clone();
        let right = right.clone();
        let active = Rc::clone(active);
        move || show_delete_dialog(active_pane(&left, &right, active.get()))
    });

    install_toggle_action(app, "hidden", false, {
        let left = left.clone();
        let right = right.clone();
        move |show| {
            left.set_show_hidden(show);
            right.set_show_hidden(show);
        }
    });

    install_toggle_action(app, "activity", false, {
        let activity = activity.clone();
        move |show| activity.set_reveal_child(show)
    });

    let home_path = home.to_owned();
    install_simple_action(app, "go-home", {
        let left = left.clone();
        let right = right.clone();
        let active = Rc::clone(active);
        move || active_pane(&left, &right, active.get()).navigate_text(&home_path)
    });

    let downloads = Path::new(home)
        .join("Downloads")
        .to_string_lossy()
        .into_owned();
    install_simple_action(app, "go-downloads", {
        let left = left.clone();
        let right = right.clone();
        let active = Rc::clone(active);
        move || active_pane(&left, &right, active.get()).navigate_text(&downloads)
    });

    install_simple_action(app, "go-root", {
        let left = left.clone();
        let right = right.clone();
        let active = Rc::clone(active);
        move || active_pane(&left, &right, active.get()).navigate_text("/")
    });

    install_simple_action(app, "about", || {
        let about = gtk::AboutDialog::builder()
            .program_name("Cyber-Pumpkin")
            .comments("Native dual-pane file transfer client")
            .website("https://github.com/adamjvr/Cyber-Pumpkin")
            .build();
        about.present();
    });
}

fn install_simple_action<F>(app: &adw::Application, name: &str, handler: F)
where
    F: Fn() + 'static,
{
    let action = gio::SimpleAction::new(name, None);
    action.connect_activate(move |_, _| handler());
    app.add_action(&action);
}

fn install_toggle_action<F>(app: &adw::Application, name: &str, initial: bool, handler: F)
where
    F: Fn(bool) + 'static,
{
    let action = gio::SimpleAction::new_stateful(name, None, &initial.to_variant());
    action.connect_activate(move |action, _| {
        let current = action
            .state()
            .and_then(|value| value.get::<bool>())
            .unwrap_or(false);
        let next = !current;
        action.set_state(&next.to_variant());
        handler(next);
    });
    app.add_action(&action);
}

fn install_accelerators(app: &adw::Application) {
    app.set_accels_for_action("app.refresh", &["<Primary>r"]);
    app.set_accels_for_action("app.new-folder", &["<Primary><Shift>n"]);
    app.set_accels_for_action("app.rename", &["F2"]);
    app.set_accels_for_action("app.delete", &["Delete"]);
    app.set_accels_for_action("app.hidden", &["<Primary>period"]);
}

fn create_activity_list() -> gtk::ListBox {
    let list = gtk::ListBox::new();
    list.set_selection_mode(gtk::SelectionMode::None);
    let label = gtk::Label::new(Some("No transfer activity yet."));
    label.set_xalign(0.0);
    label.add_css_class("dim-label");
    let row = gtk::ListBoxRow::new();
    row.set_selectable(false);
    row.set_child(Some(&label));
    list.append(&row);
    list
}

fn create_activity_revealer(list: &gtk::ListBox) -> gtk::Revealer {
    let title = gtk::Label::new(Some("Activity"));
    title.set_xalign(0.0);
    title.add_css_class("heading");

    let content = gtk::Box::new(Orientation::Vertical, 4);
    content.set_margin_top(8);
    content.set_margin_bottom(8);
    content.set_margin_start(8);
    content.set_margin_end(8);
    content.append(&title);
    content.append(list);

    let revealer = gtk::Revealer::new();
    revealer.set_transition_type(gtk::RevealerTransitionType::SlideUp);
    revealer.set_child(Some(&content));
    revealer
}

fn build_toolbar(
    left: &PaneHandle,
    right: &PaneHandle,
    active: &Rc<Cell<PaneSide>>,
    activity: &gtk::Revealer,
) -> gtk::Box {
    let toolbar = gtk::Box::new(Orientation::Horizontal, 6);
    toolbar.set_margin_top(6);
    toolbar.set_margin_bottom(2);
    toolbar.set_margin_start(8);
    toolbar.set_margin_end(8);

    let refresh = gtk::Button::from_icon_name("view-refresh-symbolic");
    refresh.set_tooltip_text(Some("Refresh active pane"));
    let new_folder = gtk::Button::with_label("New Folder");
    let rename = gtk::Button::with_label("Rename");
    let delete = gtk::Button::with_label("Delete");
    let hidden = gtk::ToggleButton::with_label("Hidden");
    let activity_button = gtk::ToggleButton::with_label("Activity");

    toolbar.append(&refresh);
    toolbar.append(&new_folder);
    toolbar.append(&rename);
    toolbar.append(&delete);
    toolbar.append(&hidden);
    toolbar.append(&activity_button);

    connect_refresh(&refresh, left, right, active);
    connect_new_folder(&new_folder, left, right, active);
    connect_rename(&rename, left, right, active);
    connect_delete(&delete, left, right, active);
    connect_hidden(&hidden, left, right);
    connect_activity(&activity_button, activity);
    toolbar
}

fn connect_refresh(
    button: &gtk::Button,
    left: &PaneHandle,
    right: &PaneHandle,
    active: &Rc<Cell<PaneSide>>,
) {
    let left = left.clone();
    let right = right.clone();
    let active = Rc::clone(active);
    button.clone().connect_clicked(move |_| {
        active_pane(&left, &right, active.get()).refresh();
    });
}

fn connect_new_folder(
    button: &gtk::Button,
    left: &PaneHandle,
    right: &PaneHandle,
    active: &Rc<Cell<PaneSide>>,
) {
    let left = left.clone();
    let right = right.clone();
    let active = Rc::clone(active);
    button.clone().connect_clicked(move |_| {
        show_new_folder_dialog(active_pane(&left, &right, active.get()));
    });
}

fn connect_rename(
    button: &gtk::Button,
    left: &PaneHandle,
    right: &PaneHandle,
    active: &Rc<Cell<PaneSide>>,
) {
    let left = left.clone();
    let right = right.clone();
    let active = Rc::clone(active);
    button.clone().connect_clicked(move |_| {
        show_rename_dialog(active_pane(&left, &right, active.get()));
    });
}

fn connect_delete(
    button: &gtk::Button,
    left: &PaneHandle,
    right: &PaneHandle,
    active: &Rc<Cell<PaneSide>>,
) {
    let left = left.clone();
    let right = right.clone();
    let active = Rc::clone(active);
    button.clone().connect_clicked(move |_| {
        show_delete_dialog(active_pane(&left, &right, active.get()));
    });
}

fn connect_hidden(button: &gtk::ToggleButton, left: &PaneHandle, right: &PaneHandle) {
    let left = left.clone();
    let right = right.clone();
    button.clone().connect_toggled(move |toggle| {
        let show = toggle.is_active();
        left.set_show_hidden(show);
        right.set_show_hidden(show);
    });
}

fn connect_activity(button: &gtk::ToggleButton, activity: &gtk::Revealer) {
    let activity = activity.clone();
    button.clone().connect_toggled(move |toggle| {
        activity.set_reveal_child(toggle.is_active());
    });
}

fn show_new_folder_dialog(pane: &PaneHandle) {
    show_text_dialog("New Folder", "Folder name", None, {
        let pane = pane.clone();
        move |text| pane.create_folder(text)
    });
}

fn show_rename_dialog(pane: &PaneHandle) {
    let Some(current) = pane.selected_name() else {
        return;
    };
    show_text_dialog("Rename", "New name", Some(&current), {
        let pane = pane.clone();
        move |text| pane.rename_selected(text)
    });
}

fn show_text_dialog<F>(title: &str, placeholder: &str, initial: Option<&str>, handler: F)
where
    F: Fn(&str) + 'static,
{
    let dialog = gtk::Dialog::builder().title(title).modal(true).build();
    dialog.add_button("Cancel", gtk::ResponseType::Cancel);
    dialog.add_button("OK", gtk::ResponseType::Accept);

    let entry = gtk::Entry::new();
    entry.set_placeholder_text(Some(placeholder));
    if let Some(initial) = initial {
        entry.set_text(initial);
        entry.select_region(0, -1);
    }
    entry.set_margin_top(12);
    entry.set_margin_bottom(12);
    entry.set_margin_start(12);
    entry.set_margin_end(12);
    dialog.content_area().append(&entry);

    dialog.connect_response(move |dialog, response| {
        if response == gtk::ResponseType::Accept {
            handler(entry.text().as_str());
        }
        dialog.close();
    });
    dialog.present();
}

fn show_delete_dialog(pane: &PaneHandle) {
    let Some(name) = pane.selected_name() else {
        return;
    };
    let dialog = gtk::Dialog::builder().title("Delete").modal(true).build();
    dialog.add_button("Cancel", gtk::ResponseType::Cancel);
    dialog.add_button("Delete", gtk::ResponseType::Accept);

    let label = gtk::Label::new(Some(&format!(
        "Delete “{name}”? Non-empty folders are not removed recursively yet."
    )));
    label.set_wrap(true);
    label.set_margin_top(16);
    label.set_margin_bottom(16);
    label.set_margin_start(16);
    label.set_margin_end(16);
    dialog.content_area().append(&label);

    let pane = pane.clone();
    dialog.connect_response(move |dialog, response| {
        if response == gtk::ResponseType::Accept {
            pane.delete_selected();
        }
        dialog.close();
    });
    dialog.present();
}

fn build_pumpkin_patch(
    home: &str,
    left: &PaneHandle,
    right: &PaneHandle,
    active: &Rc<Cell<PaneSide>>,
) -> gtk::Box {
    let sidebar = gtk::Box::new(Orientation::Vertical, 4);
    sidebar.set_margin_top(8);
    sidebar.set_margin_bottom(8);
    sidebar.set_margin_start(8);
    sidebar.set_margin_end(8);

    let title = gtk::Label::new(Some("Pumpkin Patch"));
    title.set_xalign(0.0);
    title.add_css_class("heading");
    sidebar.append(&title);

    let home_path = Path::new(home);
    let places = [
        ("Home", home_path.to_path_buf()),
        ("Desktop", home_path.join("Desktop")),
        ("Documents", home_path.join("Documents")),
        ("Downloads", home_path.join("Downloads")),
        ("Root", Path::new("/").to_path_buf()),
    ];

    for (label, path) in places {
        let button = gtk::Button::with_label(label);
        button.set_halign(gtk::Align::Fill);
        let left = left.clone();
        let right = right.clone();
        let active = Rc::clone(active);
        let text = path.to_string_lossy().into_owned();
        button.connect_clicked(move |_| {
            active_pane(&left, &right, active.get()).navigate_text(&text);
        });
        sidebar.append(&button);
    }
    sidebar
}

fn active_pane<'a>(left: &'a PaneHandle, right: &'a PaneHandle, side: PaneSide) -> &'a PaneHandle {
    match side {
        PaneSide::Left => left,
        PaneSide::Right => right,
    }
}
