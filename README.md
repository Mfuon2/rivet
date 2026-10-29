# Rivet

An open source download manager in Rust for Windows, macOS, and Linux. MIT licensed, with no activation system, subscription gate, telemetry, or embedded advertising.

**Status: working foundation, not full IDM parity.** The HTTP engine, durable queue, native desktop UI, CLI, native messaging helper, and original browser extensions are implemented. Advanced media assembly, site grabbing, FTP, secure credential storage, and platform-specific integration are tracked in [the capability matrix](docs/CAPABILITIES.md). Windows/Linux are CI targets; local verification so far is on macOS ARM64.

This is a new implementation with original source and branding. The REA investigation is a behavioral reference; proprietary program code, extensions, assets, and binaries are not part of this project or its build.

## Build and run

Rust 1.95 or newer, a C/C++ build toolchain, and CMake (the TLS dependency builds AWS-LC). Windows needs Visual Studio C++ Build Tools; Linux desktop builds also need X11/Wayland/OpenGL development packages listed in `.github/workflows/ci.yml`.

```sh
cargo build --workspace --locked
cargo run -p rivet-desktop
```

The desktop follows a compact download-manager layout: menu strip, Lucide action toolbar, category tree, resizable/sortable transfer table and selection details. Light is the default theme; View → Dark theme switches it. Column visibility/order, density and theme persist. Before the manager opens, Rivet detects the default supported browser, prepares its bundled extension and registers the native helper for the current user. Extension installation remains browser-controlled; development builds show the local loading steps. Open **Browsers** to recheck setup or configure additional browsers.

The desktop app starts with automatic queue processing **off**. Add a URL and a complete output path, then click Start. Browser captures appear in the same queue. Pause retains validated range parts; resume is available as Retry / recover followed by Start. The destination's parent folder must exist. Completed files are never overwritten.

```sh
# Download now; Ctrl-C pauses. Repeating the command resumes when a strong ETag is available.
cargo run -p rivet-cli --bin rivet -- get 'https://example.com/file.zip' -o /absolute/path/file.zip -c 4

# Verify against a publisher-supplied SHA-256 (replace the placeholder).
cargo run -p rivet-cli --bin rivet -- get URL -o OUTPUT --sha256 EXPECTED_HEX

# Persist a job and process it later.
cargo run -p rivet-cli --bin rivet -- add URL -o OUTPUT --delay-seconds 60
cargo run -p rivet-cli --bin rivet -- list
cargo run -p rivet-cli --bin rivet -- run

# Process one due queued job, then exit.
cargo run -p rivet-cli --bin rivet -- run --once
```

Use `--database /path/queue.sqlite3` for an isolated queue and `--proxy http://proxy:port` or `--proxy socks5h://proxy:port` for CLI transfers. System proxy environment variables also apply. `list` returns JSON containing source URLs, so do not publish its output if URLs contain access tokens. The GUI currently uses system proxy settings; an explicit proxy editor is planned.

Set `RIVET_DATABASE` to an absolute path to share an alternate database across the desktop, CLI and native host (useful for development). An explicit CLI `--database` overrides it.

`RIVET_DOWNLOAD_DIR` overrides the browser helper's download directory and must also be absolute.

## Browser integration

Read [browser setup](docs/BROWSER.md). Run `python3 scripts/package_browser.py` to produce Chromium and Firefox extension folders/ZIPs in `dist/`. Build `rivet-native-host` and register its generated manifest with your browser. Packaging does not install anything or change browser settings.

The extension only sends URLs after an explicit user action. It does not copy browser cookies or passwords. Direct downloads requiring session cookies are not supported yet. Site media discovery requires a separate site permission. Detected links are held only in the browser worker's memory and may disappear when that worker sleeps. HLS/DASH URLs can be saved as playlist files; assembled video downloads are not implemented yet.

## Transfer guarantees and limitations

- HTTP/HTTPS, up to 16 concurrent ranges per transfer, redirects with HTTPS downgrade refusal, connection/read timeouts, and bounded range retries.
- Segmentation/resume requires a known size, range support, and a **strong ETag**. Other resources use one stream and restart after interruption. Weak validators do not authorize splicing file representations.
- Every ranged response must match its requested offsets, total size, final URL, and ETag. Encoded responses are refused to preserve byte offsets.
- Parts live in `OUTPUT.rivet/`. A process lock prevents concurrent writes to the same output. Output publication uses a same-filesystem hard link, which fails rather than replaces an existing path. Filesystems without hard-link support are not supported for publication yet.
- SHA-256 is calculated before publication. An optional expected digest must match. A mismatch retains partial data and leaves the destination absent.
- SQLite leases coordinate CLI/GUI workers. A crashed worker can be recovered after its 30-second lease expires. Automatic recovery is not automatic startup of downloads: use Retry / recover.
- SQLite is not encrypted. Job URLs and sidecar resume URLs can contain signed access tokens. Unix database/work-directory permissions are restricted; OS account access remains the security boundary. Credential storage in the OS keychain is planned before authenticated capture.
- Complete parts are trusted according to their sizes and the server's strong ETag; local partial-file tamper detection and power-loss checkpoints are planned. An expected SHA-256 protects final integrity when available.
- Single-stream transfers do not yet retry automatically. There is no global/per-host bandwidth budget yet.

## Verify

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
python3 scripts/package_browser.py
node --check browser/shared/background.js
node --check browser/shared/popup.js
node --test browser/tests/background.test.cjs
python3 scripts/smoke.py
```

Transfer tests use local HTTP fixtures: exact segmented assembly, interruption/resume, changed ETag rejection, ignored ranges, absent validators, malformed ranges, compression refusal, short-read retries, and checksum failure. Queue/native tests cover ownership, scheduling, framing limits and filesystem-path isolation. A three-OS CI matrix is provided; it has not run on a remote service yet.

Architecture and implementation stages: [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md). Capability status: [docs/CAPABILITIES.md](docs/CAPABILITIES.md). Security design: [SECURITY.md](SECURITY.md).


Icons: [Lucide](https://lucide.dev/guide/installation), vendored from `lucide-static` 1.64.0. See [third-party notices](crates/rivet-desktop/assets/lucide/LICENSE) and [asset provenance](crates/rivet-desktop/assets/lucide/PROVENANCE.md). SVGs are embedded locally in the desktop executable.

## Source and release packages

The application and all original browser extension sources live together at [Mfuon2/rivet](https://github.com/Mfuon2/rivet). `crates/` contains the Rust desktop app, engine, CLI and native helper. `browser/` contains the Chromium/Firefox extension code. No IDM binaries, extracted components or reverse-engineering artifacts are included in this repository.

Build native release executables and a package on the target OS:

```sh
cargo build --release --workspace --locked
python3 scripts/release.py package --installer
```

The native packaging tools are Inno Setup 6 on Windows, Apple's `pkgbuild` on macOS and `dpkg-deb` on Debian/Ubuntu. The output includes an installable package plus an archive and browser extension ZIPs. These are development packages: Windows installers are unsigned, macOS app signatures are ad hoc (not Developer ID/notarized), and browser-store approval is pending. The manual **Build unsigned release packages** workflow builds each OS on its own runner and retains downloadable CI artifacts; it does not publish a release or access a private signing key.

Free release authentication uses Minisign-signed SHA-256 checksums. See [release signing](release/README.md), [download verification](release/VERIFYING.md) and [publishing preparation](docs/PUBLISHING.md). A production signing key has not been selected yet. SignPath application and store submission drafts are in [docs/publishing](docs/publishing/).
