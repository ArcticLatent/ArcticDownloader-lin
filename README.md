# Arctic ComfyUI Helper (Technical README)

This is the canonical source repository for both the Linux and Windows apps.
Platform code is selected at compile time, so normal development happens from one checkout.

Public release repo: `https://github.com/ArcticLatent/Arctic-Helper`

`README.public.md` is the public-facing README template for the release repo.
This file (`README.md`) is the internal technical reference.

See [Cross-platform development](docs/cross-platform-development.md) for the NixOS workflow,
Windows checks, CI releases, and the remaining consolidation plan.

## Product Scope

Arctic ComfyUI Helper is a Windows-first Rust + Tauri app that includes:
- Tier-aware model downloader for ComfyUI (models + dependencies + LoRAs)
- In-app LoRA metadata/preview flow (including Civitai token support)
- ComfyUI installer/manager module (uv-managed Python + `.venv`)
- Optional add-ons and custom node management (install/remove/toggle)
- System tray control for ComfyUI start/stop even when main window is hidden
- Self-update support through GitHub release `update.json`

Flatpak/Linux-specific UI/admin bits are intentionally not part of this app.

## Architecture

- Core crate: `Cargo.toml` + `src/`
  - Shared services: catalog/config/download/updater/app context
- Desktop shell: `src-tauri/`
  - Target selector: `src-tauri/src/main.rs`
  - Shared API contracts: `src-tauri/src/contracts.rs`
  - Compile-time platform adapter: `src-tauri/src/platform.rs`
  - Shared backend commands: `src-tauri/src/shared.rs` + `src-tauri/src/shared/`
  - Linux backend: `src-tauri/src/app_linux.rs`
  - Windows backend: `src-tauri/src/app_windows.rs`
  - Shared Linux/Windows frontend: `src-tauri/dist/`
    - Composition/startup: `main.js` and `features/bootstrap.js`
    - Catalog and model selection: `features/catalog.js`
    - ComfyUI install/runtime: `features/comfyui.js`
    - GPU/Torch selection: `features/gpu-torch.js`
    - Event registration and progress UI: `features/event-handlers.js` and `features/download-progress.js`
    - Shared state, DOM references, and utilities: `lib/`
  - Windows config overlay: `src-tauri/tauri.windows.conf.json`

Key identifiers and branding:
- App ID: `io.github.ArcticHelper`
- Product name: `Arctic ComfyUI Helper`
- Publisher: `Arctic Latent`
- Binary name: `Arctic-ComfyUI-Helper.exe`

## Data and Paths

Config/cache use `ProjectDirs("io.github", "ArcticHelper", "ArcticHelper")`.

Important runtime locations:
- Settings/config: `%LOCALAPPDATA%\io.github\ArcticHelper\config\settings.json`
- Cache root: `%LOCALAPPDATA%\io.github\ArcticHelper\cache\`
- ComfyUI shared runtime cache:
  `%LOCALAPPDATA%\io.github\ArcticHelper\cache\comfyui-runtime\`
  - contains shared `.tools` and `.python` for installer pipeline

ComfyUI install mode behavior:
- Install New: select base folder -> app creates `ComfyUI`, `ComfyUI-01`, `ComfyUI-02`, ...
- Manage Existing: select base with existing install(s), detect and manage installation state

## Catalog and Downloading

Catalog source behavior:
- Supabase Postgres is the catalog source of truth.
- Runtime configuration:
  - `ARCTIC_SUPABASE_URL`
  - `ARCTIC_SUPABASE_ANON_KEY` or `ARCTIC_SUPABASE_PUBLISHABLE_KEY`
- The app reads `public.catalog_documents` where `catalog_key = 'main'` and uses the `catalog` JSON document.
- The last successful catalog is cached locally for offline startup.

Model/LoRA downloader:
- Resolves target ComfyUI subfolders automatically
- Shows active/completed transfers and per-item progress
- Supports cancellation
- LoRA preview metadata supports creator/trigger/description handling
- Optional Hugging Face Xet fast-path via app toggle in Models:
  enable `HF Xet (Experimental)` and ensure `uvx hf` or `hf` CLI backend is available.

## ComfyUI Installer Module

Primary model:
- PowerShell/orchestrated from inside app (no external installer UI required)
- uv-managed Python (`3.12.10`) and per-install `.venv`
- Detected-GPU selector for mixed NVIDIA/AMD/Intel systems; Automatic prefers NVIDIA
- Torch profile is recalculated from the selected GPU and the choice is persisted
- Manual Torch profile override remains available within the selected GPU backend

Current torch profiles:
- `torch271_cu128`
- `torch280_cu128`
- `torch291_cu130`
- Linux: `torch211_rocm72` (recommended), `torch291_rocm64` (compatibility), `torch291_xpu`
- Windows: `torch291_rocm72`, `torchxpu_nightly`

Add-ons (checkbox-managed):
- SageAttention
- SageAttention3 (RTX 50 only)
- FlashAttention
- InsightFace
- Nunchaku
- Trellis2 (requires minimum Torch 2.8.0 + cu128)
- Pinned Memory (default ON)

Attention backend rules:
- Exactly one of SageAttention / SageAttention3 / FlashAttention / Nunchaku at a time
- Toggle flow supports removal/install transitions and confirmation prompts
- Existing install mode applies backend changes by uninstall/install and keeps state in sync

Custom nodes (checkbox-managed):
- comfyui-manager
- ComfyUI-Easy-Use
- rgthree-comfy
- ComfyUI-GGUF
- comfyui-kjnodes

`comfyui_controlnet_aux` was intentionally removed from selectable custom nodes.

## ComfyUI Runtime Control

App supports starting/stopping ComfyUI directly.

System tray:
- Shows app + ComfyUI status
- Right-click actions include Start/Stop/Show/Quit
- Tray remains available while window is hidden

Desktop shortcuts:
- Start shortcut creation for installed ComfyUI instances
- Naming supports multiple installs (`Start ComfyUI`, `Start ComfyUI 01`, ...)

## Update Mechanism

Updater defaults:
- Manifest URL:
  `https://github.com/ArcticLatent/Arctic-Helper/releases/latest/download/update.json`
