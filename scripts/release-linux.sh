#!/usr/bin/env bash
set -euo pipefail

VERSION=""
REPOSITORY="ArcticLatent/Arctic-Helper"
OUTPUT_DIR="dist"
INPUT_NOTES_FILE=""
SKIP_CLEAN=0
PUBLISH_COPR=0
COPR_PROJECT="${ARCTIC_COPR_PROJECT:-burcebor/arctic-helper}"
DEB_DISTROBOX="${ARCTIC_DEB_DISTROBOX:-arctic-ubuntu}"
ARCH_DISTROBOX="${ARCTIC_ARCH_DISTROBOX:-arctic-arch}"

usage() {
  cat <<'USAGE'
Usage:
  scripts/release-linux.sh [options]

Options:
  --version <x.y.z>      Release version (if omitted, prompt).
  --repository <owner/repo>
                         GitHub repository (default: ArcticLatent/Arctic-Helper).
  --output-dir <path>    Output directory (default: dist).
  --notes-file <path>    Release notes markdown file. Defaults to
                         CHANGELOG_<version>.md when it exists; otherwise prompt.
  --skip-clean           Skip cargo clean during build.
  --publish-copr        Publish the generated SRPM to Fedora COPR after verification.
  --copr-project <owner/name>
                         COPR project (default: burcebor/arctic-helper).
  --arch-distrobox <name>
                         Distrobox name for Arch package build (default: arctic-arch).
  --deb-distrobox <name> Distrobox name for Debian package build (default: arctic-ubuntu).
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
    --notes-file)
      INPUT_NOTES_FILE="${2:-}"
      shift 2
      ;;
    --skip-clean)
      SKIP_CLEAN=1
      shift
      ;;
    --publish-copr)
      PUBLISH_COPR=1
      shift
      ;;
    --copr-project)
      COPR_PROJECT="${2:-}"
      shift 2
      ;;
    --deb-distrobox)
      DEB_DISTROBOX="${2:-}"
      shift 2
      ;;
    --arch-distrobox)
      ARCH_DISTROBOX="${2:-}"
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

if [[ -z "$INPUT_NOTES_FILE" && -f "$ROOT_DIR/CHANGELOG_$VERSION.md" ]]; then
  INPUT_NOTES_FILE="$ROOT_DIR/CHANGELOG_$VERSION.md"
fi

if [[ -n "$INPUT_NOTES_FILE" ]]; then
  if [[ ! -f "$INPUT_NOTES_FILE" ]]; then
    echo "Release notes file not found: $INPUT_NOTES_FILE" >&2
    exit 1
  fi
  cp "$INPUT_NOTES_FILE" "$NOTES_TMP"
  echo "Using release notes from $INPUT_NOTES_FILE"
else
  echo
  echo "Paste release notes. End with a single line containing END"
  while IFS= read -r line; do
    [[ "$line" == "END" ]] && break
    printf '%s\n' "$line" >> "$NOTES_TMP"
  done
  if [[ ! -s "$NOTES_TMP" ]]; then
    printf 'Release %s\n' "$TAG" > "$NOTES_TMP"
  fi
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
BUILD_ARGS+=(--arch-distrobox "$ARCH_DISTROBOX" --deb-distrobox "$DEB_DISTROBOX")

(cd "$ROOT_DIR" && bash scripts/build-release-linux.sh "${BUILD_ARGS[@]}")

NOTES_FILE="$OUT_DIR/release-notes-$TAG.md"
MANIFEST_FILE="$OUT_DIR/linux-release.json"
SHAS_FILE="$OUT_DIR/SHA256SUMS"

(cd "$ROOT_DIR" && bash scripts/verify-release-linux.sh --version "$VERSION" --tag "$TAG" --repository "$REPOSITORY" --output-dir "$OUTPUT_DIR")

if ((PUBLISH_COPR == 1)); then
  mapfile -t copr_srpms < <(find "$OUT_DIR" -maxdepth 1 -type f -name '*.src.rpm' | sort)
  if ((${#copr_srpms[@]} != 1)); then
    echo "Expected exactly one SRPM for COPR publishing; found ${#copr_srpms[@]}." >&2
    exit 1
  fi
  (cd "$ROOT_DIR" && bash scripts/publish-copr.sh \
    --project "$COPR_PROJECT" \
    --srpm "${copr_srpms[0]}" \
    --yes)
fi

mapfile -t release_assets < <(find "$OUT_DIR" -maxdepth 1 -type f \( -name '*.pkg.tar.*' -o -name '*.deb' -o -name '*.rpm' -o -name '*.src.rpm' -o -name '*.flatpak' -o -name 'arctic-comfyui-helper-nix-*.tar.gz' -o -name 'arctic-comfyui-helper-*-nixos-*.tar.gz' \) | sort)
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
