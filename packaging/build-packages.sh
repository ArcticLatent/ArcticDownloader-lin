#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PACKAGING_DIR="$ROOT_DIR/packaging"
OUT_DIR="$PACKAGING_DIR/out"
ARCH_DISTROBOX="${ARCTIC_ARCH_DISTROBOX:-arctic-arch}"
DEB_DISTROBOX="${ARCTIC_DEB_DISTROBOX:-arctic-ubuntu}"
RPM_DISTROBOX="${ARCTIC_RPM_DISTROBOX:-arctic-fedora}"

usage() {
  cat <<'EOF'
Usage:
  packaging/build-packages.sh <target>

Targets:
  arch     Build Arch package (.pkg.tar.zst) with makepkg
  deb      Build Debian package (.deb) with dpkg-buildpackage
  rpm      Build Fedora/RPM package (.rpm) with rpmbuild
  flatpak  Build Flatpak bundle (.flatpak) with flatpak-builder
  all      Build all targets in order: arch/deb/rpm (distrobox), flatpak (host)

Notes:
  - Run from anywhere inside the repo.
  - Build tools must already be installed on your system.
  - `all` expects distroboxes:
      - Arch: arctic-arch (override with ARCTIC_ARCH_DISTROBOX)
      - Debian: arctic-ubuntu (override with ARCTIC_DEB_DISTROBOX)
      - Fedora: arctic-fedora (override with ARCTIC_RPM_DISTROBOX)
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

clean_arch_previous_builds() {
  echo "Cleaning previous Arch build artifacts..."
  rm -rf "$OUT_DIR/arch"
  rm -rf "$PACKAGING_DIR/arch/src" "$PACKAGING_DIR/arch/pkg"
  rm -f "$PACKAGING_DIR/arch"/arctic-comfyui-helper-*.pkg.tar.*
}

clean_deb_previous_builds() {
  echo "Cleaning previous Debian build artifacts..."
  rm -rf "$OUT_DIR/deb"
  rm -f "$ROOT_DIR"/../arctic-comfyui-helper_*_amd64.deb
  rm -f "$ROOT_DIR"/../arctic-comfyui-helper_*_amd64.changes
  rm -f "$ROOT_DIR"/../arctic-comfyui-helper_*_amd64.buildinfo
  rm -f "$ROOT_DIR"/../arctic-comfyui-helper-dbgsym_*_amd64.ddeb
}

clean_rpm_previous_builds() {
  echo "Cleaning previous RPM build artifacts..."
  rm -rf "$OUT_DIR/rpm"
}

clean_flatpak_previous_builds() {
  echo "Cleaning previous Flatpak build artifacts..."
  rm -rf "$OUT_DIR/flatpak"
  rm -rf "$PACKAGING_DIR/flatpak/build-dir" "$PACKAGING_DIR/flatpak/repo" "$PACKAGING_DIR/flatpak/staging"
}

build_arch() {
  require_cmd makepkg
  clean_arch_previous_builds
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
  clean_deb_previous_builds
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
  clean_rpm_previous_builds
  local version
  version="$(read_pkgver)"
  local rpmtop="$OUT_DIR/rpm/rpmbuild"
  local source_tar="arctic-comfyui-helper-${version}.tar.gz"

  mkdir -p "$rpmtop"/{BUILD,BUILDROOT,RPMS,SOURCES,SPECS,SRPMS}

  (
    cd "$ROOT_DIR"
    tar \
      --exclude-vcs \
      --exclude='./.flatpak-builder' \
      --exclude='./.env' \
      --exclude='./build-dir' \
      --exclude='./repo' \
      --exclude='./dist' \
      --exclude='./packaging/arch/src' \
      --exclude='./packaging/arch/pkg' \
      --exclude='./packaging/arch/*.pkg.tar.*' \
      --exclude='./target' \
      --exclude='./src-tauri/target' \
      --exclude='./packaging/out' \
      --transform "s,^\.,arctic-comfyui-helper-${version}," \
      -czf "$rpmtop/SOURCES/$source_tar" \
      .
  )

  cp -f "$PACKAGING_DIR/fedora/arctic-comfyui-helper.spec" "$rpmtop/SPECS/"

  local jobs
  jobs="$(getconf _NPROCESSORS_ONLN 2>/dev/null || nproc 2>/dev/null || echo 1)"

  rpmbuild \
    --define "_topdir $rpmtop" \
    --define "_smp_mflags -j$jobs" \
    -bb "$rpmtop/SPECS/arctic-comfyui-helper.spec"

  mkdir -p "$OUT_DIR/rpm"
  find "$rpmtop/RPMS" -type f -name '*.rpm' -print0 |
    while IFS= read -r -d '' f; do
      cp -f "$f" "$OUT_DIR/rpm/"
    done

  echo "RPM artifacts: $OUT_DIR/rpm"
}

build_flatpak() {
  require_cmd appstreamcli
  require_cmd flatpak-builder
  require_cmd flatpak
  require_cmd cargo
  clean_flatpak_previous_builds

  local version
  version="$(read_pkgver)"
  local flatpak_dir="$PACKAGING_DIR/flatpak"
  local staging_dir="$flatpak_dir/staging"
  local build_dir="$flatpak_dir/build-dir"
  local repo_dir="$flatpak_dir/repo"
  local manifest="$flatpak_dir/io.github.ArcticHelper.yml"
  local bundle_name="arctic-comfyui-helper-${version}-x86_64.flatpak"

  mkdir -p "$OUT_DIR/flatpak" "$staging_dir" "$repo_dir"

  if ! flatpak remotes --user --columns=name 2>/dev/null | grep -qx 'flathub'; then
    flatpak remote-add --user --if-not-exists flathub https://flathub.org/repo/flathub.flatpakrepo
  fi

  (
    cd "$ROOT_DIR"
    cargo build --release --manifest-path src-tauri/Cargo.toml
  )

  find_shared_library() {
    local library_name="$1"
    local candidate
    local library_dir

    candidate="$(ldconfig -p 2>/dev/null | awk -v name="$library_name" '$1 == name { path = $NF } END { if (path) print path }')"
    if [[ -n "$candidate" && -f "$candidate" ]]; then
      printf '%s\n' "$candidate"
      return 0
    fi

    # NixOS does not populate an FHS ldconfig cache. Development shells expose
    # their library closures as -L entries in NIX_LDFLAGS instead.
    while IFS= read -r library_dir; do
      [[ -n "$library_dir" ]] || continue
      candidate="$library_dir/$library_name"
      if [[ -f "$candidate" ]]; then
        printf '%s\n' "$candidate"
        return 0
      fi
    done < <(
      {
        printf '%s\n' "${NIX_LDFLAGS:-}" | tr ' ' '\n' | sed -n 's/^-L//p'
        printf '%s\n' "${LD_LIBRARY_PATH:-}" | tr ':' '\n'
      } | awk 'NF && !seen[$0]++'
    )

    return 1
  }

  for lib in \
    libayatana-appindicator3.so.1 \
    libayatana-indicator3.so.7 \
    libayatana-ido3-0.4.so.0 \
    libdbusmenu-gtk3.so.4 \
    libdbusmenu-glib.so.4; do
    lib_path="$(find_shared_library "$lib" || true)"
    if [[ -z "$lib_path" || ! -f "$lib_path" ]]; then
      echo "Required Flatpak tray library not found on host: $lib" >&2
      exit 1
    fi
    install -Dm755 "$lib_path" "$staging_dir/lib/$lib"
  done

  install -Dm755 "$ROOT_DIR/src-tauri/target/release/Arctic-ComfyUI-Helper" \
    "$staging_dir/Arctic-ComfyUI-Helper"
  install -Dm644 "$PACKAGING_DIR/linux/io.github.ArcticHelper.desktop" \
    "$staging_dir/io.github.ArcticHelper.desktop"
  install -Dm644 "$ROOT_DIR/src-tauri/dist/icon.svg" \
    "$staging_dir/io.github.ArcticHelper.svg"
  install -Dm644 "$flatpak_dir/io.github.ArcticHelper.metainfo.xml" \
    "$staging_dir/io.github.ArcticHelper.metainfo.xml"
  install -Dm644 "$ROOT_DIR/LICENSE" \
    "$staging_dir/LICENSE"

  flatpak-builder \
    --force-clean \
    --default-branch=stable \
    --user \
    --install-deps-from=flathub \
    --repo="$repo_dir" \
    "$build_dir" \
    "$manifest"

  flatpak build-bundle \
    "$repo_dir" \
    "$OUT_DIR/flatpak/$bundle_name" \
    io.github.ArcticHelper \
    stable \
    --runtime-repo=https://flathub.org/repo/flathub.flatpakrepo

  echo "Flatpak artifacts: $OUT_DIR/flatpak"
}

build_arch_in_distrobox() {
  require_cmd distrobox
  echo "Building Arch package in distrobox '$ARCH_DISTROBOX' ..."
  distrobox enter "$ARCH_DISTROBOX" -- bash -lc "
    set -euo pipefail
    export PATH=\"\$HOME/.cargo/bin:\$PATH\"
    cd '$ROOT_DIR'
    bash packaging/build-packages.sh arch
  "
}

build_deb_in_distrobox() {
  require_cmd distrobox
  echo "Building Debian package in distrobox '$DEB_DISTROBOX' ..."
  distrobox enter "$DEB_DISTROBOX" -- bash -lc "
    set -euo pipefail
    export PATH=\"\$HOME/.cargo/bin:\$PATH\"
    cd '$ROOT_DIR'
    bash packaging/build-packages.sh deb
  "
}

build_rpm_in_distrobox() {
  require_cmd distrobox
  echo "Building RPM package in distrobox '$RPM_DISTROBOX' ..."
  distrobox enter "$RPM_DISTROBOX" -- bash -lc "
    set -euo pipefail
    export PATH=\"\$HOME/.cargo/bin:\$PATH\"
    cd '$ROOT_DIR'
    bash packaging/build-packages.sh rpm
  "
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
    flatpak)
      build_flatpak
      ;;
    all)
      build_arch_in_distrobox
      build_deb_in_distrobox
      build_rpm_in_distrobox
      build_flatpak
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
