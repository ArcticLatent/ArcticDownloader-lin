# Cross-platform development

This repository is the single source of truth for Linux and Windows. The old
`ArcticDownloader-win` checkout is an import source only. The unified repository has
passed its interactive Windows smoke test, so the old repository can be archived after
these consolidation changes are committed and pushed.

## Current layout

| Concern | Linux | Windows | Shared |
| --- | --- | --- | --- |
| Tauri backend | `src-tauri/src/app_linux.rs` | `src-tauri/src/app_windows.rs` | `main.rs`, `contracts.rs`, `platform.rs`, and `shared.rs` |
| Frontend | `src-tauri/dist/` | `src-tauri/dist/` | capabilities control platform-specific options |
| Core services | — | — | `src/` |
| Dependencies | target-specific Cargo sections | target-specific Cargo sections | workspace dependency versions and one root `Cargo.lock` |

This boundary deliberately preserves the behavior of both existing applications. It
also lets common code be extracted gradually without blocking day-to-day releases.

The shared frontend is split by responsibility. `main.js` is only the composition
root; `features/bootstrap.js`, `catalog.js`, `comfyui.js`, `gpu-torch.js`,
`event-handlers.js`, and `download-progress.js` own application behavior, while
`lib/app-context.js` owns the shared state and DOM references. Feature factories use
explicit dependency injection and import only `lib/`, so the module graph has no
feature-to-feature cycles.

## Daily development on NixOS

Run and test the Linux app:

```bash
nix develop
npm ci
npm run check:frontend
npm run test:frontend
cargo test --locked --manifest-path src-tauri/Cargo.toml
cargo tauri dev
```

The root manifest is also the Cargo workspace root. Shared dependency versions live in
`[workspace.dependencies]`; each crate keeps its own feature selection. Run Cargo from
the repository root, including when using `--manifest-path src-tauri/Cargo.toml`, so
both crates consistently use the root `Cargo.lock` and workspace `target/` directory.

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
- Put UI changes in `src-tauri/dist/`; both targets now load that frontend.
- Keep `main.js` as composition/startup wiring. Put behavior in the matching
  `features/` module and reusable pure helpers in `lib/`.
- Add platform-specific UI behavior through `get_platform_capabilities`, not a second
  frontend fork.
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

1. Extract matching Tauri commands and DTOs into small shared modules. **In progress**:
   `src-tauri/src/shared.rs` (~76 functions, 16 structs, its own `#[cfg(test)]` suite)
   holds pure, side-effect-free helpers verified identical between `app_linux.rs`
   and `app_windows.rs` before moving — install-folder/state bookkeeping
   (`InstallState`, `write_install_state`, `find_in_progress_install`,
   `choose_install_folder`, `detect_existing_comfyui_root`,
   `is_recoverable_preclone_dir`, `clear_directory_contents`), custom-node path
   helpers, version/launch-arg/YAML string parsing, runtime event emission
   (`emit_comfyui_runtime_event` and friends, plus the `ComfyRuntimeEvent`/
   `DownloadProgressEvent`/etc. payload structs), the AMD/Intel GPU-detail caches,
   the model-artifact selection-key filtering used by batch downloads, and — as
   of the most recent passes — **`AppState` itself**, plus the settings/catalog/
   runtime-status commands and process-liveness checks built on it
   (`get_settings`, `get_catalog`, `save_civitai_token`, `cancel_comfyui_install`,
   `get_comfyui_runtime_status`, `comfyui_process_running`,
   `comfyui_external_running`, `comfyui_runtime_running`, `wait_for_comfyui_start`,
   `set_comfyui_custom_launch_args`, `set_comfyui_gpu_selection`,
   `set_comfyui_show_runtime_logs`). The three `set_comfyui_*` settings commands
   followed once `AppState` was shared — same shape as the others: pure
   `state.context.config.update_settings(...)` calls with no divergent callees
   (the sibling `set_comfyui_extra_model_config`/`get_comfyui_extra_model_config`/
   `get_effective_download_destination` were checked too and excluded, since they
   call `resolve_root_path`, confirmed `DIFFERENT`).
   `AppState` turned out to be a plain data struct with no `impl` block and no
   platform-specific fields — the type-identity barrier that had blocked sharing
   any `State<'_, AppState>`-taking function was artificial, not a real platform
   difference, once actually checked. `kill_managed_comfyui_child` — the "kill the
   child process this app itself spawned" half of `stop_comfyui_root_impl` — was
   pulled out the same way: byte-identical on both sides, even though the rest of
   that function (finding and killing an orphaned/externally-running listener)
   is genuinely platform-specific. See step 3 for where the rest of process
   start/stop orchestration landed.
   Cross-module `#[tauri::command]`s (e.g. `get_settings`) are
   registered in `generate_handler![...]` via their full path
   (`crate::shared::get_settings`) — the `#[tauri::command]` macro's generated
   dispatch item isn't brought into scope by a plain `use` of the function name,
   only by the qualified path or a matching `use` of the hidden macro item; the
   qualified path is the pattern used consistently here. Three rules keep the
   overall extraction safe:
   - Functions that call into something with (or likely to grow) platform-specific
     behavior — e.g. `normalize_path`, `command_available`, `python_for_root` — are
     deliberately left in place rather than extracted, since a shared function
     silently depending on one platform's semantics is worse than the duplication
     it removes.
   - **Before extracting a function, check for `#[cfg(target_os = ...)]`-gated
     duplicate definitions of the same name in the file you're pulling from.** A
     first pass at this nearly moved `kill_python_processes_for_root` as
     "identical" because a naive text scan matched Windows'
     `#[cfg(not(target_os = "windows"))]` no-op stub instead of the real
     `#[cfg(target_os = "windows")]` PowerShell implementation sitting right
     above it — grep for the function name and confirm exactly one definition
     exists on each side before trusting a diff. (That investigation did surface
     a real gap: Linux's `kill_python_processes_for_root` was a no-op. It now
     targets only the selected installation's exact virtual-environment interpreter,
     sends `TERM`, and escalates to `KILL` if necessary before reinstall/uninstall.)
   - **Before extracting a struct, read its full current field list on both sides
     by eye — don't trust a script's diff.** A second pass nearly unified
     `NvidiaGpuDetails`, but Windows' version is missing the `compute_capability`
     field that Linux's has (used for Hopper/SM90 detection) — a real, long-standing
     platform difference. An automated text-diff of the two struct bodies reported
     them as identical (a tooling bug, not a judgment call), and only a direct
     side-by-side read caught it before the struct — and the accessor function
     built on it — got merged. `AmdGpuDetails`/`IntelGpuDetails` *are* genuinely
     identical and did get extracted; `NvidiaGpuDetails`/`gpu_details_cache` stay
     platform-local, with a comment on the Linux struct explaining why.
