# Arctic Downloader

Arctic Downloader is a Rust/libadwaita desktop companion that helps ComfyUI users pull the correct
model variants (and their auxiliary files) for the GPU VRAM and system RAM they have on hand.
Viewers of the accompanying tutorial series can pick a “master model”, choose their GPU VRAM + RAM
tiers, point the app at their ComfyUI install, and let the app grab the curated artifacts from
Hugging Face into the correct `ComfyUI/models/*` subfolders. Downloads are filtered by the VRAM tier
selected in-app, while RAM-sensitive artifacts can be gated per tier so lower-memory machines only
receive the assets they can realistically run.

## Current Status

This repository currently contains:

- A Rust GTK/libadwaita application with GPU VRAM and system RAM selectors, per-tier legends, and
  artifact download progress.
- Async download services that place master-model artifacts inside dedicated folders
  (`ComfyUI/models/<category>/<model_id>/…`) and LoRA assets inside family-normalised subdirectories
  (`ComfyUI/models/loras/<family_slug>/…`).
- Optional Civitai API integration for LoRA previews, trigger words, creator attribution, and
  authenticated downloads (with clear guidance when a token is missing).
- A versioned catalog template (`data/catalog.json`) describing master models, variants, “always-on”
  assets, and target categories with optional RAM tier requirements.
- A Flatpak manifest targeting `org.gnome.Platform//49` (see `flatpak/io.github.ArcticDownloader.yaml`).

## Repository Layout

- `Cargo.toml` / `src/` — application sources.
- `data/catalog.json` — curated model/variant metadata shipped with the app.
- `flatpak/io.github.ArcticDownloader.yaml` — Flatpak builder manifest.

## Developing Locally

1. Install the GTK/libadwaita development dependencies for your distro plus the Rust toolchain.
2. Fetch Rust dependencies and build the debug binary:
   ```bash
   cargo check
   cargo run
   ```
3. The app launches with master-model, GPU VRAM, and system RAM pickers. The variant list filters to
   the chosen GPU tier, while the legend summarises the four supported tiers (S/A/B/C) and their
   expected quantisations. RAM tier selection controls which RAM-gated artifacts are offered.

### Catalog Admin Tool

Before cutting a new release you can curate `data/catalog.json` via the private admin utility:

```bash
cargo run --bin catalog_admin
```

The tool lists existing models, lets you add/edit/delete entries, organise “always-on” artifact
groups per model, and assign per-variant artifacts. RAM tier gating is configured only inside the
“always-on” groups, while variants capture a `tier` (GPU requirement) that maps directly to the
four-tier UI. Saving writes directly to `data/catalog.json`.

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

## Building the Flatpak

Ensure the Flatpak SDK, Platform, and Rust extension are installed:
```bash
flatpak install org.gnome.Platform//49 org.gnome.Sdk//49 org.freedesktop.Sdk.Extension.rust-stable//24.08
```

Then perform a local build:
```bash
flatpak-builder --user --install --force-clean build-dir flatpak/io.github.ArcticDownloader.yaml
flatpak run io.github.ArcticDownloader
```

## Releases & Auto-Update Manifest

- On startup the app fetches an update manifest from
  `https://raw.githubusercontent.com/ArcticLatent/ArcticDownloader-flatpak/refs/heads/main/update.json`
  (override with `ARCTIC_UPDATE_MANIFEST_URL`). If the manifest advertises a higher semver version,
  the app downloads the bundled `.flatpak`, verifies its SHA-256, and reinstalls it via
  `flatpak-spawn --host flatpak install --user --reinstall <bundle>`.
- Manifest schema:

  ```json
  {
    "version": "0.1.0",
    "download_url": "https://github.com/ArcticLatent/ArcticDownloader-flatpak/releases/download/v0.1.0/ArcticDownloader.flatpak",
    "sha256": "<sha256sum-of-the-flatpak>",
    "notes": "Optional release notes"
  }
  ```

- Release steps: build the Flatpak, calculate `sha256sum ArcticDownloader.flatpak`, upload it to the
  GitHub Release, update `update.json` in `ArcticLatent/ArcticDownloader-flatpak` with the new
  version/download URL/checksum, and push it. Restart the app after auto-install completes.
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
  `~/.cache/io.github.ArcticDownloader/catalog.json`) or the bundled fallback.
- On launch it performs an HTTP GET against the catalog endpoint recorded in `settings.json`
  (`catalog_endpoint`). A successful `200 OK` replaces the in-memory catalog, persists the JSON to
  the cache, and stores the returned `ETag` so subsequent runs can short-circuit with `304 Not
  Modified`.
- Builds default to `https://raw.githubusercontent.com/burce/ArcticDownloader/main/data/catalog.json`
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
