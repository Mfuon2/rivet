# Browser setup

Rivet detects the default HTTPS browser, checks known extension registrations without editing browser profiles, prepares its bundled extension files, and registers or repairs its native helper for the current user. Setup runs off the UI thread. Open **Browsers → Check and configure my default browser automatically** to repeat it, or choose another supported browser in the same dialog.

## Automatic setup

1. Open Rivet. It prepares the desktop connection and distinguishes an installed, disabled, missing or unconfirmed extension.
2. When the store listing is available, Rivet opens its installation page. On supported macOS/Windows Chromium browsers it also prepares a normal external-install request. Approve the extension in the browser; Rivet never approves permissions or uses enterprise force-install policies on your behalf. Linux and Firefox use their normal store installation pages.
3. The extension opens its connection page on first installation and connects automatically. An installed extension also makes a lightweight local connection check once a minute. Rivet watches fresh connection receipts and marks setup complete without a manual Check connection step.
4. Choose **Continue to Rivet** or **Restart Rivet**. A restart is normally unnecessary; the restart button is unavailable while transfers are active.

If Rivet finds a disabled extension, it opens the browser's extensions page so you can enable it. Browser approval is still required. Missing files, unreadable/custom profiles and old receipt history are not reported as a verified current connection. Continuing before connection defers the guide for that browser; local helper checks still run, and **Browsers** can reopen setup.

Rivet checks the known Chrome store item for public availability when an install URL has not yet been configured. An unpublished/error listing is not treated as installable. Firefox needs a Mozilla-signed add-on for permanent installation. Store approval is separate from GitHub releases.

## Automatic package preparation

When an extension is missing and no public store listing is available, Rivet downloads the ZIP matching the installed application version and browser into private temporary storage. It authenticates the release checksum manifest with its embedded Minisign key, verifies the ZIP checksum, and validates all entries, size limits, manifest version and extension identity before changing the prepared files. Symlinks, unexpected paths and incomplete packages are rejected.

The extension is unpacked into the permanent application-data folder automatically. Temporary archive files are removed; the browser never loads its extension from an expiring temp directory. A per-version preparation receipt avoids repeating the download. If the network is unavailable, the trusted copy embedded in the installed app remains ready. Rivet automatically opens browser setup and the prepared folder once, and watches the live connection. **Continue in Chrome/Edge/Chromium/Firefox** reopens this flow; **Continue to Rivet** postpones it.

Chrome-based browsers on macOS and Windows prohibit silent installation of unpublished extensions. Enable Developer mode, choose **Load unpacked**, then select the prepared folder that Rivet opens. Firefox requires Mozilla signing for permanent installation; its **Load Temporary Add-on** option can select the prepared `manifest.json` during development. These browser controls cannot be approved by Rivet. Store-approved installs use normal browser confirmation. Rivet does not modify profile preferences, force enterprise policies or ignore a browser-recorded removal.

**Connection details** contains diagnostics and an optional **Development setup** section with folder/address copy controls and a release link. Normal setup needs no terminal commands, extension-ID copying, manual ZIP download or manual extraction. Reload an already-loaded unpacked extension from the browser’s own controls after a development update.

Browser captures only queue URLs. Downloads start when you explicitly start them in Rivet or opt into automatic queue processing. The helper does not execute downloaded files or launch a shell.

## Advanced source builds and custom identities

The following commands are only for developers or custom extension identities; normal packaged setup performs helper registration automatically.

### Build

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

The shipped unpacked Chromium package uses a fixed development ID. Only custom identities need manual allowlist configuration. Firefox’s development ID is `rivet@rivet.local`. Temporary Firefox add-ons disappear after a restart. Published distribution and Firefox signing are not configured. Chrome-family browsers such as Brave/Opera have additional host-location and distribution differences; they are not yet validated Rivet targets.

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

### Native registration reference

Rivet writes these per-user registrations automatically; they are listed here for troubleshooting:

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

