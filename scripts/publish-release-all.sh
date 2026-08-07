#!/usr/bin/env bash
set -euo pipefail

VERSION=""
SOURCE_REPOSITORY="ArcticLatent/ArcticDownloader-lin"
RELEASE_REPOSITORY="ArcticLatent/Arctic-Helper"
SOURCE_BRANCH="main"
WINDOWS_WORKFLOW="release-windows.yml"
OUTPUT_DIR="dist"
NOTES_FILE=""
SKIP_WINDOWS_REHEARSAL=0
ASSUME_YES=0
RESUME=0
TEMP_DIR=""
VERIFY_DIR=""

usage() {
  cat <<'USAGE'
Usage:
  scripts/publish-release-all.sh --version <x.y.z> [options]

Builds, verifies, and publishes a complete Linux and Windows release. The
script builds and verifies Windows first, publishes Linux and Arch package
assets, creates the source tag, uploads the verified Windows files with the
local GitHub login, and verifies the downloaded public release.

Options:
  --version <x.y.z>      Required release version.
  --notes-file <path>    Release notes (default: CHANGELOG_<version>.md).
  --output-dir <path>    Linux artifact directory (default: dist).
  --skip-windows-rehearsal
                         Defer the Windows build until after Linux publication.
  --resume               Continue a partially published release/tag.
  --yes                  Skip the single publication confirmation.
  -h, --help             Show help.

The local ARCTIC_UPDATE_SIGNING_KEY is required for Linux. If it is not
already exported, the script prompts for it without echoing or saving it.
USAGE
}

fail() {
  echo "Release stopped: $*" >&2
  exit 1
}

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || fail "required command not found: $1"
}

cleanup() {
  if [[ -n "$TEMP_DIR" && -d "$TEMP_DIR" ]]; then
    rm -rf "$TEMP_DIR"
  fi
  if [[ -n "$VERIFY_DIR" && -d "$VERIFY_DIR" ]]; then
    rm -rf "$VERIFY_DIR"
  fi
}
trap cleanup EXIT

while (($# > 0)); do
  case "$1" in
    --version)
      VERSION="${2:-}"
      shift 2
      ;;
    --notes-file)
      NOTES_FILE="${2:-}"
      shift 2
      ;;
    --output-dir)
      OUTPUT_DIR="${2:-}"
      shift 2
      ;;
    --skip-windows-rehearsal)
      SKIP_WINDOWS_REHEARSAL=1
      shift
      ;;
    --resume)
      RESUME=1
      shift
      ;;
    --yes)
      ASSUME_YES=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      fail "unknown argument: $1"
      ;;
  esac
done

