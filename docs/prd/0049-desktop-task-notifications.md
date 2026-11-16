# Desktop notifications for long tasks

- Status: done
- Date: 2026-11-16
- Areas: `crates/steamtron-desktop/{src/main.rs,ui/app.js}`,
  `crates/steamtron-desktop/Cargo.toml`

## Problem

Long backups, downloads, restores, verification, export, and import could
finish while the user was away or watching fullscreen video. The completion was
only visible inside SteamTron.

## Requirements

- R1: Send a desktop notification when backup, restore, verify, download,
  library export, or metadata import finishes, succeeds or fails.
- R2: Do not notify when the SteamTron window is focused.
- R3: Include the game title when the task is scoped to a game.
- R4: Use critical urgency so fullscreen/Do Not Disturb modes are more likely to
  surface the notification.
- R5: Do not notify on intentional cancellation.

## Verification

- Notification delivery was exercised with notify-rust on this host and
  returned `Ok(NotificationHandle { id: 3, ... })`.
- `task_notification_titles_cover_success_and_failure` passes for backup,
  download, import, unknown-task, and truncation cases.
- Desktop tests: 17 passed.
- The DND/fullscreen bypass itself was not reproduced with a fullscreen video.

## Decisions and risks

- Notifications are driven by the frontend terminal operation calls so failures
  are covered even when the backend emits no error event.
- `notify-rust` uses critical urgency on Linux; Windows toast behavior depends
  on the OS notification policy.
