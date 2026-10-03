#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

cargo build --release --manifest-path "$ROOT_DIR/lib/arena-server/Cargo.toml"
exec python3 "$ROOT_DIR/modules/arena-hub/hub.py"
