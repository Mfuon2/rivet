//! Coordinate the desktop's presence without touching unrelated processes or user profiles.
use anyhow::{Context, Result, ensure};
use fs2::FileExt;
use std::{
    fs::{File, OpenOptions},
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};
fn lock(database: &Path, name: &str) -> Result<File> {
    let parent = database.parent().context("Missing app data folder")?;
    std::fs::create_dir_all(parent)?;
    Ok(OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(parent.join(name))?)
}
pub fn presence(database: &Path) -> Result<File> {
    let file = lock(database, "desktop.lock")?;
    let start = Instant::now();
    while let Err(error) = file.try_lock_exclusive() {
        if std::env::var("RIVET_SETUP_RECENT_RESTART").as_deref() != Ok("1")
            || start.elapsed() > Duration::from_secs(5)
        {
            return Err(error).context("Rivet is already open for this download queue");
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Ok(file)
}
fn running(database: &Path) -> Result<bool> {
    let file = lock(database, "desktop.lock")?;
    Ok(file.try_lock_exclusive().is_err())
}
pub fn ensure_running(database: &Path) -> Result<()> {
    let launch = lock(database, "desktop-launch.lock")?;
    launch.lock_exclusive()?;
    if running(database)? {
        return Ok(());
    }
    ensure!(
        std::env::var_os("RIVET_SETUP_ROOT").is_none(),
        "Open the isolated Rivet app before sending a test download"
    );
    let exe = std::env::current_exe()?.with_file_name(if cfg!(windows) {
        "rivet-desktop.exe"
    } else {
        "rivet-desktop"
    });
    ensure!(
        exe.is_file(),
        "Open or reinstall the complete Rivet desktop application"
    );
    let mut command = Command::new(exe);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    command.spawn()?;
    let start = Instant::now();
    while !running(database)? {
        ensure!(
            start.elapsed() < Duration::from_secs(15),
            "The video is queued, but Rivet could not open. Open the desktop app to start it."
        );
        std::thread::sleep(Duration::from_millis(100));
    }
    Ok(())
}
