#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

echo "=== StadMagic Rust Visualizer ==="

: "${DATS_PLAYER_TOKEN:?Set DATS_PLAYER_TOKEN in the environment before launching}"
STADMAGIC_ARENA_URL="${STADMAGIC_ARENA_URL:-http://stadmagic.strangled.net/play/magcarp/player/move}"
STADMAGIC_HUB_URL="${STADMAGIC_HUB_URL:-http://stadmagic.strangled.net}"

exec cargo run --release --manifest-path "$ROOT_DIR/lib/arena-visualizer/Cargo.toml" -- \
  --url "$STADMAGIC_ARENA_URL" \
  --hub-url "$STADMAGIC_HUB_URL" \
  "$@"
