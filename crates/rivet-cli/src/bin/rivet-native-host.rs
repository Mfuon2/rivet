//! Dedicated binary because native messaging manifests cannot pass subcommands.
use anyhow::Result;
use rivet_core::{browser_setup, native, queue::Queue};

fn main() {
    if let Err(error) = run() {
        eprintln!("Rivet native host: {error}");
        std::process::exit(1);
    }
}
fn run() -> Result<()> {
    let downloads = native::default_download_dir()?;
    let database = Queue::default_path()?;
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    native::serve_with_observer(
        &mut std::io::stdin().lock(),
        &mut std::io::stdout().lock(),
        &Queue::open(&database)?,
        &downloads,
        || {
            let _ = browser_setup::record_connection(&database, &arguments);
        },
    )?;
    Ok(())
}
