//! Explicit file actions use the operating system's file associations and Trash.
use anyhow::{Context, Result, ensure};
use rivet_core::queue::Queue;
use std::{
    path::Path,
    process::{Command, Stdio},
};

#[derive(Clone)]
pub enum Operation {
    Open,
    Reveal,
    Trash,
    Rename(String),
}
fn run(command: &mut Command) -> Result<()> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    ensure!(
        command.status()?.success(),
        "The operating system could not complete this file action"
    );
    Ok(())
}
fn open(path: &Path) -> Result<()> {
    #[cfg(target_os = "macos")]
    return run(Command::new("/usr/bin/open").arg("--").arg(path));
    #[cfg(windows)]
    return run(Command::new("explorer.exe").arg(path));
    #[cfg(not(any(target_os = "macos", windows)))]
    return run(Command::new("xdg-open").arg(path));
}
fn reveal(path: &Path) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        if path.exists() {
            return run(Command::new("/usr/bin/open").args(["-R", "--"]).arg(path));
        }
    }
    #[cfg(windows)]
    {
        if path.exists() {
            return run(Command::new("explorer.exe").arg("/select,").arg(path));
        }
    }
    // Portable fallback opens the containing directory, including when the file was moved externally.
    open(path.parent().context("No download folder")?)
}
pub fn execute(queue: &Queue, id: &str, operation: Operation) -> Result<String> {
    let job = queue.get(id)?;
    ensure!(
        job.status == "complete",
        "Only completed downloads can be opened or managed as files"
    );
    let path = &job.request.destination;
    match operation {
        Operation::Rename(name) => {
            queue.rename_completed(id, &name)?;
            Ok("File renamed; the download list now uses its new path.".into())
        }
        Operation::Open => {
            ensure!(
                path.is_file(),
                "The file was moved or deleted. Use Show in folder to locate its download folder."
            );
            open(path)?;
            Ok("Opened with your default application.".into())
        }
        Operation::Reveal => {
            reveal(path)?;
            Ok("Opened the download location.".into())
        }
        Operation::Trash => {
            ensure!(
                std::fs::symlink_metadata(path)?.is_file(),
                "The download is missing or is no longer a regular file"
            );
            #[cfg(target_os = "macos")]
            let context = {
                let mut context = trash::TrashContext::default();
                use trash::macos::{DeleteMethod, TrashContextExtMacos};
                context.set_delete_method(DeleteMethod::NsFileManager);
                context
            };
            #[cfg(not(target_os = "macos"))]
            let context = trash::TrashContext::default();
            context.delete(path).context(
                "Could not move this file to Trash; it has not been permanently deleted",
            )?;
            queue.remove(id)?;
            Ok("File moved to Trash and removed from the list.".into())
        }
    }
}
