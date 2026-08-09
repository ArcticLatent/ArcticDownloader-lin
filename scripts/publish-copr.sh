#!/usr/bin/env bash
set -euo pipefail

COPR_PROJECT="${ARCTIC_COPR_PROJECT:-burcebor/arctic-helper}"
SRPM_PATH=""
ENABLE_NET="on"
BACKGROUND=0
NOWAIT=0
ASSUME_YES=0
CHROOTS=()

usage() {
  cat <<'EOF'
Usage:
  scripts/publish-copr.sh [options]

Builds an SRPM from the current source tree and submits it to Fedora COPR.

Options:
  --project <owner/name> COPR project (default: burcebor/arctic-helper).
  --srpm <path>          Submit an existing SRPM instead of building one.
  --chroot <name>       Limit the build to a chroot; repeat for multiple chroots.
  --enable-net <on|off> Allow network access in the COPR build (default: on).
                        Cargo requires this unless dependencies are vendored.
  --background          Submit with lower scheduler priority.
  --nowait              Return after COPR accepts the build.
  --yes                 Skip the publication confirmation.
  -h, --help            Show help.

Environment:
  ARCTIC_COPR_PROJECT   Default COPR project override.
EOF
}

fail() {
  echo "COPR publish stopped: $*" >&2
  exit 1
}

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || fail "required command not found: $1"
}

while (($# > 0)); do
  case "$1" in
    --project)
      COPR_PROJECT="${2:-}"
      shift 2
      ;;
    --srpm)
      SRPM_PATH="${2:-}"
      shift 2
      ;;
    --chroot)
      CHROOTS+=("${2:-}")
      shift 2
      ;;
    --enable-net)
      ENABLE_NET="${2:-}"
      shift 2
      ;;
    --background)
      BACKGROUND=1
      shift
      ;;
    --nowait)
      NOWAIT=1
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

[[ "$COPR_PROJECT" =~ ^[^/[:space:]]+/[^/[:space:]]+$ ]] \
  || fail "--project must use owner/name format"
[[ "$ENABLE_NET" == "on" || "$ENABLE_NET" == "off" ]] \
  || fail "--enable-net must be 'on' or 'off'"
for chroot in "${CHROOTS[@]}"; do
  [[ -n "$chroot" ]] || fail "--chroot requires a value"
done

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RPM_OUT_DIR="$ROOT_DIR/packaging/out/rpm"

require_cmd bash
require_cmd copr
require_cmd rpm
if [[ -z "$SRPM_PATH" ]]; then
  require_cmd rpmbuild
fi

copr_user="$(copr whoami)" || fail "COPR authentication failed; refresh ~/.config/copr from https://copr.fedorainfracloud.org/api/"
copr get "$COPR_PROJECT" >/dev/null \
  || fail "COPR project is unavailable to '$copr_user': $COPR_PROJECT"

if [[ -z "$SRPM_PATH" ]]; then
  echo "Building source RPM for COPR ..."
  (cd "$ROOT_DIR" && bash packaging/build-packages.sh srpm)
  mapfile -t srpms < <(find "$RPM_OUT_DIR" -maxdepth 1 -type f -name '*.src.rpm' | sort)
  ((${#srpms[@]} == 1)) \
    || fail "expected exactly one SRPM in $RPM_OUT_DIR, found ${#srpms[@]}"
  SRPM_PATH="${srpms[0]}"
elif [[ "$SRPM_PATH" != /* ]]; then
  SRPM_PATH="$PWD/$SRPM_PATH"
fi

[[ -s "$SRPM_PATH" ]] || fail "SRPM not found or empty: $SRPM_PATH"

package_name="$(rpm -qp --queryformat '%{NAME}' "$SRPM_PATH")"
package_version="$(rpm -qp --queryformat '%{VERSION}-%{RELEASE}' "$SRPM_PATH")"
source_rpm="$(rpm -qp --queryformat '%{SOURCERPM}' "$SRPM_PATH")"
[[ "$SRPM_PATH" == *.src.rpm && "$source_rpm" == "(none)" ]] \
  || fail "not a source RPM: $SRPM_PATH"

echo "COPR account: $copr_user"
echo "COPR project: $COPR_PROJECT"
echo "Source RPM:   $SRPM_PATH"
echo "Package:      $package_name-$package_version"
if ((${#CHROOTS[@]} == 0)); then
  echo "Chroots:      all enabled project chroots"
else
  printf 'Chroots:     '
  printf ' %s' "${CHROOTS[@]}"
  printf '\n'
fi
echo "Build network: $ENABLE_NET"

if ((ASSUME_YES == 0)); then
  [[ -t 0 ]] || fail "publication confirmation requires a terminal; pass --yes for non-interactive use"
  read -r -p "Type 'publish $package_name-$package_version' to submit to COPR: " confirmation
  [[ "$confirmation" == "publish $package_name-$package_version" ]] \
    || fail "confirmation did not match"
fi

copr_args=(build --enable-net "$ENABLE_NET")
for chroot in "${CHROOTS[@]}"; do
  copr_args+=(--chroot "$chroot")
done
((BACKGROUND == 1)) && copr_args+=(--background)
((NOWAIT == 1)) && copr_args+=(--nowait)
copr_args+=("$COPR_PROJECT" "$SRPM_PATH")

copr "${copr_args[@]}"

echo
echo "COPR submission complete. Fedora users can install with:"
echo "  sudo dnf copr enable $COPR_PROJECT"
echo "  sudo dnf install $package_name"
echo "Project: https://copr.fedorainfracloud.org/coprs/$COPR_PROJECT/"
