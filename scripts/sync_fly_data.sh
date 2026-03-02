#!/usr/bin/env bash
set -euo pipefail

APP="${FLY_APP:-bp-web}"
REMOTE_DIR="${FLY_REMOTE_DATA_DIR:-/data}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
LOCAL_DIR="${FLY_LOCAL_DATA_DIR:-${REPO_ROOT}/.fly-data}"

usage() {
  cat <<'EOF'
Usage: sync_fly_data.sh [--app APP] [--remote-dir DIR] [--local-dir DIR]

Downloads the Fly volume SQLite files used by bp-web into a local directory.
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --app)
      APP="$2"
      shift 2
      ;;
    --remote-dir)
      REMOTE_DIR="$2"
      shift 2
      ;;
    --local-dir)
      LOCAL_DIR="$2"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown argument: $1" >&2
      usage >&2
      exit 1
      ;;
  esac
done

mkdir -p "${LOCAL_DIR}"

echo "Syncing Fly data from app '${APP}' (${REMOTE_DIR}) to ${LOCAL_DIR}"
if [[ -e "${LOCAL_DIR}/cache.sqlite" ]]; then
  unlink "${LOCAL_DIR}/cache.sqlite"
fi
fly ssh sftp get -a "${APP}" "${REMOTE_DIR}/cache.sqlite" "${LOCAL_DIR}/cache.sqlite"

if [[ -e "${LOCAL_DIR}/bp.sqlite" ]]; then
  unlink "${LOCAL_DIR}/bp.sqlite"
fi
if ! fly ssh sftp get -a "${APP}" "${REMOTE_DIR}/bp.sqlite" "${LOCAL_DIR}/bp.sqlite"; then
  echo "Warning: ${REMOTE_DIR}/bp.sqlite not copied" >&2
fi

if [[ -e "${LOCAL_DIR}/config.toml" ]]; then
  unlink "${LOCAL_DIR}/config.toml"
fi
if ! fly ssh sftp get -a "${APP}" "${REMOTE_DIR}/config.toml" "${LOCAL_DIR}/config.toml"; then
  echo "Warning: ${REMOTE_DIR}/config.toml not copied" >&2
fi

echo "Sync complete."
