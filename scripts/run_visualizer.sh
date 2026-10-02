#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

echo "=== DatsMagic Rust Visualizer ==="
exec cargo run --manifest-path "$ROOT_DIR/lib/arena-visualizer/Cargo.toml" -- "$@"
