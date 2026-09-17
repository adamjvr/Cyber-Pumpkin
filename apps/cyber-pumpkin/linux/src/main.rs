//! Native GTK4/libadwaita frontend for Cyber-Pumpkin on Linux.

mod activity;
mod browser;
mod connection;
mod inspector;
mod pane_workspace;
mod preferences_ui;
mod remote_edit_ui;
mod server_workspace;
mod shell;
mod sync_ui;
mod transfer_ui;

use adw::prelude::*;

fn main() {
    let app = adw::Application::builder()
        .application_id("com.rothamplification.CyberPumpkin")
        .build();
    app.connect_activate(shell::build_ui);
    app.connect_shutdown(|_| remote_edit_ui::shutdown_remote_edits());
    app.run();
}
