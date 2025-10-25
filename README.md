# Arctic Downloader

Arctic Downloader is a Rust/libadwaita desktop companion that helps ComfyUI users pull the correct
model variants (and their auxiliary files) for the GPU VRAM they have on hand. Viewers of the
accompanying tutorial series can pick a “master model”, pick their VRAM tier, point the app at their
ComfyUI install, and let the app grab the curated artifacts from Hugging Face into the correct
`ComfyUI/models/*` subfolders.

## Current Status

This repository currently contains the project scaffolding:

- Rust GTK/libadwaita application shell with drop-down selectors and placeholder actions.
- Async services for future download management and catalog handling.
- A versioned catalog template (`data/catalog.json`) describing master models, variants, artifacts,
  and ComfyUI destination categories.
- A Flatpak manifest targeting `org.gnome.Platform//49` (see `flatpak/dev.wknd.ArcticDownloader.yaml`).

## Repository Layout

- `Cargo.toml` / `src/` — application sources.
- `data/catalog.json` — curated model/variant metadata shipped with the app.
- `flatpak/dev.wknd.ArcticDownloader.yaml` — Flatpak builder manifest.

## Developing Locally

1. Install the GTK/libadwaita development dependencies for your distro plus the Rust toolchain.
2. Fetch Rust dependencies and build the debug binary:
   ```bash
   cargo check
   cargo run
   ```
3. The app will launch a placeholder UI with model and VRAM pickers. Download actions are stubbed out
   until the network pipeline is implemented.

### Catalog Admin Tool

Before cutting a new release you can curate `data/catalog.json` via the private admin utility:

```bash
cargo run --bin catalog_admin
```

The tool lists existing models, lets you add/edit/delete entries, and captures optional artifacts
(VAE, text encoders, CLIP, LoRAs, etc.) through checkbox-controlled sections. Saving writes directly
to `data/catalog.json`.

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
flatpak-builder --user --install --force-clean build-dir flatpak/dev.wknd.ArcticDownloader.yaml
flatpak run dev.wknd.ArcticDownloader
```

## Catalog Curation

The app ships and trusts the checked-in `data/catalog.json`. Each entry maps:

- `models[].variants[]` to VRAM requirements and qualitative tiers.
- `artifacts[]` to Hugging Face repositories, file paths, and ComfyUI destination categories.

Update this file between tutorial episodes to control which exact files viewers receive. Future work
will add signature verification and remote catalog refreshes.

## Roadmap Highlights

- Hook up the download manager with resume, checksums, and portal-based folder writing.
- License acknowledgement flow per Hugging Face repository.
- Settings page for concurrency limits, quantized preferences, and catalog refresh.
- Telemetry toggle (opt-in) and localization scaffolding.
