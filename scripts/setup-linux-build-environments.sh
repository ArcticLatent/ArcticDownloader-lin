#!/usr/bin/env bash
set -euo pipefail

ARCH_DISTROBOX="${ARCTIC_ARCH_DISTROBOX:-arctic-arch}"
DEB_DISTROBOX="${ARCTIC_DEB_DISTROBOX:-arctic-ubuntu}"
ARCH_IMAGE="${ARCTIC_ARCH_IMAGE:-docker.io/library/archlinux:latest}"
DEB_IMAGE="${ARCTIC_DEB_IMAGE:-docker.io/library/ubuntu:24.04}"
NIX_IMAGE="${ARCTIC_NIX_IMAGE:-docker.io/nixos/nix:2.34.0}"
SETUP_HOST=1
SETUP_CONTAINERS=1

usage() {
  cat <<'EOF'
Usage:
  scripts/setup-linux-build-environments.sh [options]

Options:
  --host-only        Install only native Fedora build dependencies.
  --containers-only Create/provision only the Arch and Ubuntu distroboxes.
  -h, --help         Show help.

Environment overrides:
  ARCTIC_ARCH_DISTROBOX  Arch distrobox name (default: arctic-arch)
  ARCTIC_DEB_DISTROBOX   Ubuntu distrobox name (default: arctic-ubuntu)
  ARCTIC_ARCH_IMAGE      Arch image (default: archlinux:latest)
  ARCTIC_DEB_IMAGE       Ubuntu image (default: ubuntu:24.04)
  ARCTIC_NIX_IMAGE       Nix image to pre-pull (default: nixos/nix:2.34.0)
EOF
}

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "Required command not found: $1" >&2
    exit 1
  }
}

while (($# > 0)); do
  case "$1" in
    --host-only)
      SETUP_HOST=1
      SETUP_CONTAINERS=0
      shift
      ;;
    --containers-only)
      SETUP_HOST=0
      SETUP_CONTAINERS=1
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

install_stable_rust() {
  export PATH="$HOME/.cargo/bin:$PATH"
  rustup toolchain install stable --profile minimal
  rustup default stable
}

setup_fedora_host() {
  if [[ ! -f /etc/fedora-release ]]; then
    echo "Native setup requires a Fedora host." >&2
    exit 1
  fi

  require_cmd sudo
  echo "Installing native Fedora RPM, Flatpak, Rust, Node, and validation tools ..."
  sudo dnf install -y \
    rpm-build rpmdevtools rust cargo rustup openssl-devel \
    gcc gcc-c++ make pkgconf-pkg-config \
    gtk3-devel webkit2gtk4.1-devel libayatana-appindicator-gtk3-devel dbus-devel \
    xdg-desktop-portal-gtk \
    appstream desktop-file-utils flatpak flatpak-builder \
    podman distrobox nodejs npm git gh copr-cli curl clang lld llvm patchelf

  install_stable_rust
}

create_distrobox() {
  local name="$1"
  local image="$2"

  if podman container exists "$name"; then
    echo "Distrobox '$name' already exists; keeping it."
  else
    echo "Creating distrobox '$name' from '$image' ..."
    distrobox create --name "$name" --image "$image" --yes
  fi
}

setup_containers() {
  require_cmd podman
  require_cmd distrobox

  create_distrobox "$ARCH_DISTROBOX" "$ARCH_IMAGE"
  create_distrobox "$DEB_DISTROBOX" "$DEB_IMAGE"

  echo "Provisioning Arch build dependencies in '$ARCH_DISTROBOX' ..."
  distrobox enter "$ARCH_DISTROBOX" -- bash -lc '
    set -euo pipefail
    sudo pacman -Syu --noconfirm --needed \
      base-devel rustup pkgconf openssl \
      gtk3 webkit2gtk-4.1 libayatana-appindicator \
      xdg-desktop-portal-gtk dbus ca-certificates curl
    export PATH="$HOME/.cargo/bin:$PATH"
    rustup toolchain install stable --profile minimal
    rustup default stable
  '

  echo "Provisioning Debian build dependencies in '$DEB_DISTROBOX' ..."
  distrobox enter "$DEB_DISTROBOX" -- bash -lc '
    set -euo pipefail
    export DEBIAN_FRONTEND=noninteractive
    sudo apt-get update
    sudo apt-get install -y \
      build-essential devscripts pkg-config debhelper-compat cargo rustc \
      libssl-dev libgtk-3-dev libwebkit2gtk-4.1-dev \
      libayatana-appindicator3-dev libdbus-1-dev \
      ca-certificates curl
    export PATH="$HOME/.cargo/bin:$PATH"
    if ! command -v rustup >/dev/null 2>&1; then
      curl https://sh.rustup.rs -sSf | sh -s -- -y --profile minimal --default-toolchain stable
    fi
    rustup toolchain install stable --profile minimal
    rustup default stable
  '

  echo "Pre-pulling the pinned Nix build image '$NIX_IMAGE' ..."
  podman pull "$NIX_IMAGE"
}

if ((SETUP_HOST == 1)); then
  setup_fedora_host
fi
if ((SETUP_CONTAINERS == 1)); then
  setup_containers
fi

echo "Linux build environment setup complete."
