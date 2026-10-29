# Browser setup

Rivet now checks the default HTTPS browser before showing the download manager. It locates its sibling native helper, prepares its bundled extension files in the app data directory, and registers its native host for the current user automatically. Matching manifest files are left untouched; missing or outdated Rivet registration is repaired. The work runs off the UI thread. Use **Browsers → Check and configure my default browser automatically** to repeat setup.

The startup screen distinguishes local setup from confirmed browser contact. It cannot reliably prove that an extension is currently installed/enabled in every browser profile. An extension handshake records only its ID and time, not page URLs. Receipt history is not treated as proof of installation. Continuing to Rivet remembers the browser/extension identity; checks still run at the next launch, while an acknowledged successful local setup does not block opening the manager.

Chrome/Edge/Chromium development packages now share a fixed public manifest key and ID, so no extension-ID copying is needed. Earlier unpacked installs may need reloading after this change. Public-store IDs/URLs are not configured yet. Development builds therefore show a one-time local load step; browser approval is not bypassed. Firefox's temporary add-on is removed after Firefox restarts.

Open **Rivet → Browsers** (or Help → Browser integration) for advanced or additional-browser setup. Rivet provides its own extension.

Install in this order: desktop helper → extension → native-host registration → connection check → send a link. Store publication and guided installers remain pending. Safari requires a separate adapter and is not supported yet.

The native helper only queues URLs. Downloads start when you explicitly start them in Rivet or opt into automatic processing. It does not launch a shell or execute downloaded files.

## Build

```sh
cargo build --release -p rivet-cli --bins
python3 scripts/package_browser.py
```

| Browser | Extension setup |
| --- | --- |
| Chrome | Open `chrome://extensions`, enable Developer mode, choose **Load unpacked**, select `dist/chromium`. |
| Edge | Open `edge://extensions`, enable Developer mode, choose **Load unpacked**, select `dist/chromium`. |
| Chromium | Open `chrome://extensions`, enable Developer mode, choose **Load unpacked**, select `dist/chromium`. |
| Firefox | Open `about:debugging#/runtime/this-firefox`, choose **Load Temporary Add-on**, select `dist/firefox/manifest.json`. |

Record the actual installed extension ID in each Chromium browser; IDs can differ across installations. Firefox’s development ID is `rivet@rivet.local`. Temporary Firefox add-ons disappear after a restart. Published distribution and Firefox signing are not configured. Chrome-family browsers such as Brave/Opera have additional host-location and distribution differences; they are not yet validated Rivet targets.

Generate a native-host manifest using the **dedicated helper**, not the `rivet` CLI binary:

```sh
target/release/rivet host-manifest \
  --executable /absolute/path/to/rivet-native-host \
  --extension-id CHROMIUM_EXTENSION_ID > org.rivet.downloads.json

# Firefox
target/release/rivet host-manifest \
  --executable /absolute/path/to/rivet-native-host \
  --extension-id rivet@rivet.local --firefox > org.rivet.downloads.json
```

On Windows, executable names end in `.exe`; use PowerShell redirection with UTF-8 output when saving JSON. The native manifest path is absolute and browser registration must point at that JSON file. The browser launches `rivet-native-host` directly and enforces the extension allowlist in the manifest. The helper uses the default Rivet database; custom CLI databases do not receive browser captures.

## Register per user

These are manual setup steps, not actions performed by this repository:

| Browser / OS | Manifest location or registration |
| --- | --- |
| Chrome macOS | `~/Library/Application Support/Google/Chrome/NativeMessagingHosts/org.rivet.downloads.json` |
| Edge macOS | `~/Library/Application Support/Microsoft Edge/NativeMessagingHosts/org.rivet.downloads.json` |
| Chromium macOS | `~/Library/Application Support/Chromium/NativeMessagingHosts/org.rivet.downloads.json` |
| Edge Linux | `~/.config/microsoft-edge/NativeMessagingHosts/org.rivet.downloads.json` |
| Chrome Linux | `~/.config/google-chrome/NativeMessagingHosts/org.rivet.downloads.json` |
| Chromium Linux | `~/.config/chromium/NativeMessagingHosts/org.rivet.downloads.json` |
| Firefox macOS | `~/Library/Application Support/Mozilla/NativeMessagingHosts/org.rivet.downloads.json` |
| Firefox Linux | `~/.mozilla/native-messaging-hosts/org.rivet.downloads.json` |
| Chrome Windows | Default value of `HKCU\Software\Google\Chrome\NativeMessagingHosts\org.rivet.downloads` = absolute manifest path |
| Edge Windows | Default value of `HKCU\Software\Microsoft\Edge\NativeMessagingHosts\org.rivet.downloads` = absolute manifest path |
| Chromium Windows | Default value of `HKCU\Software\Chromium\NativeMessagingHosts\org.rivet.downloads` = absolute manifest path |
| Firefox Windows | Default value of `HKCU\Software\Mozilla\NativeMessagingHosts\org.rivet.downloads` = absolute manifest path |