2. Move GPU detection into `gpu/linux.rs` and `gpu/windows.rs` behind one interface.
   **Investigated and re-scoped.** Splitting GPU detection into dedicated per-platform
   files turned out not to be the right move once the actual code was read closely:
   the detection logic (`query_{nvidia,amd,intel}_gpu_details_blocking` and the
   `detect_{nvidia,amd,intel}_gpu_details` caching wrappers around them) differs by
   platform in more than just the system call. The caching/retry semantics are
   genuinely different — Linux tracks a separate "probe complete" flag and gives up
   permanently after one failed probe; Windows has no such flag and resets its
   "probe started" flag on failure so a later call retries indefinitely. Relocating
   ~150-300 lines of already-divergent-in-substance code per platform into new files
   wouldn't reduce duplication, just move it, and two files that only coincidentally
   share a directory aren't "one interface" in any real sense.
   What *is* identical, and now lives in `shared.rs` as adapter-callers over
   `detect_{nvidia,amd,intel}_gpu_details` (added to the existing adapter re-export
   list from step 3, rather than a new module): `detect_nvidia_gpu`,
   `detect_amd_gpu_name`, `detect_intel_gpu_name` — thin wrappers that add nothing
   but a fake-GPU testing hook and a tuple/field reshape. This is the "one interface"
   the roadmap item was after; `shared.rs` already *is* that interface, so a
   dedicated `gpu/` module would have been the same thing with extra ceremony.
   `gpu_detection_pending` and `is_nvidia_hopper_sm90` were checked too and are
   Linux-only with no Windows counterpart at all — not candidates, just unique code.
