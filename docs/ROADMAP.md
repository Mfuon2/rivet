# Roadmap

Rivet prioritizes download integrity, acceleration, a compact desktop workflow and straightforward browser integration.

## Download engine

- Durable transfer checkpoints and per-part integrity checks.
- Adaptive splitting/rebalancing of active ranges.
- Global and per-host connection budgets and fair bandwidth limits.
- Single-stream retries and Retry-After handling.
- Broader server compatibility, recovery and performance testing.

## Desktop workflow

- Toolbar customization and complete keyboard/accessibility review.
- Recurring schedules, named queues and synchronization.
- Category/destination rules, page-wide batches and import/export.
- Native file dialogs, tray support, notifications and drag/drop.
- Translation catalogs and right-to-left layouts.

## Browser and media

- Published Chrome/Edge/Firefox extensions and signed Firefox distribution.
- Clean-machine native-host registration, extension update and removal tests.
- Optional browser download interception with duplicate handling.
- Keychain-backed, per-origin authenticated capture.
- HLS/DASH segment assembly, variants, audio tracks and subtitles.
- Safari integration and additional browser adapters.

## Protocols and automation

- FTP/FTPS and additional authentication/proxy protocols.
- Site crawling with depth/domain boundaries and persistent projects.
- ZIP inspection and optional malware-scanner integration.

## Releases

- Platform signing and notarization where available.
- Clean-machine install, upgrade and removal coverage.
- Dependency/license review, reproducibility checks and build provenance.
- Signed update manifests with key rotation and rollback handling.

See [capabilities](CAPABILITIES.md) for implemented features. Planned items should not be represented as completed functionality.