Different browser packaging/sandbox distributions may use different registration roots. Create the native-messaging host directory if needed, then place the generated manifest there. Keep the built helper at its registered absolute path. Never point the manifest at the `rivet` CLI or the desktop executable. Restart/reload the extension after registering.

## Check and use

Open the Rivet extension popup and click **Check connection**. A successful check sends only `{"type":"ping"}` to the helper and creates no job or download folder. It verifies helper connectivity, not whether a desktop window is open or whether a download site will accept a request. If you use a development `RIVET_DATABASE`, ensure the browser-launched helper and desktop resolve the same database; environment variables set only in a terminal are not automatically inherited by an already-running browser.

 Right-click a link and choose Send link to Rivet, or use the popup. Files are queued under the user's Downloads/Rivet directory with generated filename prefixes.

Registration and protocol references: [Chrome native messaging](https://developer.chrome.com/docs/extensions/develop/concepts/native-messaging), [Mozilla native messaging](https://developer.mozilla.org/en-US/docs/Mozilla/Add-ons/WebExtensions/Native_messaging).

Media discovery: open an HTTP/HTTPS site, click Enable for this site, grant the permission, reload the page and start playback. The popup lists detected media with access-token query parameters hidden from display. Links remain in worker memory only; service-worker suspension can clear them. Use Disable to remove the site grant and clear current detections.

No cookies or request credentials are forwarded in this version. HLS/DASH playlists are detected but not assembled. Browser-specific behavior has not yet been validated in a real installed browser session.

Discovery is limited to explicitly enabled media URL origins. Cross-origin CDN requests need their own grant; requesting and managing those grants from the page UI is planned. Enabled-site origins persist locally; captured URLs stay in worker memory only. The activeTab permission alone does not enable discovery.

## Troubleshooting

- **Host not found:** confirm the browser/channel-specific manifest location and the exact host name `org.rivet.downloads`.
- **Host failed to start:** confirm the executable exists, is executable on Unix, and has an absolute path. Rebuild the helper after updating Rivet.
- **Access forbidden:** use the installed Rivet extension ID in `allowed_origins` (`chrome-extension://ID/`) or Firefox’s ID in `allowed_extensions`. Use the ID of your Rivet listing.
- **Connected but no file starts:** captures enter the queue. Choose Resume for selected jobs, or Start queue to process due jobs.
- **403 or login required:** cookie/session forwarding is not implemented; a connection check does not grant site authentication.

Rivet’s host registration follows [Chrome](https://developer.chrome.com/docs/extensions/develop/concepts/native-messaging), [Edge](https://learn.microsoft.com/en-us/microsoft-edge/extensions/developer-guide/native-messaging), and [Firefox](https://developer.mozilla.org/en-US/docs/Mozilla/Add-ons/WebExtensions/Native_messaging) documentation. Actual installed-browser end-to-end checks remain pending.

## Release identity configuration

`browser/distribution.json` contains the Chromium development ID, an optional distinct published Edge ID, Firefox ID and optional official installation URLs. Populate these from your publisher dashboards before release. Chrome’s public manifest key must match the store-issued ID; changing a development public key is not a substitute for publisher-account signing. `scripts/package_browser.py` checks the Chromium key/ID and Firefox ID before packaging. The public development key is not a private signing credential. See PUBLISHING.md.

Default detection supports stable Chrome, Edge, Chromium and Firefox installations at standard locations. Safari, forks, custom app locations, custom profile roots and sandboxed distributions need separate integration work. OS-level registration/installation interactions are not yet validated on Windows/Linux. `RIVET_SETUP_ROOT` redirects the home-directory registration path for isolated macOS/Linux previews; Windows registry setup is deliberately disabled in isolated preview mode.

## Chrome Web Store upload

Build the dedicated upload ZIP:

```sh
python3 scripts/package_browser.py --chrome-store
```

Upload `dist/rivet-chrome-store.zip` to the Chrome Developer Dashboard. The regular `rivet-chromium.zip` is a development package and retains a `key` field; the store rejects that field in uploads. The dedicated upload excludes `key` and `update_url`, with `manifest.json` at the ZIP root and the same popup, scripts and icons.

After creating the store item, obtain its extension ID and public key from the dashboard. Update the production native-host allowlist and distribution configuration, keep development identity aligned with the store public key, and rebuild/test the desktop packages before submitting for review. A successful ZIP upload is not store approval and does not yet make the store-assigned ID compatible with existing development installers.
