use crate::browser::{PaneHandle, PaneSide, SortMode, build_pane};
use crate::inspector::InspectorPane;
use crate::pane_workspace::{PaneMode, PaneWorkspace};
use crate::preferences_ui;
use crate::sync_ui::SyncPanel;
use crate::transfer_ui::{CopyBar, build_copy_bar};
use adw::prelude::*;
use gtk::Orientation;
use gtk::gio;
use gtk::glib::variant::ToVariant;
use std::cell::Cell;
use std::path::Path;
use std::rc::Rc;

#[derive(Clone)]
struct ActionContext {
    home: String,
    left: PaneHandle,
    right: PaneHandle,
    left_workspace: PaneWorkspace,
    right_workspace: PaneWorkspace,
    active: Rc<Cell<PaneSide>>,
    inspector: gtk::Box,
    inspector_button: gtk::ToggleButton,
    activity_popover: gtk::Popover,
    main_stack: gtk::Stack,
    sync_panel: SyncPanel,
    title: gtk::Label,
    copy_bar: CopyBar,
}

#[allow(clippy::too_many_lines)]
pub(crate) fn build_ui(app: &adw::Application) {
    install_css();

    let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_owned());
    let active = Rc::new(Cell::new(PaneSide::Left));
    let left = build_pane("Local", &home, PaneSide::Left, &active);
    let right = build_pane("Local", &home, PaneSide::Right, &active);

    let title = gtk::Label::new(Some("Cyber-Pumpkin"));
    title.add_css_class("heading");
    let search = gtk::SearchEntry::new();
    search.set_placeholder_text(Some("Search active pane"));
    search.set_width_chars(24);

    let mode_observer: Rc<dyn Fn(PaneSide, PaneMode)> = {
        let title = title.clone();
        let search = search.clone();
        let active = Rc::clone(&active);
        let left = left.clone();
        let right = right.clone();
        Rc::new(move |side, mode| {
            if active.get() != side {
                return;
            }
            let pane = match side {
                PaneSide::Left => &left,
                PaneSide::Right => &right,
            };
            apply_mode_chrome(&title, &search, pane, mode);
        })
    };

    let left_workspace = PaneWorkspace::new(
        &home,
        left.clone(),
        PaneSide::Left,
        &active,
        Rc::clone(&mode_observer),
    );
    let right_workspace = PaneWorkspace::new(
        &home,
        right.clone(),
        PaneSide::Right,
        &active,
        Rc::clone(&mode_observer),
    );

    let inspector = InspectorPane::new();
    {
        let inspector = inspector.clone();
        let active = Rc::clone(&active);
        let title = title.clone();
        let search = search.clone();
        let workspace = left_workspace.clone();
        let pane = left.clone();
        left.set_selection_observer(move |entry, backend| {
            if active.get() == PaneSide::Left {
                inspector.update(entry, &backend);
                apply_mode_chrome(&title, &search, &pane, workspace.mode());
            }
        });
    }
    {
        let inspector = inspector.clone();
        let active = Rc::clone(&active);
        let title = title.clone();
        let search = search.clone();
        let workspace = right_workspace.clone();
        let pane = right.clone();
        right.set_selection_observer(move |entry, backend| {
            if active.get() == PaneSide::Right {
                inspector.update(entry, &backend);
                apply_mode_chrome(&title, &search, &pane, workspace.mode());
            }
        });
    }

    let activity_list = create_activity_list();
    let activity_popover = create_activity_popover(&activity_list);
    let copy_bar = build_copy_bar(&left, &right, &activity_list);

    let panes = build_panes(&left_workspace.root, &right_workspace.root);
    let browser_page = gtk::Box::new(Orientation::Vertical, 0);
    browser_page.append(&panes);
    browser_page.append(&copy_bar.root);

    let main_stack = gtk::Stack::new();
    main_stack.set_hexpand(true);
    main_stack.set_vexpand(true);
    main_stack.set_transition_type(gtk::StackTransitionType::Crossfade);
    main_stack.add_named(&browser_page, Some("browser"));

    let sync_panel = SyncPanel::new(&left, &right, &main_stack, &title, &activity_list);
    main_stack.add_named(&sync_panel.root, Some("sync"));

    let browser_with_inspector = gtk::Paned::new(Orientation::Horizontal);
    browser_with_inspector.set_position(1220);
    browser_with_inspector.set_wide_handle(false);
    browser_with_inspector.set_start_child(Some(&main_stack));
    browser_with_inspector.set_end_child(Some(&inspector.root));

    let header = adw::HeaderBar::new();
    header.set_title_widget(Some(&title));

    let sync_button = icon_button("emblem-synchronizing-symbolic", "Sync Files");
    let activity_button = gtk::MenuButton::builder()
        .icon_name("view-reveal-symbolic")
        .tooltip_text("Activity")
        .build();
    activity_button.set_popover(Some(&activity_popover));
    let inspector_button = gtk::ToggleButton::new();
    inspector_button.set_icon_name("dialog-information-symbolic");
    inspector_button.set_tooltip_text(Some("Inspector"));
    inspector_button.set_active(true);

    header.pack_end(&build_menu_button());
    header.pack_end(&search);
    header.pack_end(&inspector_button);
    header.pack_end(&activity_button);
    header.pack_end(&sync_button);

    let context = ActionContext {
        home: home.clone(),
        left: left.clone(),
        right: right.clone(),
        left_workspace: left_workspace.clone(),
        right_workspace: right_workspace.clone(),
        active: Rc::clone(&active),
        inspector: inspector.root.clone(),
        inspector_button: inspector_button.clone(),
        activity_popover: activity_popover.clone(),
        main_stack: main_stack.clone(),
        sync_panel: sync_panel.clone(),
        title: title.clone(),
        copy_bar: copy_bar.clone(),
    };

    install_actions(app, &context);
    install_accelerators(app);

    {
        let context = context.clone();
        sync_button.connect_clicked(move |_| show_sync(&context));
    }
    {
        let inspector_root = inspector.root.clone();
        inspector_button.connect_toggled(move |button| {
            inspector_root.set_visible(button.is_active());
        });
    }
    {
        let left_workspace = left_workspace.clone();
        let right_workspace = right_workspace.clone();
        let active = Rc::clone(&active);
        search.connect_search_changed(move |entry| match active.get() {
            PaneSide::Left => left_workspace.set_search_query(entry.text().as_str()),
            PaneSide::Right => right_workspace.set_search_query(entry.text().as_str()),
        });
    }

    let root = gtk::Box::new(Orientation::Vertical, 0);
    root.append(&header);
    root.append(&build_linux_menu_bar());
    root.append(&browser_with_inspector);

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("Cyber-Pumpkin")
        .default_width(1500)
        .default_height(900)
        .content(&root)
        .build();
    install_window_actions(app, &window);
    window.present();
}

