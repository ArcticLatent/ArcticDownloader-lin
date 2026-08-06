# Arctic ComfyUI Helper 0.2.6

## New

- Added a public binary Nix flake for NixOS and other `x86_64-linux` Nix systems.
- Added GPU selection for systems with multiple graphics adapters, with Torch profiles filtered to the selected NVIDIA, AMD, or Intel backend.
- Unified Linux and Windows development in one source repository, including native Windows CI and release builds.

## Improvements

- Updated Linux AMD installation profiles for ROCm 7.2 and kept Windows ROCm SDK and Intel XPU choices platform-specific.
- Improved NVIDIA detection on mixed-GPU Linux systems, including headless cards reported as PCI 3D controllers.
- Made ComfyUI update-status checks asynchronous so slow Git operations no longer freeze the application during startup.
- Improved Windows process output handling so ComfyUI Manager and custom nodes can safely write non-UTF-8 output or run with in-app runtime logs disabled.
- Replaced the deprecated Nix profile installation command with `nix profile add` in the public documentation.

## Fixes

- Fixed GPU and Torch choices that could show unsupported CUDA, ROCm, or XPU combinations.
- Fixed unavailable GPU choices remaining visible after hardware detection completed.
- Fixed ComfyUI Manager startup failures on Windows caused by an invalid standard-output handle.
- Fixed misleading repeated HTML scan warnings while inspecting Civitai download responses.

