# Arctic ComfyUI Helper 0.2.7

## Fixes

- Fixed TLS certificate verification for ComfyUI, ComfyUI Manager, custom nodes, and other managed Python processes on NixOS.
- Added automatic Linux CA bundle detection while preserving user-provided `SSL_CERT_FILE` overrides.
- Added the Nix `cacert` bundle as the default certificate source for source and binary Nix packages.

## Project

- Licensed the project under the Apache License 2.0, allowing use, modification, and redistribution under its terms.
- Simplified cross-platform release publishing and kept native Arch packages independent of AUR availability.
