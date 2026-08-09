<p align="center">
  <img src="assets/icon.svg" alt="Arctic ComfyUI Helper" width="148" />
</p>

<h1 align="center">Arctic ComfyUI Helper</h1>

<p align="center">
  A curated Windows and Linux companion for ComfyUI users who want the right models, LoRAs, and setup tools without guesswork.
</p>

<p align="center">
  <img alt="Windows" src="https://img.shields.io/badge/Platform-Windows%2010%2F11-0078D4?style=for-the-badge&logo=windows&logoColor=white" />
  <img alt="Linux" src="https://img.shields.io/badge/Platform-Linux-FCC624?style=for-the-badge&logo=linux&logoColor=black" />
  <img alt="NixOS" src="https://img.shields.io/badge/Package-NixOS-5277C3?style=for-the-badge&logo=nixos&logoColor=white" />
  <img alt="Rust" src="https://img.shields.io/badge/Built%20with-Rust-000000?style=for-the-badge&logo=rust" />
  <img alt="Tauri" src="https://img.shields.io/badge/Desktop-Tauri-24C8DB?style=for-the-badge&logo=tauri&logoColor=white" />
</p>

---

## 📚 Overview

Arctic ComfyUI Helper mirrors the exact builds shown in Arctic Latent tutorials, so you can follow along with less setup friction.

Think of it as:
- A built-in **ComfyUI installer and manager** for Windows and Linux
- A curated model/LoRA catalog matched to your hardware tiers
- A one-click downloader that places assets into the correct ComfyUI folders

---

## ✨ New in v0.2.9

- Fixed Arch packages shipping with a NixOS dynamic loader that prevented them from starting on Arch Linux
- Arch packages are now built natively on Arch and verified for the standard `/lib64/ld-linux-x86-64.so.2` interpreter
- Release verification now rejects Linux binaries containing leaked `/nix/store` or build-machine home paths
- Nix release artifacts can now be built from Arch with Podman and the official `nixos/nix` image

---

## 🧩 Core Features

- 🛠️ **ComfyUI install module** (uv-managed Python + selectable add-ons/custom nodes)
- 🧠 **Tier-aware catalog** that filters by your GPU VRAM and system RAM
- 📦 **Auto-dependency downloads** (text encoders, CLIPs, upscalers, and other required files)
- 🗂️ **Smart file placement** into the correct ComfyUI subfolders
- 📈 **Live download progress** with active/completed transfer tracking
- 🔐 **Optional Civitai token support** for authenticated LoRA downloads
- 🖼️ **LoRA preview + metadata** in-app (description, triggers, creator link)
- ♻️ **Verified update support** through signed GitHub Releases manifests
- 🧵 **System tray controls** to Start/Stop ComfyUI even when the main window is hidden

---

## 🧰 ComfyUI Installer Highlights

Inside the **ComfyUI** tab, you can:

- Select a base folder and install a fresh ComfyUI instance
- Manage an existing ComfyUI installation
- Use automatic Torch/CUDA recommendation based on detected NVIDIA GPU
- Override Torch stack manually from dropdown
- Toggle add-ons and custom nodes from UI

### Available Add-Ons

- SageAttention
- SageAttention3 (RTX 50-series only)
- FlashAttention
- InsightFace
- Nunchaku
- Trellis2 (requires Torch 2.8.0 + cu128 or newer)
- Pinned Memory (enabled by default)

### Available Custom Nodes

- comfyui-manager
- ComfyUI-Easy-Use
- rgthree-comfy
- ComfyUI-GGUF
- comfyui-kjnodes

---

## 🚀 Getting Started

1. Download the package for your operating system from this repo's **Releases** page.
2. Install or run the package.
3. In **Models** / **LoRAs**, select your existing ComfyUI folder to download assets.
4. In **ComfyUI** tab, use **Install New** (or **Manage Existing**) if you want the app to install/manage ComfyUI itself.
5. Optional advanced logging: launch from terminal with  
   `.\Arctic-ComfyUI-Helper.exe --nerdstats`

That is it. Pick your setup, click, and the app handles the rest.