The extension connects automatically. Its **Finish setup** page retries the connection, and **Check connection** remains available for troubleshooting. A successful check sends only `{"type":"ping"}` to the helper and creates no job or download folder. It verifies helper connectivity, not whether a desktop window is open or whether a download site will accept a request. If you use a development `RIVET_DATABASE`, ensure the browser-launched helper and desktop resolve the same database; environment variables set only in a terminal are not automatically inherited by an already-running browser.

 Right-click a link and choose Send link to Rivet, or use the popup. Files are queued under the user's Downloads/Rivet directory with generated filename prefixes.

Registration and protocol references: [Chrome native messaging](https://developer.chrome.com/docs/extensions/develop/concepts/native-messaging), [Mozilla native messaging](https://developer.mozilla.org/en-US/docs/Mozilla/Add-ons/WebExtensions/Native_messaging).

Media discovery: open an HTTP/HTTPS site, click Enable for this site, grant the permission, reload the page and start playback. The popup lists detected media with access-token query parameters hidden from display. Links remain in worker memory only; service-worker suspension can clear them. Use Disable to remove the site grant and clear current detections.

On enabled sites, playing a standard HTML video or audio element shows **Download with Rivet** beside the player. Open it and choose the file to queue. The widget also attaches to players added after page load and can be dismissed per player. Disabling the site or revoking its permission removes the widget. The `scripting` permission installs the bundled isolated content script only on enabled origins; no remote scripts are loaded.

Direct HTTP/HTTPS media sources can be queued, including sources hosted on a CDN. Blob URLs are never sent to the desktop helper: the widget instead offers detected links from the same frame, if available. Those links may belong to another player, so the user chooses explicitly. HLS/DASH choices are labelled **Save playlist file**; they do not assemble a complete video. Protected media, closed shadow-root players, browser-owned pages and native full-screen video overlays are not supported. Frames require their own enabled origin when different from the page.

After starting a queued download in Rivet, click its active row (or **Downloads → Download progress…**) to open the live segment popup. It shows actual byte ranges, downloaded bytes, segment states and total progress. **Pause** preserves partial data, **Resume download** continues when validated range resume is supported, and **Close** leaves the transfer running. A non-resumable server uses one stream and restarts on resume.

No cookies or request credentials are forwarded in this version. HLS/DASH playlists are detected but not assembled. Installed Chrome integration has been exercised in a test profile with a real direct MP4 transfer; complete cross-browser/OS coverage remains separate.

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

The separate `chrome_store_extension_id` records the publisher's draft Chrome item ID. Native-host registration accepts that ID alongside the development ID, and setup recognizes connections from either. The Chrome store upload omits the development manifest key. Store installation URLs remain unset until the listing is approved.

Default detection supports stable Chrome, Edge, Chromium and Firefox installations at standard locations. Safari, forks, custom app locations, custom profile roots and sandboxed distributions need separate integration work. OS-level registration/installation interactions are not yet validated on Windows/Linux. `RIVET_SETUP_ROOT` redirects the home-directory registration path for isolated macOS/Linux previews; Windows registry setup is deliberately disabled in isolated preview mode.

## Chrome Web Store upload

Build the dedicated upload ZIP:

```sh
python3 scripts/package_browser.py --chrome-store
```

Upload `dist/rivet-chrome-store.zip` to the Chrome Developer Dashboard. The regular `rivet-chromium.zip` is a development package and retains a `key` field; the store rejects that field in uploads. The dedicated upload excludes `key` and `update_url`, with `manifest.json` at the ZIP root and the same popup, scripts and icons.

After creating the store item, obtain its extension ID and public key from the dashboard. Update the production native-host allowlist and distribution configuration, keep development identity aligned with the store public key, and rebuild/test the desktop packages before submitting for review. A successful ZIP upload is not store approval and does not yet make the store-assigned ID compatible with existing development installers.