3. Move ComfyUI install/runtime operations into platform adapters behind shared commands.
   **Started, scoped to runtime start/stop/status only** — install was investigated and
   found not to fit this round (see below). The runtime orchestration functions
   (`start_comfyui_root`, `start_comfyui_root_background`, `stop_comfyui_root`,
   `stop_comfyui_for_mutation`, `resolve_comfyui_instance_name`) are byte-identical on
   both platforms and now live in `shared.rs`. They call down into leaf functions that
   are genuinely platform-specific (`resolve_root_path`, `start_comfyui_root_impl`,
   `stop_comfyui_root_impl`, `update_tray_comfy_status`, `spawn_comfyui_start_monitor`),
   which stay defined in `app_linux.rs`/`app_windows.rs` exactly as they were — only
   widened from private to `pub(crate)` — and are exposed through `platform.rs` via a
   compile-time Linux/Windows re-export pair. Shared orchestration imports only that
   boundary rather than selecting an application module itself.
   This is the "platform adapter" the roadmap item names: since only one platform is
   ever compiled into a given binary, a `#[cfg]`-gated re-export is the right-sized
   substitute for a runtime trait object here — no dynamic dispatch needed, and it's
   naturally immune to the `#[cfg]`-gated-duplicate trap from step 1 (a name is
   imported, never copied, so it resolves correctly no matter how many `#[cfg]`
   variants of it exist on the platform side — `update_tray_comfy_status` has two).
   Two things confirmed genuinely different and left alone during this pass:
   - `spawn_comfyui_start_monitor` remains platform-local, but both implementations now
     open `http://127.0.0.1:8188` after startup. This closes the Windows parity gap found
     during the VM smoke test.
   - Install (`run_comfyui_install`/`run_comfyui_install_linux`, the async
     `start_comfyui_install` command, and all `install_*` add-on/custom-node helpers)
     was checked and does **not** fit the adapter pattern this round: even the outer
     command wrapper has real behavioral differences (Linux always persists
     `comfyui_torch_profile`; Windows doesn't set it in the same place, and
     `selected_attention_backend` has a different return type — `&str` on Linux vs.
     `Option<&str>` on Windows). This is genuine divergence accumulated over time, not
     surface duplication, and needs real design work (or a decision to leave it split)
     rather than a mechanical extraction. Not attempted here.
   - **Follow-up done**: `get_comfyui_extra_model_config`, `get_effective_download_destination`,
     `set_comfyui_extra_model_config`, and the pure `normalize_optional_path` wrapper
     (previously blocked only by `normalize_path` having no adapter) now live in
     `shared.rs` too, using the same `resolve_root_path`-adapter treatment plus three
     new adapters: `normalize_path`, `comfy_extra_model_config`, `effective_download_root`,
     `write_extra_model_paths_yaml`. Each divergence was real but small (e.g.
     `effective_download_root` differs only by one `log::info!` call Linux has and
     Windows doesn't; `write_extra_model_paths_yaml` differs in path-normalization call
     and uses a `Vec::join` vs. a raw `format!` string for the same YAML output) —
     confirmed by direct read, including the `ComfyExtraModelConfig`/
     `ComfyExtraModelConfigResponse`/`EffectiveDownloadDestinationResponse` struct
     bodies *and* their derive attributes line-by-line (a derive mismatch would compile
     fine right up until some caller needed the missing trait, so it doesn't show up
     as a clean pass/fail the way a missing field does).
   - **Catalog/download commands, done as a follow-up sweep** (not one of the original
     5 roadmap items, but the same pattern): `refresh_catalog`, `get_lora_metadata`,
     `download_model_assets`, `download_model_assets_batch`, `download_workflow_asset`,
     `get_comfyui_update_status` (+`_blocking`), `get_comfyui_resume_state`,
     `model_artifacts_for_download_request`, `spawn_progress_emitter`,
     `git_commit_for_ref`, `git_current_branch` now live in `shared.rs`, using two new
     adapters (`run_command_capture`, `git_latest_release_tag`) plus the existing ones.
     **A real process mistake surfaced here, not just a near-miss**: an early
     diff script used `fn NAME(` as its match pattern and silently skipped every
     `async fn` declaration (matching nothing on either side, so empty diffed against
     empty read as "identical"). Every one of these functions is `async fn`. This
     produced a false "all verified identical" claim that got stated as fact before
     being caught — the fix wasn't a better safety net, it was re-running the exact
     same check with the pattern actually matching `async fn` too, which immediately
     turned up real, substantive differences in half the batch (`check_updates_now`
     and `auto_update_startup` both have a Flatpak/package-manager early-return Linux
     has and Windows doesn't; `download_lora_asset`, `apply_comfyui_component_toggle`,
     `update_selected_comfyui`, and `start_comfyui_install` all have real platform-specific
     logic beyond styling). Excluded, not moved. The lesson isn't "add more checks" —
     it's that a verification script's silence (no diff output) must be confirmed to
     mean "compared and found equal," never assumed, especially before it's repeated
     back as a finding.
4. Add a `get_platform_capabilities` command and merge the two frontends into one `dist/`.
   **Complete and smoke-tested in the Windows VM.** Both targets expose typed
   platform/GPU/Torch capability data, and `tauri.windows.conf.json` points to the shared
   `dist/`. The temporary rollback frontend in `dist-windows/` has been removed.
5. Run one interactive Windows smoke test, then archive `ArcticDownloader-win` read-only.
   **Smoke test complete.** Installation, model downloads, ComfyUI startup, Manager,
   platform-aware GPU/Torch selection, and the shared frontend were exercised in the
   Windows VM. After the unified branch is published, archiving the old checkout is the
   remaining manual cleanup action.

Each extraction should preserve command names and include a Linux test plus a Windows
compile check. This keeps releases available throughout the migration.