fn build_panes(left: &gtk::Box, right: &gtk::Box) -> gtk::Paned {
    let panes = gtk::Paned::new(Orientation::Horizontal);
    panes.set_hexpand(true);
    panes.set_vexpand(true);
    panes.set_position(610);
    panes.set_wide_handle(false);
    panes.set_start_child(Some(left));
    panes.set_end_child(Some(right));
    panes
}

fn show_sync(context: &ActionContext) {
    context.sync_panel.refresh();
    context.main_stack.set_visible_child_name("sync");
    context.title.set_text("Sync Files");
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

fn create_activity_popover(list: &gtk::ListBox) -> gtk::Popover {
    let title = gtk::Label::new(Some("Activity"));
    title.set_xalign(0.0);
    title.add_css_class("heading");

    let scroll = gtk::ScrolledWindow::new();
    scroll.set_min_content_width(430);
    scroll.set_min_content_height(230);
    scroll.set_child(Some(list));

    let content = gtk::Box::new(Orientation::Vertical, 8);
    content.set_margin_top(10);
    content.set_margin_bottom(10);
    content.set_margin_start(10);
    content.set_margin_end(10);
    content.append(&title);
    content.append(&scroll);

    let popover = gtk::Popover::new();
    popover.set_child(Some(&content));
    popover
}

fn apply_mode_chrome(
    title: &gtk::Label,
    search: &gtk::SearchEntry,
    pane: &PaneHandle,
    mode: PaneMode,
) {
    match mode {
        PaneMode::Browser => {
            title.set_text(&browser_title(pane));
            search.set_placeholder_text(Some("Search active pane"));
        }
        PaneMode::PumpkinPatch => {
            title.set_text("Pumpkin Patch");
            search.set_placeholder_text(Some("Search Pumpkin Patch"));
        }
        PaneMode::QuickConnect => {
            title.set_text("Quick Connect");
            search.set_placeholder_text(Some("Quick Connect"));
        }
    }
}

fn browser_title(pane: &PaneHandle) -> String {
    let location = pane.location();
    Path::new(location.as_str())
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .map_or_else(
            || pane.connection_display_name(),
            std::borrow::ToOwned::to_owned,
        )
}

fn install_css() {
    let provider = gtk::CssProvider::new();
    provider.load_from_data(
        ".navigation-sidebar row { min-height: 24px; }\n\
         .navigation-sidebar row > * { padding-top: 1px; padding-bottom: 1px; }\n\
         .boxed-list { border-radius: 8px; }\n\
         entry.flat { min-height: 26px; }\n\
         .pane-location-strip button { min-height: 24px; padding: 1px 6px; }",
    );
    if let Some(display) = gtk::gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}

fn build_menu_model() -> gio::Menu {
    let menu = gio::Menu::new();

    let file = gio::Menu::new();
    file.append(Some("Quick Connect…"), Some("app.connect-sftp"));
    file.append(Some("Pumpkin Patch"), Some("app.go-pumpkin-patch"));
    file.append(Some("Disconnect to Local"), Some("app.disconnect-local"));
    file.append(Some("New Folder…"), Some("app.new-folder"));
    file.append(Some("Rename…"), Some("app.rename"));
    file.append(Some("Delete…"), Some("app.delete"));
    file.append(Some("Close Window"), Some("app.close-window"));
    file.append(Some("Quit Cyber-Pumpkin"), Some("app.quit"));
    menu.append_submenu(Some("File"), &file);

    let edit = gio::Menu::new();
    edit.append(Some("Rename…"), Some("app.rename"));
    edit.append(Some("Delete…"), Some("app.delete"));
    edit.append(Some("Preferences…"), Some("app.preferences"));
    menu.append_submenu(Some("Edit"), &edit);

    let view = gio::Menu::new();
    view.append(Some("Refresh"), Some("app.refresh"));
    view.append(Some("Inspector"), Some("app.info"));
    view.append(Some("Activity"), Some("app.activity"));
    view.append(Some("Sync Files"), Some("app.sync"));
    view.append(Some("Show Hidden Files"), Some("app.hidden"));
    let sort = gio::Menu::new();
    sort.append(Some("Name"), Some("app.sort-name"));
    sort.append(Some("Type"), Some("app.sort-type"));
    sort.append(Some("Size"), Some("app.sort-size"));
    sort.append(Some("Date"), Some("app.sort-date"));
    sort.append(Some("Reverse Order"), Some("app.sort-reverse"));
    view.append_submenu(Some("Sort By"), &sort);
    menu.append_submenu(Some("View"), &view);

    let go = gio::Menu::new();
    go.append(Some("Home"), Some("app.go-home"));
    go.append(Some("Downloads"), Some("app.go-downloads"));
    go.append(Some("Desktop"), Some("app.go-desktop"));
    go.append(Some("Root"), Some("app.go-root"));
    go.append(Some("Pumpkin Patch"), Some("app.go-pumpkin-patch"));
    go.append(Some("Quick Connect"), Some("app.connect-sftp"));
    menu.append_submenu(Some("Go"), &go);

    let transfer = gio::Menu::new();
    transfer.append(Some("Copy to Other Pane"), Some("app.copy-other"));
    transfer.append(Some("Sync Files"), Some("app.sync"));
    transfer.append(Some("Activity"), Some("app.activity"));
    menu.append_submenu(Some("Transfer"), &transfer);

    let window = gio::Menu::new();
    window.append(Some("Inspector"), Some("app.info"));
    window.append(Some("Close Window"), Some("app.close-window"));
    menu.append_submenu(Some("Window"), &window);

    let help = gio::Menu::new();
    help.append(Some("About Cyber-Pumpkin"), Some("app.about"));
    menu.append_submenu(Some("Help"), &help);

    menu
}

fn build_linux_menu_bar() -> gtk::PopoverMenuBar {
    let menu = build_menu_model();
    let bar = gtk::PopoverMenuBar::from_model(Some(&menu));
    bar.set_hexpand(true);
    bar
}

fn build_menu_button() -> gtk::MenuButton {
    let menu = build_menu_model();
    gtk::MenuButton::builder()
        .icon_name("open-menu-symbolic")
        .tooltip_text("Cyber-Pumpkin Menu")
        .menu_model(&menu)
        .build()
}

fn install_actions(app: &adw::Application, context: &ActionContext) {
    install_file_actions(app, context);
    install_view_actions(app, context);
    install_go_actions(app, context);
    install_transfer_action(app, context);

    install_simple_action(app, "preferences", {
        let app = app.clone();
        move || preferences_ui::show_preferences(&app)
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

fn install_file_actions(app: &adw::Application, context: &ActionContext) {
    install_simple_action(app, "connect-sftp", {
        let context = context.clone();
        move || active_workspace(&context).show_quick_connect()
    });
    install_simple_action(app, "disconnect-local", {
        let context = context.clone();
        move || {
            let home = context.home.clone();
            active_workspace(&context).open_local(&home);
        }
    });
    install_simple_action(app, "refresh", {
        let context = context.clone();
        move || active_pane(&context).refresh()
    });
    install_simple_action(app, "new-folder", {
        let context = context.clone();
        move || show_new_folder_dialog(active_pane(&context))
    });
    install_simple_action(app, "rename", {
        let context = context.clone();
        move || show_rename_dialog(active_pane(&context))
    });
    install_simple_action(app, "delete", {
        let context = context.clone();
        move || show_delete_dialog(active_pane(&context))
    });
}

fn install_view_actions(app: &adw::Application, context: &ActionContext) {
    install_simple_action(app, "info", {
        let context = context.clone();
        move || {
            let next = !context.inspector.is_visible();
            context.inspector.set_visible(next);
            context.inspector_button.set_active(next);
        }
    });
    install_simple_action(app, "activity", {
        let popover = context.activity_popover.clone();
        move || popover.popup()
    });
    install_simple_action(app, "sync", {
        let context = context.clone();
        move || show_sync(&context)
    });

    install_toggle_action(app, "hidden", false, {
        let left = context.left.clone();
        let right = context.right.clone();
        move |show| {
            left.set_show_hidden(show);
            right.set_show_hidden(show);
        }
    });

    install_sort_action(app, "sort-name", context, SortMode::Name);
    install_sort_action(app, "sort-type", context, SortMode::Type);
    install_sort_action(app, "sort-size", context, SortMode::Size);
    install_sort_action(app, "sort-date", context, SortMode::Date);

    let reverse = Rc::new(Cell::new(false));
    install_simple_action(app, "sort-reverse", {
        let context = context.clone();
        let reverse = Rc::clone(&reverse);
        move || {
            let next = !reverse.get();
            reverse.set(next);
            active_pane(&context).set_sort_descending(next);
        }
    });
}

fn install_sort_action(
    app: &adw::Application,
    name: &str,
    context: &ActionContext,
    mode: SortMode,
) {
    let context = context.clone();
    install_simple_action(app, name, move || active_pane(&context).set_sort_mode(mode));
}

fn install_go_actions(app: &adw::Application, context: &ActionContext) {
    let home = context.home.clone();
    install_simple_action(app, "go-home", {
        let context = context.clone();
        move || active_workspace(&context).open_local(&home)
    });

    let downloads = Path::new(&context.home)
        .join("Downloads")
        .to_string_lossy()
        .into_owned();
    install_simple_action(app, "go-downloads", {
        let context = context.clone();
        move || active_workspace(&context).open_local(&downloads)
    });

    let desktop = Path::new(&context.home)
        .join("Desktop")
        .to_string_lossy()
        .into_owned();
    install_simple_action(app, "go-desktop", {
        let context = context.clone();
        move || active_workspace(&context).open_local(&desktop)
    });

    install_simple_action(app, "go-root", {
        let context = context.clone();
        move || active_workspace(&context).open_local("/")
    });

    install_simple_action(app, "go-pumpkin-patch", {
        let context = context.clone();
        move || active_workspace(&context).show_pumpkin_patch()
    });
}

fn install_transfer_action(app: &adw::Application, context: &ActionContext) {
    let context = context.clone();
    install_simple_action(app, "copy-other", move || match context.active.get() {
        PaneSide::Left => context.copy_bar.copy_between(&context.left, &context.right),
        PaneSide::Right => context.copy_bar.copy_between(&context.right, &context.left),
    });
}

fn install_window_actions(app: &adw::Application, window: &adw::ApplicationWindow) {
    install_simple_action(app, "close-window", {
        let window = window.clone();
        move || window.close()
    });
    install_simple_action(app, "quit", {
        let app = app.clone();
        move || app.quit()
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
    app.set_accels_for_action("app.connect-sftp", &["<Primary>k"]);
    app.set_accels_for_action("app.preferences", &["<Primary>comma"]);
    app.set_accels_for_action("app.new-folder", &["<Primary><Shift>n"]);
    app.set_accels_for_action("app.rename", &["F2"]);
    app.set_accels_for_action("app.delete", &["Delete"]);
    app.set_accels_for_action("app.info", &["<Primary>i"]);
    app.set_accels_for_action("app.copy-other", &["<Primary><Shift>c"]);
    app.set_accels_for_action("app.hidden", &["<Primary>period"]);
    app.set_accels_for_action("app.close-window", &["<Primary>w"]);
    app.set_accels_for_action("app.quit", &["<Primary>q"]);
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

fn active_pane(context: &ActionContext) -> &PaneHandle {
    match context.active.get() {
        PaneSide::Left => &context.left,
        PaneSide::Right => &context.right,
    }
}

fn active_workspace(context: &ActionContext) -> &PaneWorkspace {
    match context.active.get() {
        PaneSide::Left => &context.left_workspace,
        PaneSide::Right => &context.right_workspace,
    }
}

fn icon_button(icon: &str, tooltip: &str) -> gtk::Button {
    let button = gtk::Button::from_icon_name(icon);
    button.set_tooltip_text(Some(tooltip));
    button
}
