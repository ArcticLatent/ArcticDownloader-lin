<p align="center">
  <img src="assets/icon.svg" alt="Arctic Downloader" width="160">
</p>

# Arctic Downloader

Arctic Downloader is a helper app for people who run ComfyUI and want a simple way to grab the right models, VAE files, and LoRAs for their setup. It’s curated to mirror the builds shown in my YouTube tutorials so you can follow along without hunting for the assets yourself. Think of it as a catalog with a big “download” button that understands your hardware limits so you don’t have to guess which files will actually fit.

## What it does

- Shows a catalog of hand-picked ComfyUI models and LoRAs.
- Lets you choose your GPU VRAM and system RAM, then highlights the variants that make sense for those limits.
- Saves everything into the right subfolders under your ComfyUI installation so you can start using the files right away.
- Gives you live progress for each download and, when it’s done, lists exactly which files landed where (with quick-open buttons).
- Supports optional Civitai API tokens for LoRAs that need an account.

## Getting Started

1. **Install ComfyUI** and make sure you know where its folder lives.
2. **Download the latest `.flatpak` release** from this repository’s Releases page.
3. Install it (for example on Linux: `flatpak install arctic-downloader.flatpak`).
4. Launch Arctic Downloader and pick your ComfyUI folder when prompted.

That’s it—browse the catalog, pick what you want, and click download. The app handles the rest.

## Tips

- The VRAM tiers (S, A, B, C) give you a quick way to match files to your GPU size. If you’re unsure, pick the lowest tier that matches your card to avoid running out of memory.
- Use the legend inside the app if you want a refresher on the quantization shorthand (fp16, fp8, GGUF, etc.).
- If you drop in new hardware later, just change your tier in the app and it will show the upgraded variants automatically.

## Need Help?

If you need help, hit a problem, or spot a bug in the app, please open an issue in this GitHub repository so we can take a look.

Enjoy smoother ComfyUI setups!
