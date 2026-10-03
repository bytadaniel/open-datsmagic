#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
APP_DIR="$ROOT_DIR/lib/arena-bots/rust_bytadaniel"

: "${DATS_PLAYER_TOKEN:?Set DATS_PLAYER_TOKEN in the environment before launching}"
export DATS_SERVER_URL="${DATS_SERVER_URL:-http://127.0.0.1:8080}"

cd "$APP_DIR"
exec cargo run --release
