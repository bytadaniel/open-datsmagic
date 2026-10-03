#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
APP_DIR="$ROOT_DIR/lib/arena-bots/java_sas4eka"

: "${DATS_PLAYER_TOKEN:?Set DATS_PLAYER_TOKEN in the environment before launching}"
: "${DATS_GAME_API_URL:?Set DATS_GAME_API_URL to the full game move endpoint}"
export DATS_GAME_MODE="${DATS_GAME_MODE:-api}"

BUILD_DIR="$(mktemp -d "${TMPDIR:-/tmp}/stadmagic-java-sas4eka.XXXXXX")"
javac -cp "$APP_DIR/lib/*" -d "$BUILD_DIR" "$APP_DIR"/src/*.java
cd "$APP_DIR"
exec java -cp "$BUILD_DIR:$APP_DIR/lib/*" Main
