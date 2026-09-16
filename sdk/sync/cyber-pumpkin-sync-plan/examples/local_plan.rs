//! Prints a deterministic one-way local synchronization plan.

use cyber_pumpkin_core::{BackendId, BackendPath};
use cyber_pumpkin_local::LocalBackend;
use cyber_pumpkin_sync_plan::{SyncOptions, plan_one_way};
use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args().skip(1);
    let source = args.next().ok_or("missing source directory")?;
    let destination = args.next().ok_or("missing destination directory")?;

    let source_backend = LocalBackend::new(BackendId::new("source")?);
    let destination_backend = LocalBackend::new(BackendId::new("destination")?);
    let plan = plan_one_way(
        &source_backend,
        &BackendPath::new(source)?,
        &destination_backend,
        &BackendPath::new(destination)?,
        &SyncOptions::default(),
    )?;

    for action in plan.actions() {
        println!(
            "{:?}\t{}\t{}",
            action.kind, action.relative_path, action.reason
        );
    }
    Ok(())
}
