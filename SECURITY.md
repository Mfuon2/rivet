# Security and privacy design

- No activation, remote account requirement, telemetry, ads, or implicit update network calls.
- Standard TLS verification is enabled. The client refuses redirect downgrades from HTTPS to HTTP and does not synthesize automatic Referer headers.
- Browser communication uses native messaging with a browser-enforced extension allowlist. No unauthenticated localhost HTTP/WebSocket service is exposed.
- Native frames are limited to 64 KiB and parsed as strict JSON; accepted messages are a download URL or a queue-free connection ping. Browser input cannot choose output paths, pass command arguments, or execute downloaded content.
- Browser capture is explicit. Media discovery requires an optional site grant. The initial implementation does not collect or forward cookies/passwords. It also does not intercept all browser downloads automatically.
- Range responses require matching offsets, total size, URL, and strong ETag before bytes are appended. Unsupported or inconsistent servers fail safely or use a single stream. Output is published only after length/hash checks.
- Existing destinations are never replaced. Advisory process locks and SQLite ownership leases coordinate workers. Trusted local filesystem/account ownership remains an assumption.
- URLs are stored in the queue and resume manifests, including signed query strings. Database/work-directory permissions are restricted on Unix; encryption at rest is not implemented. `rivet list` intentionally exports full queue data. Keep this output private.
- SQLite database siblings and Unix work directories are private. Windows uses the user's normal account/ACL boundary; a hardened application-data ACL installer is planned.
- Network error display avoids reqwest URL formatting. No browser-captured request data is sent to a Rivet service.
- Expected SHA-256 verification is optional and useful for verifying publisher-provided hashes. A calculated hash alone does not authenticate a file or scan it for malware.

Before public release: add checkpoint/per-part integrity, keychain-backed credentials, fuzzing, dependency/license auditing, signed updates and independent platform testing. Local same-user tampering, hostile server protocol behavior outside tested cases, and all OS-specific integration surfaces require continued review.

Report issues privately to the repository maintainer once a public repository and reporting channel are established. No reporting endpoint is configured in this local project.

Release authenticity: the offline Minisign workflow signs final package checksums and verifies signatures before trusting the manifest. Private keys stay outside the repository; CI builds unsigned artifacts only. The production public key is published in release/minisign.pub. The encrypted private key stays outside the repository and its password is protected by the maintainer's macOS Keychain. Windows Authenticode identity and Apple notarization are not configured. Signing instructions are in release/README.md.
