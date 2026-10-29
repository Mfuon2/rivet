//! HTTP transfers with independently validated ranges and conservative resume.
use anyhow::{Context, Result, bail, ensure};
use fs2::FileExt;
use futures_util::StreamExt;
use reqwest::{Client, StatusCode, Url, header};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};
use tokio::{
    fs::{self, File, OpenOptions},
    io::{AsyncReadExt, AsyncWriteExt},
    task::JoinSet,
};
use tokio_util::sync::CancellationToken;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DownloadRequest {
    pub url: String,
    pub destination: PathBuf,
    pub connections: u8,
    pub expected_sha256: Option<String>,
}

impl DownloadRequest {
    pub fn new(url: String, destination: PathBuf) -> Self {
        Self {
            url,
            destination,
            connections: 4,
            expected_sha256: None,
        }
    }
    pub fn validate(&self) -> Result<()> {
        let url = Url::parse(&self.url).context("invalid URL")?;
        ensure!(
            matches!(url.scheme(), "http" | "https"),
            "only HTTP and HTTPS URLs are supported"
        );
        ensure!(
            url.username().is_empty() && url.password().is_none(),
            "URL credentials are not accepted; use a public URL or a signed link"
        );
        ensure!(
            (1..=16).contains(&self.connections),
            "connections must be between 1 and 16"
        );
        ensure!(
            self.destination.file_name().is_some(),
            "destination must name a file"
        );
        if let Some(hash) = &self.expected_sha256 {
            ensure!(
                hash.len() == 64 && hash.bytes().all(|c| c.is_ascii_hexdigit()),
                "SHA-256 must contain 64 hexadecimal characters"
            );
        }
        Ok(())
    }
}

#[derive(Clone, Default)]
pub struct Progress {
    pub completed: Arc<AtomicU64>,
    pub total: Arc<AtomicU64>,
}
impl Progress {
    pub fn snapshot(&self) -> (u64, u64) {
        (
            self.completed.load(Ordering::Relaxed),
            self.total.load(Ordering::Relaxed),
        )
    }
}

#[derive(Debug, Serialize)]
pub struct DownloadResult {
    pub bytes: u64,
    pub sha256: String,
    pub destination: PathBuf,
}

