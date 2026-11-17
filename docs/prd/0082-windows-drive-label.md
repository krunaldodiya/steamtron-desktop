# Normalize Windows drive labels

- Status: active
- Date: 2026-11-17
- Areas: `crates/steamtron-desktop/src/main.rs`, `ui/app.js`

## Problem

Registering `C:\` canonicalizes it to `\\?\C:\`. The registered drive label
became `System (\\?\C:\)` and the picker rendered it twice:

```text
System (\\?\C:\) · \\?\C:\
```

## Fix

- Store local Windows drive roots without the `\\?\` prefix.
- Compare both canonical and display forms when marking a drive registered.
- Normalize existing labels/paths in the UI as a migration fallback.

## Verification

- Fresh registration shows `System (C:) · C:\`.
- Existing `\\?\` registrations render cleanly.
- Drive operations still use valid absolute paths.
