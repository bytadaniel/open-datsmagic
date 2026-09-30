#!/usr/bin/env bash
set -e

# Запуск сервера DatsMagic
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

echo "=== Запуск игрового сервера DatsMagic ==="
cd "$ROOT_DIR"
cargo run --manifest-path server/Cargo.toml "$@"