### Fedora (COPR)

```bash
sudo dnf copr enable burcebor/arctic-helper
sudo dnf install arctic-comfyui-helper
```

Project page: [burcebor/arctic-helper](https://copr.fedorainfracloud.org/coprs/burcebor/arctic-helper/)

### NixOS / Nix

Run without installing:

```bash
nix run 'tarball+https://github.com/ArcticLatent/Arctic-Helper/releases/latest/download/arctic-comfyui-helper-nix-x86_64.tar.gz'
```

Install into your user profile:

```bash
nix profile add 'tarball+https://github.com/ArcticLatent/Arctic-Helper/releases/latest/download/arctic-comfyui-helper-nix-x86_64.tar.gz'
```

Update a profile installation with `nix profile upgrade --refresh arctic-comfyui-helper`.
For a declarative NixOS configuration, add the same tarball URL as a flake input
and add `inputs.arctic-helper.packages.${pkgs.system}.default` to
`environment.systemPackages`. Only `x86_64-linux` is currently supported.

Nix installations check the signed release manifest and display the latest
available version in the app. The **How to Update** action opens the latest
release page and records Nix guidance in the application log. Installation
remains delegated to Nix because applications cannot modify binaries in the
immutable Nix store.

---

## 🖼️ Demo Preview

![Arctic Downloader Demo](assets/demo.png)

---

## 🔄 Updates and Release Verification

On supported installations, Arctic ComfyUI Helper checks the signed release
manifests published with each GitHub release.

- Windows uses `update.json`.
- Linux packages use `linux-release.json` to select the matching Debian,
  Fedora, or Arch artifact.
- Nix installations use the same signed Linux manifest to detect newer
  versions without downloading or installing them automatically.
- Release manifests are authenticated with Ed25519 signatures, and downloaded
  application files are verified against SHA-256 checksums before installation.
- Nix profile installations are updated with
  `nix profile upgrade --refresh arctic-comfyui-helper`; declarative
  installations are updated through their flake configuration and normal
  system rebuild.

---

## ✅ Requirements

- Latest NVIDIA drivers installed
- Internet connection (for catalog, model files, and optional installer tasks)
- For some Civitai LoRAs: a valid Civitai API token

---

## 💡 Usage Tips

- If a LoRA says unauthorized, add your Civitai token in-app and save it.
- If you run multiple ComfyUI installs, use the ComfyUI tab's install/manage mode and detected installs list.

---

## 🆘 Need Help?

Open an issue in this repository with:
- What you clicked
- What you expected
- What happened
- Any log lines shown in the app
- If possible, run with `--nerdstats` and include the exact terminal logs in your issue

---

## 🧊 Author

Burce Boran 🎥 Asset Supervisor / VFX Artist | 🐧 Arctic Latent

[![YouTube – Arctic Latent](https://img.shields.io/badge/YouTube-%40ArcticLatent-FF0000?logo=youtube&logoColor=white)](https://youtube.com/@ArcticLatent)
[![Patreon – Arctic Latent](https://img.shields.io/badge/Patreon-Arctic%20Latent-FF424D?logo=patreon&logoColor=white)](https://patreon.com/ArcticLatent)
[![Hugging Face – Arctic Latent](https://img.shields.io/badge/HuggingFace-Arctic%20Latent-FFD21E?logo=huggingface&logoColor=white)](https://huggingface.co/arcticlatent)
[![Vimeo – Demo Reel](https://img.shields.io/badge/Vimeo-Demo%20Reel-1ab7ea?logo=vimeo&logoColor=white)](https://vimeo.com/1044521891)

---

## License

Copyright 2026 Arctic Latent.

Arctic ComfyUI Helper is licensed under the [Apache License 2.0](https://www.apache.org/licenses/LICENSE-2.0). You may use, modify, and redistribute the software under its terms. The complete license text is included with the source and release packages.

As provided by Section 6 of the license, Apache-2.0 does not grant permission to use Arctic Helper trade names, trademarks, service marks, or product names except as required for reasonable and customary description of the software's origin.
