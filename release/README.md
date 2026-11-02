# Release and publication pipeline

Merging into `main` triggers [Release and publish](../.github/workflows/release.yml). A direct push to `main` does the same. No paid signing account is required for the signed-checksum workflow.

```text
main update → allocate version → test/build Windows + macOS + Linux
            → inspect packages → sign checksums → publish Latest → verify downloads
            → authenticate store ZIP → upload → Google review → Chrome update
```

## Automatic versions

The workflow allocates the next unused numeric patch version in the source version's major/minor series. For example, after `v0.1.0`, the next successful main update produces `v0.1.1`. It stamps the workspace version, the three workspace entries in `Cargo.lock`, and both browser manifests inside the runner checkout. It does not commit version changes back to `main` or create a recursive release loop.

To reproduce a release, check out its source tag, run `python3 scripts/publish.py stamp VERSION`, then use the committed lockfile to build. The immutable release tag identifies the source commit; the release notes record the generated version. A rerun for a commit already published is skipped. Published assets are not overwritten by the automatic pipeline. An incomplete draft for the same source commit can be retried.

Main release runs are serialized. If main changes rapidly, GitHub concurrency keeps the active run and newest pending run; intermediate pending runs may be superseded. The latest main state is eventually released without version-allocation races.

## Build and verification

Each platform runs Python packaging/signature tests, Rust formatting/lints/tests, browser tests, an optimized build and end-to-end checks against the actual release binaries. Native packaging produces:

- Windows x64: an Inno Setup installer and ZIP. CI exercises silent installation, the installed CLI/helper, and uninstallation.
- macOS: a DMG, PKG and app archive for the runner architecture; installer payload and icons are checked.
- Linux x64: a DEB and tarball built on Ubuntu 24.04; the DEB is extracted and its CLI, launcher and icon checked.
- Chromium and Firefox development ZIPs plus a separate Chrome Web Store upload ZIP.

Each app also contains pinned yt-dlp, QuickJS-NG and a minimal FFmpeg/FFprobe build. The media preparation step verifies upstream checksums, compiles FFmpeg from unmodified LGPL source, exercises a real video/audio mux, and includes source archives, license notices and build configuration. Windows media builds require MSYS2 Make and Visual Studio C++ tools; MSYS2 supplies the configure shell. Package inspection runs the installed media tools.

All platform jobs must succeed before publication. Collection requires every supported platform and rejects missing or conflicting packages. Browser ZIPs use normalized metadata so duplicate cross-platform outputs have identical bytes.

## Signing and publication

The publish job uses the GitHub `release` environment, restricted to `main`. Its environment secrets are `MINISIGN_SECRET_KEY` (the encrypted private key) and `MINISIGN_PASSWORD`. They are never committed, printed or uploaded as build artifacts. The private key is materialized only in a temporary directory for signing and removed afterward. Secrets are available only to the signing step; build and pull-request jobs do not receive them.

The original encrypted key and Keychain-protected password remain on the maintainer's Mac as a local backup. `release/minisign.pub` is the public verification key; do not replace that identity casually. Only trusted changes should be merged into main because the release workflow executes main's code.

After signing `SHA256SUMS`, the job verifies the signature against the committed public key, creates a draft GitHub release, uploads installers/archives/extensions and authentication files, downloads and verifies them, publishes the release, and verifies the public downloads again. The GitHub token has write access only in the publication job. Actions are pinned to commit IDs.

Installable files are published as **GitHub Release assets**. The GitHub Packages tab is a separate package/container registry and is not required for these downloads. The workflow also records the release URL in the Actions summary and `release` deployment environment.

Normal main releases are published as stable GitHub releases and explicitly marked **Latest**. To create a preview, run **Release and publish** manually on main and enable **preview**. Previews use the Preview title, remain prereleases, are not Latest, and are not submitted to Chrome. The source tag itself (for example `v0.1.8`) does not determine these GitHub flags.

