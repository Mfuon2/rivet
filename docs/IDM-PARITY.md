# Product parity contract

Scope supplied by the user: all capabilities on [IDM's features page](https://www.internetdownloadmanager.com/features2.html), plus a modern, equally dense desktop workflow, three operating systems, open source, privacy, reliability and no activation. **Download acceleration is priority 1.**

The table paraphrases the feature headings; statuses and acceptance criteria describe Rivet engineering work. Vendor speed/safety marketing is not accepted as test evidence. The page's “load more” link resolved to the same features page during review.

| Requirement | Current implementation | Completion evidence still needed |
| --- | --- | --- |
| Faster transfers | Pooled connections and bounded range scheduling | Adaptive active splitting, throughput benchmarks, connection budgets |
| Website/media capture | Opt-in direct-media URL discovery | Authenticated transfers, streaming media assembly, site tests |
| Broad browser coverage | Original Chromium/Firefox packages and native helper | Installed Chrome/Edge/Firefox smoke tests; Safari adapter; compatibility policy for legacy browsers |
| Automatic link handoff | User-triggered queue handoff | Opt-in browser download takeover with interruption/duplicate handling |
| HTTP, HTTPS, FTP, MMS | HTTP/HTTPS engine | FTP/FTPS and MMS implementation and fixtures |
| Light/dark presentation | Light desktop default, selectable dark theme and browser popup system theme | Cross-platform contrast/accessibility review |
| Adaptive range allocation | Work stealing among preplanned ranges | Safe splitting/rebalancing of active ranges with durable checkpoints |
| Interrupted-transfer recovery | Strong-ETag resume and range validation | Power-loss checkpoints, per-part integrity and broader server compatibility |
| Timers, multiple queues and synchronization | Durable queue and one-time delays | Recurring start/stop schedules, named queues, sync and opt-in completion actions |
| Crawling/offline projects | Not implemented | Filters, depth/domain boundaries, scheduled project persistence |
| Configurable dense workspace | Compact menus, Lucide toolbar/tree/file icons, light/dark, selection/batch actions, sortable/resizable columns, visibility/order and saved preferences implemented | Toolbar customization, full keyboard/accessibility review and cross-platform interaction checks |
| Automatic file grouping | Planned | Type/site rules, custom groups and destination folders |
| Page-wide batch handoff | Planned | Preview/filter/deduplicate page links and queue selected items |
| Localized interface | English foundation | Translation catalogs, pluralization, RTL and locale tests |
| Application updates | Not implemented | Signed release manifests, user approval, rollback and changelogs |
| Bandwidth/data budgets | Not implemented | Global/per-host speed policies and rolling transfer quotas |
| Malware-scanner hooks | Not implemented | Configurable scanner execution and trustworthy failure states |
| Guided installation | Automatic startup default-browser detection, per-user host registration/repair, embedded extension preparation and connection receipts | Native OS installers/removal, published/signed extensions, actual browser install checks and cross-platform validation |
| Proxy compatibility | HTTP/SOCKS CLI option and environment settings | PAC, authenticated/enterprise/FTP proxy coverage |
| Login negotiation | Not implemented | Basic, Negotiate, NTLM, Kerberos plus keychain-backed secrets |
| Drag/drop workflows | Not implemented | Incoming links and outgoing file drags on each OS |
| Capture beyond browser APIs | Not implemented | Separate, optional platform adapters and signed privileged components |

Additional user enhancements: checksum verification, explicit secret handling, no tracking, accessibility, modern TLS, inspectable local state, reproducible builds, documented threat model and cross-platform validation. MIT licensing and absence of activation are product requirements, not premium options.

## UI reference

Reviewed [main-window documentation](https://www.internetdownloadmanager.com/support/using_idm/using_idm.html), [official screenshot](https://www.internetdownloadmanager.com/support/using_idm/pictures/main-w.png), and [scheduler documentation](https://www.internetdownloadmanager.com/support/idm-scheduler/idm_scheduler.html). Target hierarchy: menu strip → action toolbar → category/queue tree beside a compact sortable transfer list → selection/status details. New downloads and settings belong in dialogs, preserving space for transfer rows. Rivet uses its own visuals and branding.

## Browser/distribution references

- [Chrome listing](https://chromewebstore.google.com/detail/idm-integration-module/ngpampappnmepgilojfohadhhmbhlaek): context-link handoff and browser package distribution.
- [Firefox listing](https://addons.mozilla.org/en-US/firefox/addon/tonec-idm-integration-module/): context actions, download panel, interception and native application dependency. Its package is declared All Rights Reserved; it is not included in Rivet.
- [Edge listing](https://microsoftedge.microsoft.com/addons/detail/idm-integration-module/llbjbkhnmlidjebalopleeepgdfgcpec): reviewed URL; the browserless response exposed only site chrome, so details remain unverified.
- [Download/setup page](https://www.internetdownloadmanager.com/download.html): desktop setup followed by browser-specific integration. Rivet must provide its own installers and published extension IDs; no existing IDM ID will be reused.

## Priority order

1. **Acceleration and integrity:** controlled benchmarks; dynamic work allocation; connection reuse; adaptive active splitting; bandwidth fairness; disk I/O and reassembly profiling; protocol and power-loss tests.
2. **Dense desktop workflow:** familiar action placement and category hierarchy, table selection/batch actions, live rate/ETA, editable metadata, scheduling, light/dark and persistent column preferences.
3. **Browser/media parity:** real browser integration tests, page batches, opt-in interception, credentials, HLS/DASH and Safari.
4. **Remaining protocol/automation and release parity:** enterprise authentication/proxies, FTP/MMS, grabbing/synchronization, scanner hooks, updates, installers, localization and platform adapters.

All rows remain in scope. Implemented foundations must not be presented as complete parity.
