#!/usr/bin/env bash
set -euo pipefail

VERSION=""
REPOSITORY="ArcticLatent/Arctic-Helper"
TAG=""
OUTPUT_DIR="dist"
NOTES_FILE=""
SKIP_CLEAN=0
DEB_DISTROBOX="arctic-ubuntu"
RPM_DISTROBOX="arctic-fedora"

usage() {
  cat <<'USAGE'
Usage:
  scripts/build-release-linux.sh --version <x.y.z> [options]

Options:
  --version <x.y.z>      Required semantic version.
  --repository <owner/repo>
                         GitHub repository used for download URLs.
  --tag <tag>            Release tag (default: v<version>).
  --output-dir <path>    Output directory for release artifacts (default: dist).
  --notes-file <path>    Optional markdown notes file copied into output dir.
  --skip-clean           Skip cargo clean.
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
    --tag)
      TAG="${2:-}"
      shift 2
      ;;
    --output-dir)
      OUTPUT_DIR="${2:-}"
      shift 2
      ;;
    --notes-file)
      NOTES_FILE="${2:-}"
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
  echo "--version is required" >&2
  usage
  exit 1
fi

if [[ ! "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "Version must be semantic version x.y.z" >&2
  exit 1
fi

if [[ -z "$TAG" ]]; then
  TAG="v$VERSION"
fi

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PACKAGING_DIR="$ROOT_DIR/packaging"
OUT_ABS_DIR="$ROOT_DIR/$OUTPUT_DIR"
# Ensure rustup cargo/rustc are visible even when invoked from fish or clean shells.
export PATH="$HOME/.cargo/bin:$PATH"

if [[ -n "$NOTES_FILE" && ! -f "$NOTES_FILE" ]]; then
  echo "Notes file not found: $NOTES_FILE" >&2
  exit 1
fi

require_cmd cargo
require_cmd sha256sum
require_cmd bash
require_cmd distrobox

update_simple_version() {
  local file="$1"
  local pattern="$2"
  local replacement="$3"
  local tmp
  tmp="$(mktemp)"
  if ! sed -E "$pattern" "$file" > "$tmp"; then
    rm -f "$tmp"
    echo "Failed updating $file" >&2
    exit 1
  fi
  mv "$tmp" "$file"
}

prepend_debian_changelog() {
  local file="$1"
  local version="$2"
  local summary="$3"

  local header="arctic-comfyui-helper (${version}-1) unstable; urgency=medium"
  local current
  current="$(head -n 1 "$file" || true)"
  if [[ "$current" == "$header" ]]; then
    return
  fi

  local when
  when="$(date -R)"
  local tmp
  tmp="$(mktemp)"
  {
    echo "$header"
    echo
    echo "  * ${summary}"
    echo
    echo " -- Arctic Latent <contact@arcticlatent.com>  ${when}"
    echo
    cat "$file"
  } > "$tmp"
  mv "$tmp" "$file"
}

summary_note="Release v$VERSION"
if [[ -n "$NOTES_FILE" ]]; then
  first_line="$(grep -m1 -v '^\s*$' "$NOTES_FILE" || true)"
  if [[ -n "$first_line" ]]; then
    summary_note="$first_line"
  fi
fi

echo "Updating versions to $VERSION ..."
update_simple_version "$ROOT_DIR/Cargo.toml" '0,/^version\s*=\s*"[^"]+"/{s//version = "'"$VERSION"'"/}'
update_simple_version "$ROOT_DIR/src-tauri/Cargo.toml" '0,/^version\s*=\s*"[^"]+"/{s//version = "'"$VERSION"'"/}'
update_simple_version "$ROOT_DIR/src-tauri/tauri.conf.json" '0,/"version"\s*:\s*"[^"]+"/{s//"version": "'"$VERSION"'"/}'
update_simple_version "$PACKAGING_DIR/arch/PKGBUILD" 's/^pkgver=.*/pkgver='"$VERSION"'/'
update_simple_version "$PACKAGING_DIR/fedora/arctic-comfyui-helper.spec" 's/^Version:\s*.*/Version:        '"$VERSION"'/'
prepend_debian_changelog "$PACKAGING_DIR/debian/debian/changelog" "$VERSION" "$summary_note"

if ((SKIP_CLEAN == 0)); then
  echo "Running clean build ..."
  (cd "$ROOT_DIR" && cargo clean --manifest-path src-tauri/Cargo.toml)
fi

rm -rf "$PACKAGING_DIR/out"
rm -rf "$OUT_ABS_DIR"
mkdir -p "$OUT_ABS_DIR"

echo "Building Arch package on host ..."
(cd "$ROOT_DIR" && bash packaging/build-packages.sh arch)

echo "Building Debian package in distrobox '$DEB_DISTROBOX' ..."
distrobox enter "$DEB_DISTROBOX" -- bash -lc "
  set -euo pipefail
  export PATH=\"\$HOME/.cargo/bin:\$PATH\"
  sudo apt purge -y arctic-comfyui-helper || true
  sudo apt autoremove -y || true
  cd '$ROOT_DIR'
  bash packaging/build-packages.sh deb
"

echo "Building RPM package in distrobox '$RPM_DISTROBOX' ..."
distrobox enter "$RPM_DISTROBOX" -- bash -lc "
  set -euo pipefail
  export PATH=\"\$HOME/.cargo/bin:\$PATH\"
  sudo dnf remove -y arctic-comfyui-helper || true
  cd '$ROOT_DIR'
  bash packaging/build-packages.sh rpm
"

mapfile -t artifacts < <(find "$PACKAGING_DIR/out" -type f \( -name '*.pkg.tar.*' -o -name '*.deb' -o -name '*.rpm' -o -name '*.src.rpm' \) | sort)
if ((${#artifacts[@]} == 0)); then
  echo "No package artifacts were produced." >&2
  exit 1
fi

for f in "${artifacts[@]}"; do
  cp -f "$f" "$OUT_ABS_DIR/"
done

if [[ -n "$NOTES_FILE" ]]; then
  cp -f "$NOTES_FILE" "$OUT_ABS_DIR/release-notes-$TAG.md"
fi

(
  cd "$OUT_ABS_DIR"
  rm -f SHA256SUMS
  mapfile -t copied < <(find . -maxdepth 1 -type f \( -name '*.pkg.tar.*' -o -name '*.deb' -o -name '*.rpm' -o -name '*.src.rpm' \) -printf '%f\n' | sort)
  sha256sum "${copied[@]}" > SHA256SUMS
)

manifest="$OUT_ABS_DIR/linux-release.json"
{
  echo "{"
  echo "  \"version\": \"$VERSION\"," 
  echo "  \"tag\": \"$TAG\"," 
  echo "  \"repository\": \"$REPOSITORY\"," 
  echo "  \"assets\": ["
  mapfile -t copied < <(find "$OUT_ABS_DIR" -maxdepth 1 -type f \( -name '*.pkg.tar.*' -o -name '*.deb' -o -name '*.rpm' -o -name '*.src.rpm' \) -printf '%f\n' | sort)
  for i in "${!copied[@]}"; do
    name="${copied[$i]}"
    sha="$(sha256sum "$OUT_ABS_DIR/$name" | awk '{print $1}')"
    url="https://github.com/$REPOSITORY/releases/download/$TAG/$name"
    comma=","
    if [[ "$i" -eq "$((${#copied[@]} - 1))" ]]; then
      comma=""
    fi
    echo "    {\"name\": \"$name\", \"sha256\": \"$sha\", \"download_url\": \"$url\"}$comma"
  done
  echo "  ]"
  echo "}"
} > "$manifest"

echo "Build release artifacts complete:"
echo "  Output: $OUT_ABS_DIR"
echo "  Manifest: $manifest"
echo "  Checksums: $OUT_ABS_DIR/SHA256SUMS"
