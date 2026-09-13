use crate::browser::{PaneHandle, PaneSide, SortMode, build_pane, format_size};
use crate::transfer_ui::{CopyBar, build_copy_bar};
use adw::prelude::*;
use gtk::Orientation;
use gtk::gio;
use gtk::glib::variant::ToVariant;
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

    let panes = build_panes(&left, &right);
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

    install_actions(
        app,
        &home,
        &left,
        &right,
        &active,
        &activity_revealer,
        &copy_bar,
    );
    install_accelerators(app);

    let root = gtk::Box::new(Orientation::Vertical, 0);
    root.append(&header);
    root.append(&body);

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("Cyber-Pumpkin")
        .default_width(1380)
        .default_height(860)
        .content(&root)
        .build();
    window.present();
}

fn build_panes(left: &PaneHandle, right: &PaneHandle) -> gtk::Paned {
    let panes = gtk::Paned::new(Orientation::Horizontal);
    panes.set_hexpand(true);
    panes.set_vexpand(true);
    panes.set_position(590);
    panes.set_wide_handle(true);
    panes.set_start_child(Some(&left.root));
    panes.set_end_child(Some(&right.root));
    panes
}

fn build_menu_button() -> gtk::MenuButton {
    let menu = gio::Menu::new();

    let file = gio::Menu::new();
    file.append(Some("New Folder…"), Some("app.new-folder"));
    file.append(Some("Get Info"), Some("app.info"));
    file.append(Some("Rename…"), Some("app.rename"));
    file.append(Some("Delete…"), Some("app.delete"));
    menu.append_submenu(Some("File"), &file);

    let view = gio::Menu::new();
    view.append(Some("Refresh"), Some("app.refresh"));
    view.append(Some("Show Hidden Files"), Some("app.hidden"));
    view.append(Some("Show Activity"), Some("app.activity"));
    let sort = gio::Menu::new();
    sort.append(Some("Name"), Some("app.sort-name"));
    sort.append(Some("Type"), Some("app.sort-type"));
    sort.append(Some("Size"), Some("app.sort-size"));
    sort.append(Some("Reverse Order"), Some("app.sort-reverse"));
    view.append_submenu(Some("Sort By"), &sort);
    menu.append_submenu(Some("View"), &view);

    let go = gio::Menu::new();
    go.append(Some("Home"), Some("app.go-home"));
    go.append(Some("Downloads"), Some("app.go-downloads"));
    go.append(Some("Root"), Some("app.go-root"));
    menu.append_submenu(Some("Go"), &go);

    let transfer = gio::Menu::new();
    transfer.append(Some("Copy to Other Pane"), Some("app.copy-other"));
    menu.append_submenu(Some("Transfer"), &transfer);

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
    copy_bar: &CopyBar,
) {
    install_file_actions(app, left, right, active);
    install_view_actions(app, left, right, active, activity);
    install_go_actions(app, home, left, right, active);
    install_transfer_action(app, left, right, active, copy_bar);
    install_simple_action(app, "about", || {
        let about = gtk::AboutDialog::builder()
            .program_name("Cyber-Pumpkin")
            .comments("Native dual-pane file transfer client")
            .website("https://github.com/adamjvr/Cyber-Pumpkin")
            .build();
        about.present();
    });
}

fn install_file_actions(
    app: &adw::Application,
    left: &PaneHandle,
    right: &PaneHandle,
    active: &Rc<Cell<PaneSide>>,
) {
    install_simple_action(
        app,
        "refresh",
        pane_action(left, right, active, |pane| {
            pane.refresh();
        }),
    );
    install_simple_action(
        app,
        "new-folder",
        pane_action(left, right, active, show_new_folder_dialog),
    );
    install_simple_action(
        app,
        "rename",
        pane_action(left, right, active, show_rename_dialog),
    );
    install_simple_action(
        app,
        "delete",
        pane_action(left, right, active, show_delete_dialog),
    );
    install_simple_action(
        app,
        "info",
        pane_action(left, right, active, show_info_dialog),
    );
}

