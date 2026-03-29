#!/usr/bin/env bash
set -euo pipefail

VERSION=""
OUTPUT_DIR="dist"
REPOSITORY="ArcticLatent/Arctic-Helper"
AUR_PACKAGE="arctic-comfyui-helper-bin"
AUR_PKGREL="1"
AUR_REPO_DIR="${HOME}/aur/arctic-comfyui-helper-bin"

usage() {
  cat <<'USAGE'
Usage:
  scripts/publish-aur-only.sh --version <x.y.z> [options]

Options:
  --version <x.y.z>      Required release version.
  --output-dir <path>    Directory containing existing built artifacts (default: dist).
  --repository <owner/repo>
                         GitHub repository used for release URLs.
  --aur-package <name>   AUR package name to update (default: arctic-comfyui-helper-bin).
  --aur-pkgrel <n>       AUR pkgrel value (default: 1).
  --aur-repo-dir <path>  Local AUR git repo checkout (default: ~/aur/arctic-comfyui-helper-bin).
  -h, --help             Show help.
USAGE
}

while (($# > 0)); do
  case "$1" in
    --version)
      VERSION="${2:-}"
      shift 2
      ;;
    --output-dir)
      OUTPUT_DIR="${2:-}"
      shift 2
      ;;
    --repository)
      REPOSITORY="${2:-}"
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
  echo "--version is required" >&2
  usage
  exit 1
fi

if [[ ! "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "Version must be semantic version x.y.z" >&2
  exit 1
fi

if [[ ! "$AUR_PKGREL" =~ ^[0-9]+$ ]]; then
  echo "AUR pkgrel must be a positive integer" >&2
  exit 1
fi

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="$ROOT_DIR/$OUTPUT_DIR"
AUR_REPO_DIR="${AUR_REPO_DIR/#\~/$HOME}"

if [[ ! -d "$OUT_DIR" ]]; then
  echo "Output directory not found: $OUT_DIR" >&2
  exit 1
fi

if [[ ! -f "$OUT_DIR/SHA256SUMS" ]]; then
  echo "SHA256SUMS not found in: $OUT_DIR" >&2
  exit 1
fi

echo "Updating AUR package metadata for $AUR_PACKAGE from existing artifacts in $OUT_DIR ..."
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

echo
echo "AUR publish complete:"
echo "  Version:   $VERSION-$AUR_PKGREL"
echo "  Package:   $AUR_PACKAGE"
echo "  Output:    $OUT_DIR"
echo "  Repo:      $AUR_REPO_DIR"
