# Rivet privacy statement

Draft for the current implementation. Publisher contact: **to be configured before store submission**.

Rivet has no developer-operated telemetry, advertising service, activation service or required remote account. Downloads and browser integration are processed locally, except for requests to the servers the user chooses to download from.

The browser extension passes a selected HTTP/HTTPS URL to the locally installed Rivet native helper after a user action. Optional media discovery observes URLs and response content types only for explicitly enabled origins. Detected links are held in browser-worker memory; enabled origins persist in browser local storage. No browser cookies or passwords are forwarded.

The desktop app stores source URLs, destination paths, download state, descriptions and checksums in a local SQLite database. Resume state can also contain source URLs. URLs may include private access tokens. A successful native-helper exchange stores the extension ID and time locally to support setup diagnostics. No diagnostic receipt contains a page URL or browser password.

Starting a download contacts its source server and may follow permitted redirects. Those servers receive normal network information, such as the client's IP address and request URL, and follow their own privacy policies. Downloading an extension from a browser store is subject to that store's policies.

Removing an extension removes its browser storage according to the browser's behavior. Rivet queue/history and downloaded/partial files remain local until the user deletes them; uninstalling the app does not promise to delete those files. URLs copied from properties can include secret query parameters.

This statement must be updated if future versions add authenticated capture, crash reporting, update requests or other data flows. Review it against the released code before publishing it as a store privacy policy.
