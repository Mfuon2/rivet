# Capabilities

Implementation status describes the current source. It does not imply production readiness or validation on every operating system.

| Capability | Status |
| --- | --- |
| HTTP/HTTPS transfers | Implemented |
| Multiple range connections | Implemented, 1–16 connections |
| Pause/resume | Implemented with strong ETag, known size and range support |
| Integrity checks | SHA-256 calculation and optional expected hash |
| Range retries | Bounded retries; single-stream automatic retry planned |
| Persistent queue/history | SQLite jobs and completed hashes |
| One-time delayed jobs | Implemented; recurring schedules planned |
| Desktop UI | Compact menus, toolbar, categories, saved theme/density/columns |
| Transfer popup | Live byte ranges, segment states/progress, pause/resume and retained session snapshots |
| CLI | Downloads, queue management, proxy options and manifest generation |
| Chromium/Firefox handoff | Extension sources, packages and native helper implemented; store distribution pending |
| Default-browser setup | Read-only extension discovery, helper repair, approved-store requests, automatic handshake and GitHub ZIP fallback |
| Media URL discovery | Opt-in per-site discovery; detections held in worker memory |
| In-page media widget | Standard HTML video/audio on enabled sites; explicit direct-file handoff, no protected-media bypass |
| Authenticated browser downloads | Planned; cookies/passwords are not forwarded |
| HLS/DASH assembly | Planned; current URLs can be saved as playlist files |
| HTTP/SOCKS proxy | CLI and environment support; desktop proxy editor/PAC planned |
| Native packages | Windows setup EXE, macOS PKG/app archive, Linux DEB/tarball tooling |
| Release authentication | Minisign-signed checksums and published verification key |
| Platform publisher signing | Windows Authenticode and Apple Developer ID/notarization pending |
| Localization | English interface; translation support planned |
| Accessibility | Native accessibility enabled; full review pending |
| FTP/FTPS, site crawling, ZIP preview | Planned |
| Bandwidth/concurrency budgets | Planned |
| Signed automatic updates | Planned |

The project supports native Windows, macOS and Linux builds. Local desktop interaction checks run on macOS ARM64; CI provides automated builds and tests for all three platforms. Clean-machine installation and browser interaction coverage remain separate work.

## Public YouTube downloads

The extension widget and popup send a typed video-page download action to the native helper. The desktop opens and downloads video/audio streams through bundled yt-dlp and QuickJS-NG, then merges and validates a complete MP4 with FFmpeg/FFprobe. Downloads support retained partial streams and explicit pause/resume. Counters refer to the current stream; they are not byte-range segment measurements from Rivet’s HTTP engine. Public completed YouTube videos are supported; authentication, browser cookies, DRM and live streams are excluded. General HLS/DASH assembly on other sites remains unsupported. Tools are pinned by checksums in `packaging/media-tools.lock.json` and travel in signed-checksum application packages.
