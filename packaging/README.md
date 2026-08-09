# Native Linux Packaging

This directory contains native packaging scaffolds for Linux distributions.

## Quick build script

From repo root, use:

- `bash packaging/build-packages.sh arch`
- `bash packaging/build-packages.sh deb`
- `bash packaging/build-packages.sh rpm`
- `bash packaging/build-packages.sh srpm`
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
- On Fedora this enters the `arctic-arch` Distrobox. When invoked from inside
  Arch it runs `makepkg` directly.
- The PKGBUILD isolates the native Arch compiler/linker environment from any
  inherited Nix variables and rejects binaries containing `/nix/store` or
  `/home/` paths. `scripts/verify-release-linux.sh` repeats that check against
  the binary extracted from the finished package.

## Debian/Ubuntu (`.deb`)

- Directory: `packaging/debian/debian`
- Build from repo root:
  - `bash packaging/build-packages.sh deb`
- On Fedora this enters the `arctic-ubuntu` Distrobox. When invoked from inside
  Debian or Ubuntu it runs `dpkg-buildpackage` directly.

## Fedora/RHEL (`.rpm`)

- File: `packaging/fedora/arctic-comfyui-helper.spec`
- Build from repo root:
  - `bash packaging/build-packages.sh rpm`
- RPM builds run directly on the Fedora host; no Fedora container is used.
- `srpm` creates only the source RPM and does not require the native GTK/WebKit
  build dependencies.

### Fedora COPR

Publish the current source snapshot to `burcebor/arctic-helper`:

```bash
bash scripts/publish-copr.sh
```

The script verifies the local COPR login and project, builds an SRPM, confirms
the package NEVRA, and waits for the remote build. Use `--nowait` for an
asynchronous submission or `--chroot fedora-44-x86_64` to limit the target.
Cargo dependencies are not vendored, so COPR network access defaults to on.

Fedora users can then install the package with:

```bash
sudo dnf copr enable burcebor/arctic-helper
sudo dnf install arctic-comfyui-helper
```

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

## NixOS (flake)

- Source package: `packaging/nix/source-package.nix`
- Local development build: `nix build`
- Local development run: `nix run`
- Public binary flake templates: `packaging/nix/*.in`
- Release artifact generator: `scripts/build-nix-release.sh`
- On hosts without Nix, the release generator uses Podman with the pinned
  official `nixos/nix` OCI image. Override it with `ARCTIC_NIX_IMAGE`.

The full Linux release flow generates a Nix-native runtime archive and
`arctic-comfyui-helper-nix-x86_64.tar.gz`, then uploads both to GitHub Releases.
The public flake fetches the native binary without publishing this private source
tree.

## Notes

- Bootstrap a new Fedora build machine with
  `bash scripts/setup-linux-build-environments.sh`. Use `--containers-only` or
  `--host-only` when only one side needs refreshing.
- Package metadata (version/release/deps) may need adjustment per distro policy.
- These specs intentionally build from your current source tree layout.
