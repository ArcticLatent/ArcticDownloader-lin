# Arctic ComfyUI Helper 0.2.2

## What's New

- Added a manual `Refresh Catalog` action so the running app can reload the remote model catalog without a full restart.

## Improvements

- Models with no variants but with `always` artifacts, such as FlashVSR-style entries, now appear in the Models tab and can be downloaded through an `Always Artifacts Only` flow.
- Flatpak builds now bundle the Ayatana tray runtime libraries they need so the tray-enabled app can start correctly inside the Flatpak sandbox.
- Flatpak builds now request access to `org.kde.StatusNotifierWatcher` so the tray icon can register with the host tray/status notifier service.
- When running inside Flatpak, system command probes now use `flatpak-spawn --host`, so preflight checks and tool detection reflect the real host environment instead of the limited sandbox runtime.
- Flatpak builds now request session-bus access as well, which gives the tray icon a better chance to register correctly across Linux desktops that expose StatusNotifier/AppIndicator support.
- Linux tray icons now use dedicated PNG assets instead of ICO files, which avoids invisible tray icons on some Linux/Flatpak desktop panels.
- Flatpak now writes Linux tray icon temp files into a host-visible folder under the user's home directory, which fixes tray items appearing without a visible icon when the host panel could not read sandbox temp paths.
- Flatpak now launches ComfyUI itself through the host command wrapper, so custom nodes like ComfyUI-Manager can access the real host `git` executable instead of failing inside the sandboxed runtime.
- Flatpak stop actions now terminate the host-side ComfyUI process by its selected `main.py` path, fixing cases where `Stop ComfyUI` only killed the local wrapper and left the real server running.
- Flatpak stop notifications now ignore harmless shutdown races where one matched PID exits during `kill`, so successful stops no longer show a false failure toast.
- Flatpak host stop now signals matched ComfyUI PIDs individually instead of as one bulk `kill` command, which prevents false stop-failure notifications when one PID exits slightly earlier than the others.
- Flatpak stop now treats the shutdown as successful as soon as the ComfyUI listener on `127.0.0.1:8188` is gone, which removes the extra 5-10 second wait and updates the tray state immediately once the server is actually down.

## Notes

- The catalog refresh action pulls the current remote catalog into the running app immediately, so newly published model families can appear without restarting.
- The always-only model path preserves the existing variant-based workflow for normal models while allowing catalog entries that only define shared artifacts to be selected and downloaded.
- The Flatpak package keeps tray support enabled and now carries the required Ayatana and dbusmenu shared libraries inside the bundle.