- Fallback standalone name: `Arctic-ComfyUI-Helper.exe`

Manifest schema:
```json
{
  "version": "0.1.0",
  "download_url": "https://github.com/ArcticLatent/Arctic-Helper/releases/download/v0.1.0/Arctic-ComfyUI-Helper.exe",
  "sha256": "<sha256>",
  "notes": "Release notes"
}
```

Notes:
- `Check Updates` will error until release repo has a valid `update.json` asset.
- Startup auto-update can be toggled via existing env flags.

## Icons and Branding Assets

- Primary Windows icon is sourced from `assets/favicon.ico`
- Tauri bundle icon points to `src-tauri/icons/favicon.ico`
- Same icon is used for app/tray/shortcut flows where supported

If Windows still shows old icon after changing `.ico`, clear icon cache and rebuild.

## Development Commands

Linux/NixOS, from the repository root:

```bash
nix develop
npm ci
npm run check:frontend
npm run test:frontend
cargo check --manifest-path src-tauri/Cargo.toml
cargo tauri dev
```

Cross-check the Windows target without leaving NixOS:

```bash
nix develop .#windows
./scripts/check-windows.sh
```

The cross-check catches Rust and Tauri compile errors. Native Windows CI remains the
authority for Windows builds and artifacts because it runs on `windows-latest`.

On a native Windows machine, if interactive testing is needed:

```powershell
# dev run
cargo tauri dev

# sanity check
cargo check --manifest-path .\src-tauri\Cargo.toml

# production binary (no installer)
cargo tauri build --no-bundle
```

## Memory Leak Check

Use the built-in memory trend script from repository root:

```powershell
# Launch app and sample for 30 minutes, then stop it
powershell -ExecutionPolicy Bypass -File .\scripts\memory-leak-check.ps1 -DurationSeconds 1800 -StopProcessOnExit

# Attach to an already running process (replace PID)
powershell -ExecutionPolicy Bypass -File .\scripts\memory-leak-check.ps1 -TargetPid 12345 -DurationSeconds 1200
```

Outputs are written to `dist/`:
- `<prefix>-<timestamp>.csv` with time-series samples
- `<prefix>-<timestamp>-summary.txt` with growth slopes and a leak-risk assessment

Release binary output:
- `target\release\Arctic-ComfyUI-Helper.exe` (the Cargo workspace target directory)

## Automated Release Flow

Windows releases are built from this repository by
`.github/workflows/release-windows.yml` on a native GitHub Windows runner. Configure:

- The following Actions secrets on the private `ArcticLatent/ArcticDownloader-lin`
  source repository (the repository where the workflow runs).
