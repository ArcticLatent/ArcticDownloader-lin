#!/usr/bin/env bash
set -euo pipefail

VERSION=""
REPOSITORY="ArcticLatent/Arctic-Helper"
OUTPUT_DIR="dist"
SKIP_CLEAN=0
DEB_DISTROBOX="arctic-ubuntu"
RPM_DISTROBOX="arctic-fedora"
AUR_PACKAGE="arctic-comfyui-helper-bin"
AUR_PKGREL="1"
AUR_REPO_DIR="${HOME}/aur/arctic-comfyui-helper-bin"
SKIP_AUR=0

usage() {
  cat <<'USAGE'
Usage:
  scripts/release-linux.sh [options]

Options:
  --version <x.y.z>      Release version (if omitted, prompt).
  --repository <owner/repo>
                         GitHub repository (default: ArcticLatent/Arctic-Helper).
  --output-dir <path>    Output directory (default: dist).
  --skip-clean           Skip cargo clean during build.
  --deb-distrobox <name> Distrobox name for Debian package build (default: arctic-ubuntu).
  --rpm-distrobox <name> Distrobox name for RPM package build (default: arctic-fedora).
  --aur-package <name>   AUR package name to update (default: arctic-comfyui-helper-bin).
  --aur-pkgrel <n>       AUR pkgrel value (default: 1).
  --aur-repo-dir <path>  Local AUR git repo checkout (default: ~/aur/arctic-comfyui-helper-bin).
  --skip-aur             Skip automatic AUR update/push.
  -h, --help             Show help.
USAGE
}

require_cmd() {
  local cmd="$1"
  command -v "$cmd" >/dev/null 2>&1 || {
    echo "Required command not found: $cmd" >&2
    exit 1
  }
}

while (($# > 0)); do
  case "$1" in
    --version)
      VERSION="${2:-}"
      shift 2
      ;;
    --repository)
      REPOSITORY="${2:-}"
      shift 2
      ;;
    --output-dir)
      OUTPUT_DIR="${2:-}"
      shift 2
      ;;
    --skip-clean)
      SKIP_CLEAN=1
      shift
      ;;
    --deb-distrobox)
      DEB_DISTROBOX="${2:-}"
      shift 2
      ;;
    --rpm-distrobox)
      RPM_DISTROBOX="${2:-}"
      shift 2
      ;;
    --aur-package)
      AUR_PACKAGE="${2:-}"
      shift 2
      ;;
    --aur-pkgrel)
      AUR_PKGREL="${2:-}"
      shift 2
      ;;
    --aur-repo-dir)
      AUR_REPO_DIR="${2:-}"
      shift 2
      ;;
    --skip-aur)
      SKIP_AUR=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown argument: $1" >&2
      usage
      exit 1
      ;;
  esac
done

if [[ -z "$VERSION" ]]; then
  read -r -p "Release version (example: 0.1.1): " VERSION
fi
if [[ ! "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "Version must be semantic version x.y.z" >&2
  exit 1
fi
if [[ ! "$AUR_PKGREL" =~ ^[0-9]+$ ]]; then
  echo "AUR pkgrel must be a positive integer" >&2
  exit 1
fi

TAG="v$VERSION"
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="$ROOT_DIR/$OUTPUT_DIR"
AUR_REPO_DIR="${AUR_REPO_DIR/#\~/$HOME}"
NOTES_TMP="$(mktemp)"
trap 'rm -f "$NOTES_TMP"' EXIT

echo
echo "Paste release notes. End with a single line containing END"
while IFS= read -r line; do
  [[ "$line" == "END" ]] && break
  printf '%s\n' "$line" >> "$NOTES_TMP"
done
if [[ ! -s "$NOTES_TMP" ]]; then
  printf 'Release %s\n' "$TAG" > "$NOTES_TMP"
fi

require_cmd gh
require_cmd bash
require_cmd git

echo "Checking GitHub auth ..."
gh auth status >/dev/null

echo "Building release artifacts ..."
BUILD_ARGS=(--version "$VERSION" --repository "$REPOSITORY" --tag "$TAG" --output-dir "$OUTPUT_DIR" --notes-file "$NOTES_TMP")
if ((SKIP_CLEAN == 1)); then
  BUILD_ARGS+=(--skip-clean)
fi
BUILD_ARGS+=(--deb-distrobox "$DEB_DISTROBOX" --rpm-distrobox "$RPM_DISTROBOX")

(cd "$ROOT_DIR" && bash scripts/build-release-linux.sh "${BUILD_ARGS[@]}")

NOTES_FILE="$OUT_DIR/release-notes-$TAG.md"
MANIFEST_FILE="$OUT_DIR/linux-release.json"
SHAS_FILE="$OUT_DIR/SHA256SUMS"

(cd "$ROOT_DIR" && bash scripts/verify-release-linux.sh --version "$VERSION" --tag "$TAG" --repository "$REPOSITORY" --output-dir "$OUTPUT_DIR")

mapfile -t release_assets < <(find "$OUT_DIR" -maxdepth 1 -type f \( -name '*.pkg.tar.*' -o -name '*.deb' -o -name '*.rpm' -o -name '*.src.rpm' \) | sort)
release_assets+=("$SHAS_FILE" "$MANIFEST_FILE")

echo "Publishing GitHub release $TAG to $REPOSITORY ..."
if gh release view "$TAG" --repo "$REPOSITORY" >/dev/null 2>&1; then
  gh release edit "$TAG" --repo "$REPOSITORY" --title "$TAG" --notes-file "$NOTES_FILE"
  gh release upload "$TAG" "${release_assets[@]}" --repo "$REPOSITORY" --clobber
else
  gh release create "$TAG" "${release_assets[@]}" --repo "$REPOSITORY" --title "$TAG" --notes-file "$NOTES_FILE"
fi

if ((SKIP_AUR == 0)); then
  echo "Updating AUR package metadata for $AUR_PACKAGE ..."
  (cd "$ROOT_DIR" && bash scripts/update-aur-bin.sh --version "$VERSION" --pkgrel "$AUR_PKGREL" --output-dir "$OUTPUT_DIR" --repository "$REPOSITORY")

  if [[ ! -d "$AUR_REPO_DIR/.git" ]]; then
    echo "Cloning AUR repo into $AUR_REPO_DIR ..."
    mkdir -p "$(dirname "$AUR_REPO_DIR")"
    git clone "ssh://aur@aur.archlinux.org/${AUR_PACKAGE}.git" "$AUR_REPO_DIR"
  fi

  cp "$ROOT_DIR/packaging/aur-bin/PKGBUILD" "$AUR_REPO_DIR/PKGBUILD"
  cp "$ROOT_DIR/packaging/aur-bin/.SRCINFO" "$AUR_REPO_DIR/.SRCINFO"

  (
    cd "$AUR_REPO_DIR"
    if [[ -n "$(git status --porcelain)" ]]; then
      git add PKGBUILD .SRCINFO
      git commit -m "Update to ${VERSION}-${AUR_PKGREL}"
      git push origin master
      echo "AUR package pushed: $AUR_PACKAGE"
    else
      echo "AUR package already up to date: $AUR_PACKAGE"
    fi
  )
fi

echo
echo "Release complete:"
echo "  Repo:      $REPOSITORY"
echo "  Tag:       $TAG"
echo "  Output:    $OUT_DIR"
echo "  Manifest:  $MANIFEST_FILE"
if ((SKIP_AUR == 0)); then
  echo "  AUR:       $AUR_PACKAGE"
  echo "  AUR repo:  $AUR_REPO_DIR"
fi