[[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || fail "--version must be semantic version x.y.z"

[[ -n "$OUTPUT_DIR" && "$OUTPUT_DIR" != /* ]] || fail "--output-dir must be a non-empty path relative to the repository"
IFS='/' read -r -a output_components <<<"$OUTPUT_DIR"
for output_component in "${output_components[@]}"; do
  [[ -n "$output_component" && "$output_component" != "." && "$output_component" != ".." ]] \
    || fail "--output-dir contains an unsafe path component: $OUTPUT_DIR"
done

TAG="v$VERSION"
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

if [[ -z "$NOTES_FILE" ]]; then
  NOTES_FILE="$ROOT_DIR/CHANGELOG_$VERSION.md"
elif [[ "$NOTES_FILE" != /* ]]; then
  NOTES_FILE="$ROOT_DIR/$NOTES_FILE"
fi
[[ -s "$NOTES_FILE" ]] || fail "release notes file is missing or empty: $NOTES_FILE"

for command_name in bash gh git nix; do
  require_cmd "$command_name"
done

current_branch="$(git branch --show-current)"
[[ "$current_branch" == "$SOURCE_BRANCH" ]] || fail "run from branch '$SOURCE_BRANCH' (current: '$current_branch')"

worktree_status="$(git status --porcelain --untracked-files=all)"
if [[ -n "$worktree_status" ]]; then
  printf '%s\n' "$worktree_status" >&2
  fail "the source worktree must be clean"
fi

echo "Checking source branch and GitHub access ..."
git fetch origin --tags --prune --prune-tags
local_head="$(git rev-parse HEAD)"
remote_head="$(git rev-parse "origin/$SOURCE_BRANCH")"
[[ "$local_head" == "$remote_head" ]] || fail "local $SOURCE_BRANCH is not synchronized with origin/$SOURCE_BRANCH"
gh auth status >/dev/null
can_push_release="$(gh api "repos/$RELEASE_REPOSITORY" --jq '.permissions.push')"
[[ "$can_push_release" == "true" ]] || fail "the active GitHub account cannot publish to $RELEASE_REPOSITORY"

required_secrets=(
  ARCTIC_SUPABASE_PUBLISHABLE_KEY
  ARCTIC_SUPABASE_URL
  ARCTIC_UPDATE_SIGNING_KEY
)
secret_names="$(gh secret list --repo "$SOURCE_REPOSITORY" --json name --jq '.[].name')"
for secret_name in "${required_secrets[@]}"; do
  grep -Fxq "$secret_name" <<<"$secret_names" || fail "missing GitHub Actions secret: $secret_name"
done

read_cargo_version() {
  awk -F '"' '/^version[[:space:]]*=/ {print $2; exit}' "$1"
}

[[ "$(read_cargo_version Cargo.toml)" == "$VERSION" ]] || fail "Cargo.toml is not version $VERSION"
[[ "$(read_cargo_version src-tauri/Cargo.toml)" == "$VERSION" ]] || fail "src-tauri/Cargo.toml is not version $VERSION"
nix develop -c python3 - "$VERSION" <<'PY'
import json
import sys

with open("src-tauri/tauri.conf.json", encoding="utf-8") as handle:
    actual = json.load(handle)["version"]
if actual != sys.argv[1]:
    raise SystemExit(f"src-tauri/tauri.conf.json is version {actual}, expected {sys.argv[1]}")
PY
grep -Eq "^  version = \"$VERSION\";" packaging/nix/source-package.nix || fail "Nix package is not version $VERSION"
grep -Fxq "pkgver=$VERSION" packaging/arch/PKGBUILD || fail "Arch package is not version $VERSION"
grep -Eq "^Version:[[:space:]]+$VERSION$" packaging/fedora/arctic-comfyui-helper.spec || fail "RPM package is not version $VERSION"
expected_debian_header="arctic-comfyui-helper ($VERSION-1) unstable; urgency=medium"
[[ "$(head -n 1 packaging/debian/debian/changelog)" == "$expected_debian_header" ]] \
  || fail "Debian changelog does not start with version $VERSION"

if gh release view "$TAG" --repo "$RELEASE_REPOSITORY" >/dev/null 2>&1; then
  ((RESUME == 1)) || fail "$RELEASE_REPOSITORY already has release $TAG; use --resume only after inspecting it"
fi

local_tag_exists=0
if git rev-parse --verify --quiet "refs/tags/$TAG" >/dev/null; then
  local_tag_exists=1
  tag_commit="$(git rev-list -n 1 "$TAG")"
  [[ "$tag_commit" == "$local_head" ]] || fail "tag $TAG exists at $tag_commit, not current HEAD $local_head"
  ((RESUME == 1)) || fail "source tag $TAG already exists; use --resume to retry its release"
fi
remote_tag_exists=0
if git ls-remote --exit-code --tags origin "refs/tags/$TAG" >/dev/null 2>&1; then
  remote_tag_exists=1
fi

if [[ -z "${ARCTIC_UPDATE_SIGNING_KEY:-}" ]]; then
  [[ -t 0 ]] || fail "ARCTIC_UPDATE_SIGNING_KEY is not exported and no terminal is available for a secure prompt"
  printf 'Paste ARCTIC_UPDATE_SIGNING_KEY (input hidden): ' >&2
  IFS= read -r -s ARCTIC_UPDATE_SIGNING_KEY
  printf '\n' >&2
  export ARCTIC_UPDATE_SIGNING_KEY
fi
[[ -n "$ARCTIC_UPDATE_SIGNING_KEY" ]] || fail "ARCTIC_UPDATE_SIGNING_KEY is empty"

if ((ASSUME_YES == 0)); then
  echo
  echo "This will publish $TAG to $RELEASE_REPOSITORY"
  echo "from $SOURCE_REPOSITORY@$local_head."
  read -r -p "Type 'publish $TAG' to continue: " confirmation
  [[ "$confirmation" == "publish $TAG" ]] || fail "confirmation did not match"
fi

capture_windows_runs() {
  gh run list \
    --repo "$SOURCE_REPOSITORY" \
    --workflow "$WINDOWS_WORKFLOW" \
    --limit 100 \
    --json databaseId \
    --jq '.[].databaseId' > "$1"
}

wait_for_new_windows_run() {
  local known_file="$1"
  local expected_sha="$2"
  local expected_event="$3"
  local run_id run_sha

  local attempt
  for ((attempt = 1; attempt <= 60; attempt++)); do
    while IFS=$'\t' read -r run_id run_sha; do
      if [[ "$run_sha" == "$expected_sha" ]] && ! grep -Fxq "$run_id" "$known_file"; then
        printf '%s\n' "$run_id"
        return 0
      fi
    done < <(
      gh run list \
        --repo "$SOURCE_REPOSITORY" \
        --workflow "$WINDOWS_WORKFLOW" \
        --event "$expected_event" \
        --limit 20 \
        --json databaseId,headSha \
        --jq '.[] | [.databaseId, .headSha] | @tsv'
    )
    sleep 2
  done
  fail "timed out waiting for the new Windows release workflow"
}

verify_windows_files() {
  local directory="$1"
  local asset="$directory/Arctic-ComfyUI-Helper.exe"
  local manifest="$directory/update.json"
  [[ -s "$asset" ]] || fail "Windows artifact is missing: $asset"
  [[ -s "$manifest" ]] || fail "Windows manifest is missing: $manifest"

  nix develop -c cargo run --quiet --release \
    --manifest-path tools/manifest-signer/Cargo.toml -- \
    verify --format update --manifest "$manifest"

  nix develop -c python3 - "$manifest" "$asset" "$VERSION" "$RELEASE_REPOSITORY" "$TAG" <<'PY'
import hashlib
import json
import pathlib
import sys

manifest_path, asset_path, version, repository, tag = sys.argv[1:]
with open(manifest_path, encoding="utf-8-sig") as handle:
    manifest = json.load(handle)
asset = pathlib.Path(asset_path)
actual_hash = hashlib.sha256(asset.read_bytes()).hexdigest()
expected_url = f"https://github.com/{repository}/releases/download/{tag}/{asset.name}"
if manifest.get("version") != version:
    raise SystemExit("Windows manifest version mismatch")
if manifest.get("download_url") != expected_url:
    raise SystemExit("Windows manifest download URL mismatch")
if manifest.get("sha256", "").lower() != actual_hash:
    raise SystemExit("Windows artifact SHA-256 mismatch")
PY
}

build_windows_artifacts() {
  local stage="$1"
  local known_runs="$TEMP_DIR/windows-runs-before-$stage"

  echo "Starting Windows $stage build ..."
  capture_windows_runs "$known_runs"
  gh workflow run "$WINDOWS_WORKFLOW" \
    --repo "$SOURCE_REPOSITORY" \
    --ref "$SOURCE_BRANCH" \
    -f "version=$VERSION"
  windows_run_id="$(wait_for_new_windows_run "$known_runs" "$local_head" workflow_dispatch)"
  echo "Waiting for Windows build run $windows_run_id ..."
  gh run watch "$windows_run_id" --repo "$SOURCE_REPOSITORY" --exit-status

  windows_dir="$TEMP_DIR/windows-build"
  mkdir -p "$windows_dir"
  gh run download "$windows_run_id" \
    --repo "$SOURCE_REPOSITORY" \
    --name "arctic-comfyui-helper-windows-$VERSION" \
    --dir "$windows_dir"
  verify_windows_files "$windows_dir"
  echo "Windows artifacts verified."
}

TEMP_DIR="$(mktemp -d)"
windows_dir=""
windows_run_id=""

if ((SKIP_WINDOWS_REHEARSAL == 0)); then
  build_windows_artifacts "pre-publish"
fi

echo "Building, verifying, and publishing Linux $TAG ..."
linux_args=(
  --version "$VERSION"
  --repository "$RELEASE_REPOSITORY"
  --output-dir "$OUTPUT_DIR"
  --notes-file "$NOTES_FILE"
)
nix develop -c bash scripts/release-linux.sh "${linux_args[@]}"

if ((remote_tag_exists == 0)); then
  if ((local_tag_exists == 0)); then
    git tag -a "$TAG" -m "Release $TAG"
  else
    tag_commit="$(git rev-list -n 1 "$TAG")"
    [[ "$tag_commit" == "$local_head" ]] \
      || fail "local tag $TAG no longer points to the release commit $local_head"
  fi
  git push origin "refs/tags/$TAG"
else
  echo "Tag $TAG already exists; continuing the idempotent publication."
fi

if ((SKIP_WINDOWS_REHEARSAL == 1)); then
  build_windows_artifacts "post-publish"
fi

[[ -n "$windows_dir" ]] || fail "Windows artifacts were not built"
echo "Publishing verified Windows artifacts with the local GitHub login ..."
gh release upload "$TAG" \
  "$windows_dir/Arctic-ComfyUI-Helper.exe" \
  "$windows_dir/update.json" \
  --repo "$RELEASE_REPOSITORY" \
  --clobber

echo "Downloading and verifying the completed public release ..."
VERIFY_DIR="$(mktemp -d "$ROOT_DIR/.release-verify.XXXXXX")"
verify_output_dir="${VERIFY_DIR#"$ROOT_DIR/"}"
gh release download "$TAG" --repo "$RELEASE_REPOSITORY" --dir "$VERIFY_DIR"

nix develop -c bash scripts/verify-release-linux.sh \
  --version "$VERSION" \
  --tag "$TAG" \
  --repository "$RELEASE_REPOSITORY" \
  --output-dir "$verify_output_dir"
verify_windows_files "$VERIFY_DIR"

gh release edit "$TAG" \
  --repo "$RELEASE_REPOSITORY" \
  --title "Arctic ComfyUI Helper $VERSION" \
  --notes-file "$NOTES_FILE"

release_url="$(gh release view "$TAG" --repo "$RELEASE_REPOSITORY" --json url --jq '.url')"
echo
echo "Complete release verified:"
echo "  Source:  https://github.com/$SOURCE_REPOSITORY/tree/$TAG"
echo "  Public:  $release_url"
echo "  Windows workflow: https://github.com/$SOURCE_REPOSITORY/actions/runs/$windows_run_id"
