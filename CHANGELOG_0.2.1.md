# Arctic ComfyUI Helper 0.2.1

## What's New

- Added a full-screen loading transition when switching into `Manage Existing` and when changing between managed ComfyUI installs.
- Added Linux Flatpak packaging to the multi-package build and GitHub release flow.

## Improvements

- The ComfyUI tab now hides manage-only controls when you are in `Install New`, so the workflow is less confusing when an existing install has already been detected.
- Entering `Manage Existing` now gives immediate visual feedback instead of leaving the previous UI visible while the app loads the selected install.
- The Linux packaging flow now includes `.flatpak` artifacts in release outputs, checksums, manifest generation, and GitHub uploads.

## Notes

- The Flatpak package targets `org.gnome.Platform//50`.
- The Flatpak release flow produces a single-file `.flatpak` bundle alongside the existing Arch, Debian, and RPM artifacts.
