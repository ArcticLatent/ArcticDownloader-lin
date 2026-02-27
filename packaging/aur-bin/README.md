# AUR Binary Package

This directory contains the AUR recipe for the prebuilt Arch package:

- Package name: `arctic-comfyui-helper-bin`
- Source asset: GitHub release `.pkg.tar.zst`

## Update for a new release

Preferred:

```bash
bash scripts/update-aur-bin.sh --version 0.1.4
```

This updates:

- `pkgver`
- `pkgrel`
- release asset filename
- GitHub release URL
- `sha256sums_x86_64`
- `.SRCINFO`

Manual fallback:

1. Update `pkgver` and `pkgrel` in `PKGBUILD`.
2. Update `sha256sums_x86_64` to match the new release asset.
3. Regenerate `.SRCINFO`:

```bash
cd packaging/aur-bin
makepkg --printsrcinfo > .SRCINFO
```

## Publish to AUR

```bash
git clone ssh://aur@aur.archlinux.org/arctic-comfyui-helper-bin.git
cd arctic-comfyui-helper-bin
cp /home/abra/Projects/ArcticDownloader-lin/packaging/aur-bin/PKGBUILD .
cp /home/abra/Projects/ArcticDownloader-lin/packaging/aur-bin/.SRCINFO .
git add PKGBUILD .SRCINFO
git commit -m "Update to 0.1.4"
git push
```
