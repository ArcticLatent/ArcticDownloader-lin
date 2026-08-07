#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
target="x86_64-pc-windows-msvc"

for command_name in rustup cargo cargo-xwin; do
  if ! command -v "$command_name" >/dev/null 2>&1; then
    echo "Missing $command_name. Enter the Windows development shell first:" >&2
    echo "  nix develop .#windows" >&2
    exit 1
  fi
done

rustup toolchain install stable --profile minimal --no-self-update
rustup target add "$target" --toolchain stable
rustup component add clippy --toolchain stable

cd "$repo_root"
cargo +stable xwin check \
  --locked \
  --target "$target" \
  --manifest-path src-tauri/Cargo.toml \
  "$@"

cargo +stable xwin clippy \
  --locked \
  --target "$target" \
  --manifest-path src-tauri/Cargo.toml \
  --all-targets \
  "$@" \
  -- \
  -D warnings

# scripts/build-release.ps1 and verify-release.ps1 run this tool directly on
# the Windows release runner, so it needs to build for the target too.
cargo +stable xwin check \
  --locked \
  --target "$target" \
  --manifest-path tools/manifest-signer/Cargo.toml \
  "$@"

cargo +stable xwin clippy \
  --locked \
  --target "$target" \
  --manifest-path tools/manifest-signer/Cargo.toml \
  --all-targets \
  "$@" \
  -- \
  -D warnings
