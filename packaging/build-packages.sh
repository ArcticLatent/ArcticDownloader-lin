#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PACKAGING_DIR="$ROOT_DIR/packaging"
OUT_DIR="$PACKAGING_DIR/out"

usage() {
  cat <<'EOF'
Usage:
  packaging/build-packages.sh <target>

Targets:
  arch     Build Arch package (.pkg.tar.zst) with makepkg
  deb      Build Debian package (.deb) with dpkg-buildpackage
  rpm      Build Fedora/RPM package (.rpm) with rpmbuild
  all      Build all targets in order: arch, deb, rpm

Notes:
  - Run from anywhere inside the repo.
  - Build tools must already be installed on your system.
EOF
}

require_cmd() {
  local cmd="$1"
  if ! command -v "$cmd" >/dev/null 2>&1; then
    echo "Missing required command: $cmd" >&2
    exit 1
  fi
}

read_pkgver() {
  awk -F= '/^pkgver=/{gsub(/'\''|"/, "", $2); print $2; exit}' "$PACKAGING_DIR/arch/PKGBUILD"
}

build_arch() {
  require_cmd makepkg
  mkdir -p "$OUT_DIR/arch"
  (
    cd "$PACKAGING_DIR/arch"
    makepkg -f
    shopt -s nullglob
    local pkgs=(arctic-comfyui-helper-*.pkg.tar.*)
    if ((${#pkgs[@]} == 0)); then
      echo "Arch build succeeded but no package artifact was found." >&2
      exit 1
    fi
    cp -f "${pkgs[@]}" "$OUT_DIR/arch/"
  )
  echo "Arch artifacts: $OUT_DIR/arch"
}

build_deb() {
  require_cmd dpkg-buildpackage
  mkdir -p "$OUT_DIR/deb"
  (
    cd "$ROOT_DIR"
    rm -rf debian
    cp -a packaging/debian/debian ./debian
    dpkg-buildpackage -us -uc -b
    rm -rf debian
    shopt -s nullglob
    local debs=(../arctic-comfyui-helper_*_amd64.deb)
    local changes=(../arctic-comfyui-helper_*_amd64.changes ../arctic-comfyui-helper_*_amd64.buildinfo)
    if ((${#debs[@]} == 0)); then
      echo "Debian build succeeded but no .deb artifact was found." >&2
      exit 1
    fi
    cp -f "${debs[@]}" "$OUT_DIR/deb/"
    if ((${#changes[@]} > 0)); then
      cp -f "${changes[@]}" "$OUT_DIR/deb/" || true
    fi
  )
  echo "Debian artifacts: $OUT_DIR/deb"
}

build_rpm() {
  require_cmd rpmbuild
  local version
  version="$(read_pkgver)"
  local rpmtop="$OUT_DIR/rpm/rpmbuild"
  local source_tar="arctic-comfyui-helper-${version}.tar.gz"

  mkdir -p "$rpmtop"/{BUILD,BUILDROOT,RPMS,SOURCES,SPECS,SRPMS}

  (
    cd "$ROOT_DIR"
    tar \
      --exclude-vcs \
      --exclude='./target' \
      --exclude='./src-tauri/target' \
      --exclude='./packaging/out' \
      --transform "s,^\.,arctic-comfyui-helper-${version}," \
      -czf "$rpmtop/SOURCES/$source_tar" \
      .
  )

  cp -f "$PACKAGING_DIR/fedora/arctic-comfyui-helper.spec" "$rpmtop/SPECS/"

  rpmbuild \
    --define "_topdir $rpmtop" \
    -ba "$rpmtop/SPECS/arctic-comfyui-helper.spec"

  mkdir -p "$OUT_DIR/rpm"
  find "$rpmtop/RPMS" "$rpmtop/SRPMS" -type f \( -name '*.rpm' -o -name '*.src.rpm' \) -print0 |
    while IFS= read -r -d '' f; do
      cp -f "$f" "$OUT_DIR/rpm/"
    done

  echo "RPM artifacts: $OUT_DIR/rpm"
}

main() {
  if (($# != 1)); then
    usage
    exit 1
  fi

  case "$1" in
    arch)
      build_arch
      ;;
    deb)
      build_deb
      ;;
    rpm)
      build_rpm
      ;;
    all)
      build_arch
      build_deb
      build_rpm
      ;;
    -h|--help|help)
      usage
      ;;
    *)
      echo "Unknown target: $1" >&2
      usage
      exit 1
      ;;
  esac
}

main "$@"