fn install_view_actions(
    app: &adw::Application,
    left: &PaneHandle,
    right: &PaneHandle,
    active: &Rc<Cell<PaneSide>>,
    activity: &gtk::Revealer,
) {
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

    install_simple_action(
        app,
        "sort-name",
        pane_action(left, right, active, |pane| {
            pane.set_sort_mode(SortMode::Name);
        }),
    );
    install_simple_action(
        app,
        "sort-type",
        pane_action(left, right, active, |pane| {
            pane.set_sort_mode(SortMode::Type);
        }),
    );
    install_simple_action(
        app,
        "sort-size",
        pane_action(left, right, active, |pane| {
            pane.set_sort_mode(SortMode::Size);
        }),
    );

    let reverse = Rc::new(Cell::new(false));
    install_simple_action(app, "sort-reverse", {
        let left = left.clone();
        let right = right.clone();
        let active = Rc::clone(active);
        let reverse = Rc::clone(&reverse);
        move || {
            let next = !reverse.get();
            reverse.set(next);
            active_pane(&left, &right, active.get()).set_sort_descending(next);
        }
    });
}

fn install_go_actions(
    app: &adw::Application,
    home: &str,
    left: &PaneHandle,
    right: &PaneHandle,
    active: &Rc<Cell<PaneSide>>,
) {
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
}

fn install_transfer_action(
    app: &adw::Application,
    left: &PaneHandle,
    right: &PaneHandle,
    active: &Rc<Cell<PaneSide>>,
    copy_bar: &CopyBar,
) {
    let left = left.clone();
    let right = right.clone();
    let active = Rc::clone(active);
    let copy_bar = copy_bar.clone();

    install_simple_action(app, "copy-other", move || match active.get() {
        PaneSide::Left => copy_bar.copy_between(&left, &right),
        PaneSide::Right => copy_bar.copy_between(&right, &left),
    });
}

fn pane_action<F>(
    left: &PaneHandle,
    right: &PaneHandle,
    active: &Rc<Cell<PaneSide>>,
    handler: F,
) -> impl Fn() + 'static
where
    F: Fn(&PaneHandle) + 'static,
{
    let left = left.clone();
    let right = right.clone();
    let active = Rc::clone(active);
    move || handler(active_pane(&left, &right, active.get()))
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
    app.set_accels_for_action("app.info", &["<Primary>i"]);
    app.set_accels_for_action("app.copy-other", &["<Primary><Shift>c"]);
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
    let info = gtk::Button::with_label("Info");
    let rename = gtk::Button::with_label("Rename");
    let delete = gtk::Button::with_label("Delete");
    let hidden = gtk::ToggleButton::with_label("Hidden");
    let activity_button = gtk::ToggleButton::with_label("Activity");
    let sort = create_sort_combo();
    let reverse = gtk::ToggleButton::with_label("Reverse");

    for widget in [
        refresh.upcast_ref::<gtk::Widget>(),
        new_folder.upcast_ref::<gtk::Widget>(),
        info.upcast_ref::<gtk::Widget>(),
        rename.upcast_ref::<gtk::Widget>(),
        delete.upcast_ref::<gtk::Widget>(),
        hidden.upcast_ref::<gtk::Widget>(),
        activity_button.upcast_ref::<gtk::Widget>(),
        sort.upcast_ref::<gtk::Widget>(),
        reverse.upcast_ref::<gtk::Widget>(),
    ] {
        toolbar.append(widget);
    }

    connect_toolbar_actions(
        &refresh,
        &new_folder,
        &info,
        &rename,
        &delete,
        &hidden,
        &activity_button,
        &sort,
        &reverse,
        left,
        right,
        active,
        activity,
    );

    toolbar
}

#[allow(clippy::too_many_arguments)]
fn connect_toolbar_actions(
    refresh: &gtk::Button,
    new_folder: &gtk::Button,
    info: &gtk::Button,
    rename: &gtk::Button,
    delete: &gtk::Button,
    hidden: &gtk::ToggleButton,
    activity_button: &gtk::ToggleButton,
    sort: &gtk::ComboBoxText,
    reverse: &gtk::ToggleButton,
    left: &PaneHandle,
    right: &PaneHandle,
    active: &Rc<Cell<PaneSide>>,
    activity: &gtk::Revealer,
) {
    connect_refresh(refresh, left, right, active);
    connect_new_folder(new_folder, left, right, active);
    connect_info(info, left, right, active);
    connect_rename(rename, left, right, active);
    connect_delete(delete, left, right, active);
    connect_hidden(hidden, left, right);
    connect_activity(activity_button, activity);
    connect_sort(sort, left, right, active);
    connect_reverse(reverse, left, right, active);
}

