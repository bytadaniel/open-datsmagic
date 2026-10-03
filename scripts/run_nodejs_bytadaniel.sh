#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
app_dir="$repo_root/lib/arena-bots/nodejs_bytadaniel"

: "${DATS_PLAYER_TOKEN:?Set DATS_PLAYER_TOKEN to your team token}"
: "${DATS_GAME_API_URL:?Set DATS_GAME_API_URL to the full /play/magcarp/player/move URL}"

export DATS_GAME_MODE="${DATS_GAME_MODE:-api}"
if [[ "$DATS_GAME_MODE" != "api" ]]; then
  echo "DATS_GAME_MODE must be api for this network player" >&2
  exit 2
fi

cd "$app_dir"
if [[ ! -x node_modules/.bin/tsx ]]; then
  npm ci
fi

exec npm run start:actioner
