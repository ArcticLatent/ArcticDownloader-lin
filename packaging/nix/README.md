# NixOS packaging

The root `flake.nix` builds the application from this private source tree for
development and verification:

```bash
nix build
nix run
```

Public releases remain binary-only. `scripts/build-nix-release.sh` builds a
Nix-native executable from the private tree, creates a runtime archive, and
creates a small flake tarball that fetches that archive. Both tarballs are
uploaded with the other GitHub release assets.

The generated package adds the command-line tools used by the ComfyUI installer
to the application's `PATH`. It also marks the application as Nix-managed so the
in-app updater cannot attempt to mutate the immutable Nix store. Users update it
through their Nix profile or their NixOS flake instead.
