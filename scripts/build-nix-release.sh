#!/usr/bin/env bash
set -euo pipefail

VERSION=""
REPOSITORY="ArcticLatent/Arctic-Helper"
TAG=""
OUTPUT_DIR="dist"

usage() {
  cat <<'USAGE'
Usage:
  scripts/build-nix-release.sh --version <x.y.z> [options]

Options:
  --version <x.y.z>      Required semantic version.
  --repository <owner/repo>
                         GitHub repository hosting the release artifacts.
  --tag <tag>            Release tag (default: v<version>).
  --output-dir <path>    Release artifact directory (default: dist).
  -h, --help             Show help.
USAGE
}

while (($# > 0)); do
  case "$1" in
    --version) VERSION="${2:-}"; shift 2 ;;
    --repository) REPOSITORY="${2:-}"; shift 2 ;;
    --tag) TAG="${2:-}"; shift 2 ;;
    --output-dir) OUTPUT_DIR="${2:-}"; shift 2 ;;
    -h|--help) usage; exit 0 ;;
    *) echo "Unknown argument: $1" >&2; usage; exit 1 ;;
  esac
done

if [[ ! "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "--version must be a semantic version (x.y.z)" >&2
  exit 1
fi
[[ -n "$TAG" ]] || TAG="v$VERSION"

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

load_public_catalog_env() {
  local env_file="${ARCTIC_ENV_FILE:-$ROOT_DIR/.env}"
  local line name value

  if [[ -f "$env_file" ]]; then
    while IFS= read -r line || [[ -n "$line" ]]; do
      line="${line#"${line%%[![:space:]]*}"}"
      [[ -n "$line" && "$line" != \#* && "$line" == *=* ]] || continue
      name="${line%%=*}"
      value="${line#*=}"
      name="$(printf '%s' "$name" | sed -E 's/[[:space:]]+$//')"
      value="$(printf '%s' "$value" | sed -E 's/^[[:space:]]+//; s/[[:space:]]+$//')"
      if [[ "$value" == \"*\" && "$value" == *\" ]] || [[ "$value" == \'*\' && "$value" == *\' ]]; then
        value="${value:1:$((${#value} - 2))}"
      fi
      case "$name" in
        ARCTIC_SUPABASE_URL|ARCTIC_SUPABASE_ANON_KEY|ARCTIC_SUPABASE_PUBLISHABLE_KEY)
          if [[ -z "${!name:-}" ]]; then
            export "$name=$value"
          fi
          ;;
      esac
    done < "$env_file"
  fi

  if [[ -z "${ARCTIC_SUPABASE_URL:-}" ]]; then
    echo "Missing ARCTIC_SUPABASE_URL. Set it in .env or the current shell." >&2
    exit 1
  fi
  if [[ -z "${ARCTIC_SUPABASE_ANON_KEY:-}" && -z "${ARCTIC_SUPABASE_PUBLISHABLE_KEY:-}" ]]; then
    echo "Missing ARCTIC_SUPABASE_ANON_KEY or ARCTIC_SUPABASE_PUBLISHABLE_KEY." >&2
    exit 1
  fi
}

load_public_catalog_env

if [[ "$OUTPUT_DIR" = /* ]]; then
  OUT_DIR="$OUTPUT_DIR"
else
  OUT_DIR="$ROOT_DIR/$OUTPUT_DIR"
fi
TEMPLATE_DIR="$ROOT_DIR/packaging/nix"
STAGING_DIR="$(mktemp -d)"
trap 'rm -rf "$STAGING_DIR"' EXIT

command -v nix >/dev/null 2>&1 || {
  echo "Required command not found: nix" >&2
  exit 1
}

mkdir -p "$OUT_DIR" "$STAGING_DIR/native"
nix_output="$(nix build \
  --impure \
  --no-link \
  --print-out-paths \
  --expr "
    let
      flake = builtins.getFlake \"path:$ROOT_DIR\";
      pkgs = import flake.inputs.nixpkgs { system = \"x86_64-linux\"; };
    in
    pkgs.callPackage $ROOT_DIR/packaging/nix/source-package.nix {
      arcticSupabaseUrl = builtins.getEnv \"ARCTIC_SUPABASE_URL\";
      arcticSupabaseAnonKey = builtins.getEnv \"ARCTIC_SUPABASE_ANON_KEY\";
      arcticSupabasePublishableKey = builtins.getEnv \"ARCTIC_SUPABASE_PUBLISHABLE_KEY\";
    }
  ")"

install -Dm755 "$nix_output/bin/.arctic-comfyui-helper-wrapped" \
  "$STAGING_DIR/native/arctic-comfyui-helper"
install -Dm644 "$ROOT_DIR/packaging/linux/io.github.ArcticHelper.desktop" \
  "$STAGING_DIR/native/io.github.ArcticHelper.desktop"
install -Dm644 "$ROOT_DIR/src-tauri/dist/icon.svg" \
  "$STAGING_DIR/native/io.github.ArcticHelper.svg"
install -Dm644 "$ROOT_DIR/README.public.md" \
  "$STAGING_DIR/native/README.md"
install -Dm644 "$ROOT_DIR/LICENSE" \
  "$STAGING_DIR/native/LICENSE"

native_name="arctic-comfyui-helper-${VERSION}-nixos-x86_64.tar.gz"
native_artifact="$OUT_DIR/$native_name"
tar \
  --sort=name \
  --mtime='@0' \
  --owner=0 \
  --group=0 \
  --numeric-owner \
  -C "$STAGING_DIR/native" \
  -czf "$native_artifact" \
  arctic-comfyui-helper \
  io.github.ArcticHelper.desktop \
  io.github.ArcticHelper.svg \
  README.md \
  LICENSE

native_sha256="$(sha256sum "$native_artifact" | awk '{print $1}')"
native_url="https://github.com/$REPOSITORY/releases/download/$TAG/$native_name"

sed \
  -e "s|@VERSION@|$VERSION|g" \
  "$TEMPLATE_DIR/binary-flake.nix.in" > "$STAGING_DIR/flake.nix"
sed \
  -e "s|@VERSION@|$VERSION|g" \
  -e "s|@NATIVE_URL@|$native_url|g" \
  -e "s|@NATIVE_SHA256@|$native_sha256|g" \
  "$TEMPLATE_DIR/binary-package.nix.in" > "$STAGING_DIR/package.nix"

cp "$ROOT_DIR/flake.lock" "$STAGING_DIR/flake.lock"

artifact="$OUT_DIR/arctic-comfyui-helper-nix-x86_64.tar.gz"
tar -C "$STAGING_DIR" -czf "$artifact" flake.nix flake.lock package.nix

echo "Nix release flake created:"
echo "  $native_artifact"
echo "  $artifact"
