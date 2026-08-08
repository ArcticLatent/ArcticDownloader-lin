#!/usr/bin/env bash
set -euo pipefail

VERSION=""
TAG=""
REPOSITORY="ArcticLatent/Arctic-Helper"
OUTPUT_DIR="dist"

usage() {
  cat <<'USAGE'
Usage:
  scripts/verify-release-linux.sh --version <x.y.z> --tag <tag> [options]

Options:
  --version <x.y.z>      Expected release version.
  --tag <tag>            Expected release tag (example: v0.1.1).
  --repository <owner/repo>
                         Expected GitHub repository in manifest URLs.
  --output-dir <path>    Release artifact directory (default: dist).
  -h, --help             Show help.
USAGE
}

while (($# > 0)); do
  case "$1" in
    --version)
      VERSION="${2:-}"
      shift 2
      ;;
    --tag)
      TAG="${2:-}"
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

if [[ -z "$VERSION" || -z "$TAG" ]]; then
  echo "--version and --tag are required" >&2
  usage
  exit 1
fi

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="$ROOT_DIR/$OUTPUT_DIR"
MANIFEST="$OUT_DIR/linux-release.json"
SHASUMS="$OUT_DIR/SHA256SUMS"
VERIFY_TMP_DIR=""

cleanup() {
  if [[ -n "$VERIFY_TMP_DIR" && -d "$VERIFY_TMP_DIR" ]]; then
    rm -rf "$VERIFY_TMP_DIR"
  fi
}
trap cleanup EXIT

[[ -d "$OUT_DIR" ]] || { echo "Missing output dir: $OUT_DIR" >&2; exit 1; }
[[ -f "$MANIFEST" ]] || { echo "Missing manifest: $MANIFEST" >&2; exit 1; }
[[ -f "$SHASUMS" ]] || { echo "Missing checksums file: $SHASUMS" >&2; exit 1; }

if ! grep -q '"version": "'"$VERSION"'"' "$MANIFEST"; then
  echo "Manifest version does not match expected $VERSION" >&2
  exit 1
fi
if ! grep -q '"tag": "'"$TAG"'"' "$MANIFEST"; then
  echo "Manifest tag does not match expected $TAG" >&2
  exit 1
fi
if ! grep -q '"repository": "'"$REPOSITORY"'"' "$MANIFEST"; then
  echo "Manifest repository does not match expected $REPOSITORY" >&2
  exit 1
fi

(
  cd "$OUT_DIR"
  sha256sum -c SHA256SUMS
)

mapfile -t listed_assets < <(awk '{print $2}' "$SHASUMS" | sed 's|^\*||')
for asset in "${listed_assets[@]}"; do
  [[ -f "$OUT_DIR/$asset" ]] || { echo "Missing asset listed in SHA256SUMS: $asset" >&2; exit 1; }
  expected="https://github.com/$REPOSITORY/releases/download/$TAG/$asset"
  if ! grep -q "$expected" "$MANIFEST"; then
    echo "Manifest missing expected download URL: $expected" >&2
    exit 1
  fi
done

mapfile -t arch_packages < <(find "$OUT_DIR" -maxdepth 1 -type f -name '*.pkg.tar.*' | sort)
for package in "${arch_packages[@]}"; do
  if ! command -v bsdtar >/dev/null 2>&1; then
    echo "Required command not found while verifying Arch package: bsdtar" >&2
    exit 1
  fi

  VERIFY_TMP_DIR="$(mktemp -d)"
  bsdtar -xf "$package" -C "$VERIFY_TMP_DIR" usr/bin/arctic-comfyui-helper
  bash "$ROOT_DIR/scripts/verify-linux-elf.sh" \
    "$VERIFY_TMP_DIR/usr/bin/arctic-comfyui-helper" \
    '/lib64/ld-linux-x86-64.so.2'
  rm -rf "$VERIFY_TMP_DIR"
  VERIFY_TMP_DIR=""
done

echo "Verifying release manifest signature ..."
(cd "$ROOT_DIR" && cargo run --quiet --release --manifest-path tools/manifest-signer/Cargo.toml -- \
  verify --format linux-release --manifest "$MANIFEST")

echo "Release artifacts verified:"
echo "  Output:    $OUT_DIR"
echo "  Manifest:  $MANIFEST"
echo "  SHA256SUMS: $SHASUMS"
echo "  Version:   $VERSION"
echo "  Tag:       $TAG"
