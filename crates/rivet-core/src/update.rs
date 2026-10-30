//! Authenticated releases and a detached installer. No update executes before user approval.
use anyhow::{Context, Result, bail, ensure};
use fs2::FileExt;
use minisign_verify::{PublicKey, Signature};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};
use tokio_util::sync::CancellationToken;

const REPOSITORY: &str = "https://github.com/Mfuon2/rivet";
pub const RELEASES: &str = "https://api.github.com/repos/Mfuon2/rivet/releases?per_page=100";
const KEY: &str = include_str!("../../../release/minisign.pub");
const MAX_PACKAGE: u64 = 1024 * 1024 * 1024;
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version(pub u64, pub u64, pub u64);
impl Version {
    pub fn parse(text: &str) -> Result<Self> {
        let parts: Vec<_> = text.strip_prefix('v').unwrap_or(text).split('.').collect();
        ensure!(
            parts.len() == 3
                && parts.iter().all(|s| !s.is_empty()
                    && s.bytes().all(|b| b.is_ascii_digit())
                    && (s.len() == 1 || !s.starts_with('0'))),
            "Invalid release version"
        );
        Ok(Self(
            parts[0].parse()?,
            parts[1].parse()?,
            parts[2].parse()?,
        ))
    }
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Asset {
    pub name: String,
    pub browser_download_url: String,
    pub size: u64,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Release {
    pub tag_name: String,
    #[serde(default)]
    pub draft: bool,
    #[serde(default)]
    pub prerelease: bool,
    #[serde(default)]
    pub body: Option<String>,
    pub assets: Vec<Asset>,
}
impl Release {
    pub fn version(&self) -> &str {
        self.tag_name.strip_prefix('v').unwrap_or(&self.tag_name)
    }
    pub fn page(&self) -> String {
        format!("{REPOSITORY}/releases/tag/{}", self.tag_name)
    }
    pub fn asset(&self, name: &str) -> Result<&Asset> {
        let matches: Vec<_> = self.assets.iter().filter(|a| a.name == name).collect();
        ensure!(
            matches.len() == 1,
            "Release package is missing or duplicated: {name}"
        );
        let asset = matches[0];
        ensure!(
            asset.browser_download_url
                == format!("{REPOSITORY}/releases/download/{}/{name}", self.tag_name),
            "Unexpected release asset address"
        );
        ensure!(
            asset.size > 0 && asset.size <= MAX_PACKAGE,
            "Invalid release asset size"
        );
        Ok(asset)
    }
}
pub fn select_release(data: &[u8], current: &str) -> Result<Option<Release>> {
    let current = Version::parse(current)?;
    let releases: Vec<Release> = serde_json::from_slice(data)?;
    // Published previews are included: Rivet currently ships a documented preview channel.
    Ok(releases
        .into_iter()
        .filter(|r| !r.draft && r.tag_name.starts_with('v'))
        .filter_map(|r| Version::parse(r.version()).ok().map(|v| (v, r)))
        .filter(|(v, _)| *v > current)
        .max_by_key(|(v, _)| *v)
        .map(|(_, r)| r))
}
fn client(timeout: u64) -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .user_agent(concat!("Rivet/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(timeout))
        .redirect(reqwest::redirect::Policy::limited(5))
        .https_only(true)
        .build()?)
}
async fn bounded(client: &reqwest::Client, url: &str, limit: usize) -> Result<Vec<u8>> {
    let mut response = client.get(url).send().await?.error_for_status()?;
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        ensure!(
            bytes.len() + chunk.len() <= limit,
            "Update metadata is too large"
        );
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}
async fn cancellable_metadata(
    client: &reqwest::Client,
    url: &str,
    limit: usize,
    cancel: &CancellationToken,
) -> Result<Vec<u8>> {
    tokio::select! {
        _ = cancel.cancelled() => bail!("Update download cancelled"),
        result = bounded(client, url, limit) => result,
    }
}
pub async fn check(current: &str) -> Result<Option<Release>> {
    select_release(
        &bounded(&client(20)?, RELEASES, 2 * 1024 * 1024).await?,
        current,
    )
}
pub fn authenticated_checksums(
    manifest: &[u8],
    signature: &str,
) -> Result<BTreeMap<String, String>> {
    let key = PublicKey::from_base64(KEY.lines().nth(1).context("Missing release key")?)?;
    key.verify(manifest, &Signature::decode(signature)?, false)
        .context("Release signature verification failed")?;
    parse_checksums(manifest)
}
pub fn parse_checksums(manifest: &[u8]) -> Result<BTreeMap<String, String>> {
    let mut entries = BTreeMap::new();
    for line in std::str::from_utf8(manifest)?.lines() {
        let (digest, name) = line.split_once("  ").context("Invalid checksum entry")?;
        ensure!(
            digest.len() == 64
                && digest
                    .bytes()
                    .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
            "Invalid checksum digest"
        );
        ensure!(
            !name.is_empty()
                && name
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
                && !name.starts_with('.'),
            "Unsafe checksum filename"
        );
        ensure!(
            entries.insert(name.to_owned(), digest.to_owned()).is_none(),
            "Duplicate checksum entry"
        );
    }
    ensure!(!entries.is_empty(), "Empty checksum manifest");
    Ok(entries)
}
pub fn verify_file(path: &Path, expected: &str) -> Result<()> {
    ensure!(
        !path.is_symlink() && path.is_file(),
        "Invalid update package"
    );
    let mut file = File::open(path)?;
    let mut hash = Sha256::new();
    let mut buf = [0u8; 65536];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hash.update(&buf[..n]);
    }
    ensure!(
        format!("{:x}", hash.finalize()) == expected,
        "Update package checksum does not match"
    );
    Ok(())
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum Installation {
    MacArchive,
    MacInstaller,
    Windows { root: PathBuf },
    LinuxDeb,
    LinuxPortable { root: PathBuf },
}
impl Installation {
    pub fn detect(exe: &Path) -> Result<Self> {
        #[cfg(target_os = "macos")]
        {
            ensure!(
                exe == Path::new("/Applications/Rivet.app/Contents/MacOS/rivet-desktop"),
                "Automatic installation is supported from /Applications/Rivet.app. Download the installer to update a development or custom-location copy."
            );
            Ok(
                if Command::new("/bin/test")
                    .args(["-w", "/Applications"])
                    .status()?
                    .success()
                {
                    Self::MacArchive
                } else {
                    Self::MacInstaller
                },
            )
        }
        #[cfg(windows)]
        {
            ensure!(
                exe.file_name().is_some_and(|n| n == "rivet-desktop.exe"),
                "Unrecognized application executable"
            );
            let bin = exe.parent().context("Missing application folder")?;
            ensure!(
                bin.file_name().is_some_and(|n| n == "bin"),
                "Unrecognized application layout"
            );
            Ok(Self::Windows {
                root: bin
                    .parent()
                    .context("Missing application folder")?
                    .to_path_buf(),
            })
        }
        #[cfg(not(any(target_os = "macos", windows)))]
        {
            if exe == Path::new("/opt/rivet/rivet-desktop") {
                return Ok(Self::LinuxDeb);
            }
            let bin = exe.parent().context("Missing application folder")?;
            let root = bin.parent().context("Missing application folder")?;
            ensure!(
                bin.file_name().is_some_and(|n| n == "bin")
                    && root
                        .file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| n.starts_with("rivet-")),
                "Automatic installation requires the Rivet DEB or its extracted portable archive"
            );
            Ok(Self::LinuxPortable {
                root: root.to_path_buf(),
            })
        }
    }
    pub fn executable(&self) -> PathBuf {
        match self {
            Self::MacArchive | Self::MacInstaller => {
                PathBuf::from("/Applications/Rivet.app/Contents/MacOS/rivet-desktop")
            }
            Self::Windows { root } => root.join("bin/rivet-desktop.exe"),
            Self::LinuxDeb => PathBuf::from("/opt/rivet/rivet-desktop"),
            Self::LinuxPortable { root } => root.join("bin/rivet-desktop"),
        }
    }
    pub fn package_name(&self, version: &str) -> Result<String> {
        Version::parse(version)?;
        let platform = match self {
            Self::MacArchive | Self::MacInstaller => {
                ensure!(
                    cfg!(all(target_os = "macos", target_arch = "aarch64")),
                    "No release package for this Mac architecture"
                );
                if matches!(self, Self::MacArchive) {
                    "macos-aarch64.zip"
                } else {
                    "macos-aarch64.pkg"
                }
            }
            Self::Windows { .. } => {
                ensure!(
                    cfg!(all(windows, target_arch = "x86_64")),
                    "No release package for this Windows architecture"
                );
                "windows-x86_64-setup.exe"
            }
            Self::LinuxDeb => {
                ensure!(
                    cfg!(all(target_os = "linux", target_arch = "x86_64")),
                    "No release package for this Linux architecture"
                );
                "linux-x86_64.deb"
            }
            Self::LinuxPortable { .. } => {
                ensure!(
                    cfg!(all(target_os = "linux", target_arch = "x86_64")),
                    "No release package for this Linux architecture"
                );
                "linux-x86_64.tar.gz"
            }
        };
        Ok(format!("rivet-{version}-{platform}"))
    }
}
pub fn cache_dir() -> Result<PathBuf> {
    Ok(directories::ProjectDirs::from("org", "rivet", "Rivet")
        .context("Missing cache folder")?
        .cache_dir()
        .join("updates"))
}
#[derive(Debug, Deserialize, Serialize)]
pub struct Plan {
    pub parent_pid: u32,
    pub release: Release,
    pub installation: Installation,
    pub package: PathBuf,
}
pub struct Prepared {
    pub plan: PathBuf,
    pub directory: PathBuf,
    pub lock: File,
}
/// Fetch an exact published release without assuming GitHub's stable-only latest endpoint.
pub async fn release(version: &str) -> Result<Release> {
    Version::parse(version)?;
    let url = format!("{}/tags/v{version}", RELEASES.split('?').next().unwrap());
    let release: Release =
        serde_json::from_slice(&bounded(&client(20)?, &url, 2 * 1024 * 1024).await?)?;
    ensure!(
        !release.draft && release.tag_name == format!("v{version}"),
        "Release is not published"
    );
    Ok(release)
}
/// Download an asset into a private staging directory and authenticate it with the pinned key.
/// The manifest/signature are retained beside it for a later independent install-time check.
pub async fn verified_asset(
    release: &Release,
    name: &str,
    directory: &Path,
    cancel: &CancellationToken,
    progress: impl Fn(u64, u64),
) -> Result<PathBuf> {
    let asset = release.asset(name)?;
    let http = client(if asset.size > 8 * 1024 * 1024 {
        300
    } else {
        60
    })?;
    let manifest = cancellable_metadata(
        &http,
        &release.asset("SHA256SUMS")?.browser_download_url,
        1024 * 1024,
        cancel,
    )
    .await?;
    let signature = cancellable_metadata(
        &http,
        &release.asset("SHA256SUMS.minisig")?.browser_download_url,
        8192,
        cancel,
    )
    .await?;
    let hashes = authenticated_checksums(&manifest, std::str::from_utf8(&signature)?)?;
    let hash = hashes
        .get(name)
        .context("Package missing from signed checksums")?;
    let partial = directory.join("package.part");
    let package = directory.join(name);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&partial)?;
    let response = tokio::select! { _=cancel.cancelled()=>bail!("Download cancelled"), result=http.get(&asset.browser_download_url).send()=>result? };
    let mut response = response.error_for_status()?;
    let mut downloaded = 0;
    loop {
        let chunk = tokio::select! { _=cancel.cancelled()=>bail!("Download cancelled"), result=response.chunk()=>result? };
        let Some(chunk) = chunk else {
            break;
        };
        downloaded += chunk.len() as u64;
        ensure!(
            downloaded <= asset.size,
            "Download exceeded its expected size"
        );
        file.write_all(&chunk)?;
        progress(downloaded, asset.size);
    }
    ensure!(downloaded == asset.size, "Download was incomplete");
    file.sync_all()?;
    drop(file);
    verify_file(&partial, hash)?;
    fs::rename(partial, &package)?;
    fs::write(directory.join("SHA256SUMS"), manifest)?;
    fs::write(directory.join("SHA256SUMS.minisig"), signature)?;
    Ok(package)
}
pub async fn download(
    release: Release,
    installation: Installation,
    cancel: CancellationToken,
    progress: impl Fn(u64, u64),
) -> Result<Prepared> {
    ensure!(
        std::env::var_os("RIVET_SETUP_ROOT").is_none(),
        "Update installation is disabled in isolated preview mode"
    );
    let name = installation.package_name(release.version())?;
    let directory = cache_dir()?.join(uuid::Uuid::new_v4().to_string());
    fs::create_dir_all(&directory)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))?;
    }
    let result = async {
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(directory.join("app.lock"))?;
        lock.try_lock_exclusive()?;
        let package = verified_asset(&release, &name, &directory, &cancel, progress).await?;
        let plan = directory.join("plan.json");
        fs::write(
            &plan,
            serde_json::to_vec(&Plan {
                parent_pid: std::process::id(),
                release,
                installation,
                package,
            })?,
        )?;
        Ok(Prepared {
            plan,
            directory: directory.clone(),
            lock,
        })
    }
    .await;
    if result.is_err() {
        let _ = fs::remove_dir_all(&directory);
    }
    result
}
pub fn launch(prepared: &Prepared) -> Result<()> {
    ensure!(
        std::env::var_os("RIVET_SETUP_ROOT").is_none(),
        "Update installation is disabled in isolated preview mode"
    );
    let exe = std::env::current_exe()?;
    let cli = exe.with_file_name(if cfg!(windows) { "rivet.exe" } else { "rivet" });
    ensure!(
        cli.is_file(),
        "The application updater helper is missing. Reinstall the full Rivet package."
    );
    let helper = prepared.directory.join(if cfg!(windows) {
        "rivet-updater.exe"
    } else {
        "rivet-updater"
    });
    fs::copy(cli, &helper)?;
    let mut command = Command::new(helper);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000); // CREATE_NO_WINDOW: the native installer owns the progress UI.
    }
    command
        .arg("apply-update")
        .arg(&prepared.plan)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    Ok(())
}
pub fn apply(plan_file: &Path) -> Result<()> {
    ensure!(
        std::env::var_os("RIVET_SETUP_ROOT").is_none(),
        "Update installation is disabled in isolated preview mode"
    );
    let directory = plan_file.parent().context("Missing update folder")?;
    ensure!(
        fs::metadata(plan_file)?.len() < 2 * 1024 * 1024,
        "Invalid update plan"
    );
    let plan: Plan = serde_json::from_slice(&fs::read(plan_file)?)?;
    // Plans do not authorize an arbitrary target, older package, URL or executable.
    let exe = plan.installation.executable();
    let detected = Installation::detect(&exe)?;
    let name = detected.package_name(plan.release.version())?;
    ensure!(
        Version::parse(plan.release.version())? > Version::parse(env!("CARGO_PKG_VERSION"))?,
        "Refusing an update downgrade"
    );
    ensure!(
        plan.package == directory.join(&name),
        "Unexpected update package path"
    );
    let hashes = authenticated_checksums(
        &fs::read(directory.join("SHA256SUMS"))?,
        &fs::read_to_string(directory.join("SHA256SUMS.minisig"))?,
    )?;
    let expected = hashes
        .get(&name)
        .context("Package missing from signed checksums")?;
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .open(directory.join("app.lock"))?;
    let start = Instant::now();
    while lock.try_lock_exclusive().is_err() {
        ensure!(
            start.elapsed() < Duration::from_secs(120),
            "Rivet did not close in time; installation cancelled"
        );
        std::thread::sleep(Duration::from_millis(200));
    }
    ensure!(
        plan.parent_pid > 0 && plan.parent_pid != std::process::id(),
        "Invalid updater parent"
    );
    #[cfg(unix)]
    while Command::new("/bin/kill")
        .args(["-0", &plan.parent_pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?
        .success()
    {
        ensure!(
            start.elapsed() < Duration::from_secs(120),
            "Rivet did not exit; update cancelled"
        );
        std::thread::sleep(Duration::from_millis(200));
    }
    #[cfg(windows)]
    successful(Command::new("powershell.exe").args(["-NoProfile", "-NonInteractive", "-Command", &format!("$p = Get-Process -Id {} -ErrorAction SilentlyContinue; if ($p) {{ if (-not $p.WaitForExit(120000)) {{ exit 1 }} }}", plan.parent_pid)]))?;
    let global = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(cache_dir()?.join("install.lock"))?;
    while global.try_lock_exclusive().is_err() {
        ensure!(
            start.elapsed() < Duration::from_secs(120),
            "Another Rivet update is still installing"
        );
        std::thread::sleep(Duration::from_millis(200));
    }
    let outcome = (|| -> Result<()> {
        verify_file(&plan.package, expected)?;
        // Another Rivet instance may have installed an update while this one downloaded.
        let cli = exe.with_file_name(if cfg!(windows) { "rivet.exe" } else { "rivet" });
        let installed = binary_version(&cli)?;
        if installed >= Version::parse(plan.release.version())? {
            return Ok(());
        }
        install(&plan, directory)?;
        ensure!(
            binary_version(&cli)? == Version::parse(plan.release.version())?,
            "Installer did not install the expected Rivet version"
        );
        Ok(())
    })();
    fs::write(
        cache_dir()?.join("last-result.json"),
        serde_json::to_vec(
            &serde_json::json!({"version":plan.release.version(),"ok":outcome.is_ok(),"message":outcome.as_ref().err().map(|e|e.to_string())}),
        )?,
    )?;
    // Startup repairs helper registrations and refreshes bundled extension sources.
    #[cfg(target_os = "macos")]
    {
        Command::new("/usr/bin/open")
            .args(["-n", "/Applications/Rivet.app"])
            .spawn()?;
    }
    #[cfg(not(target_os = "macos"))]
    {
        Command::new(exe)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
    }
    outcome
}
fn binary_version(cli: &Path) -> Result<Version> {
    let mut command = Command::new(cli);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let output = command.arg("--version").output()?;
    ensure!(
        output.status.success(),
        "Could not read the installed Rivet version"
    );
    Version::parse(
        String::from_utf8_lossy(&output.stdout)
            .split_whitespace()
            .last()
            .context("Missing installed version")?,
    )
}
fn successful(command: &mut Command) -> Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    ensure!(
        command.status()?.success(),
        "Installation was cancelled or failed. Your previous installation has been retained where possible."
    );
    Ok(())
}
fn install(plan: &Plan, directory: &Path) -> Result<()> {
    let _ = (plan, directory);
    #[cfg(target_os = "macos")]
    {
        if matches!(plan.installation, Installation::MacArchive) {
            let unpacked = directory.join("unpacked");
            fs::create_dir(&unpacked)?;
            successful(
                Command::new("/usr/bin/ditto")
                    .args(["-x", "-k"])
                    .arg(&plan.package)
                    .arg(&unpacked),
            )?;
            let staged = unpacked.join(format!(
                "rivet-{}-macos-aarch64/Rivet.app",
                plan.release.version()
            ));
            successful(
                Command::new("/usr/bin/codesign")
                    .args(["--verify", "--deep", "--strict"])
                    .arg(&staged),
            )?;
            for binary in ["rivet", "rivet-native-host", "rivet-desktop"] {
                ensure!(
                    staged.join("Contents/MacOS").join(binary).is_file(),
                    "Application update is incomplete"
                );
            }
            let parent = Path::new("/Applications");
            let incoming = parent.join(format!(".Rivet-incoming-{}.app", uuid::Uuid::new_v4()));
            successful(Command::new("/usr/bin/ditto").arg(staged).arg(&incoming))?;
            successful(
                Command::new("/usr/bin/codesign")
                    .args(["--verify", "--deep", "--strict"])
                    .arg(&incoming),
            )?;
            let target = parent.join("Rivet.app");
            let backup = parent.join(format!(".Rivet-backup-{}.app", uuid::Uuid::new_v4()));
            fs::rename(&target, &backup)?;
            if let Err(error) = fs::rename(&incoming, &target) {
                fs::rename(backup, target)?;
                return Err(error.into());
            }
            return Ok(());
        }
        let path = plan.package.to_str().context("Unsupported package path")?;
        ensure!(!path.chars().any(char::is_control), "Invalid package path");
        let shell = format!(
            "/usr/sbin/installer -pkg '{}' -target /",
            path.replace('\'', "'\\''")
        );
        let script = format!(
            "do shell script {} with administrator privileges",
            serde_json::to_string(&shell)?
        );
        successful(Command::new("/usr/bin/osascript").args(["-e", &script]))
    }
    #[cfg(windows)]
    {
        let Installation::Windows { root } = &plan.installation else {
            bail!("Invalid Windows installation");
        };
        successful(
            Command::new(&plan.package)
                .args([
                    "/SILENT",
                    "/SP-",
                    "/NORESTART",
                    "/CLOSEAPPLICATIONS",
                    "/NORESTARTAPPLICATIONS",
                ])
                .arg(format!("/DIR={}", root.display())),
        )
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        match &plan.installation {
            Installation::LinuxDeb => successful(
                Command::new("pkexec")
                    .args(["/usr/bin/dpkg", "-i"])
                    .arg(&plan.package),
            ),
            Installation::LinuxPortable { root } => {
                let unpacked = directory.join("unpacked");
                fs::create_dir(&unpacked)?;
                successful(
                    Command::new("tar")
                        .arg("-xzf")
                        .arg(&plan.package)
                        .arg("-C")
                        .arg(&unpacked),
                )?;
                let staged =
                    unpacked.join(format!("rivet-{}-linux-x86_64", plan.release.version()));
                for binary in ["rivet", "rivet-native-host", "rivet-desktop"] {
                    ensure!(
                        staged.join("bin").join(binary).is_file(),
                        "Portable update is incomplete"
                    );
                }
                let parent = root
                    .parent()
                    .context("Missing portable application parent")?;
                let incoming = parent.join(format!(".rivet-incoming-{}", uuid::Uuid::new_v4()));
                // mv supports different cache/install filesystems; final replacement stays on one filesystem.
                successful(Command::new("mv").arg(&staged).arg(&incoming))?;
                let backup = parent.join(format!(".rivet-backup-{}", uuid::Uuid::new_v4()));
                fs::rename(root, &backup)?;
                if let Err(error) = fs::rename(&incoming, root) {
                    fs::rename(&backup, root)?;
                    return Err(error.into());
                }
                Ok(())
            }
            _ => bail!("Invalid Linux installation"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Router, routing::get};
    async fn server() -> String {
        let app = Router::new()
            .route("/large", get(|| async { "x".repeat(10000) }))
            .route(
                "/slow",
                get(|| async {
                    tokio::time::sleep(Duration::from_secs(5)).await;
                    "[]"
                }),
            )
            .route(
                "/failure",
                get(|| async { axum::http::StatusCode::SERVICE_UNAVAILABLE }),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        format!("http://{addr}")
    }
    #[tokio::test]
    async fn oversized_metadata_http_errors_and_timeouts_are_recoverable() {
        let base = server().await;
        let http = reqwest::Client::builder()
            .timeout(Duration::from_millis(100))
            .build()
            .unwrap();
        assert!(bounded(&http, &format!("{base}/large"), 100).await.is_err());
        assert!(
            bounded(&http, &format!("{base}/failure"), 100)
                .await
                .is_err()
        );
        let start = Instant::now();
        assert!(bounded(&http, &format!("{base}/slow"), 100).await.is_err());
        assert!(start.elapsed() < Duration::from_secs(2));
    }
    #[tokio::test]
    async fn cancelling_a_pending_update_request_does_not_wait_for_its_timeout() {
        let base = server().await;
        let http = reqwest::Client::new();
        let cancel = CancellationToken::new();
        let signal = cancel.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(20)).await;
            signal.cancel();
        });
        let start = Instant::now();
        let error = cancellable_metadata(&http, &format!("{base}/slow"), 100, &cancel)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("cancelled"));
        assert!(start.elapsed() < Duration::from_secs(1));
    }
}
