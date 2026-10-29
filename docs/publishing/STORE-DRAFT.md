# Browser-store submission draft — not submitted

Name: Rivet — Download Manager
Summary: Send download links to your local Rivet queue.
Publisher: Mfuon2
Support: https://github.com/Mfuon2/rivet/issues
Repository: https://github.com/Mfuon2/rivet
Privacy policy: https://github.com/Mfuon2/rivet/blob/main/PRIVACY.md

## Upload package

For Chrome, run `python3 scripts/package_browser.py --chrome-store` and upload `dist/rivet-chrome-store.zip`. The standard development ZIP retains a fixed manifest `key`; it is not a Chrome Web Store upload package. The Chrome draft item ID from the publisher dashboard is included in the desktop/native-helper allowlist alongside the separate development ID. Store installation URLs remain unset until approval. A store public key is needed only if unpacked builds should use the same identity as the store item.

## Description

Rivet connects your browser to the open-source Rivet desktop download manager. Right-click a link or direct audio/video file to queue it locally. The popup lets you send a URL, check the native helper connection, and optionally enable direct-media discovery for a chosen site.

On enabled sites, playing a standard video/audio element shows a Download with Rivet widget. Choose a direct file link to send it to the local queue. Blob players offer detected same-frame links when available; protected media is not supported. Playlist options save the playlist file only.

The desktop app and its registered native helper are required. Captured links enter the local queue; start them in Rivet. Session-cookie forwarding, automatic browser-download takeover and HLS/DASH video assembly are not available in this version. Safari is not supported.

## Permission explanations

- contextMenus: user-triggered link and media handoff.
- nativeMessaging: queue URLs and check the installed Rivet native helper.
- activeTab: identify the current tab when the popup is used.
- storage: remember the origins the user enabled for media discovery.
- webRequest: observe response URLs/content types for optional direct-media discovery; gated by explicit site permission.
- scripting: inject the packaged media-widget content script only on explicitly enabled sites with current host permission, so users can choose a media link beside the player.
- Optional HTTP/HTTPS origins: granted for a site only when the user enables discovery; not pre-granted for all sites.

Selected download URLs leave the browser for the local Rivet helper and are stored locally. Rivet's engine contacts the selected URL's server when the download starts. URLs can contain private query tokens; disclose this rather than declaring that the extension handles no data. No cookies, passwords, telemetry or advertising data are forwarded.

## Reviewer instructions

Install the matching desktop/native-helper package; launch Rivet to prepare default-browser registration. Install the extension, use Check connection, then queue a public HTTP/HTTPS file link. Confirm it appears locally and does not automatically execute. Optional media discovery: grant a site, reload and play a direct-media file; disable the site and confirm detections clear. Public store IDs must match the packaged host allowlist.

## Submission blockers

- Enter and verify the publisher contact email in Chrome dashboard Settings, complete the Privacy practices fields and certify the declarations.
- Extension icons now use the maintainer-supplied Rivet globe/arrow artwork. Final extension screenshots are still needed.
- Store approval/installation URLs and reviewer installation instructions for the matching native package.
- Current Firefox data-transmission declaration review. Native messaging transfers URLs outside the local browser; do not select “none” without checking Mozilla's applicable policy. The current development manifest is not represented as store-ready.
- Installed Chrome/Edge/Firefox interaction, permission and extension-update testing.

Chrome: one-time registration fee; Edge: free registration; Firefox: Mozilla signing, no purchased code-signing certificate. Submit through each official developer dashboard and retain the same listing/identity for updates.
