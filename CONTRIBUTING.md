# Contributing to SteamTron

Thanks for helping improve SteamTron. This project is a Rust/Tauri desktop app
with a small web UI and an optional FastAPI account backend.

## Before you start

- Read [`README.md`](README.md), [`API.md`](API.md), and
  [`AGENTS.md`](AGENTS.md).
- Search existing issues and pull requests.
- For a feature or non-trivial fix, open a PRD under `docs/prd/` before writing
  implementation code.
- Small fixes use the same PRD file with only the sections that apply. Do not
  skip the PRD because a change looks trivial.

## Development setup

### Linux

```sh
sudo apt install libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev \
  patchelf rpm libfuse2
cargo check --workspace
cargo test -p steamtron-desktop
node --check crates/steamtron-desktop/ui/app.js
```

### Windows

Install:

- Rust stable with the MSVC toolchain.
- Visual Studio Build Tools with the **Desktop development with C++** workload.
- WebView2.
- [WinFsp](https://winfsp.dev/rel/) for the overlay.

```powershell
cargo check -p steamtron-desktop
cargo test -p steamtron-desktop
```

## Pull request process

1. Create a branch from `main`.
2. Add/update the relevant `docs/prd/NNNN-*.md`.
3. Keep the change focused; do not mix unrelated refactors.
4. Add regression coverage when a bug has no existing test.
5. Run the narrowest relevant tests, then the workspace tests when practical.
6. If packaging or the desktop binary changed, run the release build and verify
   the artifact.
7. Commit and push. The project expects completed work on `origin/main`; do not
   leave a finished branch local-only.
8. Open a PR describing the problem, the change, the platform tested, and the
   exact verification commands/results.

## PRD requirement

Every feature or issue gets a PRD:

```text
docs/prd/NNNN-<slug>.md
```

Create it from [`docs/prd/TEMPLATE.md`](docs/prd/TEMPLATE.md), take the next
number from [`docs/prd/README.md`](docs/prd/README.md), and update the index in
the same commit. Record:

- Problem and evidence.
- Scope and non-goals.
- Affected crates/areas.
- Requirements.
- Acceptance criteria.
- Verification commands and observed results.
- Decisions and risks.

Update the PRD status from `draft` to `active` to `done` in the same commit as
the implementation.

## Coding rules

- Rust formatting: `cargo fmt --all` (avoid reformatting unrelated files).
- Use existing patterns before adding a second convention.
- Prefer boring designs over new abstractions.
- Do not introduce avoidable allocations, copies, or repeated work.
- Preserve existing library data, backups, DLC state, and prefix compatibility.
- Do not log credentials, IGDB tokens, or account tokens.
- Do not commit `target/`, release bundles, downloaded tools, game files, or
  local configuration.

## Testing matrix

At minimum, run what your change exercises:

```sh
cargo check -p steamtron-desktop
cargo test -p steamtron-desktop
node --check crates/steamtron-desktop/ui/app.js
```

For library/backend changes:

```sh
cargo test --workspace
```

For packaging changes:

```sh
bash scripts/build-linux.sh
# verify files under target/release/bundle/
```

For the FastAPI backend, see the backend repository and run its test suite and
lint separately.

## Release process

Release artifacts are built by `.github/workflows/release.yml`:

- Linux: `.deb`, `.rpm`, `.AppImage`
- Windows: NSIS `*-setup.exe`

1. Add the release section to [`CHANGELOG.md`](CHANGELOG.md).
2. Confirm the website changelog section is updated.
3. Create and push a version tag:

```sh
git tag v0.1.1
git push origin v0.1.1
```

The workflow verifies that `CHANGELOG.md` contains the matching version, creates
the GitHub Release, and uploads the packages plus `SHA256SUMS.txt`. Do not upload
local build artifacts by hand unless a workflow failure is being recovered.

## Reporting bugs

Include:

- SteamTron version and commit.
- Linux distro/version or Windows version.
- Desktop/fullscreen mode and controller/keyboard input.
- Exact steps to reproduce.
- Relevant logs from the per-game diagnostics folder.
- Whether the issue happens with fixes disabled and with a fresh prefix.

Do not paste account tokens, API keys, or personal paths you do not want public.

## License

By contributing, you agree that your contribution is licensed under the MIT
License in [`LICENSE`](LICENSE).
