#!/usr/bin/env bash
set -euo pipefail

VERSION=""
REPOSITORY="ArcticLatent/Arctic-Helper"
OUTPUT_DIR="dist"
SKIP_CLEAN=0
DEB_DISTROBOX="arctic-ubuntu"
RPM_DISTROBOX="arctic-fedora"

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

TAG="v$VERSION"
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="$ROOT_DIR/$OUTPUT_DIR"
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

echo
echo "Release complete:"
echo "  Repo:      $REPOSITORY"
echo "  Tag:       $TAG"
echo "  Output:    $OUT_DIR"
echo "  Manifest:  $MANIFEST_FILE"
