#!/usr/bin/env python3
"""Minimal StadMagic bot using only Python's standard library."""

import json
import getpass
import math
import os
import sys
import time
import urllib.error
import urllib.request


API_URL = os.environ.get(
    "DATS_GAME_API_URL",
    "https://stadmagic.strangled.net/play/magcarp/player/move",
)
TOKEN = os.environ.get("DATS_PLAYER_TOKEN", "").strip()


def request(commands):
    body = json.dumps({"transports": commands}).encode()
    req = urllib.request.Request(
        API_URL,
        data=body,
        method="POST",
        headers={
            "X-Auth-Token": TOKEN,
            "Content-Type": "application/json",
            "Accept": "application/json",
        },
    )
    with urllib.request.urlopen(req, timeout=10) as response:
        return json.loads(response.read())


def acceleration(carpet, target, maximum):
    dx, dy = target["x"] - carpet["x"], target["y"] - carpet["y"]
    distance = math.hypot(dx, dy)
    if distance < 1e-9 or maximum <= 0:
        return {"x": 0, "y": 0}
    # A simple demonstrator: point full acceleration toward the nearest coin.
    return {"x": maximum * dx / distance, "y": maximum * dy / distance}


def main():
    global TOKEN
    if not TOKEN:
        TOKEN = getpass.getpass("Team token (input is hidden): ").strip()
    if not TOKEN:
        print("A registered team token is required.", file=sys.stderr)
        return 2

    print("Connecting to StadMagic (token is never printed). Ctrl-C to stop.")
    last_report = 0.0
    try:
        state = request([])  # Empty command list obtains this team's first snapshot.
        while True:
            coins = state.get("bounties", [])
            carpets = [c for c in state.get("transports", []) if c.get("status") == "alive"]
            maximum = float(state.get("maxAccel", 0))
            commands = []
            for carpet in carpets:
                if coins:
                    coin = min(coins, key=lambda b: (b["x"] - carpet["x"]) ** 2 + (b["y"] - carpet["y"]) ** 2)
                    aim = acceleration(carpet, coin, maximum)
                else:
                    # Keep a non-zero control vector when the arena currently has no coins.
                    aim = {"x": maximum, "y": 0}
                commands.append({"id": carpet["id"], "acceleration": aim})
            state = request(commands)

            now = time.monotonic()
            if now - last_report >= 5:
                print(
                    f"tick={state.get('tick', '?')}  team={state.get('name', '?')}  "
                    f"gold={state.get('points', '?')}  carpets={len(carpets)}  coins={len(coins)}",
                    flush=True,
                )
                last_report = now
            time.sleep(0.21)  # 4.76 cycles/s: below the documented 5 req/s token limit.
    except KeyboardInterrupt:
        print("\nStopped.")
        return 0
    except urllib.error.HTTPError as exc:
        detail = exc.read().decode("utf-8", "replace")[:500]
        if exc.code == 401:
            print("401 Unauthorized: check that the team token is registered and correct.", file=sys.stderr)
        elif exc.code == 429:
            print("429 Too Many Requests: reduce request frequency below 5 per second.", file=sys.stderr)
        else:
            print(f"HTTP {exc.code}: {detail}", file=sys.stderr)
        return 1
    except (urllib.error.URLError, TimeoutError, ValueError, KeyError) as exc:
        print(f"Connection/API error: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
