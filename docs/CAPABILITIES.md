# Capability and enhancement matrix

“Implemented” describes the current source, not a claim of production readiness or verification on every OS. “Observed” refers to prior static REA code/resources; “declared” means strings/configuration rather than executed behavior.

| Capability | Reference evidence | Rivet status |
| --- | --- | --- |
| HTTP/HTTPS download engine | Range/proxy/TLS pseudocode | Implemented |
| Multiple connections / partial ranges | Range-building/parsing code | Implemented, 1–16 connections |
| Pause and resume | Range handling; UI declarations | Implemented with strong ETag and size |
| Integrity verification | Enhancement | Implemented SHA-256; optional expected hash |
| Bounded retries | Download-engine requirement | Implemented for ranges; single-stream retry planned |
| Persistent history/queue | Queue/scheduler resource declarations | Implemented SQLite jobs and final hashes |
| Delayed jobs | Scheduler resource declarations | Implemented CLI delay; recurring/UI schedules planned |
| Desktop manager | Main program/UI resources | Implemented compact native GUI, Lucide icons, light/dark, sortable/resizable columns and saved preferences |
| Windows/macOS/Linux | User requirement/enhancement | Shared source and CI matrix; local macOS validation only |
| Chromium/Firefox handoff | Original extension source + native-host pseudocode | Original extensions and Rust native host implemented; browser installation smoke tests pending |
| Media URL discovery | Original extension request handlers | Opt-in site discovery implemented; worker-memory lifetime |
| Cookie/header-authenticated capture | Original extension message building | Planned with keychain protection and per-origin permission |
| HLS/DASH video assembly, subtitles | Main media strings; requirements candidate | Planned; currently saves raw playlists only |
| Categories and automatic naming | Installer language resource declarations | Planned; initial explicit output paths |
| Global bandwidth limits | User speed/reliability requirement | Planned |
| Per-host/global queue concurrency | User speed/reliability requirement | Planned; current queue processing is sequential by default |
| HTTP/SOCKS proxy | CONNECT code and proxy declarations | CLI proxy option/system environment implemented; UI/PAC planned |
| FTP/FTPS | Installer language resource declarations | Planned |
| Site login profiles/authentication | Auth code and language declarations | Planned; URL credentials currently rejected |
| Site grabber / synchronization | Language resource declarations | Planned |
| ZIP preview | Language resource declaration | Planned |
| Antivirus scanner integration | Language resource declaration | Planned |
| Shell/browser installation helpers | Packaged DLLs/manifests | Automatic default-browser host registration/repair, embedded packages and connection check; OS installers pending |
| Windows WFP interception | Driver imports + application IOCTL calls | Planned as separately scoped optional adapter |
| macOS/Linux integration | User cross-platform requirement | Planned native adapters |
| Localization/accessibility | Language payloads; user modern UI requirement | Planned; initial UI is English |
| Modern TLS | Enhancement | Rustls transport; no proprietary OpenSSL DLL reuse |
| Privacy/transparency | Enhancement | No telemetry, opt-in site discovery, local queue, no activation |
| Signed updates and supply-chain checks | Enhancement | Planned |
| Open source, no activation | Explicit user requirement | MIT licensed; no activation subsystem |

Full capability parity is a multi-milestone objective. Feature claims must be backed by tests and platform evidence before the relevant row moves to complete.
