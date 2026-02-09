<p align="center">
  <img src="assets/icon.svg" alt="Arctic Downloader" width="160">
</p>

# Arctic Downloader

### ComfyUI Asset Helper by Arctic Latent

Arctic Downloader is a helper app for people who run ComfyUI and want a simple way to grab the right models, VAE files, and LoRAs for their setup. It’s curated to mirror the builds shown in my YouTube tutorials so you can follow along without hunting for the assets yourself. Think of it as a catalog with a big “download” button: you tell it your GPU VRAM and RAM tiers, and it surfaces the options that match.

## What it does

- Shows a catalog of hand-picked ComfyUI models and LoRAs.
- Lets you choose your GPU VRAM and system RAM, then highlights the variants that make sense for those limits.
- Automatically grabs the “always needed” extras (text encoders, CLIPs, upscalers, and similar helpers) so nothing is missing.
- Saves everything into the right subfolders under your ComfyUI installation so you can start using the files right away.
- Gives you live progress for each download and, when it’s done, lists exactly which files landed where (with quick-open buttons).
- Supports optional Civitai API tokens for LoRAs that need an account.

## Getting Started

1. **Install ComfyUI** and make sure you know where its folder lives.
2. **Download the latest Windows release** from this repository’s Releases page.
3. Run the installer (`ArcticDownloader-setup.exe`).
4. Launch Arctic Downloader and pick your ComfyUI folder when prompted.

That’s it—browse the catalog, pick what you want, and click download. The app handles the rest.

## Automatic updates

- On launch, the app checks for a new version using a small manifest at `https://github.com/ArcticLatent/ArcticDownloader-win/releases/latest/download/update.json` (override with `ARCTIC_UPDATE_MANIFEST_URL`).
- Manifest format:

  ```json
  {
    "version": "0.1.0",
    "download_url": "https://github.com/ArcticLatent/ArcticDownloader-win/releases/download/v0.1.0/ArcticDownloader-setup.exe",
    "sha256": "<sha256-of-the-exe>",
    "notes": "Optional release notes"
  }
  ```

- Publish a new build: attach `ArcticDownloader-setup.exe` to a GitHub Release, calculate its SHA-256 checksum, update `update.json` with the new version, URL, and checksum, and publish both assets.
- When a new version exists, the app downloads and verifies the installer, runs it, then restarts the app after update completes.
- To disable the auto-check (for testing), set `ARCTIC_SKIP_AUTO_UPDATE=1`. Re-enable with `ARCTIC_AUTO_UPDATE=1`.

## Tips

- The VRAM tiers (S, A, B, C) give you a quick way to match files to your GPU size. If you’re unsure, pick the lowest tier that matches your card to avoid running out of memory.
- Use the legend inside the app if you want a refresher on the quantization shorthand (fp16, fp8, GGUF, etc.).
- If you drop in new hardware later, just change your tier in the app and it will show the upgraded variants automatically.

## Requirements

- Active internet connection for downloading models and LoRAs.
- Windows 10/11.
- Some Civitai creators require you to be logged in to download their LoRAs. If you see an “unauthorized” download error, create a free API key on the Civitai website, paste it into the LoRA section inside Arctic Downloader, and hit Save.
- Your API key never leaves your machine. It’s stored in the local configuration file and only attached to the authenticated download request sent to Civitai.

## Need Help?

If you need help, hit a problem, or spot a bug in the app, please open an issue in this GitHub repository so we can take a look.

Enjoy smoother ComfyUI setups!
