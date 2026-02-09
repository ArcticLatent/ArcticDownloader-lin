# Arctic Downloader

Arctic Downloader is a Rust/Slint desktop companion that helps ComfyUI users pull the correct
model variants (and their auxiliary files) for the GPU VRAM and system RAM they have on hand.
Viewers of the accompanying tutorial series can pick a “master model”, choose their GPU VRAM + RAM
tiers, point the app at their ComfyUI install, and let the app grab the curated artifacts from
Hugging Face into the correct `ComfyUI/models/*` subfolders. Downloads are filtered by the VRAM tier
selected in-app, while RAM-sensitive artifacts can be gated per tier so lower-memory machines only
receive the assets they can realistically run.

## Current Status

This repository currently contains:

- A Rust Slint desktop application (Windows-first) with GPU VRAM and system RAM selectors, per-tier legends, and
  artifact download progress.
- Async download services that place master-model artifacts inside dedicated folders
  (`ComfyUI/models/<category>/<model_id>/…`) and LoRA assets inside family-normalised subdirectories
  (`ComfyUI/models/loras/<family_slug>/…`).
- Optional Civitai API integration for LoRA previews, trigger words, creator attribution, and
  authenticated downloads (with clear guidance when a token is missing).
- A versioned catalog template (`data/catalog.json`) describing master models, variants, “always-on”
  assets, and target categories with optional RAM tier requirements.

## Repository Layout

- `Cargo.toml` / `src/` — application sources.
- `data/catalog.json` — curated model/variant metadata shipped with the app.

## Developing Locally

1. Install the Rust toolchain and Visual Studio Build Tools (Desktop development with C++).
2. Fetch Rust dependencies and build the debug binary:
   ```bash
   cargo check
   cargo run
   ```
3. The app launches with master-model, GPU VRAM, and system RAM pickers. The variant list filters to
   the chosen GPU tier, while the legend summarises the four supported tiers (S/A/B/C) and their
   expected quantisations. RAM tier selection controls which RAM-gated artifacts are offered.

### Tauri Preview Shell

A Tauri shell is now available in `src-tauri/` and reuses the same Rust backend services
(`catalog`, `config`, `download`, `updater`) via Tauri commands.

Run it with:
```bash
cargo run --manifest-path src-tauri/Cargo.toml
```

Build an installer-ready Windows bundle (`.exe`) with:
```bash
cd src-tauri
cargo tauri build
```
The generated artifacts are written under `src-tauri/target/release/bundle/`.
If `cargo tauri` is unavailable, install it once with `cargo install tauri-cli --version "^2"`.

### Windows Build Note

The Windows build intentionally excludes `catalog_admin`. Curate `data/catalog.json` in your source
workflow and ship updates through the remote catalog endpoint used by the app.

> **Note:** LoRA previews and downloads that originate from Civitai require a personal API token.
> Enter this once in the LoRA page and the app will handle creator metadata, trigger words, and
> authenticated downloads. If the token is missing or invalid the UI now explains the issue instead
> of leaving empty folders.

### Formatting & Lints

Rustfmt and Clippy are recommended:
```bash
cargo fmt
cargo clippy --all-features
```
(Install the `rustfmt` and `clippy` components through `rustup component add rustfmt clippy` if they
are not already available.)

## Building a Windows Installer

Build directly through Tauri:
```bash
cd src-tauri
cargo tauri build
```

Publish the generated installer from `src-tauri/target/release/bundle/` in your GitHub release workflow.

## Releases & Auto-Update Manifest

- On startup the app fetches an update manifest from
  `https://github.com/ArcticLatent/ArcticDownloader-win/releases/latest/download/update.json`
  (override with `ARCTIC_UPDATE_MANIFEST_URL`). If the manifest advertises a higher semver version,
  the app downloads the published `.exe` installer, verifies its SHA-256, launches installer execution, exits, and restarts after installation completes.
- Manifest schema:

  ```json
  {
    "version": "0.1.0",
    "download_url": "https://github.com/ArcticLatent/ArcticDownloader-win/releases/download/v0.1.0/ArcticDownloader-setup.exe",
    "sha256": "<sha256sum-of-the-exe>",
    "notes": "Optional release notes"
  }
  ```

- Release steps: run `scripts/build-release.ps1 -Version <x.y.z>` to generate
  `dist/ArcticDownloader-setup.exe` and `dist/update.json`, then publish both assets to the matching
  GitHub release tag (`v<x.y.z>`).
- Disable the automatic check with `ARCTIC_SKIP_AUTO_UPDATE=1`; re-enable with `ARCTIC_AUTO_UPDATE=1`.

## Catalog Curation

The app ships and trusts the checked-in `data/catalog.json`. Each entry maps:

- `models[].always[]` to named groups of artifacts that are always downloaded for a model.
- `models[].variants[]` to GPU `tier` requirements (S/A/B/C) plus optional notes, sizes, and
  quantisation strings.
- `artifacts[]` to Hugging Face repositories, file paths, ComfyUI destination categories, and
  optional RAM tier requirements.

LoRA definitions (`catalog.loras[]`) now download into folders derived from their `family` name so
multiple LoRAs can coexist without filename clashes.

Update this file between tutorial episodes to control which exact files viewers receive. Future work
will add signature verification and remote catalog refreshes.

### Remote Catalog Refresh

- The app boots using a cached copy of `catalog.json` (stored under
  `~/.cache/io.github.ArcticHelper/catalog.json`) or the bundled fallback.
- On launch it performs an HTTP GET against the catalog endpoint recorded in `settings.json`
  (`catalog_endpoint`). A successful `200 OK` replaces the in-memory catalog, persists the JSON to
  the cache, and stores the returned `ETag` so subsequent runs can short-circuit with `304 Not
  Modified`.
- Builds default to `https://raw.githubusercontent.com/ArcticLatent/ArcticDownloader-win/refs/heads/main/data/catalog.json`
  as the remote source. Override this at runtime with `ARCTIC_CATALOG_URL` or at build-time via the
  `ARCTIC_DEFAULT_CATALOG_URL` environment variable. Setting either to an empty string disables the
  remote fetch.

## LoRA Downloads & Civitai Integration

- LoRA assets fetched from Civitai display preview media, trigger words, and creator attribution.
- Each downloaded LoRA is placed under `ComfyUI/models/loras/<family_slug>/…`; the slug is derived
  from the LoRA family name and normalised to lowercase with underscores.
- If a Civitai request returns `401/403` the UI surfaces a clear “You are not authorized…” message
  instead of silently failing. Paste your personal Civitai API token in the LoRA section once to
  enable authenticated downloads.

## Roadmap Highlights

- Hook up the download manager with resume, checksums, and portal-based folder writing.
- License acknowledgement flow per Hugging Face repository.
- Settings page for concurrency limits, quantized preferences, and catalog refresh.
- Telemetry toggle (opt-in) and localization scaffolding.



