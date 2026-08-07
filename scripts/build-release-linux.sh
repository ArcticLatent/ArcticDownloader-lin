#!/usr/bin/env bash
set -euo pipefail

VERSION=""
REPOSITORY="ArcticLatent/Arctic-Helper"
TAG=""
OUTPUT_DIR="dist"
NOTES_FILE=""
SKIP_CLEAN=0
PUBLISH_GITHUB=0
ARCH_ONLY=0
ASSEMBLE_ONLY=0
ARCH_DISTROBOX="${ARCTIC_ARCH_DISTROBOX:-arctic-arch}"
DEB_DISTROBOX="${ARCTIC_DEB_DISTROBOX:-arctic-ubuntu}"
RPM_DISTROBOX="${ARCTIC_RPM_DISTROBOX:-arctic-fedora}"
DEB_SUDO_PASSWORD="${ARCTIC_DEB_SUDO_PASSWORD:-}"
RPM_SUDO_PASSWORD="${ARCTIC_RPM_SUDO_PASSWORD:-}"
ARCH_SUDO_PASSWORD="${ARCTIC_ARCH_SUDO_PASSWORD:-}"
ARCH_BASE_DIR=""
MANIFEST_TMP=""

cleanup() {
  if [[ -n "$ARCH_BASE_DIR" && -d "$ARCH_BASE_DIR" ]]; then
    rm -rf "$ARCH_BASE_DIR"
  fi
  if [[ -n "$MANIFEST_TMP" ]]; then
    rm -f "$MANIFEST_TMP"
  fi
}
trap cleanup EXIT

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
                         Default: CHANGELOG_<version>.md in the repo root when present.
  --skip-clean           Skip cargo clean.
  --assemble-only        Reuse package artifacts already present in packaging/out.
                         Rebuild Nix assets, checksums, and the release manifest.
  --publish-github       Create/update the GitHub release and upload built assets.
  --arch-only            Build only the native Arch package and update its GitHub release asset.
  --arch-distrobox <name>
                         Arch package build container (default: arctic-arch).
  --deb-distrobox <name> Distrobox name for Debian package build (default: arctic-ubuntu).
  --rpm-distrobox <name> Distrobox name for RPM package build (default: arctic-fedora).
  Environment variables for non-interactive sudo:
    ARCTIC_ARCH_SUDO_PASSWORD
    ARCTIC_DEB_SUDO_PASSWORD
    ARCTIC_RPM_SUDO_PASSWORD
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

extract_summary_note() {
  local file="$1"
  local line trimmed
  while IFS= read -r line || [[ -n "$line" ]]; do
    trimmed="$(printf '%s' "$line" | sed -E 's/^[[:space:]]+//; s/[[:space:]]+$//')"
    [[ -z "$trimmed" ]] && continue
    case "$trimmed" in
      \#*) continue ;;
      -\ *|\*\ *)
        printf '%s\n' "${trimmed#??}"
        return 0
        ;;
      *)
        printf '%s\n' "$trimmed"
        return 0
        ;;
    esac
  done < "$file"
  return 0
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
    --assemble-only)
      ASSEMBLE_ONLY=1
      shift
      ;;
    --publish-github)
      PUBLISH_GITHUB=1
      shift
      ;;
    --arch-only)
      ARCH_ONLY=1
      PUBLISH_GITHUB=1
      shift
      ;;
    --arch-distrobox)
      ARCH_DISTROBOX="${2:-}"
      shift 2
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

  printf -v SUPABASE_URL_Q '%q' "$ARCTIC_SUPABASE_URL"
  printf -v SUPABASE_ANON_KEY_Q '%q' "${ARCTIC_SUPABASE_ANON_KEY:-}"
  printf -v SUPABASE_PUBLISHABLE_KEY_Q '%q' "${ARCTIC_SUPABASE_PUBLISHABLE_KEY:-}"
  echo "Public Supabase catalog environment loaded for Linux release builds."
}

load_public_catalog_env
if [[ -z "${ARCTIC_UPDATE_SIGNING_KEY:-}" ]]; then
  echo "ARCTIC_UPDATE_SIGNING_KEY is not set in the environment." >&2
  echo "A release build is refused before packaging because the app will not trust an unsigned manifest." >&2
  exit 1
