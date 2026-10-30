//! Portable filenames and atomic duplicate handling without overwriting existing files.
use anyhow::{Result, ensure};
use std::path::{Path, PathBuf};

pub fn safe_name(raw: &str) -> String {
    let mut name = String::new();
    for c in raw.chars() {
        let c = if c.is_control() || "<>:\"/\\|?*".contains(c) {
            '_'
        } else {
            c
        };
        if name.len() + c.len_utf8() > 180 {
            break;
        }
        name.push(c);
    }
    let mut name = name
        .trim()
        .trim_start_matches('.')
        .trim_end_matches(['.', ' '])
        .to_owned();
    if name.is_empty() || name == "." || name == ".." {
        name = "download".into();
    }
    let stem = name.split('.').next().unwrap_or("").to_ascii_uppercase();
    let device = ["CON", "PRN", "AUX", "NUL"].contains(&stem.as_str())
        || (stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && matches!(stem.as_bytes()[3], b'1'..=b'9'));
    if device {
        name.insert(0, '_');
    }
    name
}
pub fn numbered(parent: &Path, name: &str, index: usize) -> PathBuf {
    if index == 1 {
        return parent.join(name);
    }
    let path = Path::new(name);
    let stem = path.file_stem().unwrap_or_default().to_string_lossy();
    let extension = path
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    parent.join(format!("{stem} ({index}){extension}"))
}
pub fn publish(source: &Path, parent: &Path, name: &str) -> Result<PathBuf> {
    ensure!(safe_name(name) == name, "Unsafe filename");
    for index in 1..=10000 {
        let destination = numbered(parent, name, index);
        match std::fs::hard_link(source, &destination) {
            Ok(()) => return Ok(destination),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        }
    }
    anyhow::bail!("Too many files with this name; choose another name")
}
