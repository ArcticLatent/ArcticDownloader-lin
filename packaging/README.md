# Native Linux Packaging

This directory contains native packaging scaffolds for Linux distributions.

## Shared assets

- Desktop entry: `packaging/linux/io.github.ArcticHelper.desktop`
- Icon: `src-tauri/dist/icon.svg`

## Arch Linux (`.pkg.tar.zst`)

- File: `packaging/arch/PKGBUILD`
- Build from repo root:
  - `cd packaging/arch`
  - `makepkg -si`

## Debian/Ubuntu (`.deb`)

- Directory: `packaging/debian/debian`
- Build from repo root:
  - `cp -a packaging/debian/debian ./debian`
  - `dpkg-buildpackage -us -uc -b`

## Fedora/RHEL (`.rpm`)

- File: `packaging/fedora/arctic-comfyui-helper.spec`
- Typical workflow:
  - `tar --exclude-vcs -czf ~/rpmbuild/SOURCES/arctic-comfyui-helper-0.1.0.tar.gz .`
  - `cp packaging/fedora/arctic-comfyui-helper.spec ~/rpmbuild/SPECS/`
  - `rpmbuild -ba ~/rpmbuild/SPECS/arctic-comfyui-helper.spec`

## Notes

- Package metadata (version/release/deps) may need adjustment per distro policy.
- These specs intentionally build from your current source tree layout.