fi
# Ensure rustup cargo/rustc are visible even when invoked from fish or clean shells.
export PATH="$HOME/.cargo/bin:$PATH"

if [[ -z "$NOTES_FILE" ]]; then
  default_notes_file="$ROOT_DIR/CHANGELOG_$VERSION.md"
  if [[ -f "$default_notes_file" ]]; then
    NOTES_FILE="$default_notes_file"
    echo "Using changelog notes file: $NOTES_FILE"
  fi
fi

if [[ -n "$NOTES_FILE" && ! -f "$NOTES_FILE" ]]; then
  echo "Notes file not found: $NOTES_FILE" >&2
  exit 1
fi

require_cmd sha256sum
require_cmd bash
if ((ASSEMBLE_ONLY == 0)); then
  require_cmd cargo
  require_cmd distrobox
  if ((ARCH_ONLY == 0)); then
    require_cmd flatpak
    require_cmd flatpak-builder
  fi
fi
if ((PUBLISH_GITHUB == 1)); then
  require_cmd gh
fi
# An Arch-only rebuild must preserve the other platform assets already in the
# signed Linux manifest. Save the existing metadata outside dist/ before the
# clean build removes it; the signer verifies it before merging below.
if ((ARCH_ONLY == 1)) && gh release view "$TAG" --repo "$REPOSITORY" >/dev/null 2>&1; then
  ARCH_BASE_DIR="$(mktemp -d)"
  if gh release download "$TAG" --repo "$REPOSITORY" --pattern 'linux-release.json' --dir "$ARCH_BASE_DIR" >/dev/null 2>&1; then
    gh release download "$TAG" --repo "$REPOSITORY" --pattern 'SHA256SUMS' --dir "$ARCH_BASE_DIR" >/dev/null 2>&1 || true
    echo "Existing Linux release metadata saved for the Arch-only merge."
  else
    echo "No existing Linux release manifest found; creating an Arch-only manifest."
  fi
fi

