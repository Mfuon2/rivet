# Rivet

<img src="assets/branding/icons/icon-128.png" width="96" height="96" alt="Rivet logo">

**An open-source desktop download manager for Windows, macOS, and Linux.**

Rivet helps you organize HTTP/HTTPS downloads, use multiple connections, pause and resume supported transfers, and send download links from your browser. It includes a desktop application, command-line tools, and Chromium/Firefox extensions.

[![Release and publish](https://github.com/Mfuon2/rivet/actions/workflows/release.yml/badge.svg)](https://github.com/Mfuon2/rivet/actions/workflows/release.yml)
[![Rust, browser, packaging and smoke tests](https://github.com/Mfuon2/rivet/actions/workflows/ci.yml/badge.svg)](https://github.com/Mfuon2/rivet/actions/workflows/ci.yml)

[Downloads](https://github.com/Mfuon2/rivet/releases) · [Browser setup](docs/BROWSER.md) · [Roadmap](docs/ROADMAP.md) · [Security](SECURITY.md)

![Desktop preview](docs/desktop-preview.png)

## Features

- HTTP/HTTPS downloads with up to 16 concurrent range connections.
- Pause and resume when the server provides range support and a strong ETag.
- SHA-256 verification, with an optional expected checksum.
- Persistent queue and download history, one-time scheduling, and batch actions.
- Compact desktop interface with categories, live progress, sortable/resizable columns, and light/dark themes.
- Click an active download to open live segment progress, byte ranges, pause and resume controls.
- Right-click a finished download to open/play it, reveal its folder, rename it, copy its path/source, remove its history entry or move the file to Trash. Double-click completed files to open them.
- Media downloads retain the resolved video title; browser file links retain their filename. Duplicate names receive a number without replacing existing files.
- Chrome, Edge, Chromium, and Firefox link handoff through a local native helper.
- Download public YouTube videos as complete MP4 files with audio through the browser widget or extension popup.
- Optional media-link discovery and an in-page download widget for sites you explicitly enable.
- CLI support for downloads, queue management, and HTTP/SOCKS proxy settings.
- Startup update checks, verified downloads, and complete application updates with restart.

Rivet has no activation, subscription gate, advertising, or telemetry.

## Download and install

Download the package matching your operating system and processor from [GitHub Releases](https://github.com/Mfuon2/rivet/releases).

| Platform | Packages |
| --- | --- |
| Windows | Setup EXE or ZIP archive |
| macOS | PKG installer or application archive |
| Linux | DEB package or tarball |

**Preview release:** some features and platform integrations are still in development. Linux packages are built on Ubuntu 24.04. Windows installers do not have Authenticode publisher signatures; macOS packages are not Developer ID-signed or notarized, so platform warnings may appear.

Release checksums are signed with Minisign. Follow the [verification instructions](release/VERIFYING.md) before installing. The public verification key is in [release/minisign.pub](release/minisign.pub).

## Application updates

Rivet checks GitHub for a newer published version in the background whenever it starts. Current preview releases are included. When an update is available, choose **Download update**, then **Install and restart**. You can postpone it or cancel the download. **Help → Check for updates** opens the checker again.

The app verifies the release's Minisign signature with its embedded public key, then verifies the package checksum before installing. It checks the package again in a detached updater after the running app exits. Updates replace the desktop app, CLI, download engine, native helper and bundled browser files together. Queue/history, settings, downloads and partial files remain in their existing data folders. Pause active downloads before installing.

Standard Windows installations update in place using the installer. On macOS, `/Applications/Rivet.app` is replaced as a complete bundle, with the previous bundle retained for rollback; protected installations use the PKG installer and may request administrator approval. Linux DEB installations use the system installer with administrator approval; extracted Linux portable archives update their whole application folder. Development builds, custom Mac locations and unsupported architectures offer a GitHub download link instead of replacing an unknown installation.

Store-managed browser extensions continue to update through their browser stores. Rivet refreshes its bundled development extension files and native helper registration on startup; an unpacked extension already loaded by the browser may need **Reload** in the browser's extensions page.

Offline checks never block app startup. Update requests contact GitHub and its release CDN; see [Privacy](PRIVACY.md). CI checks Rust, browser behavior, packaging/signatures and actual executables on Windows, Linux and macOS; the release workflow also builds, inspects, signs, uploads and verifies release packages.

## Getting started

1. Install and open Rivet.
2. Follow the default-browser setup screen, or continue to the download manager.
3. Choose **Add URL**, enter a download address and destination filename, then add it to the queue.
4. Select the entry and choose **Resume**, or choose **Start queue** to process queued downloads.

Automatic queue processing is off at startup. An explicit **Download** action in the widget or popup opens Rivet and starts that selected download automatically. **Stop** pauses selected active transfers; **Stop all** stops queue processing and pauses active transfers. Completed destination files are never overwritten.

## Browser integration

On startup, Rivet detects a supported default browser, prepares the extension files, and registers its native helper for the current user. Open **Browsers** in the toolbar to check or repeat setup.

Setup checks whether the extension is present, prepares the native connection, opens an available store installation page, and confirms the connection automatically after browser approval. You can continue or restart Rivet once setup completes.

If a missing extension is not yet available from a store, Rivet downloads the matching release ZIP into private temporary storage, verifies its release signature and checksum, validates and unpacks the package, and opens browser setup with a permanent prepared folder. There are no manual download or extraction steps. Offline setup uses the trusted bundled files. Chrome/Edge/Chromium still require **Developer mode → Load unpacked** for unpublished extensions; Firefox temporary add-ons need **Load Temporary Add-on** and disappear on browser restart. Published store installations use the browser’s approval flow.

The extension connects automatically after installation; its **Finish setup** page and **Check connection** button are available for troubleshooting. Right-click a download link and select **Send link to Rivet**. Captured links appear in the desktop queue.

See [browser setup](docs/BROWSER.md) for detailed instructions and troubleshooting.

## Video downloads

Enable media discovery for the site in the extension popup and approve its browser permission. On a public YouTube video, open **Download with Rivet**, then choose **Download video with audio · MP4**. Rivet opens automatically, resolves the video and audio streams, and merges them into an MP4 in `Downloads/Rivet`. The popup also has a **Download this video as MP4** button. Completed files use the video title, such as `Bubble Gum Riddim Mix.mp4`, and the listing stores the actual final path. A duplicate is saved as `Bubble Gum Riddim Mix (2).mp4`. Click the active job in Rivet to see progress and pause/resume controls.

Release installers include pinned yt-dlp, QuickJS-NG, FFmpeg and FFprobe. No terminal setup is required. Media components update with Rivet’s verified application updates. Public, completed YouTube videos are supported; private, sign-in-only, live and DRM-protected content is not. Rivet does not import browser cookies. Other sites currently support direct HTTP/HTTPS media files; arbitrary HLS/DASH playlists are saved as playlist files only. Only download media you have permission to save.

After updating a development installation, reload the unpacked extension in the browser’s extensions page and refresh existing video tabs once.

## Build from source

Requirements: Rust 1.95 or newer, Python 3.11 or newer for packaging scripts, CMake, and a C/C++ toolchain. Windows builds need Visual Studio C++ Build Tools. Linux desktop builds also need the development libraries listed in [the CI workflow](.github/workflows/ci.yml).

```sh
git clone https://github.com/Mfuon2/rivet.git
cd rivet
cargo build --workspace --locked
cargo run -p rivet-desktop
```

Build optimized executables and a native package on the target OS:

```sh
cargo build --release --workspace --locked
python3 scripts/prepare_media_tools.py
python3 scripts/release.py package --installer
```

Media packaging requires GNU Make and a C toolchain (MSYS2 Make plus Visual Studio C++ on Windows); the preparation script downloads checksum-pinned components and builds FFmpeg from source. For debug media downloads, set `RIVET_MEDIA_TOOLS` to the absolute path of `target/media-tools`. Installer packaging requires Inno Setup 6 on Windows, `pkgbuild` on macOS, or `dpkg-deb` on Debian/Ubuntu. See the [release guide](release/README.md) for maintainer packaging and signing instructions.

## Command-line usage

Download a file:

```sh
cargo run -p rivet-cli --bin rivet -- get 'https://example.com/file.zip' -o /absolute/path/file.zip -c 4
```

Add a queued job and process due jobs:

```sh
cargo run -p rivet-cli --bin rivet -- add 'https://example.com/file.zip' -o /absolute/path/file.zip
cargo run -p rivet-cli --bin rivet -- run
```

Use `--help` for available commands. `--database` selects a queue database; `--proxy` configures a CLI proxy. `RIVET_DATABASE` and `RIVET_DOWNLOAD_DIR` accept absolute paths for an alternate shared database and browser download directory.

## Current limitations

- Segmented resume requires a known size, byte-range support, and a strong ETag. Other resources restart after interruption.
- General HLS/DASH assembly outside the supported YouTube download path, session-cookie forwarding, automatic browser download takeover, FTP, site crawling, recurring schedules, and bandwidth budgets are not implemented.
- Safari is not supported yet.
- Destination filesystems must support hard links for final file publication.
- Local queues and resume metadata are not encrypted and may contain access tokens from download URLs.
- Automated builds/tests do not replace clean-machine installation and real-browser testing.

See [capabilities](docs/CAPABILITIES.md) and the [roadmap](docs/ROADMAP.md) for implementation status.

## Automatic releases

Updates merged into `main` automatically run the checks, build Windows/macOS/Linux packages, sign release checksums, publish a versioned GitHub release, and verify the uploaded downloads. Each release includes native installers, archives and browser ZIPs. See the [release guide](release/README.md) for versioning and recovery details.

## Development

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
python3 -m unittest discover -s scripts/tests -v
node --test browser/tests/background.test.cjs
python3 scripts/smoke.py
```

The Rust application is in `crates/`, extension sources are in `browser/`, and packaging tools are in `scripts/` and `packaging/`. See [architecture](docs/ARCHITECTURE.md) for component details. Bug reports and pull requests are welcome through [GitHub](https://github.com/Mfuon2/rivet/issues).

## Privacy and security

Read the [privacy statement](PRIVACY.md) and [security design](SECURITY.md). Browser handoff does not forward cookies or passwords. Download requests go to the source servers selected by the user.

## License

Rivet is licensed under the [MIT License](LICENSE). Action and file icons use [Lucide](https://lucide.dev/) with the included [Lucide/Feather notices](crates/rivet-desktop/assets/lucide/LICENSE). The default font is Segoe UI when installed or supplied locally. Poppins is bundled as a fallback under the [SIL Open Font License](assets/fonts/OFL.txt). Microsoft font files are not included in source or releases. The product logo is included in [assets/branding](assets/branding/).

Bundled media tools retain their own licenses: yt-dlp standalone distribution (GPL-3.0-or-later), FFmpeg (LGPL-2.1-or-later configuration), and QuickJS-NG (MIT). Sources, build configuration and notices accompany the media programs in each application package.
