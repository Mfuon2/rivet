# Release and publication pipeline

Merging into `main` triggers [Release and publish](../.github/workflows/release.yml). A direct push to `main` does the same. No paid signing account is required for the signed-checksum workflow.

```text
main update → allocate version → test/build Windows + macOS + Linux
            → inspect packages → sign checksums → publish → verify downloads
```

## Automatic versions

The workflow allocates the next unused numeric patch version in the source version's major/minor series. For example, after `v0.1.0`, the next successful main update produces `v0.1.1`. It stamps the workspace version, the three workspace entries in `Cargo.lock`, and both browser manifests inside the runner checkout. It does not commit version changes back to `main` or create a recursive release loop.

To reproduce a release, check out its source tag, run `python3 scripts/publish.py stamp VERSION`, then use the committed lockfile to build. The immutable release tag identifies the source commit; the release notes record the generated version. A rerun for a commit already published is skipped. Published assets are not overwritten by the automatic pipeline. An incomplete draft for the same source commit can be retried.

Main release runs are serialized. If main changes rapidly, GitHub concurrency keeps the active run and newest pending run; intermediate pending runs may be superseded. The latest main state is eventually released without version-allocation races.

## Build and verification

Each platform runs Python packaging/signature tests, Rust formatting/lints/tests, browser tests, an optimized build and end-to-end checks against the actual release binaries. Native packaging produces:

- Windows x64: an Inno Setup installer and ZIP. CI exercises silent installation, the installed CLI/helper, and uninstallation.
- macOS: a PKG and app archive for the runner architecture; installer payload and icons are checked.
- Linux x64: a DEB and tarball built on Ubuntu 24.04; the DEB is extracted and its CLI, launcher and icon checked.
- Chromium and Firefox development ZIPs plus a separate Chrome Web Store upload ZIP.

All platform jobs must succeed before publication. Collection requires every supported platform and rejects missing or conflicting packages. Browser ZIPs use normalized metadata so duplicate cross-platform outputs have identical bytes.

## Signing and publication

The publish job uses the GitHub `release` environment, restricted to `main`. Its environment secrets are `MINISIGN_SECRET_KEY` (the encrypted private key) and `MINISIGN_PASSWORD`. They are never committed, printed or uploaded as build artifacts. The private key is materialized only in a temporary directory for signing and removed afterward. Secrets are available only to the signing step; build and pull-request jobs do not receive them.

The original encrypted key and Keychain-protected password remain on the maintainer's Mac as a local backup. `release/minisign.pub` is the public verification key; do not replace that identity casually. Only trusted changes should be merged into main because the release workflow executes main's code.

After signing `SHA256SUMS`, the job verifies the signature against the committed public key, creates a draft GitHub release, uploads installers/archives/extensions and authentication files, downloads and verifies them, publishes the release, and verifies the public downloads again. The GitHub token has write access only in the publication job. Actions are pinned to commit IDs.

Installable files are published as **GitHub Release assets**. The GitHub Packages tab is a separate package/container registry and is not required for these downloads. The workflow also records the release URL in the Actions summary and `release` deployment environment.

Releases remain marked as previews while the project is under development. Minisign authentication is not Windows Authenticode or Apple Developer ID/notarization. Browser-store upload/review is separate; the pipeline produces the upload ZIP but does not claim store approval or bypass browser consent.

## Manual operation and recovery

Use Actions → **Release and publish** → **Run workflow** on main to retry. No manual ZIP creation, version edit, signing or release upload is needed for ordinary merges. A missing signing secret or failed check stops the pipeline rather than publishing an unsigned authenticated-release claim.

For a local build:

```sh
cargo build --release --workspace --locked
python3 scripts/release.py package --installer
```

For local checksum signing with the existing Mac Keychain credential:

```sh
python3 scripts/release.py checksums dist/releases/0.1.0
python3 scripts/release.py sign dist/releases/0.1.0 --keychain
```

Local packaging requires Inno Setup 6 on Windows, Apple's `pkgbuild` on macOS or `dpkg-deb` on Debian/Ubuntu. Users can verify downloads using [VERIFYING.md](VERIFYING.md).

## Other publishing services

Apple notarization, Windows publisher certificates/SignPath and browser-store publication require their separate enrollment or approval processes. Their credentials are not configured in this pipeline. Installer inspection is automated, but comprehensive clean-machine desktop/browser interaction coverage and independent security review remain separate work.