update_simple_version() {
  local file="$1"
  local pattern="$2"
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

build_deb_with_podman() {
  require_cmd podman
  echo "Distrobox Debian build failed; retrying in a temporary Ubuntu Podman container ..."
  podman run --rm \
    --name "arctic-deb-build-$VERSION" \
    --volume "$ROOT_DIR:/work:rw" \
    --workdir /work \
    --env HOME=/root \
    --env ARCTIC_SUPABASE_URL \
    --env ARCTIC_SUPABASE_ANON_KEY \
    --env ARCTIC_SUPABASE_PUBLISHABLE_KEY \
    docker.io/library/ubuntu:24.04 \
    bash -lc '
      set -euo pipefail
      export DEBIAN_FRONTEND=noninteractive
      apt-get update
      apt-get install -y \
        build-essential devscripts pkg-config \
        debhelper-compat cargo rustc \
        libssl-dev \
        libgtk-3-dev libwebkit2gtk-4.1-dev \
        libayatana-appindicator3-dev \
        ca-certificates curl

      if ! command -v rustup >/dev/null 2>&1; then
        curl https://sh.rustup.rs -sSf | sh -s -- -y --profile minimal --default-toolchain stable
      fi
      export PATH="$HOME/.cargo/bin:$PATH"
      rustup toolchain install stable --profile minimal >/dev/null
      rustup default stable >/dev/null

      bash packaging/build-packages.sh deb
    '
}

build_rpm_with_podman() {
  require_cmd podman
  echo "Distrobox RPM build failed; retrying in a temporary Fedora Podman container ..."
  podman run --rm \
    --name "arctic-rpm-build-$VERSION" \
    --volume "$ROOT_DIR:/work:rw" \
    --workdir /work \
    --env HOME=/root \
    --env ARCTIC_SUPABASE_URL \
    --env ARCTIC_SUPABASE_ANON_KEY \
    --env ARCTIC_SUPABASE_PUBLISHABLE_KEY \
    registry.fedoraproject.org/fedora:latest \
    bash -lc '
      set -euo pipefail
      dnf install -y \
        rpm-build rpmdevtools \
        rust cargo openssl-devel \
        gcc gcc-c++ make pkgconf-pkg-config \
        gtk3-devel webkit2gtk4.1-devel \
        libayatana-appindicator-gtk3-devel \
        ca-certificates curl

      if ! command -v rustup >/dev/null 2>&1; then
        curl https://sh.rustup.rs -sSf | sh -s -- -y --profile minimal --default-toolchain stable
      fi
      export PATH="$HOME/.cargo/bin:$PATH"
      rustup toolchain install stable --profile minimal >/dev/null
      rustup default stable >/dev/null

      bash packaging/build-packages.sh rpm
    '
}

summary_note="Release v$VERSION"
if [[ -n "$NOTES_FILE" ]]; then
  first_line="$(extract_summary_note "$NOTES_FILE" || true)"
  if [[ -n "$first_line" ]]; then
    summary_note="$first_line"
  fi
fi

if ((ASSEMBLE_ONLY == 0)); then
echo "Updating versions to $VERSION ..."
update_simple_version "$ROOT_DIR/Cargo.toml" '0,/^version\s*=\s*"[^"]+"/{s//version = "'"$VERSION"'"/}'
update_simple_version "$ROOT_DIR/src-tauri/Cargo.toml" '0,/^version\s*=\s*"[^"]+"/{s//version = "'"$VERSION"'"/}'
update_simple_version "$ROOT_DIR/src-tauri/tauri.conf.json" '0,/"version"\s*:\s*"[^"]+"/{s//"version": "'"$VERSION"'"/}'
update_simple_version "$PACKAGING_DIR/nix/source-package.nix" 's/^  version = "[^"]+";/  version = "'"$VERSION"'";/'
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

echo "Building Arch package in distrobox '$ARCH_DISTROBOX' ..."
distrobox enter "$ARCH_DISTROBOX" -- bash -lc "
  set -euo pipefail
  export ARCTIC_SUPABASE_URL=$SUPABASE_URL_Q
  export ARCTIC_SUPABASE_ANON_KEY=$SUPABASE_ANON_KEY_Q
  export ARCTIC_SUPABASE_PUBLISHABLE_KEY=$SUPABASE_PUBLISHABLE_KEY_Q
  SUDO_PASSWORD='${ARCH_SUDO_PASSWORD//\'/\'\"\'\"\'}'
  as_root() {
    if [[ \"\$(id -u)\" -eq 0 ]]; then
      \"\$@\"
    elif command -v sudo >/dev/null 2>&1; then
      if [[ -n \"\$SUDO_PASSWORD\" ]]; then
        printf '%s\n' \"\$SUDO_PASSWORD\" | sudo -S -p '' \"\$@\"
      else
        sudo \"\$@\"
      fi
    else
      echo \"Need root privileges to install Arch build dependencies (missing sudo).\" >&2
      exit 1
    fi
  }

  as_root pacman -Syu --noconfirm --needed \
    base-devel rust pkgconf openssl \
    gtk3 webkit2gtk-4.1 xdg-desktop-portal-gtk

  as_root pacman -S --noconfirm --needed libayatana-appindicator

  export PATH=\"\$HOME/.cargo/bin:\$PATH\"
  cd '$ROOT_DIR'
  bash packaging/build-packages.sh arch
"

if ((ARCH_ONLY == 0)); then
echo "Building Debian package in distrobox '$DEB_DISTROBOX' ..."
if ! distrobox enter "$DEB_DISTROBOX" -- bash -lc "
  set -euo pipefail
  export ARCTIC_SUPABASE_URL=$SUPABASE_URL_Q
  export ARCTIC_SUPABASE_ANON_KEY=$SUPABASE_ANON_KEY_Q
  export ARCTIC_SUPABASE_PUBLISHABLE_KEY=$SUPABASE_PUBLISHABLE_KEY_Q
  SUDO_PASSWORD='${DEB_SUDO_PASSWORD//\'/\'\"\'\"\'}'
  as_root() {
    if [[ \"\$(id -u)\" -eq 0 ]]; then
      \"\$@\"
    elif command -v sudo >/dev/null 2>&1; then
      if [[ -n \"\$SUDO_PASSWORD\" ]]; then
        printf '%s\n' \"\$SUDO_PASSWORD\" | sudo -S -p '' \"\$@\"
      else
        sudo \"\$@\"
      fi
    else
      echo \"Need root privileges to install Debian build dependencies (missing sudo).\" >&2
      exit 1
    fi
  }

  ensure_deb_build_tools() {
    local missing=0
    for cmd in dpkg-buildpackage cargo rustc; do
      command -v \"\$cmd\" >/dev/null 2>&1 || missing=1
    done
    if [[ \"\$missing\" -eq 1 ]]; then
      as_root apt update
      as_root apt install -y \
        build-essential devscripts pkg-config \
        debhelper-compat cargo rustc \
        libssl-dev \
        libgtk-3-dev libwebkit2gtk-4.1-dev \
        libayatana-appindicator3-dev
    fi
  }

  ensure_modern_rust() {
    local min_cargo=\"1.89.0\"
    if ! command -v rustup >/dev/null 2>&1; then
      if ! command -v curl >/dev/null 2>&1; then
        as_root apt update
        as_root apt install -y curl
      fi
      curl https://sh.rustup.rs -sSf | sh -s -- -y --profile minimal --default-toolchain stable
    fi

    export PATH=\"\$HOME/.cargo/bin:\$PATH\"
    rustup toolchain install stable --profile minimal >/dev/null
    rustup default stable >/dev/null
    hash -r

    local cargo_version
    cargo_version=\"\$(cargo --version | awk '{print \$2}')\"
    if [[ \"\$(printf '%s\n' \"\$cargo_version\" \"\$min_cargo\" | sort -V | head -n1)\" != \"\$min_cargo\" ]]; then
      echo \"Cargo \$cargo_version is too old (need >= \$min_cargo).\" >&2
      exit 1
    fi
  }

  ensure_deb_build_tools
  ensure_modern_rust
  export PATH=\"\$HOME/.cargo/bin:\$PATH\"
  as_root apt purge -y arctic-comfyui-helper || true
  as_root apt autoremove -y || true
  cd '$ROOT_DIR'
  bash packaging/build-packages.sh deb
"; then
  build_deb_with_podman
fi

echo "Building RPM package in distrobox '$RPM_DISTROBOX' ..."
if ! distrobox enter "$RPM_DISTROBOX" -- bash -lc "
  set -euo pipefail
  export ARCTIC_SUPABASE_URL=$SUPABASE_URL_Q
  export ARCTIC_SUPABASE_ANON_KEY=$SUPABASE_ANON_KEY_Q
  export ARCTIC_SUPABASE_PUBLISHABLE_KEY=$SUPABASE_PUBLISHABLE_KEY_Q
  SUDO_PASSWORD='${RPM_SUDO_PASSWORD//\'/\'\"\'\"\'}'
  as_root() {
    if [[ \"\$(id -u)\" -eq 0 ]]; then
      \"\$@\"
    elif command -v sudo >/dev/null 2>&1; then
      if [[ -n \"\$SUDO_PASSWORD\" ]]; then
        printf '%s\n' \"\$SUDO_PASSWORD\" | sudo -S -p '' \"\$@\"
      else
        sudo \"\$@\"
      fi
    else
      echo \"Need root privileges to install RPM build dependencies (missing sudo).\" >&2
      exit 1
    fi
  }

  ensure_rpm_build_tools() {
    local missing=0
    for cmd in rpmbuild cargo rustc; do
      command -v \"\$cmd\" >/dev/null 2>&1 || missing=1
    done
    if [[ \"\$missing\" -eq 1 ]] \
      || ! rpm -q openssl-devel >/dev/null 2>&1 \
      || ! rpm -q gtk3-devel >/dev/null 2>&1 \
      || ! rpm -q webkit2gtk4.1-devel >/dev/null 2>&1 \
      || ! rpm -q libayatana-appindicator-gtk3-devel >/dev/null 2>&1; then
      as_root dnf install -y \
        rpm-build rpmdevtools \
        rust cargo openssl-devel \
        gcc gcc-c++ make pkgconf-pkg-config \
        gtk3-devel webkit2gtk4.1-devel \
        libayatana-appindicator-gtk3-devel
    fi
  }

  ensure_modern_rust() {
    local min_cargo=\"1.89.0\"
    if ! command -v rustup >/dev/null 2>&1; then
      if ! command -v curl >/dev/null 2>&1; then
        as_root dnf install -y curl
      fi
      curl https://sh.rustup.rs -sSf | sh -s -- -y --profile minimal --default-toolchain stable
    fi

    export PATH=\"\$HOME/.cargo/bin:\$PATH\"
    rustup toolchain install stable --profile minimal >/dev/null
    rustup default stable >/dev/null
    hash -r

    local cargo_version
    cargo_version=\"\$(cargo --version | awk '{print \$2}')\"
    if [[ \"\$(printf '%s\n' \"\$cargo_version\" \"\$min_cargo\" | sort -V | head -n1)\" != \"\$min_cargo\" ]]; then
      echo \"Cargo \$cargo_version is too old (need >= \$min_cargo).\" >&2
      exit 1
    fi
  }

  ensure_rpm_build_tools
  ensure_modern_rust
  export PATH=\"\$HOME/.cargo/bin:\$PATH\"
  as_root dnf remove -y arctic-comfyui-helper || true
  cd '$ROOT_DIR'
  bash packaging/build-packages.sh rpm
"; then
  build_rpm_with_podman
fi

echo "Building Flatpak bundle on host ..."
(cd "$ROOT_DIR" && bash packaging/build-packages.sh flatpak)
fi
else
  echo "Reusing package artifacts from $PACKAGING_DIR/out ..."
  mkdir -p "$OUT_ABS_DIR"
  find "$OUT_ABS_DIR" -maxdepth 1 -type f \( \
    -name '*.pkg.tar.*' -o \
    -name '*.deb' -o \
    -name '*.rpm' -o \
    -name '*.src.rpm' -o \
    -name '*.flatpak' -o \
    -name 'arctic-comfyui-helper-nix-*.tar.gz' -o \
    -name 'arctic-comfyui-helper-*-nixos-*.tar.gz' \
  \) -delete
fi

if ((ARCH_ONLY == 1)); then
  mapfile -t artifacts < <(find "$PACKAGING_DIR/out/arch" -type f -name '*.pkg.tar.*' | sort)
else
  mapfile -t artifacts < <(find "$PACKAGING_DIR/out" -type f \( -name '*.pkg.tar.*' -o -name '*.deb' -o -name '*.rpm' -o -name '*.src.rpm' -o -name '*.flatpak' \) | sort)
fi
if ((${#artifacts[@]} == 0)); then
  echo "No package artifacts were produced." >&2
  exit 1
fi

for f in "${artifacts[@]}"; do
  cp -f "$f" "$OUT_ABS_DIR/"
done

if ((ARCH_ONLY == 0)); then
  echo "Creating NixOS binary flake ..."
  (cd "$ROOT_DIR" && bash scripts/build-nix-release.sh \
    --version "$VERSION" \
    --repository "$REPOSITORY" \
    --tag "$TAG" \
    --output-dir "$OUTPUT_DIR")
fi

if [[ -n "$NOTES_FILE" ]]; then
  cp -f "$NOTES_FILE" "$OUT_ABS_DIR/release-notes-$TAG.md"
fi

(
  cd "$OUT_ABS_DIR"
  rm -f SHA256SUMS
  mapfile -t copied < <(find . -maxdepth 1 -type f \( -name '*.pkg.tar.*' -o -name '*.deb' -o -name '*.rpm' -o -name '*.src.rpm' -o -name '*.flatpak' -o -name 'arctic-comfyui-helper-nix-*.tar.gz' -o -name 'arctic-comfyui-helper-*-nixos-*.tar.gz' \) -printf '%f\n' | sort)
  sha256sum "${copied[@]}" > SHA256SUMS
)

if ((ARCH_ONLY == 1)) && [[ -n "$ARCH_BASE_DIR" && -f "$ARCH_BASE_DIR/SHA256SUMS" ]]; then
  current_sums="$(mktemp)"
  merged_sums="$(mktemp)"
  cp "$OUT_ABS_DIR/SHA256SUMS" "$current_sums"
  awk '$2 !~ /\.pkg\.tar/' "$ARCH_BASE_DIR/SHA256SUMS" > "$merged_sums"
  cat "$current_sums" >> "$merged_sums"
  sort -k2,2 -u "$merged_sums" > "$OUT_ABS_DIR/SHA256SUMS"
  rm -f "$current_sums" "$merged_sums"
fi

manifest="$OUT_ABS_DIR/linux-release.json"
MANIFEST_TMP="$(mktemp "$OUT_ABS_DIR/.linux-release.json.XXXXXX")"
{
  echo "{"
  echo "  \"version\": \"$VERSION\"," 
  echo "  \"tag\": \"$TAG\"," 
  echo "  \"repository\": \"$REPOSITORY\"," 
  echo "  \"assets\": ["
  mapfile -t copied < <(find "$OUT_ABS_DIR" -maxdepth 1 -type f \( -name '*.pkg.tar.*' -o -name '*.deb' -o -name '*.rpm' -o -name '*.src.rpm' -o -name '*.flatpak' -o -name 'arctic-comfyui-helper-nix-*.tar.gz' -o -name 'arctic-comfyui-helper-*-nixos-*.tar.gz' \) -printf '%f\n' | sort)
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
} > "$MANIFEST_TMP"

if ((ARCH_ONLY == 1)) && [[ -n "$ARCH_BASE_DIR" && -f "$ARCH_BASE_DIR/linux-release.json" ]]; then
  echo "Merging the rebuilt Arch asset into the existing Linux release manifest ..."
  (cd "$ROOT_DIR" && cargo run --quiet --release --manifest-path tools/manifest-signer/Cargo.toml -- \
    merge-linux-release \
    --base "$ARCH_BASE_DIR/linux-release.json" \
    --replacement "$MANIFEST_TMP" \
    --output "$MANIFEST_TMP")
fi

echo "Signing release manifest ..."
(cd "$ROOT_DIR" && cargo run --quiet --release --manifest-path tools/manifest-signer/Cargo.toml -- \
  sign --format linux-release --manifest "$MANIFEST_TMP")
(cd "$ROOT_DIR" && cargo run --quiet --release --manifest-path tools/manifest-signer/Cargo.toml -- \
  verify --format linux-release --manifest "$MANIFEST_TMP")
mv "$MANIFEST_TMP" "$manifest"
MANIFEST_TMP=""

echo "Build release artifacts complete:"
echo "  Output: $OUT_ABS_DIR"
echo "  Manifest: $manifest"
echo "  Checksums: $OUT_ABS_DIR/SHA256SUMS"

  if ((PUBLISH_GITHUB == 1)); then
  echo "Publishing GitHub release '$TAG' to '$REPOSITORY' ..."
  if ((ARCH_ONLY == 1)); then
    # Replacing the Arch package changes its checksum, so publish the newly
    # signed manifest and checksum list with it. Leaving the old metadata on
    # the release would make the updater reject the replacement package.
    mapfile -t release_files < <(find "$OUT_ABS_DIR" -maxdepth 1 -type f \( -name '*.pkg.tar.*' -o -name 'SHA256SUMS' -o -name 'linux-release.json' \) | sort)
  else
    mapfile -t release_files < <(find "$OUT_ABS_DIR" -maxdepth 1 -type f \( -name '*.pkg.tar.*' -o -name '*.deb' -o -name '*.rpm' -o -name '*.src.rpm' -o -name '*.flatpak' -o -name 'arctic-comfyui-helper-nix-*.tar.gz' -o -name 'arctic-comfyui-helper-*-nixos-*.tar.gz' -o -name 'SHA256SUMS' -o -name 'linux-release.json' \) | sort)
  fi
  if gh release view "$TAG" --repo "$REPOSITORY" >/dev/null 2>&1; then
    gh release upload "$TAG" "${release_files[@]}" --repo "$REPOSITORY" --clobber
    if [[ -n "$NOTES_FILE" ]]; then
      gh release edit "$TAG" --repo "$REPOSITORY" --notes-file "$NOTES_FILE"
    fi
  else
    release_title="Arctic ComfyUI Helper $VERSION"
    if [[ -n "$NOTES_FILE" ]]; then
      gh release create "$TAG" "${release_files[@]}" --repo "$REPOSITORY" --title "$release_title" --notes-file "$NOTES_FILE"
    else
      gh release create "$TAG" "${release_files[@]}" --repo "$REPOSITORY" --title "$release_title" --notes "$summary_note"
    fi
  fi
  echo "GitHub release publish complete:"
  echo "  https://github.com/$REPOSITORY/releases/tag/$TAG"
fi
