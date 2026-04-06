# Native Linux Packaging

This directory contains native packaging scaffolds for Linux distributions.

## Quick build script

From repo root, use:

- `bash packaging/build-packages.sh arch`
- `bash packaging/build-packages.sh deb`
- `bash packaging/build-packages.sh rpm`
- `bash packaging/build-packages.sh flatpak`
- `bash packaging/build-packages.sh all`

Artifacts are copied to `packaging/out/`.

## Shared assets

- Desktop entry: `packaging/linux/io.github.ArcticHelper.desktop`
- Icon: `src-tauri/dist/icon.svg`

## Arch Linux (`.pkg.tar.zst`)

- File: `packaging/arch/PKGBUILD`
- Build from repo root:
  - `bash packaging/build-packages.sh arch`

## Debian/Ubuntu (`.deb`)

- Directory: `packaging/debian/debian`
- Build from repo root:
  - `bash packaging/build-packages.sh deb`

## Fedora/RHEL (`.rpm`)

- File: `packaging/fedora/arctic-comfyui-helper.spec`
- Build from repo root:
  - `bash packaging/build-packages.sh rpm`

## Flatpak (`.flatpak`)

- Files:
  - `packaging/flatpak/io.github.ArcticHelper.yml`
  - `packaging/flatpak/io.github.ArcticHelper.metainfo.xml`
- Build from repo root:
  - `bash packaging/build-packages.sh flatpak`
- Notes:
  - Builds a single-file Flatpak bundle for `io.github.ArcticHelper`
  - Requires `flatpak` and `flatpak-builder` on the host
  - Uses the Flathub remote to resolve the runtime/SDK during build

## Notes

- Package metadata (version/release/deps) may need adjustment per distro policy.
- These specs intentionally build from your current source tree layout.
