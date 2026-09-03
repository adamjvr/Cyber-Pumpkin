//! Cyber-Pumpkin command-line companion.

fn main() {
    let command = std::env::args().nth(1);
    match command.as_deref() {
        Some("about") => println!("Cyber-Pumpkin cpk 0.1.0 — shared-core CLI bootstrap"),
        Some(command) => {
            eprintln!("unknown command: {command}");
            std::process::exit(2);
        }
        None => println!("usage: cpk about"),
    }
}