#[derive(Clone)]
pub struct Engine {
    client: Client,
    retries: u8,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Remote {
    url: String,
    total: Option<u64>,
    // Only strong ETags authorize combining bytes across requests/restarts.
    etag: Option<String>,
    ranges: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Segment {
    start: u64,
    end: u64,
}
#[derive(Serialize, Deserialize)]
struct Manifest {
    version: u32,
    url: String,
    remote: Remote,
    segments: Vec<Segment>,
}

impl Engine {
    pub fn new(proxy: Option<&str>) -> Result<Self> {
        let mut builder = Client::builder()
            .user_agent(concat!("Rivet/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(Duration::from_secs(15))
            .read_timeout(Duration::from_secs(30))
            .pool_max_idle_per_host(16)
            .tcp_keepalive(Duration::from_secs(30))
            .no_gzip()
            .no_brotli()
            .no_deflate()
            .no_zstd()
            .referer(false)
            .redirect(reqwest::redirect::Policy::custom(|attempt| {
                if attempt.previous().len() >= 10 {
                    return attempt.error("too many redirects");
                }
                if attempt.previous().iter().any(|u| u.scheme() == "https")
                    && attempt.url().scheme() != "https"
                {
                    return attempt.error("HTTPS downgrade refused");
                }
                attempt.follow()
            }));
        if let Some(proxy) = proxy {
            builder = builder.proxy(reqwest::Proxy::all(proxy)?);
        }
        Ok(Self {
            client: builder.build()?,
            retries: 3,
        })
    }

    async fn probe(&self, url: &str, cancel: &CancellationToken) -> Result<Remote> {
        let response = tokio::select! {
            _ = cancel.cancelled() => bail!("download paused"),
            response = self.client.get(url).header(header::RANGE, "bytes=0-0").header(header::ACCEPT_ENCODING, "identity").send() => response?,
        };
        let status = response.status();
        // Empty resources may reply 416 to a 0-0 probe; fetch them normally.
        if status == StatusCode::RANGE_NOT_SATISFIABLE
            && response
                .headers()
                .get(header::CONTENT_RANGE)
                .and_then(|v| v.to_str().ok())
                == Some("bytes */0")
        {
            return Ok(Remote {
                url: response.url().to_string(),
                total: Some(0),
                etag: None,
                ranges: false,
            });
        }
        ensure!(
            status == StatusCode::OK || status == StatusCode::PARTIAL_CONTENT,
            "probe returned HTTP {}",
            status.as_u16()
        );
        identity_encoding(&response)?;
        let ranges = status == StatusCode::PARTIAL_CONTENT;
        let total = if ranges {
            let (start, end, total) = content_range(&response)?;
            ensure!(
                start == 0 && end == 0,
                "server returned an invalid probe range"
            );
            Some(total)
        } else {
            response.content_length()
        };
        let etag = response
            .headers()
            .get(header::ETAG)
            .and_then(|v| v.to_str().ok())
            .filter(|s| s.starts_with('"') && s.ends_with('"'))
            .map(str::to_owned);
        Ok(Remote {
            url: response.url().to_string(),
            total,
            etag,
            ranges,
        })
    }

    pub async fn download(
        &self,
        request: DownloadRequest,
        cancel: CancellationToken,
        progress: Progress,
    ) -> Result<DownloadResult> {
        request.validate()?;
        ensure!(
            !request.destination.try_exists()?,
            "destination already exists; Rivet never overwrites it"
        );
        let parent = request
            .destination
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        ensure!(parent.is_dir(), "destination directory does not exist");
        let mut work_name = request.destination.as_os_str().to_os_string();
        work_name.push(".rivet");
        let work = PathBuf::from(work_name);
        fs::create_dir_all(&work).await?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&work, std::fs::Permissions::from_mode(0o700)).await?;
        }
        let lock = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(work.join("lock"))?;
        FileExt::try_lock_exclusive(&lock)
            .context("another Rivet process owns this destination")?;
        progress.completed.store(0, Ordering::Relaxed);
        let remote = self.probe(&request.url, &cancel).await?;
        progress
            .total
            .store(remote.total.unwrap_or(0), Ordering::Relaxed);
        let resumable =
            remote.ranges && remote.etag.is_some() && remote.total.is_some_and(|n| n > 0);
        let segments = if resumable {
            plan(remote.total.unwrap(), request.connections)
        } else {
            vec![]
        };
        let manifest_path = work.join("manifest.json");
        if manifest_path.try_exists()? {
            let saved: Manifest = serde_json::from_slice(&fs::read(&manifest_path).await?)
                .context("invalid resume manifest")?;
            ensure!(
                saved.version == 1 && saved.url == request.url && saved.remote == remote,
                "remote resource or URL changed; keep the partial data for inspection or remove the .rivet directory to restart"
            );
            // Preserve the original segment plan even if the connection option changes.
            if resumable {
                validate_plan(&saved.segments, remote.total.unwrap())?;
                return self
                    .transfer(
                        request,
                        remote,
                        saved.segments,
                        work,
                        lock,
                        cancel,
                        progress,
                    )
                    .await;
            }
        } else {
            let manifest = Manifest {
                version: 1,
                url: request.url.clone(),
                remote: remote.clone(),
                segments: segments.clone(),
            };
            let mut file = OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&manifest_path)
                .await?;
            file.write_all(&serde_json::to_vec_pretty(&manifest)?)
                .await?;
            file.sync_all().await?;
        }
        self.transfer(request, remote, segments, work, lock, cancel, progress)
            .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn transfer(
        &self,
        request: DownloadRequest,
        remote: Remote,
        segments: Vec<Segment>,
        work: PathBuf,
        _lock: std::fs::File,
        cancel: CancellationToken,
        progress: Progress,
    ) -> Result<DownloadResult> {
        let assembled = work.join("assembled");
        if segments.is_empty() {
            // No strong validator: restart a single stream rather than splice representations.
            self.single(&request.url, &remote, &assembled, &cancel, &progress)
                .await?;
        } else {
            let children = cancel.child_token();
            let mut tasks = JoinSet::new();
            let mut pending = std::collections::VecDeque::new();
            for (index, segment) in segments.iter().enumerate() {
                let path = work.join(format!("part-{index:02}"));
                let existing = match fs::metadata(&path).await {
                    Ok(m) => m.len(),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => 0,
                    Err(e) => return Err(e.into()),
                };
                ensure!(
                    existing <= segment.end - segment.start + 1,
                    "partial segment is larger than expected"
                );
                progress.completed.fetch_add(existing, Ordering::Relaxed);
                if existing < segment.end - segment.start + 1 {
                    pending.push_back((path, segment.clone()));
                }
            }
            let launch = |tasks: &mut JoinSet<Result<()>>, (path, segment): (PathBuf, Segment)| {
                let (engine, remote, token, progress) = (
                    self.clone(),
                    remote.clone(),
                    children.clone(),
                    progress.clone(),
                );
                tasks.spawn(async move {
                    engine
                        .segment(&remote, &segment, &path, &token, &progress)
                        .await
                });
            };
            // Workers take another range as soon as they finish: fast connections do not
            // sit idle behind a single slow static partition. All clients share one pool.
            while tasks.len() < usize::from(request.connections) {
                let Some(part) = pending.pop_front() else {
                    break;
                };
                launch(&mut tasks, part);
            }
            let mut error = None;
            while let Some(result) = tasks.join_next().await {
                if let Err(e) = result.context("transfer worker failed").and_then(|r| r)
                    && error.is_none()
                {
                    error = Some(e);
                    children.cancel();
                }
                if error.is_none()
                    && !cancel.is_cancelled()
                    && let Some(part) = pending.pop_front()
                {
                    launch(&mut tasks, part);
                }
            }
            if let Some(error) = error {
                return Err(error);
            }
            ensure!(!cancel.is_cancelled(), "download paused");
            let mut output = File::create(&assembled).await?;
            for index in 0..segments.len() {
                let mut input = File::open(work.join(format!("part-{index:02}"))).await?;
                tokio::io::copy(&mut input, &mut output).await?;
            }
            output.sync_all().await?;
        }
        ensure!(!cancel.is_cancelled(), "download paused");
        let bytes = fs::metadata(&assembled).await?.len();
        if let Some(total) = remote.total {
            ensure!(bytes == total, "download length mismatch");
        }
        let mut file = File::open(&assembled).await?;
        let mut hasher = Sha256::new();
        let mut buffer = vec![0; 256 * 1024];
        loop {
            ensure!(!cancel.is_cancelled(), "download paused");
            let n = file.read(&mut buffer).await?;
            if n == 0 {
                break;
            }
            hasher.update(&buffer[..n]);
        }
        drop(file);
        let sha256 = format!("{:x}", hasher.finalize());
        if let Some(expected) = request.expected_sha256 {
            ensure!(
                sha256.eq_ignore_ascii_case(&expected),
                "SHA-256 mismatch; output was not published"
            );
        }
        // Same-filesystem hard link publishes atomically and cannot replace an existing file.
        fs::hard_link(&assembled, &request.destination)
            .await
            .context("could not publish output without overwriting; partial files retained")?;
        // Keep lock/manifest so another process never locks a replaced inode. Parts can be reclaimed.
        for index in 0..segments.len() {
            let _ = fs::remove_file(work.join(format!("part-{index:02}"))).await;
        }
        let _ = fs::remove_file(&assembled).await;
        Ok(DownloadResult {
            bytes,
            sha256,
            destination: request.destination,
        })
    }

    async fn segment(
        &self,
        remote: &Remote,
        segment: &Segment,
        path: &Path,
        cancel: &CancellationToken,
        progress: &Progress,
    ) -> Result<()> {
        for attempt in 0..=self.retries {
            let result = self
                .segment_attempt(remote, segment, path, cancel, progress)
                .await;
            match result {
                Ok(()) => return Ok(()),
                Err(e) if attempt == self.retries || cancel.is_cancelled() => return Err(e),
                Err(_) => {
                    tokio::select! { _ = cancel.cancelled() => bail!("download paused"), _ = tokio::time::sleep(Duration::from_millis(250 * (1 << attempt))) => {} }
                }
            }
        }
        unreachable!()
    }

    async fn segment_attempt(
        &self,
        remote: &Remote,
        segment: &Segment,
        path: &Path,
        cancel: &CancellationToken,
        progress: &Progress,
    ) -> Result<()> {
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .await?;
        let existing = file.metadata().await?.len();
        let size = segment.end - segment.start + 1;
        ensure!(existing <= size, "invalid partial segment length");
        if existing == size {
            return Ok(());
        }
        let start = segment.start + existing;
        let mut response = tokio::select! {
            _ = cancel.cancelled() => bail!("download paused"),
            r = self.client.get(&remote.url).header(header::RANGE, format!("bytes={start}-{}", segment.end))
                .header(header::IF_RANGE, remote.etag.as_deref().unwrap()).header(header::ACCEPT_ENCODING, "identity").send() => r?,
        };
        ensure!(
            response.status() == StatusCode::PARTIAL_CONTENT,
            "server refused a validated range (HTTP {})",
            response.status().as_u16()
        );
        identity_encoding(&response)?;
        let (actual_start, actual_end, total) = content_range(&response)?;
        ensure!(
            actual_start == start && actual_end == segment.end && Some(total) == remote.total,
            "server returned a mismatched range"
        );
        ensure!(
            response.url().as_str() == remote.url,
            "range redirect changed the resource"
        );
        ensure!(
            response
                .headers()
                .get(header::ETAG)
                .and_then(|v| v.to_str().ok())
                == remote.etag.as_deref(),
            "range ETag changed or disappeared"
        );
        let mut remaining = size - existing;
        if let Some(length) = response.content_length() {
            ensure!(
                length == remaining,
                "range body length does not match its bounds"
            );
        }
        loop {
            let chunk = tokio::select! {
                _ = cancel.cancelled() => { file.sync_all().await?; bail!("download paused"); },
                result = response.chunk() => result?,
            };
            let Some(chunk) = chunk else {
                break;
            };
            if chunk.len() as u64 > remaining {
                // A complete-looking part from a malformed response must not be trusted on retry.
                let written = file.metadata().await?.len();
                file.set_len(0).await?;
                file.sync_all().await?;
                progress.completed.fetch_sub(written, Ordering::Relaxed);
                bail!("server exceeded requested range");
            }
            file.write_all(&chunk).await?;
            remaining -= chunk.len() as u64;
            progress
                .completed
                .fetch_add(chunk.len() as u64, Ordering::Relaxed);
        }
        file.sync_all().await?;
        ensure!(remaining == 0, "server truncated the requested range");
        Ok(())
    }

    async fn single(
        &self,
        url: &str,
        remote: &Remote,
        path: &Path,
        cancel: &CancellationToken,
        progress: &Progress,
    ) -> Result<()> {
        let response = tokio::select! {
            _ = cancel.cancelled() => bail!("download paused"),
            r = self.client.get(url).header(header::ACCEPT_ENCODING, "identity").send() => r?,
        };
        ensure!(
            response.status() == StatusCode::OK,
            "single stream returned HTTP {}",
            response.status().as_u16()
        );
        identity_encoding(&response)?;
        ensure!(
            response.url().as_str() == remote.url,
            "resource redirect changed since probe"
        );
        if let Some(etag) = &remote.etag {
            ensure!(
                response
                    .headers()
                    .get(header::ETAG)
                    .and_then(|v| v.to_str().ok())
                    == Some(etag.as_str()),
                "resource changed since probe"
            );
        }
        if let (Some(before), Some(after)) = (remote.total, response.content_length()) {
            ensure!(before == after, "resource length changed since probe");
        }
        let mut file = File::create(path).await?;
        progress.completed.store(0, Ordering::Relaxed);
        let mut stream = response.bytes_stream();
        loop {
            let chunk = tokio::select! {
                _ = cancel.cancelled() => { file.sync_all().await?; bail!("download paused"); },
                result = stream.next() => result,
            };
            let Some(chunk) = chunk else {
                break;
            };
            let chunk = chunk?;
            let completed = progress.completed.load(Ordering::Relaxed);
            if let Some(total) = remote.total {
                ensure!(
                    completed + chunk.len() as u64 <= total,
                    "server exceeded declared length"
                );
            }
            file.write_all(&chunk).await?;
            progress
                .completed
                .fetch_add(chunk.len() as u64, Ordering::Relaxed);
        }
        file.sync_all().await?;
        Ok(())
    }
}

fn identity_encoding(response: &reqwest::Response) -> Result<()> {
    ensure!(
        response
            .headers()
            .get(header::CONTENT_ENCODING)
            .is_none_or(|v| v == "identity"),
        "encoded response refused to preserve byte offsets"
    );
    Ok(())
}

fn content_range(response: &reqwest::Response) -> Result<(u64, u64, u64)> {
    let value = response
        .headers()
        .get(header::CONTENT_RANGE)
        .context("missing Content-Range")?
        .to_str()?;
    let (range, total) = value
        .strip_prefix("bytes ")
        .context("invalid range unit")?
        .split_once('/')
        .context("invalid Content-Range")?;
    let (start, end) = range.split_once('-').context("invalid byte range")?;
    let (start, end, total): (u64, u64, u64) = (start.parse()?, end.parse()?, total.parse()?);
    ensure!(start <= end && end < total, "invalid range bounds");
    Ok((start, end, total))
}

fn plan(total: u64, connections: u8) -> Vec<Segment> {
    // Bounded work-stealing ranges, with a 256 KiB floor to limit negotiation
    // overhead. This is scheduling across ranges, not mid-response splitting.
    let count = if connections == 1 {
        1
    } else {
        (u64::from(connections) * 8)
            .min(total.div_ceil(256 * 1024))
            .max(1)
    };
    let size = total.div_ceil(count);
    (0..count)
        .filter_map(|i| {
            let start = i * size;
            (start < total).then_some(Segment {
                start,
                end: start.saturating_add(size - 1).min(total - 1),
            })
        })
        .collect()
}

fn validate_plan(segments: &[Segment], total: u64) -> Result<()> {
    ensure!(
        !segments.is_empty() && segments.len() <= 128,
        "invalid saved segment plan"
    );
    let mut next = 0;
    for s in segments {
        ensure!(
            s.start == next && s.end >= s.start && s.end < total,
            "invalid saved segment bounds"
        );
        next = s.end + 1;
    }
    ensure!(next == total, "incomplete saved segment plan");
    Ok(())
}