- `ARCTIC_SUPABASE_URL`
- `ARCTIC_SUPABASE_PUBLISHABLE_KEY`
- `ARCTIC_UPDATE_SIGNING_KEY`: the base64 Ed25519 private key used only while
  generating the signed update manifest. See
  [Update manifest signing](docs/cross-platform-development.md#update-manifest-signing).

Dispatching the workflow with `X.Y.Z` builds `Arctic-ComfyUI-Helper.exe`, verifies
`update.json`, and stores both as a workflow artifact. The all-platform publisher
downloads and verifies that artifact before uploading it with the local GitHub login.

The local PowerShell flow remains available for emergency/native Windows releases:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\release.ps1
```

It will:
1. Prompt for version
2. Prompt for release notes (end with `END` line)
3. Bump versions in:
   - `Cargo.toml`
   - `src-tauri/Cargo.toml`
   - `src-tauri/tauri.conf.json`
4. Clean + build (`cargo clean`, `cargo tauri build --no-bundle`)
5. Generate artifacts in `dist/`:
   - `Arctic-ComfyUI-Helper.exe`
   - `Arctic-ComfyUI-Helper.exe.sha256`
   - `update.json`
   - release notes markdown
6. Create/update GitHub release on `ArcticLatent/Arctic-Helper`

Prerequisite:
- GitHub CLI authenticated: `gh auth login`

## Automated Linux Release Flow

Publish Linux and Windows together with one guarded command:

```bash
bash ./scripts/publish-release-all.sh --version 0.2.9
```

The command prompts once for the local manifest-signing key when needed and
once for confirmation. It rehearses Windows without publishing, builds and
verifies Linux, publishes the GitHub assets (including the container-built Arch
`.pkg.tar.*` package), tags the source, uploads the verified native Windows
artifact, and verifies the downloaded public release. On Fedora, it uses the
native Cargo/Python/RPM/Flatpak toolchain and delegates Arch, Debian, and Nix
artifact construction to containers. Add `--yes` to skip the confirmation or
`--resume` after inspecting a partially completed release.

For a Linux-only release, use:

```bash
bash ./scripts/release-linux.sh
```

On a new Fedora workstation, set up the native dependencies plus the
`arctic-arch` and `arctic-ubuntu` Distroboxes with:

```bash
bash scripts/setup-linux-build-environments.sh
```

RPM and Flatpak artifacts are built natively on Fedora. Arch and Debian
packages use their matching Distroboxes. NixOS is not a supported Distrobox
guest, so the Nix release is built with Podman and the official pinned
`nixos/nix` image whenever the host has no `nix` command.

This publishes to `ArcticLatent/Arctic-Helper`. The Arch `.pkg.tar.*` package
is a normal signed-manifest release asset and does not depend on AUR availability.
It will:

1. Prompt for version (example: `0.1.1`)
2. Prompt for release notes (end with `END` line)
3. Build + verify + publish GitHub release (Arch + Deb + RPM artifacts)

Optional non-interactive variant:

```bash
bash ./scripts/release-linux.sh --version 0.1.1
```

Publish the verified Fedora source package to the personal COPR as part of the
Linux release:

```bash
bash ./scripts/release-linux.sh --version 0.1.1 --publish-copr
```

For a COPR-only build from the current source tree:

```bash
bash scripts/publish-copr.sh
```

The default project is `burcebor/arctic-helper`. The COPR build has network
access because Cargo dependencies are locked but not vendored. Override the
project with `--project owner/name` or `ARCTIC_COPR_PROJECT`.

Build-only:

```bash
bash ./scripts/build-release-linux.sh --version 0.1.1 --repository ArcticLatent/Arctic-Helper
```

If package builds completed but a later assembly step failed, reuse the contents
of `packaging/out` without rebuilding every distribution package:

```bash
bash ./scripts/build-release-linux.sh --version 0.1.1 --repository ArcticLatent/Arctic-Helper --assemble-only
```

Verify-only:

```bash
bash ./scripts/verify-release-linux.sh --version 0.1.1 --tag v0.1.1 --repository ArcticLatent/Arctic-Helper
```

Linux flow does:

1. Bump versions in Rust/Tauri/package metadata.
2. Update Debian changelog entry.
3. Build Arch (`.pkg.tar.zst`), Debian (`.deb`), Fedora/RPM (`.rpm`/`.src.rpm`), Flatpak, and a binary Nix flake artifact.
4. Generate `SHA256SUMS` + `linux-release.json`.
5. Optionally submit the Fedora SRPM to COPR.
6. Create/update GitHub release via `gh`.

### Local NixOS build

The repository flake builds directly from the private source tree:

```bash
nix build
nix run
```

The release flow publishes a binary-only flake tarball. Nix-managed builds set
`ARCTIC_PACKAGE_MANAGER=nix`, so application updates are performed through Nix
instead of attempting to modify the immutable Nix store.

## Repo Notes

- This is the internal repo and can include technical/implementation notes.
- `README.public.md` is maintained separately for public release consumption.

## License

Copyright 2026 Arctic Latent.

Licensed under the [Apache License 2.0](LICENSE).