fn create_sort_combo() -> gtk::ComboBoxText {
    let combo = gtk::ComboBoxText::new();
    combo.append_text("Name");
    combo.append_text("Type");
    combo.append_text("Size");
    combo.set_active(Some(0));
    combo.set_tooltip_text(Some("Sort active pane"));
    combo
}

fn connect_refresh(
    button: &gtk::Button,
    left: &PaneHandle,
    right: &PaneHandle,
    active: &Rc<Cell<PaneSide>>,
) {
    connect_button_to_pane(button, left, right, active, PaneHandle::refresh);
}

fn connect_new_folder(
    button: &gtk::Button,
    left: &PaneHandle,
    right: &PaneHandle,
    active: &Rc<Cell<PaneSide>>,
) {
    connect_button_to_pane(button, left, right, active, show_new_folder_dialog);
}

fn connect_info(
    button: &gtk::Button,
    left: &PaneHandle,
    right: &PaneHandle,
    active: &Rc<Cell<PaneSide>>,
) {
    connect_button_to_pane(button, left, right, active, show_info_dialog);
}

fn connect_rename(
    button: &gtk::Button,
    left: &PaneHandle,
    right: &PaneHandle,
    active: &Rc<Cell<PaneSide>>,
) {
    connect_button_to_pane(button, left, right, active, show_rename_dialog);
}

fn connect_delete(
    button: &gtk::Button,
    left: &PaneHandle,
    right: &PaneHandle,
    active: &Rc<Cell<PaneSide>>,
) {
    connect_button_to_pane(button, left, right, active, show_delete_dialog);
}

fn connect_button_to_pane<F>(
    button: &gtk::Button,
    left: &PaneHandle,
    right: &PaneHandle,
    active: &Rc<Cell<PaneSide>>,
    handler: F,
) where
    F: Fn(&PaneHandle) + 'static,
{
    let left = left.clone();
    let right = right.clone();
    let active = Rc::clone(active);
    button.clone().connect_clicked(move |_| {
        handler(active_pane(&left, &right, active.get()));
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

fn connect_sort(
    combo: &gtk::ComboBoxText,
    left: &PaneHandle,
    right: &PaneHandle,
    active: &Rc<Cell<PaneSide>>,
) {
    let left = left.clone();
    let right = right.clone();
    let active = Rc::clone(active);
    combo.clone().connect_changed(move |combo| {
        let mode = match combo.active() {
            Some(1) => SortMode::Type,
            Some(2) => SortMode::Size,
            _ => SortMode::Name,
        };
        active_pane(&left, &right, active.get()).set_sort_mode(mode);
    });
}

fn connect_reverse(
    button: &gtk::ToggleButton,
    left: &PaneHandle,
    right: &PaneHandle,
    active: &Rc<Cell<PaneSide>>,
) {
    let left = left.clone();
    let right = right.clone();
    let active = Rc::clone(active);
    button.clone().connect_toggled(move |toggle| {
        active_pane(&left, &right, active.get()).set_sort_descending(toggle.is_active());
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

fn show_info_dialog(pane: &PaneHandle) {
    let Some(entry) = pane.selected_entry() else {
        return;
    };

    let kind = format!("{:?}", entry.kind);
    let message = format!(
        "Name: {}\nType: {kind}\nSize: {}\nPath: {}",
        entry.name,
        format_size(entry.size),
        entry.path.as_str()
    );

    let dialog = gtk::Dialog::builder().title("Get Info").modal(true).build();
    dialog.add_button("Close", gtk::ResponseType::Close);

    let label = gtk::Label::new(Some(&message));
    label.set_selectable(true);
    label.set_xalign(0.0);
    label.set_margin_top(16);
    label.set_margin_bottom(16);
    label.set_margin_start(16);
    label.set_margin_end(16);
    dialog.content_area().append(&label);

    dialog.connect_response(|dialog, _| dialog.close());
    dialog.present();
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
