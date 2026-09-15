use crate::browser::{PaneHandle, PaneSide};
use crate::server_workspace::{self, PumpkinPatchPanel};
use gtk::Orientation;
use gtk::prelude::*;
use std::cell::Cell;
use std::path::Path;
use std::rc::Rc;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PaneMode {
    Browser,
    PumpkinPatch,
    QuickConnect,
}

#[derive(Clone)]
pub(crate) struct PaneWorkspace {
    pub(crate) root: gtk::Box,
    pane: PaneHandle,
    stack: gtk::Stack,
    mode: Rc<Cell<PaneMode>>,
    pumpkin_patch: Rc<PumpkinPatchPanel>,
    side: PaneSide,
    active: Rc<Cell<PaneSide>>,
    observer: Rc<dyn Fn(PaneSide, PaneMode)>,
}

impl PaneWorkspace {
    pub(crate) fn new(
        home: &str,
        pane: PaneHandle,
        side: PaneSide,
        active: &Rc<Cell<PaneSide>>,
        observer: Rc<dyn Fn(PaneSide, PaneMode)>,
    ) -> Self {
        let root = gtk::Box::new(Orientation::Vertical, 0);
        let stack = gtk::Stack::new();
        stack.set_hexpand(true);
        stack.set_vexpand(true);
        stack.set_transition_type(gtk::StackTransitionType::Crossfade);

        let mode = Rc::new(Cell::new(PaneMode::Browser));
        let active_cell = Rc::clone(active);
        let stack_for_browser = stack.clone();
        let mode_for_browser = Rc::clone(&mode);
        let observer_for_browser = Rc::clone(&observer);
        let on_browser: Rc<dyn Fn()> = Rc::new(move || {
            active_cell.set(side);
            mode_for_browser.set(PaneMode::Browser);
            stack_for_browser.set_visible_child_name("browser");
            observer_for_browser(side, PaneMode::Browser);
        });

        let active_for_quick = Rc::clone(active);
        let stack_for_quick = stack.clone();
        let mode_for_quick = Rc::clone(&mode);
        let observer_for_quick = Rc::clone(&observer);
        let on_quick_connect: Rc<dyn Fn()> = Rc::new(move || {
            active_for_quick.set(side);
            mode_for_quick.set(PaneMode::QuickConnect);
            stack_for_quick.set_visible_child_name("quick-connect");
            observer_for_quick(side, PaneMode::QuickConnect);
        });

        let pumpkin_patch =
            PumpkinPatchPanel::new(&pane, Rc::clone(&on_browser), &on_quick_connect);
        let quick_connect = server_workspace::build_quick_connect_panel(
            &pane,
            &pumpkin_patch,
            Rc::clone(&on_browser),
        );

        stack.add_named(&pane.root, Some("browser"));
        stack.add_named(&pumpkin_patch.root, Some("pumpkin-patch"));
        stack.add_named(&quick_connect, Some("quick-connect"));

        let workspace = Self {
            root,
            pane,
            stack,
            mode,
            pumpkin_patch,
            side,
            active: Rc::clone(active),
            observer,
        };

        workspace.root.append(&workspace.build_location_strip(home));
        workspace.root.append(&workspace.stack);
        workspace
    }

    pub(crate) fn mode(&self) -> PaneMode {
        self.mode.get()
    }

    pub(crate) fn set_search_query(&self, query: &str) {
        match self.mode.get() {
            PaneMode::Browser => self.pane.set_filter_query(query),
            PaneMode::PumpkinPatch => self.pumpkin_patch.set_filter_query(query),
            PaneMode::QuickConnect => {}
        }
    }

    pub(crate) fn show_pumpkin_patch(&self) {
        self.pumpkin_patch.refresh();
        self.switch_mode(PaneMode::PumpkinPatch);
    }

    pub(crate) fn show_quick_connect(&self) {
        self.switch_mode(PaneMode::QuickConnect);
    }

    pub(crate) fn open_local(&self, path: &str) {
        self.active.set(self.side);
        match self.pane.disconnect_to_local(path) {
            Ok(()) => self.switch_mode(PaneMode::Browser),
            Err(error) => eprintln!("failed to open local location: {error}"),
        }
    }

    fn switch_mode(&self, mode: PaneMode) {
        self.active.set(self.side);
        self.mode.set(mode);
        self.stack.set_visible_child_name(match mode {
            PaneMode::Browser => "browser",
            PaneMode::PumpkinPatch => "pumpkin-patch",
            PaneMode::QuickConnect => "quick-connect",
        });
        (self.observer)(self.side, mode);
    }

    fn build_location_strip(&self, home: &str) -> gtk::Box {
        let strip = gtk::Box::new(Orientation::Horizontal, 3);
        strip.add_css_class("pane-location-strip");
        strip.set_margin_top(3);
        strip.set_margin_bottom(3);
        strip.set_margin_start(6);
        strip.set_margin_end(6);

        let home_path = Path::new(home).to_path_buf();
        let locations = [
            ("user-home-symbolic", "Home", home_path.clone()),
            (
                "folder-download-symbolic",
                "Downloads",
                home_path.join("Downloads"),
            ),
            (
                "user-desktop-symbolic",
                "Desktop",
                home_path.join("Desktop"),
            ),
        ];

        for (icon, label, path) in locations {
            let button = shortcut_button(icon, label);
            let workspace = self.clone();
            let text = path.to_string_lossy().into_owned();
            button.connect_clicked(move |_| workspace.open_local(&text));
            strip.append(&button);
        }

        let spacer = gtk::Box::new(Orientation::Horizontal, 0);
        spacer.set_hexpand(true);
        strip.append(&spacer);

        let patch = gtk::Button::with_label("Pumpkin Patch");
        patch.add_css_class("flat");
        patch.set_tooltip_text(Some("Saved connections"));
        {
            let workspace = self.clone();
            patch.connect_clicked(move |_| workspace.show_pumpkin_patch());
        }
        strip.append(&patch);

        let quick = gtk::Button::with_label("Quick Connect");
        quick.add_css_class("flat");
        quick.set_tooltip_text(Some("Open a direct connection in this pane"));
        {
            let workspace = self.clone();
            quick.connect_clicked(move |_| workspace.show_quick_connect());
        }
        strip.append(&quick);

        strip
    }
}

fn shortcut_button(icon: &str, label: &str) -> gtk::Button {
    let button = gtk::Button::new();
    button.add_css_class("flat");
    button.set_tooltip_text(Some(label));

    let content = gtk::Box::new(Orientation::Horizontal, 4);
    let image = gtk::Image::from_icon_name(icon);
    image.set_pixel_size(14);
    let label = gtk::Label::new(Some(label));
    content.append(&image);
    content.append(&label);
    button.set_child(Some(&content));
    button
}
