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
| [0044](0044-name-platform-unlock-backend.md) | Name the platform unlock backend | done | 2026-11-16 |
| [0045](0045-backend-install-requires-steam.md) | Backend install requires Steam | done | 2026-11-16 |
| [0046](0046-backup-progress-streaming.md) | Stream backup progress through large files | done | 2026-11-16 |
| [0047](0047-safe-backup-overwrite.md) | Safe backup overwrite | done | 2026-11-16 |
| [0048](0048-backup-free-space-preflight.md) | Backup free-space preflight | done | 2026-11-16 |
| [0049](0049-desktop-task-notifications.md) | Desktop notifications for long tasks | done | 2026-11-16 |
| [0050](0050-backup-restore-activity-line.md) | Current-file activity for backup and restore | done | 2026-11-16 |
| [0051](0051-backup-fdatasync.md) | Faster backup archives with fdatasync | done | 2026-11-16 |
| [0052](0052-repair-backup-metadata.md) | Repair backup metadata without redoing content | done | 2026-11-16 |
| [0053](0053-repair-backup-fixes.md) | Repair backup fixes from installed fixes | done | 2026-11-16 |
| [0054](0054-updates-from-bundle.md) | Updates tab reads the persistent bundle | done | 2026-11-16 |
| [0055](0055-dlc-queue-and-install.md) | DLC selection, queue, install, and removal | done | 2026-11-16 |
| [0056](0056-dlc-sizes.md) | DLC sizes | done | 2026-11-16 |
| [0057](0057-dlc-ddm-direct.md) | Direct DepotDownloaderMod for DLC | done | 2026-11-16 |
| [0058](0058-dlc-toggle-without-reload.md) | DLC toggle without reloading the tab | done | 2026-11-16 |
| [0059](0059-dlc-download-manager-state.md) | DLC download manager state | done | 2026-11-16 |
| [0060](0060-rebase-stale-launch-target.md) | Rebase stale launch targets | done | 2026-11-16 |
| [0061](0061-existing-prefix-without-steam.md) | Use an existing Steam-style prefix without Steam | done | 2026-11-16 |
| [0062](0062-dlc-content-group-progress.md) | DLC content-group progress | done | 2026-11-16 |
| [0063](0063-dlc-downloading-button.md) | Keep the Downloading button while DLC runs | done | 2026-11-16 |
| [0064](0064-dlc-primary-button-progress.md) | Primary button reflects DLC download progress | done | 2026-11-16 |
| [0065](0065-cache-dlc-catalogue.md) | Cache the DLC catalogue | done | 2026-11-16 |
| [0066](0066-cache-updates-tab.md) | Cache the Updates tab | done | 2026-11-16 |
| [0067](0067-per-game-cache-sidecar.md) | Per-game cache sidecar | active | 2026-11-16 |
| [0068](0068-umutron-parity-backlog.md) | UmuTron feature parity backlog | draft | 2026-11-16 |
| [0069](0069-direct-igdb-store.md) | Direct IGDB Store with per-user backend token | done | 2026-11-16 |
| [0070](0070-open-source-release.md) | Open-source ready distribution and contributor docs | done | 2026-11-16 |
| [0071](0071-signin-progress.md) | Visible sign-in progress | done | 2026-11-16 |
| [0072](0072-windows-drive-discovery.md) | Windows library drive discovery | done | 2026-11-17 |
| [0073](0073-hidden-console.md) | Hide helper console windows on Windows | active | 2026-11-16 |
| [0074](0074-winfsp-bootstrap.md) | Windows prerequisite bootstrap | draft | 2026-11-17 |
| [0075](0075-program-files-install.md) | Install Windows build under Program Files (x86) | active | 2026-11-16 |
| [0076](0076-windows-platform-ui.md) | Hide Linux-only Proton UI on Windows | active | 2026-11-17 |
| [0077](0077-windows-native-launch.md) | Native Windows game launch | done | 2026-11-17 |
| [0079](0079-windows-smd-native.md) | Windows SMD native downloader | superseded | 2026-11-17 |
| [0081](0081-windows-vcredist.md) | Install Visual C++ runtime on Windows | done | 2026-11-17 |
| [0082](0082-windows-drive-label.md) | Normalize Windows drive labels | active | 2026-11-17 |
| [0083](0083-windows-ddm-dotnet.md) | Windows downloads use DepotDownloaderMod + .NET | done | 2026-11-17 |
| [0084](0084-store-warmup-flash.md) | Prevent Store flash on app launch | active | 2026-11-17 |
| [0085](0085-windows-smoke-verification.md) | Windows smoke verification | done | 2026-11-17 |
| [0086](0086-running-game-state.md) | Running game state on Windows | active | 2026-11-17 |
| [0087](0087-linux-backend-repair.md) | Linux SLSsteam-only repair | done | 2026-11-17 |
| [0088](0088-backend-install-without-steam.md) | Do not offer backend install without Steam | done | 2026-11-17 |
| [0089](0089-repair-marker.md) | Repair completion marker must not wait on background installers | done | 2026-11-17 |
| [0090](0090-minimal-doctor.md) | Minimal Doctor and Windows backend install | done | 2026-11-17 |
| [0091](0091-library-sync.md) | Sync SteamTron library to Steam | done | 2026-11-17 |
| [0092](0092-restart-and-vc-detection.md) | Restart guard and x64 prerequisite detection | done | 2026-11-17 |
| [0093](0093-steam-dlc-parity.md) | Match Steam DLC list and sizes | active | 2026-11-17 |
| [0094](0094-dlc-size-column.md) | Keep the DLC size column on one line | done | 2026-11-17 |
| [0095](0095-skip-installed-prereq-extraction.md) | Skip extracting installed prerequisites | done | 2026-11-17 |
| [0096](0096-windows-union-overlay.md) | Layer fixes over Windows games with a WinFsp union | done | 2026-11-18 |
| [0097](0097-windows-launch-log.md) | Windows launch diagnostic log | done | 2026-11-18 |
| [0098](0098-windows-game-runtimes.md) | Windows game runtime provisioning | active | 2026-11-18 |
| [0099](0099-store-settings-loading.md) | Keep Settings and Store shells during loading | done | 2026-11-18 |
| [0100](0100-restored-artwork.md) | Restored game keeps its artwork | done | 2026-11-18 |
| [0101](0101-remove-steam-toggle.md) | Remove the per-game Steam-running toggle | done | 2026-11-18 |
| [0102](0102-doctor-backend-name.md) | Fix Doctor install action reference | done | 2026-11-18 |
| [0103](0103-windows-artwork-scope.md) | Windows artwork asset scope | done | 2026-11-18 |
| [0104](0104-bettersteamtools-1-0-9.md) | Install maintained BetterSteamTools | done | 2026-11-18 |
| [0105](0105-windows-stplug-registration.md) | Windows registration writes stplug-in | done | 2026-11-18 |
| [0106](0106-antivirus-exclusions.md) | Antivirus exclusions cover the unlock backend | done | 2026-11-18 |
| [0108](0108-windows-no-depot-manifests.md) | Windows must not publish depot manifests to Steam | done | 2026-11-18 |
| [0109](0109-windows-parity-link-tray-fixes.md) | Windows parity: link tabs, external fixes, close to tray | active | 2026-11-19 |
| [0110](0110-unlink-keeps-library-entry.md) | Unlink keeps the library entry | active | 2026-11-19 |
| [0111](0111-large-fixes-import.md) | Import large fixes archives | done | 2026-11-19 |
| [0112](0112-fixes-tar-format.md) | Fixes TAR import and export | done | 2026-11-19 |
| [0113](0113-linux-overlay-launch-env.md) | Linux overlay carries launch environment | active | 2026-11-19 |
| [0114](0114-game-named-fixes-backup.md) | Game-named fixes archive in backups | active | 2026-11-19 |
| [0115](0115-fixes-transfer-progress.md) | Progress for fixes import and export | done | 2026-11-19 |
| [0116](0116-linked-fixes-managed-location.md) | Managed fixes folder for linked games | done | 2026-11-19 |
| [0117](0117-fixes-launcher-overlay-lifetime.md) | Fixes-folder launchers and overlay lifetime | active | 2026-11-19 |
| [0118](0118-silent-installer-noop.md) | Silent installer exits without installing | draft | 2026-11-19 |
| [0119](0119-orphaned-backup-staging.md) | Orphaned backup staging after a killed backup | draft | 2026-11-19 |
| [0120](0120-launcher-exit-run-tracking.md) | Managed run tracking survives a launcher exit on Windows | done | 2026-11-19 |
| [0121](0121-portable-backup-name-safety.md) | Portable backup archives restore on Windows and Linux | active | 2026-11-21 |
| [0122](0122-playstation-runtime-provisioning.md) | Install a game's bundled PlayStation PC SDK runtime before launch | active | 2026-11-22 |
| [0123](0123-automatic-antivirus-exclusions.md) | Automatic Windows Security exclusions for fixes | active | 2026-11-22 |
| [0124](0124-restore-local-metadata.md) | Restore backups from their own metadata | active | 2026-11-22 |
| [0125](0125-restore-appmanifest-not-cache.md) | Restore selects the appmanifest, not its cache sidecar | active | 2026-11-22 |
| [0126](0126-about-section.md) | About section in General settings | active | 2026-11-22 |
| [0127](0127-restore-legacy-backups.md) | Restore legacy and incomplete backups | active | 2026-11-22 |
| [0128](0128-installed-files-actions.md) | Installed Files actions follow the install location | active | 2026-11-23 |
| [0129](0129-apply-launch-options.md) | Apply game launch options to every launch path | active | 2026-11-23 |
| [0130](0130-appmanifest-action-restored.md) | Restore the appmanifest action in Installed Files | active | 2026-11-23 |
| [0131](0131-bootstrapper-upgrade-retry.md) | Windows bootstrapper retries the inner installer after an upgrade | active | 2026-11-23 |
| [0132](0132-bundled-runtime-redistributables.md) | Prefer game-bundled redistributables for runtime provisioning | done | 2026-11-23 |
| [0133](0133-download-progress-aggregate.md) | Aggregate download progress across all depots | active | 2026-11-23 |
| [0134](0134-bootstrapper-silent-pluginsdir.md) | Bootstrapper initializes the plugins directory before extracting the inner setup | active | 2026-11-23 |
