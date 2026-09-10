//! Native GTK4/libadwaita frontend for Cyber-Pumpkin on Linux.

mod browser;
mod shell;
mod transfer_ui;

use adw::prelude::*;

fn main() {
    let app = adw::Application::builder()
        .application_id("com.rothamplification.CyberPumpkin")
        .build();
    app.connect_activate(shell::build_ui);
    app.run();
}
