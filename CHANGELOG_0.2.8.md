# Arctic ComfyUI Helper 0.2.8

## Improvements

- Nix-managed installations now fetch and verify the signed Linux release manifest instead of reporting that no update is available.
- The application displays the latest version and a **How to Update** action when a newer Nix package is available.
- The update action opens the latest release page and records Nix profile and declarative-configuration guidance in the application log.

## Safety

- Nix installations remain externally managed and never attempt to overwrite binaries in the immutable Nix store.
- Automatic installation remains disabled by the Nix wrapper; Debian, Ubuntu, Fedora, and Arch update installation behavior is unchanged.
