# Cross-platform development

This repository is the single source of truth for Linux and Windows. The old
`ArcticDownloader-win` checkout is an import source only and should be archived after
the unified repository has passed an interactive Windows smoke test.

## Current layout

| Concern | Linux | Windows | Shared |
| --- | --- | --- | --- |
| Tauri backend | `src-tauri/src/app_linux.rs` | `src-tauri/src/app_windows.rs` | `src-tauri/src/main.rs` selects the target |
| Frontend | `src-tauri/dist/` | `src-tauri/dist-windows/` | Tauri selects `tauri.windows.conf.json` automatically |
| Core services | — | — | `src/` |
| Dependencies | target-specific Cargo sections | target-specific Cargo sections | common Cargo dependencies |

This boundary deliberately preserves the behavior of both existing applications. It
also lets common code be extracted gradually without blocking day-to-day releases.

## Daily development on NixOS

Run and test the Linux app:

```bash
nix develop
cargo test --locked --manifest-path src-tauri/Cargo.toml
cargo tauri dev
```

Check the Windows build from Linux:

```bash
nix develop .#windows
./scripts/check-windows.sh
```

The first Windows check installs the stable Rust toolchain through `rustup` and downloads
the Microsoft CRT/SDK files used by `cargo-xwin`. Later checks reuse those caches. No
Windows license or Windows installation is required for this compile check.

`cargo-xwin` cannot prove that WebView2, the tray, dialogs, GPU detection, installers, or
PowerShell flows behave correctly at runtime. Pushes and pull requests therefore run a
second compile check on GitHub's native `windows-latest` runner.

## Where changes go

- Put cross-platform catalog, settings, download, update, RAM, and VRAM logic in `src/`.
- Put OS process handling and installer commands in the matching `app_*.rs` module.
- Keep a command name and payload identical on both platforms when the frontend calls it.
- If a UI change is platform-neutral, apply it to both frontend directories until they
  are merged.
- Before opening a pull request, run the Linux tests and `scripts/check-windows.sh`.

## Windows releases

`.github/workflows/release-windows.yml` builds on a real Windows runner and publishes to
`ArcticLatent/Arctic-Helper`. The source repository needs these Actions secrets:

- `ARCTIC_RELEASE_TOKEN`: a fine-grained token with Contents: write permission for the
  public release repository.
- `ARCTIC_SUPABASE_URL`: catalog project URL embedded in the release build.
- `ARCTIC_SUPABASE_PUBLISHABLE_KEY`: public read key embedded in the release build.

Both Cargo manifests and `src-tauri/tauri.conf.json` must contain the release version.
Push a matching `vX.Y.Z` tag, or run the workflow manually with `X.Y.Z`. The workflow:

1. builds and checks `Arctic-ComfyUI-Helper.exe` on Windows;
2. generates and verifies `update.json` against the public release URL and SHA-256;
3. retains both as a GitHub Actions artifact;
4. creates or updates the matching public release and uploads both files.

MSI/NSIS installers can be added later with `cargo tauri build` on this same native runner.
The standalone executable remains the current compatibility-preserving artifact.

## Consolidation roadmap

The repository boundary is complete, but the two large backend modules still contain
duplicated code. Consolidate them incrementally:

1. Extract matching Tauri commands and DTOs into small shared modules.
2. Move GPU detection into `gpu/linux.rs` and `gpu/windows.rs` behind one interface.
3. Move ComfyUI install/runtime operations into platform adapters behind shared commands.
4. Add a `get_platform_capabilities` command and merge the two frontends into one `dist/`.
5. Run one interactive Windows smoke test, then archive `ArcticDownloader-win` read-only.

Each extraction should preserve command names and include a Linux test plus a Windows
compile check. This keeps releases available throughout the migration.
