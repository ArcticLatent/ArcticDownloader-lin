# Arctic ComfyUI Helper 0.2.9

## Fixes

- Fixed the Arch package containing a NixOS dynamic loader and Nix store paths, which prevented the application from starting on a normal Arch Linux installation.
- Added release checks that reject Arch binaries with an unexpected ELF interpreter or leaked Nix and build-machine paths.

## Packaging

- Arch packages are now built natively on the Arch host instead of in an Arch container.
- Corrected Debian and Flatpak workspace output paths and isolated their Cargo build directories from other distribution builds.
- Debian and RPM packages continue to use their Ubuntu and Fedora Distrobox environments.
- Nix release artifacts now use the official `nixos/nix` Podman image when Nix is not installed on the host.
- Updated the Linux and Windows development prerequisites for the Arch-based release machine.
