//! Public page media downloads through packaged extractors. No browser cookies or DRM bypass.
use crate::{
    CancellationToken, DownloadRequest, DownloadResult, Progress, progress::TransferState,
};
use anyhow::{Context, Result, bail, ensure};
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    process::Stdio,
    sync::atomic::Ordering,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, BufReader},
    process::Command,
};

pub fn youtube_page(raw: &str) -> Result<String> {
    let url = reqwest::Url::parse(raw)?;
    ensure!(
        url.scheme() == "https"
            && url.username().is_empty()
            && url.password().is_none()
            && url.port().is_none(),
        "Use a public HTTPS YouTube video link"
    );
    let id = match url.host_str() {
        Some("youtu.be") => url
            .path_segments()
            .and_then(|mut s| s.next())
            .map(str::to_owned),
        Some("youtube.com" | "www.youtube.com" | "m.youtube.com" | "music.youtube.com") => {
            if url.path() == "/watch" {
                url.query_pairs()
                    .find(|(k, _)| k == "v")
                    .map(|(_, v)| v.into_owned())
            } else {
                let parts: Vec<_> = url.path().split('/').filter(|p| !p.is_empty()).collect();
                if parts.len() == 2 && ["shorts", "embed", "live"].contains(&parts[0]) {
                    Some(parts[1].to_owned())
                } else {
                    None
                }
            }
        }
        _ => None,
    }
    .context("This page is not a supported YouTube video")?;
    ensure!(
        id.len() == 11
            && id
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"_-".contains(&c)),
        "Invalid YouTube video ID"
    );
    Ok(format!("https://www.youtube.com/watch?v={id}"))
}
pub fn tool(name: &str) -> Result<PathBuf> {
    let exe = std::env::current_exe()?;
    let filename = format!("{name}{}", if cfg!(windows) { ".exe" } else { "" });
    let bundled = exe
        .parent()
        .context("No application folder")?
        .join("media")
        .join(&filename);
    if bundled.is_file() {
        return Ok(bundled);
    }
    // Developer builds may use a prepared tool folder; released apps include it next to the binaries.
    if let Some(directory) = std::env::var_os("RIVET_MEDIA_TOOLS") {
        let path = PathBuf::from(directory).join(&filename);
        if path.is_file() {
            return Ok(path);
        }
    }
    bail!("Rivet's media components are missing. Install the complete latest Rivet package.")
}
fn configure(command: &mut Command) {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(unix)]
    command.process_group(0);
    #[cfg(windows)]
    {
        command.creation_flags(0x08000000 | 0x00000200);
    }
}
async fn stop_tree(child: &mut tokio::process::Child) {
    if let Some(id) = child.id() {
        #[cfg(unix)]
        {
            let _ = Command::new("/bin/kill")
                .args(["-KILL", "--", &format!("-{id}")])
                .status()
                .await;
        }
        #[cfg(windows)]
        {
            let _ = Command::new("taskkill.exe")
                .args(["/PID", &id.to_string(), "/T", "/F"])
                .creation_flags(0x08000000)
                .status()
                .await;
        }
    }
    let _ = child.kill().await;
    let _ = child.wait().await;
}
/// Fixed arguments; page URLs never become shell commands, templates, options or plugin paths.
pub fn arguments(
    request: &DownloadRequest,
    work: &Path,
    ffmpeg: &Path,
    qjs: &Path,
) -> Result<Vec<std::ffi::OsString>> {
    let url = youtube_page(&request.url)?;
    let mut args: Vec<std::ffi::OsString> = [
        "--ignore-config",
        "--no-plugin-dirs",
        "--no-remote-components",
        "--no-playlist",
        "--no-warnings",
        "--no-colors",
        "--no-cache-dir",
        "--no-overwrites",
        "--keep-video",
        "--match-filters",
        "!is_live & duration > 0",
        "--continue",
        "--socket-timeout",
        "20",
        "--retries",
        "3",
        "--fragment-retries",
        "3",
        "--newline",
        "--progress",
        "--progress-delta",
        "0.3",
        "--progress-template",
        "download:RIVET:%(progress.downloaded_bytes)s:%(progress.total_bytes)s:%(progress.total_bytes_estimate)s",
        "--print",
        "after_move:RIVET_FILE:%(filepath)s",
        "--format",
        "bv*[vcodec^=avc1]+ba[acodec^=mp4a]/b[ext=mp4]/bv*+ba/b",
        "--merge-output-format",
        "mp4",
        "--remux-video",
        "mp4",
        "--ffmpeg-location",
    ].into_iter().map(Into::into).collect();
    args.push(ffmpeg.as_os_str().to_owned());
    args.push("--no-js-runtimes".into());
    args.push("--js-runtimes".into());
    args.push(format!("quickjs:{}", qjs.display()).into());
    args.push("--output".into());
    args.push(
        format!(
            "{}/video.%(ext)s",
            work.to_str()
                .context("Unsupported destination path")?
                .replace('%', "%%")
        )
        .into(),
    );
    args.push("--".into());
    args.push(url.into());
    Ok(args)
}
pub(crate) fn progress_line(line: &str, progress: &Progress) {
    if let Some(values) = line.strip_prefix("RIVET:") {
        let parts: Vec<_> = values.split(':').collect();
        let number = |i| {
            parts
                .get(i)
                .and_then(|s: &&str| s.parse::<f64>().ok())
                .filter(|v| v.is_finite() && *v >= 0.)
                .map(|v| v as u64)
        };
        if let Some(done) = number(0) {
            progress.completed.store(done, Ordering::Relaxed);
        }
        if let Some(total) = number(1).or_else(|| number(2)) {
            progress.total.store(total, Ordering::Relaxed);
        }
        progress.set_state(TransferState::Receiving);
    } else if line.starts_with("[Merger]") || line.starts_with("[VideoRemuxer]") {
        progress.set_state(TransferState::Assembling);
    }
}
pub async fn download(
    request: DownloadRequest,
    cancel: CancellationToken,
    progress: Progress,
) -> Result<DownloadResult> {
    youtube_page(&request.url)?;
    ensure!(
        request.expected_sha256.is_none(),
        "An expected checksum cannot be applied before resolving a media page"
    );
    let parent = request
        .destination
        .parent()
        .context("No media destination folder")?;
    tokio::fs::create_dir_all(parent).await?;
    ensure!(
        !request.destination.exists(),
        "Destination already exists; choose another filename"
    );
    let workspace = parent.join(format!(
        ".{}.rivet-media",
        request
            .destination
            .file_name()
            .context("Missing filename")?
            .to_string_lossy()
    ));
    tokio::fs::create_dir_all(&workspace).await?;
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(workspace.join("download.lock"))?;
    fs2::FileExt::try_lock_exclusive(&lock).context("This media download is already running")?;
    let source = youtube_page(&request.url)?;
    let source_file = workspace.join("source.json");
    if source_file.exists() {
        ensure!(
            std::fs::read_to_string(&source_file)? == source,
            "Partial streams belong to another video"
        );
    } else {
        std::fs::write(&source_file, &source)?;
    }
    // Rebuild a final MP4 from retained source streams after an interrupted merge.
    let _ = tokio::fs::remove_file(workspace.join("video.mp4")).await;
    let ytdlp = tool("yt-dlp")?;
    let ffmpeg = tool("ffmpeg")?;
    let qjs = tool("qjs")?;
    let mut command = Command::new(ytdlp);
    configure(&mut command);
    command.args(arguments(&request, &workspace, &ffmpeg, &qjs)?);
    progress.initialize(Vec::new(), true);
    progress.set_state(TransferState::Connecting);
    let mut child = command
        .spawn()
        .context("Could not start Rivet's media resolver")?;
    let stderr = child.stderr.take().unwrap();
    let error_progress = progress.clone();
    let errors = tokio::spawn(async move {
        let mut lines = BufReader::new(stderr).lines();
        let mut bytes = Vec::new();
        while let Ok(Some(line)) = lines.next_line().await {
            progress_line(&line, &error_progress);
            let retain = line.len().min(65536usize.saturating_sub(bytes.len()));
            bytes.extend_from_slice(&line.as_bytes()[..retain]);
        }
        bytes
    });
    let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
    let mut output = None;
    loop {
        let line = tokio::select! {
            _=cancel.cancelled()=>{stop_tree(&mut child).await;errors.abort();progress.finish(TransferState::Paused);bail!("Media download paused; resume to continue available partial streams");},
            line=lines.next_line()=>line?
        };
        let Some(line) = line else {
            break;
        };
        if line.len() > 16384 {
            continue;
        }
        if let Some(path) = line.strip_prefix("RIVET_FILE:") {
            output = Some(PathBuf::from(path));
        }
        progress_line(&line, &progress);
    }
    let status = tokio::select! {_ = cancel.cancelled()=>{stop_tree(&mut child).await;bail!("Media download paused");},status=child.wait()=>status?};
    let diagnostics = errors.await.unwrap_or_default();
    if !status.success() {
        progress.finish(TransferState::Failed);
        let text = String::from_utf8_lossy(&diagnostics).to_lowercase();
        if text.contains("drm") {
            bail!("This media is DRM-protected and cannot be downloaded by Rivet");
        }
        if text.contains("sign in") || text.contains("login") || text.contains("private video") {
            bail!(
                "This video requires authentication or is restricted. Rivet does not read browser cookies or bypass access restrictions."
            );
        }
        bail!(
            "The media service could not provide a downloadable public video. Retry or update Rivet's media components."
        );
    }
    let output = output
        .context("Media resolver did not produce a completed file")?
        .canonicalize()?;
    ensure!(
        output.parent() == Some(workspace.canonicalize()?.as_path())
            && output.file_name().and_then(|p| p.to_str()) == Some("video.mp4"),
        "Unexpected media output path"
    );
    let mut probe_command = Command::new(tool("ffprobe")?);
    configure(&mut probe_command);
    let probe = probe_command
        .args([
            "-v",
            "error",
            "-show_entries",
            "stream=codec_type",
            "-of",
            "json",
        ])
        .arg(&output)
        .output()
        .await?;
    ensure!(
        probe.status.success(),
        "The downloaded MP4 could not be validated"
    );
    let tracks: serde_json::Value = serde_json::from_slice(&probe.stdout)?;
    ensure!(
        tracks["streams"]
            .as_array()
            .is_some_and(|s| s.iter().any(|v| v["codec_type"] == "video")),
        "The download contains no video track"
    );
    let metadata = tokio::fs::metadata(&output).await?;
    ensure!(metadata.len() > 0, "The downloaded media file is empty");
    progress.set_state(TransferState::Verifying);
    let mut file = tokio::fs::File::open(&output).await?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        let n = file.read(&mut buffer).await?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    drop(file);
    // A hard-link atomically publishes without replacing an existing user file on the same filesystem.
    tokio::fs::hard_link(&output, &request.destination)
        .await
        .context("Could not publish media file without overwriting the destination")?;
    progress.completed.store(metadata.len(), Ordering::Relaxed);
    progress.total.store(metadata.len(), Ordering::Relaxed);
    progress.finish(TransferState::Complete);
    drop(lock);
    let _ = tokio::fs::remove_dir_all(&workspace).await;
    Ok(DownloadResult {
        bytes: metadata.len(),
        sha256: format!("{:x}", hash.finalize()),
        destination: request.destination,
    })
}
