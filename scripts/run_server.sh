#!/usr/bin/env bash
set -e

# Запуск сервера StadMagic
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

echo "=== Запуск игрового сервера StadMagic ==="
cd "$ROOT_DIR"
cargo run --manifest-path lib/arena-server/Cargo.toml "$@"
