# Per-feature PRDs

One PRD per feature or issue. [`docs/PRD.md`](../PRD.md) stays the product-wide
source of truth; the files here track individual work items, their acceptance
criteria, and the evidence that they were verified.

## Lifecycle

| Status | Meaning |
|---|---|
| `draft` | Problem agreed; requirements still changing |
| `active` | Implementation in progress |
| `done` | Acceptance criteria verified; evidence recorded |

Update the status and verification in the same commit as the implementation.

## Naming

`NNNN-<slug>.md` — four-digit sequence, lowercase hyphenated slug. Take the next
number from the index below and add the new row in the same commit.

Start every file from [`TEMPLATE.md`](TEMPLATE.md).

## Index

| ID | Title | Status | Updated |
|---|---|---|---|
| [0001](0001-prd-process.md) | Per-feature PRD tracking | done | 2026-11-14 |
| [0002](0002-nav-focus.md) | Restore keyboard and controller focus across routes | done | 2026-11-15 |
| [0003](0003-startup-steam-flash.md) | Remove startup Steam-installation error flash | done | 2026-11-15 |
| [0004](0004-default-home.md) | Launch on the Home screen | done | 2026-11-15 |
| [0005](0005-route-focus.md) | Independent route focus and Home rail looping | done | 2026-11-15 |
| [0006](0006-store-cache.md) | Use the local Store catalogue cache | done | 2026-11-15 |
| [0007](0007-detail-focus.md) | One visible focus and detail-page action focus | done | 2026-11-15 |
| [0008](0008-panel-nav-focus.md) | Store pane isolation and route-tab Up focus | done | 2026-11-15 |
| [0009](0009-store-card-open.md) | Open Store cards without waiting for the detail request | done | 2026-11-15 |
| [0010](0010-chip-focus-ring.md) | One focus ring on pill chips | done | 2026-11-15 |
| [0011](0011-fullscreen-escape.md) | Fullscreen Escape matches desktop back behavior | done | 2026-11-15 |
| [0012](0012-library-backup.md) | Library index backup and restore | done | 2026-11-15 |
| [0013](0013-guest-login.md) | Guest sign-in screen and default-browser login | done | 2026-11-15 |
| [0014](0014-unified-modals.md) | Unified dialog windows | done | 2026-11-15 |
| [0015](0015-home-focus-after-signin.md) | Keep Home focus after sign-in window refocus | done | 2026-11-15 |
| [0016](0016-dialog-focus-return.md) | Return focus to the dialog opener | done | 2026-11-15 |
| [0017](0017-tray.md) | Close to tray and restore from the tray icon | done | 2026-11-15 |
| [0018](0018-steamless-operations.md) | Steam-independent managed games | active | 2026-11-15 |
| [0019](0019-consolidated-backup.md) | One consolidated library backup | done | 2026-11-15 |
| [0020](0020-proton-defaults.md) | Proton and prefix defaults without Steam | done | 2026-11-15 |
| [0021](0021-stop-hang.md) | Stop game without freezing the window | done | 2026-11-15 |
| [0022](0022-smd-downloader.md) | Self-provisioned SMD downloader | done | 2026-11-15 |
| [0023](0023-native-fixes-tools.md) | Native Goldberg extraction and Steamless unpacking | done | 2026-11-15 |
| [0024](0024-download-manager.md) | Reopenable download manager with pause, resume, and cancel | done | 2026-11-15 |
| [0025](0025-uninstall-keeps-library.md) | Uninstall removes game files without removing the library entry | done | 2026-11-15 |
| [0026](0026-hero-scroll.md) | Full hero reveal and smooth focus scrolling | done | 2026-11-15 |
| [0027](0027-background-downloads.md) | Downloads continue after SteamTron exits | done | 2026-11-15 |
| [0028](0028-smd-manifest-fallback.md) | SMD manifest fallback completes in verbose mode | done | 2026-11-15 |
| [0029](0029-ddm-engine.md) | Use DepotDownloaderMod instead of SMD native engine | done | 2026-11-15 |
| [0030](0030-operation-manager.md) | Shared operation manager for backup, restore, and verification | done | 2026-11-15 |
| [0031](0031-store-retry.md) | Retry Store requests with a fresh client | done | 2026-11-15 |
| [0032](0032-direct-verify.md) | Verify with cached manifests through DepotDownloaderMod | active | 2026-11-15 |
| [0033](0033-portable-backup.md) | Portable backup folders (format 2) | active | 2026-11-15 |
| [0034](0034-per-game-library-export.md) | Per-game library metadata export | done | 2026-11-16 |
| [0035](0035-persistent-bundle-store.md) | Persistent global Lua/manifest bundle store | done | 2026-11-16 |
| [0036](0036-separate-appmanifest-lua-actions.md) | Remove the unused appmanifest action | done | 2026-11-16 |
| [0037](0037-appmanifest-backup-and-manifest-folders.md) | Appmanifest backups and per-game manifest folders | done | 2026-11-16 |
| [0038](0038-steam-installation-detection.md) | Reliable Steam installation detection | done | 2026-11-16 |
| [0039](0039-per-game-backup-bundle-fallback.md) | Per-game backup Lua/manifest fallback | done | 2026-11-16 |
| [0040](0040-populate-managed-appmanifest.md) | Populate managed appmanifest files | done | 2026-11-16 |
| [0041](0041-remove-legacy-restore.md) | Remove the legacy consolidated restore | done | 2026-11-16 |
| [0042](0042-folder-metadata-export-import.md) | ZIP metadata export and import | done | 2026-11-16 |
| [0043](0043-export-import-settings-ux.md) | Export/import settings UX | done | 2026-11-16 |
