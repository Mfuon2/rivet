# Rivet privacy statement

Last updated: October 29, 2026.

Publisher: Mfuon2, maintainer of Rivet. Contact: https://github.com/Mfuon2/rivet/issues.

Rivet has no developer-operated telemetry, advertising service, activation service or required remote account. Download management and browser integration are processed locally. Network requests are made to the selected download servers and, during extension setup, the official store or GitHub release pages described below.

The browser extension passes a selected HTTP/HTTPS URL to the locally installed Rivet native helper after a user action. Optional media discovery observes URLs and response content types only for explicitly enabled origins. Detected links are held in browser-worker memory; enabled origins persist in browser local storage. No browser cookies or passwords are forwarded.

On enabled sites, a bundled isolated content script inspects standard video/audio elements and their media source URLs to display a Rivet download button. A selected direct media URL can refer to a different host, such as a CDN. Media URLs are sent to the local helper only after the user chooses a download. The widget does not read unrelated page text or form fields and does not send page content to a developer-operated service. Revoking site access removes its controls.

Desktop setup checks known browser-profile extension registration metadata locally to determine whether Rivet is installed or disabled. It does not edit browser profile preferences or collect browsing history. When no store URL is configured, setup may request the known public Chrome store listing to check availability. The store receives normal HTTP connection information; browsing history and download URLs are not included in this check. Clicking an installation or GitHub ZIP link contacts that service under its own privacy policy.

The extension makes a lightweight native connection check locally about once a minute, and retries while its setup page is connecting. These checks contain no page URL and create no download job. Their extension ID and time are stored locally as connection diagnostics.

The desktop app stores source URLs, destination paths, download state, descriptions and checksums in a local SQLite database. Resume state can also contain source URLs. URLs may include private access tokens. A successful native-helper exchange stores the extension ID and time locally to support setup diagnostics. No diagnostic receipt contains a page URL or browser password.

Starting a download contacts its source server and may follow permitted redirects. Those servers receive normal network information, such as the client's IP address and request URL, and follow their own privacy policies. Downloading an extension from a browser store is subject to that store's policies.

Removing an extension removes its browser storage according to the browser's behavior. Rivet queue/history and downloaded/partial files remain local until the user deletes them; uninstalling the app does not promise to delete those files. URLs copied from properties can include secret query parameters.

Rivet uses URL and media information only to provide the download features described here. Rivet does not sell this information, use it for advertising or creditworthiness decisions, or share it with unrelated services. Rivet's use and transfer of information obtained through Chrome APIs adheres to the Chrome Web Store User Data Policy, including its Limited Use requirements.

This policy will be updated if Rivet adds authenticated capture, crash reporting, update requests or other data flows.
