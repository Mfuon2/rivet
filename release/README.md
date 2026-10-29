# Free release signing

No paid account or certificate is required for this workflow. Minisign signs release checksums with an encrypted private key held outside the repository. It does not replace browser-store signing, Windows Authenticode, or Apple Developer ID/notarization.

## First-time key setup (maintainer's own terminal)

Install Minisign (macOS: `brew install minisign`; Ubuntu: `sudo apt install minisign`). Then:

```sh
python3 scripts/release.py keygen
```

A production public key is now published in `release/minisign.pub`; do not regenerate or replace that identity. The original maintainer's encrypted key stays outside Git. For a new independent fork, enter a strong password in your terminal. The script creates the encrypted private key at `~/.config/rivet-signing/minisign.key` and the public key at `release/minisign.pub`. Commit/publish only the public key and establish its fingerprint through trusted channels. Keep an offline backup of the private key and password separately. No private key or password is sent to GitHub by this workflow. The maintainer may instead authorize `python3 scripts/release.py keygen --keychain` on macOS. This generates a random password, stores it in the current user's Keychain under `org.rivet.downloads.minisign`, and encrypts the private key without putting the password in process arguments, logs or GitHub. Sign with `python3 scripts/release.py sign DIRECTORY --keychain`. Keychain Access can be used by the maintainer to manage/back up the credential. Never upload it or the private key.

## Build and package

On each target operating system:

```sh
cargo build --release --workspace --locked
python3 scripts/release.py package --installer
```

Artifacts land in `dist/releases/0.1.0/` for the current version. macOS builds a `.app` archive and unsigned `.pkg` (ad-hoc app signature only); Windows builds a ZIP and unsigned Inno Setup EXE (install Inno Setup 6 first); Linux builds a tarball and DEB (requires `dpkg-deb`). Linux compatibility depends on the build machine's glibc/runtime; the CI package targets Ubuntu 24.04 rather than claiming every Linux distribution. Only architectures actually built and tested are supported.

Do not install a test package over a development copy without accounting for native-helper paths. The app registers the installed sibling helper at first launch. Windows includes an uninstaller; macOS/Linux removal follows their package management and per-user browser-registration cleanup remains future work. Queue/history/downloads are preserved.

## Sign locally, then upload

Collect the packages from the same source revision/version into one directory. Platform publisher signing, if used later, must happen before checksum generation.

```sh
python3 scripts/release.py checksums dist/releases/0.1.0
python3 scripts/release.py sign dist/releases/0.1.0
python3 scripts/release.py verify dist/releases/0.1.0
```

Signing prompts for the private-key password. Publish the packages, `SHA256SUMS`, `SHA256SUMS.minisig`, public key and release notes only after verification. The public key should also live in the established source repository; never rely solely on a key attached beside a download. See VERIFYING.md.

The manual GitHub Actions packaging workflow uploads unsigned build artifacts only; it has no signing secrets and does not automatically publish a release. A missing production key never silently falls back to an unsigned authenticated-release claim.

## Public release readiness

Native packaging scripts exist but need clean-machine install/upgrade/uninstall tests on every target. Store approval, a production signing identity, reproducibility validation, third-party dependency/license review, OS-specific signing and notarization are not implied by successful local builds. SignPath acceptance also requires a public documented project and an existing release; see docs/publishing/SIGNPATH-DRAFT.md.
