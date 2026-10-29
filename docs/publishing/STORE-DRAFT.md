# Browser-store submission draft — not submitted

Name: Rivet — Download Manager
Summary: Send download links to your local Rivet queue.
Publisher: **MAINTAINER TO COMPLETE**
Support email: **MAINTAINER TO COMPLETE**
Repository: https://github.com/Mfuon2/rivet
Privacy policy: publish the reviewed PRIVACY.md content at a stable public URL.

## Description

Rivet connects your browser to the open-source Rivet desktop download manager. Right-click a link or direct audio/video file to queue it locally. The popup lets you send a URL, check the native helper connection, and optionally enable direct-media discovery for a chosen site.

The desktop app and its registered native helper are required. Captured links enter the local queue; start them in Rivet. Session-cookie forwarding, automatic browser-download takeover and HLS/DASH video assembly are not available in this version. Safari is not supported.

## Permission explanations

- contextMenus: user-triggered link and media handoff.
- nativeMessaging: queue URLs and check the installed Rivet native helper.
- activeTab: identify the current tab when the popup is used.
- storage: remember the origins the user enabled for media discovery.
- webRequest: observe response URLs/content types for optional direct-media discovery; gated by explicit site permission.
- Optional HTTP/HTTPS origins: granted for a site only when the user enables discovery; not pre-granted for all sites.

Selected download URLs leave the browser for the local Rivet helper and are stored locally. Rivet's engine contacts the selected URL's server when the download starts. URLs can contain private query tokens; disclose this rather than declaring that the extension handles no data. No cookies, passwords, telemetry or advertising data are forwarded.

## Reviewer instructions

Install the matching desktop/native-helper package; launch Rivet to prepare default-browser registration. Install the extension, use Check connection, then queue a public HTTP/HTTPS file link. Confirm it appears locally and does not automatically execute. Optional media discovery: grant a site, reload and play a direct-media file; disable the site and confirm detections clear. Public store IDs must match the packaged host allowlist.

## Submission blockers

- Final publisher/support/privacy-policy URLs.
- Store icons and extension screenshots.
- Production IDs/public keys, actual signed installers/extensions and clean-machine test instructions.
- Current Firefox data-transmission declaration review. Native messaging transfers URLs outside the local browser; do not select “none” without checking Mozilla's applicable policy. The current development manifest is not represented as store-ready.
- Installed Chrome/Edge/Firefox interaction, permission and extension-update testing.

Chrome: one-time registration fee; Edge: free registration; Firefox: Mozilla signing, no purchased code-signing certificate. Submit through each official developer dashboard and retain the same listing/identity for updates.
