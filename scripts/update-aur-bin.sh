#!/usr/bin/env bash
set -euo pipefail

VERSION=""
PKGREL="1"
OUTPUT_DIR="dist"
AUR_DIR="packaging/aur-bin"
REPOSITORY="ArcticLatent/Arctic-Helper"

usage() {
  cat <<'EOF'
Usage:
  scripts/update-aur-bin.sh --version <x.y.z> [options]

Options:
  --version <x.y.z>      Required release version.
  --pkgrel <n>           Arch package release number (default: 1).
  --output-dir <path>    Directory containing built release artifacts (default: dist).
  --aur-dir <path>       AUR package directory to update (default: packaging/aur-bin).
  --repository <owner/repo>
                         GitHub repository used for release URLs.
  -h, --help             Show help.
EOF
}

while (($# > 0)); do
  case "$1" in
    --version)
      VERSION="${2:-}"
      shift 2
      ;;
    --pkgrel)
      PKGREL="${2:-}"
      shift 2
      ;;
    --output-dir)
      OUTPUT_DIR="${2:-}"
      shift 2
      ;;
    --aur-dir)
      AUR_DIR="${2:-}"
      shift 2
      ;;
    --repository)
      REPOSITORY="${2:-}"
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

if [[ ! "$PKGREL" =~ ^[0-9]+$ ]]; then
  echo "pkgrel must be a positive integer" >&2
  exit 1
fi

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="$ROOT_DIR/$OUTPUT_DIR"
TARGET_AUR_DIR="$ROOT_DIR/$AUR_DIR"
PKGBUILD_PATH="$TARGET_AUR_DIR/PKGBUILD"
SRCINFO_PATH="$TARGET_AUR_DIR/.SRCINFO"
ASSET="arctic-comfyui-helper-${VERSION}-${PKGREL}-x86_64.pkg.tar.zst"
SHASUMS_PATH="$OUT_DIR/SHA256SUMS"

if [[ ! -f "$PKGBUILD_PATH" ]]; then
  echo "PKGBUILD not found: $PKGBUILD_PATH" >&2
  exit 1
fi

if [[ ! -f "$SHASUMS_PATH" ]]; then
  echo "SHA256SUMS not found: $SHASUMS_PATH" >&2
  exit 1
fi

if [[ ! -f "$OUT_DIR/$ASSET" ]]; then
  echo "Release asset not found: $OUT_DIR/$ASSET" >&2
  exit 1
fi

SHA="$(awk -v asset="$ASSET" '$2 == asset { print $1; exit }' "$SHASUMS_PATH")"
if [[ -z "$SHA" ]]; then
  echo "Could not find checksum for $ASSET in $SHASUMS_PATH" >&2
  exit 1
fi

tmp="$(mktemp)"
trap 'rm -f "$tmp"' EXIT

awk \
  -v version="$VERSION" \
  -v pkgrel="$PKGREL" \
  -v repository="$REPOSITORY" \
  -v asset="$ASSET" \
  -v sha="$SHA" '
  /^pkgver=/ {
    print "pkgver=" version
    next
  }
  /^pkgrel=/ {
    print "pkgrel=" pkgrel
    next
  }
  /^url=/ {
    print "url='\''https://github.com/" repository "'\''"
    next
  }
  /^_asset=/ {
    print "_asset=\"" asset "\""
    next
  }
  /^source_x86_64=/ {
    print "source_x86_64=(\"${url}/releases/download/v${pkgver}/${_asset}\")"
    next
  }
  /^noextract=/ {
    print "noextract=(\"${_asset}\")"
    next
  }
  /^sha256sums_x86_64=/ {
    print "sha256sums_x86_64=(" sprintf("%c", 39) sha sprintf("%c", 39) ")"
    next
  }
  { print }
  ' "$PKGBUILD_PATH" > "$tmp"

mv "$tmp" "$PKGBUILD_PATH"

(
  cd "$TARGET_AUR_DIR"
  makepkg --printsrcinfo > "$SRCINFO_PATH"
)

echo "Updated AUR binary package files:"
echo "  PKGBUILD:  $PKGBUILD_PATH"
echo "  .SRCINFO:  $SRCINFO_PATH"
echo "  Version:   $VERSION-$PKGREL"
echo "  Asset:     $ASSET"
echo "  SHA256:    $SHA"
