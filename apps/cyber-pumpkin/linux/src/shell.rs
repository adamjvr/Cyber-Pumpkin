use crate::browser::{PaneHandle, PaneSide, build_pane};
use crate::transfer_ui::build_copy_bar;
use adw::prelude::*;
use gtk::Orientation;
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
    body.set_start_child(Some(&build_places(&home, &left, &right, &active)));
    body.set_end_child(Some(&browser));

    let header = adw::HeaderBar::new();
    header.set_title_widget(Some(&gtk::Label::new(Some("Cyber-Pumpkin"))));

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
    let hidden = gtk::ToggleButton::with_label("Hidden");
    let activity_button = gtk::ToggleButton::with_label("Activity");

    toolbar.append(&refresh);
    toolbar.append(&new_folder);
    toolbar.append(&hidden);
    toolbar.append(&activity_button);

    connect_refresh(&refresh, left, right, active);
    connect_new_folder(&new_folder, left, right, active);
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

fn show_new_folder_dialog(pane: &PaneHandle) {
    let dialog = gtk::Dialog::builder()
        .title("New Folder")
        .modal(true)
        .build();
    dialog.add_button("Cancel", gtk::ResponseType::Cancel);
    dialog.add_button("Create", gtk::ResponseType::Accept);

    let entry = gtk::Entry::new();
    entry.set_placeholder_text(Some("Folder name"));
    entry.set_margin_top(12);
    entry.set_margin_bottom(12);
    entry.set_margin_start(12);
    entry.set_margin_end(12);
    dialog.content_area().append(&entry);

    let pane = pane.clone();
    dialog.connect_response(move |dialog, response| {
        if response == gtk::ResponseType::Accept {
            pane.create_folder(entry.text().as_str());
        }
        dialog.close();
    });
    dialog.present();
}

fn build_places(
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
