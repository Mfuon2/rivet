//! Browser native messaging: bounded little-endian JSON frames; captures queue, explicit download actions start the desktop.
use crate::{DownloadRequest, queue::Queue};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    io::{Read, Write},
    path::Path,
};

pub const MAX_MESSAGE: usize = 64 * 1024;

pub fn default_download_dir() -> Result<std::path::PathBuf> {
    if let Some(path) = std::env::var_os("RIVET_DOWNLOAD_DIR") {
        let path = std::path::PathBuf::from(path);
        ensure!(
            path.is_absolute(),
            "RIVET_DOWNLOAD_DIR must be an absolute path"
        );
        return Ok(path);
    }
    directories::UserDirs::new()
        .and_then(|d| d.download_dir().map(|p| p.join("Rivet")))
        .context("cannot locate Downloads directory")
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Capture {
    pub url: String,
}

#[derive(Serialize)]
pub struct CaptureReply {
    pub ok: bool,
    pub id: Option<String>,
    pub error: Option<String>,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "lowercase", deny_unknown_fields)]
pub enum Control {
    Ping {},
    Media { url: String },
    Download { url: String },
}

#[derive(Deserialize)]
#[serde(untagged)]
pub enum NativeMessage {
    Capture(Capture),
    Control(Control),
}

pub fn read_frame(reader: &mut impl Read) -> Result<Option<NativeMessage>> {
    let mut length = [0; 4];
    // EOF is valid only between complete frames.
    if reader.read(&mut length[..1])? == 0 {
        return Ok(None);
    }
    reader
        .read_exact(&mut length[1..])
        .context("truncated native message header")?;
    let size = u32::from_le_bytes(length) as usize;
    ensure!(
        size > 0 && size <= MAX_MESSAGE,
        "native message exceeds bounds"
    );
    let mut data = vec![0; size];
    reader
        .read_exact(&mut data)
        .context("truncated native message")?;
    Ok(Some(serde_json::from_slice(&data)?))
}

pub fn write_frame(writer: &mut impl Write, reply: &CaptureReply) -> Result<()> {
    let data = serde_json::to_vec(reply)?;
    writer.write_all(&(data.len() as u32).to_le_bytes())?;
    writer.write_all(&data)?;
    writer.flush()?;
    Ok(())
}

pub fn serve(
    reader: &mut impl Read,
    writer: &mut impl Write,
    queue: &Queue,
    downloads: &Path,
) -> Result<()> {
    serve_with_observer(reader, writer, queue, downloads, || {})
}

pub fn serve_with_observer(
    reader: &mut impl Read,
    writer: &mut impl Write,
    queue: &Queue,
    downloads: &Path,
    mut connected: impl FnMut(),
) -> Result<()> {
    serve_with_actions(reader, writer, queue, downloads, &mut connected, || Ok(()))
}
pub fn serve_with_actions(
    reader: &mut impl Read,
    writer: &mut impl Write,
    queue: &Queue,
    downloads: &Path,
    mut connected: impl FnMut(),
    mut start_media: impl FnMut() -> Result<()>,
) -> Result<()> {
    while let Some(message) = read_frame(reader)? {
        let mut automatic = false;
        let capture = match message {
            NativeMessage::Control(Control::Download { url }) => {
                automatic = true;
                Capture { url }
            }
            NativeMessage::Control(Control::Media { url }) => {
                let result = (|| {
                    let canonical = crate::media::youtube_page(&url)?;
                    let id = canonical.split("v=").nth(1).unwrap();
                    std::fs::create_dir_all(downloads)?;
                    let destination = downloads.join(format!(
                        "video-{id}-{}.mp4",
                        &uuid::Uuid::new_v4().simple().to_string()[..8]
                    ));
                    start_media()?;
                    let job = queue.enqueue_media(DownloadRequest::new(canonical, destination))?;
                    Ok::<_, anyhow::Error>(job)
                })();
                let reply = match result {
                    Ok(id) => CaptureReply {
                        ok: true,
                        id: Some(id),
                        error: None,
                    },
                    Err(error) => CaptureReply {
                        ok: false,
                        id: None,
                        error: Some(error.to_string()),
                    },
                };
                write_frame(writer, &reply)?;
                if reply.ok {
                    connected();
                }
                continue;
            }
            NativeMessage::Control(Control::Ping {}) => {
                write_frame(
                    writer,
                    &CaptureReply {
                        ok: true,
                        id: None,
                        error: None,
                    },
                )?;
                connected();
                continue;
            }
            NativeMessage::Capture(capture) => capture,
        };
        let result = (|| {
            std::fs::create_dir_all(downloads)?;
            let url = reqwest::Url::parse(&capture.url)?;
            let suffix = uuid::Uuid::new_v4().simple().to_string();
            let name = url
                .path_segments()
                .and_then(|mut s| s.next_back())
                .filter(|s| !s.is_empty())
                .unwrap_or("download.bin");
            // Never accept filesystem paths, control characters, or browser-supplied output directories.
            let name: String = name
                .chars()
                .take(100)
                .map(|c| {
                    if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                        c
                    } else {
                        '_'
                    }
                })
                .collect();
            let name = name.trim_start_matches('.');
            let destination = downloads.join(format!(
                "{}-{}",
                &suffix[..8],
                if name.is_empty() {
                    "download.bin"
                } else {
                    name
                }
            ));
            let request = DownloadRequest::new(capture.url, destination);
            request.validate()?;
            if automatic {
                start_media()?;
                queue.enqueue_download(request)
            } else {
                queue.enqueue(request, 0)
            }
        })();
        let reply = match result {
            Ok(id) => CaptureReply {
                ok: true,
                id: Some(id),
                error: None,
            },
            Err(error) => CaptureReply {
                ok: false,
                id: None,
                error: Some(error.to_string()),
            },
        };
        write_frame(writer, &reply)?;
        if reply.ok {
            connected();
        }
    }
    Ok(())
}
