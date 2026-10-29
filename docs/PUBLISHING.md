# Publishing Rivet with a verifiable publisher identity

Status: development builds only. No browser-store submission, OS publisher signing, notarization or publisher-account configuration has been performed. A public Minisign verification key is available in release/minisign.pub; preview GitHub releases use signed checksums. Local tests do not constitute an independent security audit. Native packaging templates and a free signed-checksum workflow are now available. Complete installed-browser/platform installation tests and select a production signing identity before a public stable release.

## 1. Establish your publisher identity

Choose the individual or organization that owns Rivet. Use the same identity in the source repository, download website, privacy policy, store listings and signing accounts. Set up a support email and an HTTPS website you control. Use passkeys/security keys or MFA for publisher, repository and domain accounts. Keep recovery access separately; never commit credentials, private signing keys, recovery codes or store API tokens.

Use Rivet’s name, screenshots and artwork consistently across listings and packages. Preserve the MIT license and the Lucide/Feather notices in source and distributed packages.

## 2. Publish browser extensions through official channels

| Target | Publisher channel | Upload |
| --- | --- | --- |
| Chrome | [Chrome Developer Dashboard](https://chrome.google.com/webstore/devconsole/) | `dist/rivet-chrome-store.zip` produced with `--chrome-store` |
| Edge | [Microsoft Partner Center](https://partner.microsoft.com/dashboard/microsoftedge/overview) | Chromium-compatible ZIP after Edge testing |
| Firefox | [Mozilla Add-on Developer Hub](https://addons.mozilla.org/developers/) | Firefox package for review/signing; public listing or signed self-distribution |

Run `python3 scripts/package_browser.py` to create development ZIPs. These are not already store-signed releases. Before submission, finalize app/extension icons, descriptions, screenshots, support links, privacy policy, data declarations, permission justifications and reviewer test instructions. Explain that the extension requires the Rivet native helper and provide a runnable installer for reviewers. Review current Firefox submission/manifest requirements at submission time.

After the initial store upload, record each actual extension ID. Update `browser/distribution.json` with the Chrome/Chromium ID, optional separate Edge ID, Firefox ID and official installation URLs. Replace the Chromium development public key with the public key obtained from the Chrome dashboard so unpacked testing matches the production identity. Store ZIPs/identities must be tested before enabling those URLs in a release. The bundled development public key is public metadata, not a private signing key or publisher identity.

Keep future updates on the same store item/identity. Chrome on Windows/macOS requires approved distribution and user confirmation for externally requested installation. Firefox release/beta installations require Mozilla signing. Rivet must not bypass browser approval or use enterprise force-install policies on unmanaged consumer machines.

Official instructions: [Chrome publishing](https://developer.chrome.com/docs/webstore/publish), [Chrome stable IDs](https://developer.chrome.com/docs/extensions/reference/manifest/key), [Chrome external installation](https://developer.chrome.com/docs/extensions/how-to/distribute/install-extensions), [Edge publishing](https://learn.microsoft.com/en-us/microsoft-edge/extensions/publish/publish-extension), [Firefox signing](https://extensionworkshop.com/documentation/publish/signing-and-distribution-overview/).

## 3. Selected initial approach: free signed checksums

Use [Minisign](https://jedisct1.github.io/minisign/) with an encrypted private key kept outside the repository. The tested scripts are in `scripts/release.py`; see [release/README.md](../release/README.md) for key generation, packaging, signing and verification. Native package templates now produce unsigned Windows EXE installers, macOS PKGs with ad-hoc-signed apps and Debian packages. These are development artifacts until installation testing and release signing are completed.

The GitHub release workflow tests/builds all platforms, signs their collected checksums using protected environment secrets, publishes Release assets and verifies the downloads. It runs automatically on main updates; pull-request and build jobs do not receive signing credentials. The maintainer has authorized a production encrypted key with its random password stored in macOS Keychain; authorized copies now reside in the protected GitHub release environment for automatic signing. Neither secret is included in Git or release assets. Free Windows platform signing can be requested from [SignPath Foundation](https://signpath.org/terms) after the project meets its public release and governance requirements. The [application draft](publishing/SIGNPATH-DRAFT.md) and [store draft](publishing/STORE-DRAFT.md) are not submitted applications.

Platform-recognized signing below is a separate optional later step. Open-source distribution and signed-checksum authentication do not require buying these services. macOS without Developer ID/notarization can involve additional user approval, and browser stores still enforce their extension signing/install rules.

## 4. Optional platform-recognized signing

| Platform | Publication approach |
| --- | --- |
| Windows | Sign every distributed EXE, including the native helper, and the finished installer using Authenticode with a publicly trusted, identity-validated code-signing service/certificate. Timestamp signatures and verify the finished artifacts. |
| macOS | Sign embedded executables and the app with Developer ID Application, sign a PKG with Developer ID Installer, use hardened runtime as required, submit for notarization and staple the returned ticket to supported deliverables. |
| Linux | Publish release checksums with a cryptographic signature whose verification key is distributed through your official channels. For an APT/RPM repository, implement the corresponding repository/package signing and key-management process. |

Windows signing-provider eligibility depends on jurisdiction and individual/organization status. Check [Microsoft’s current signing options](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/code-signing-options); an eligible managed signing provider or trusted CA-backed certificate is appropriate. Self-signed testing certificates are not equivalent to public publisher verification. New signed Windows releases may still show SmartScreen reputation prompts.

For macOS, enroll with the intended publisher identity in the [Apple Developer Program](https://developer.apple.com/programs/), obtain the appropriate [Developer ID certificates](https://developer.apple.com/help/account/certificates/create-developer-id-certificates), and follow [Apple’s notarization procedure](https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution). Do not instruct public users to disable platform security controls.

## 5. Build and distribute a traceable release

Build from an identified source tag on each supported OS/architecture, using the committed lockfile. Run the test/quality checks plus clean-machine install, upgrade, uninstall and real-browser integration checks. Include dependency/license information and document the supported platform versions. Keep the signing step isolated from untrusted pull-request jobs; prefer hardware/cloud protected keys and short-lived CI authentication where available.

Sign completed artifacts, then generate checksums of the final signed files. Publish those files, signed checksums and release notes from the official HTTPS download page/repository release. A hash alone detects changed bytes only when the expected hash is trusted; it does not establish publisher identity. A signature establishes identity/integrity within its trust model, not that the program is free of bugs or malicious behavior.

Before implementing automatic desktop updates, define a signed update manifest, pin the verification public key in the app, verify packages before installation, and handle version rollback/key rotation. The current code has no production updater.

## Immediate prerequisites

- Publisher name, support email and official domain/repository.
- Chrome, Edge and Mozilla developer accounts.
- An encrypted Minisign release key/public key for the free workflow; optional SignPath application for Windows. Apple membership remains optional for a later notarized release.
- Clean-machine installation/upgrade/removal validation for the provided installer templates.

Enter credentials only into the official service or your secret manager; they are not needed in chat. Account enrollment, identity verification, store review and signing cannot be replaced by repository configuration alone.