Minisign authentication is separate from Windows Authenticode or Apple Developer ID/notarization; the Latest label does not remove platform warnings.

## Manual operation and recovery

Use Actions → **Release and publish** → **Run workflow** on main to retry. No manual ZIP creation, version edit, signing or release upload is needed for ordinary merges. A missing signing secret or failed check stops the pipeline rather than publishing an unsigned authenticated-release claim.

For a local build:

```sh
cargo build --release --workspace --locked
python3 scripts/prepare_media_tools.py
python3 scripts/release.py package --installer
```

For local checksum signing with the existing Mac Keychain credential:

```sh
python3 scripts/release.py checksums dist/releases/0.1.0
python3 scripts/release.py sign dist/releases/0.1.0 --keychain
```

Local packaging requires Inno Setup 6 on Windows, Apple's `pkgbuild` on macOS or `dpkg-deb` on Debian/Ubuntu. Users can verify downloads using [VERIFYING.md](VERIFYING.md).

## Other publishing services

Apple notarization, Windows publisher certificates/SignPath and browser-store publication require their separate enrollment or approval processes. Their separate credentials and approvals are required. Installer inspection is automated, but comprehensive clean-machine desktop/browser interaction coverage and independent security review remain separate work.


## Automatic Chrome updates after a release

The release workflow calls [Chrome Web Store](../.github/workflows/chrome-store.yml) after publication and public download verification. That job downloads the published release, verifies its Minisign signature against the committed key and all package hashes, validates the Chrome ZIP version/manifest, uploads it to the existing public item `oloipikeghldaokdiglloelbpfheeeik`, then submits it for review with `DEFAULT_PUBLISH`. Google deploys it after approval; Chrome distributes the approved extension update to installed users. There is no ZIP download or developer loading required for store-installed extensions.

Configure this **once** in GitHub Settings → Environments → **release**:

| Type | Name | Value |
| --- | --- | --- |
| Variable | `CWS_PUBLISHER_ID` | Publisher ID from the Chrome Developer Dashboard account settings (not the extension ID). |
| Secret | `CWS_CLIENT_ID` | Google OAuth client ID authorized for the publisher account. |
| Secret | `CWS_CLIENT_SECRET` | Its OAuth client secret. |
| Secret | `CWS_REFRESH_TOKEN` | Refresh token with `https://www.googleapis.com/auth/chromewebstore` scope. |

Enable the Chrome Web Store API in the Google Cloud project and authorize the OAuth client as the account that can manage Rivet. Follow Google's [API authentication instructions](https://developer.chrome.com/docs/webstore/using-api). Keep these values in GitHub secrets, never in source or chat. This uses the existing Chrome publisher account; GitHub authentication does not grant access to Google publishing.

When none of these values are present, the job clearly reports **Publisher setup required** and skips store submission. GitHub releases remain available. Partial configuration, API rejection or invalid signatures fail the store job without hiding the problem or changing the published desktop assets.

A rerun recognizes an already published or already submitted version. It preserves any different version currently in review and stops with instructions to retry after that submission finishes. It does not cancel reviews or skip Google's checks. To retry only this step after configuring credentials or resolving a store issue, run Actions → **Chrome Web Store** → **Run workflow** on main, with the stable release tag; leave the tag blank to use GitHub Latest. Preview/draft releases are rejected. Firefox/Edge store submission is separate.

New permissions in this update (`downloads` and the public GitHub API host) must be explained in the store Privacy tab; see [PUBLISHING.md](../docs/PUBLISHING.md). Chrome may request user approval for added permissions. Store approval and browser/OS installation consent remain under Google/user control.

On macOS, download the `.dmg` for your processor, open it, drag **Rivet.app** onto **Applications**, then open Rivet from Applications and eject the image. The app prepares its browser helper on first launch. The DMG is inspected in CI by mounting it, copying the app off the image, verifying its code signature, and running its bundled tools. Apple Developer ID/notarization is still separate; changing to DMG does not remove Gatekeeper verification prompts.
