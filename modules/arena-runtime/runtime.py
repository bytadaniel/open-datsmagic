#!/usr/bin/env python3
"""Private lifecycle API for the Rust arena process inside its Compose container."""

from __future__ import annotations

import hmac
import json
import os
import re
import signal
import subprocess
import sys
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Any
from urllib.parse import parse_qs, urlparse


ROOT = Path("/app")
DATA_DIR = Path(os.environ.get("HUB_DATA_DIR", "/app/data"))
WORLDS_PATH = Path(os.environ.get("DATS_WORLDS_PATH", ROOT / "assets/worlds.json"))
ARENA_BIN = Path(os.environ.get("DATS_ARENA_BIN", "/usr/local/bin/server"))
CONTROL_TOKEN = os.environ.get("ARENA_CONTROL_TOKEN", "")
CONTROL_HOST = os.environ.get("ARENA_CONTROL_HOST", "0.0.0.0")
CONTROL_PORT = int(os.environ.get("ARENA_CONTROL_PORT", "9001"))
GAME_HOST = os.environ.get("ARENA_GAME_HOST", "0.0.0.0")
GAME_PORT = int(os.environ.get("ARENA_GAME_PORT", "8080"))


class ArenaRuntime:
    def __init__(self) -> None:
        self.lock = threading.RLock()
        self.process: subprocess.Popen[bytes] | None = None
        self.run_id: str | None = None
        self.exit_code: int | None = None
        self.log_thread: threading.Thread | None = None
        self.watchdog: threading.Thread | None = None
        self.stop_event = threading.Event()

    def start(self, request: dict[str, Any]) -> dict[str, Any]:
        run_id = request.get("run_id")
        world_id = request.get("world_id")
        arena_name = request.get("arena_name")
        observer_token = request.get("observer_token")
        duration_sec = request.get("duration_sec")
        if not isinstance(run_id, str) or not re.fullmatch(r"[a-f0-9]{32}", run_id):
            raise ValueError("run_id must be a 32-character hex ID")
        if not isinstance(world_id, str) or not world_id:
            raise ValueError("world_id is required")
        if not isinstance(arena_name, str) or not arena_name or len(arena_name) > 160:
            raise ValueError("arena_name is required")
        if not isinstance(observer_token, str) or not observer_token or len(observer_token) > 256:
            raise ValueError("observer_token is required")
        if isinstance(duration_sec, bool) or not isinstance(duration_sec, (int, float)) or not 1 <= duration_sec <= 86_400:
            raise ValueError("duration_sec must be between 1 and 86400")

        catalog = json.loads(WORLDS_PATH.read_text(encoding="utf-8"))
        if not any(item.get("id") == world_id for item in catalog.get("worlds", [])):
            raise ValueError("world_id is not present in worlds.json")
        if not ARENA_BIN.is_file():
            raise RuntimeError(f"arena binary not found: {ARENA_BIN}")

        with self.lock:
            if self.process and self.process.poll() is None:
                if self.run_id == run_id:
                    return {"run_id": run_id, "pid": self.process.pid, "status": "running"}
                print(f"[runtime] replacing stale run={self.run_id} with run={run_id}", flush=True)
                self.stop(self.run_id or "")
            if self.process:
                self.exit_code = self.process.returncode
            report_path = DATA_DIR / "arena" / f"run_{run_id}" / "metrics.json"
            status_path = DATA_DIR / "current_world.json"
            env = os.environ.copy()
            env.update({
                "HOST": GAME_HOST,
                "PORT": str(GAME_PORT),
                "DATS_WORLD_ID": world_id,
                "DATS_WORLD_RUN_NAME": arena_name,
                "DATS_WORLDS_PATH": str(WORLDS_PATH),
                "DATS_TOKEN_REGISTRY_PATH": str(DATA_DIR / "registry.json"),
                "DATS_OBSERVER_TOKEN": observer_token,
                "DATS_LEADERBOARD_PATH": str(report_path),
                "DATS_WORLD_STATUS_PATH": str(status_path),
            })
            report_path.parent.mkdir(parents=True, exist_ok=True)
            print(f"[runtime] starting Rust arena run={run_id} world={world_id} name={arena_name}", flush=True)
            process = subprocess.Popen(
                [str(ARENA_BIN)], cwd=ROOT, env=env,
                stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
            )
            self.process = process
            self.run_id = run_id
            self.exit_code = None
            self.stop_event = threading.Event()
            self.log_thread = threading.Thread(
                target=self._forward_logs, args=(process.stdout, run_id),
                name=f"arena-log-{run_id[:8]}", daemon=True,
            )
            self.log_thread.start()
            self.watchdog = threading.Thread(
                target=self._watch_run, args=(run_id, float(duration_sec)),
                name=f"arena-watchdog-{run_id[:8]}", daemon=True,
            )
            self.watchdog.start()
            return {"run_id": run_id, "pid": process.pid, "status": "running"}

    def _watch_run(self, run_id: str, duration_sec: float) -> None:
        if self.stop_event.wait(duration_sec):
            return
        with self.lock:
            if self.run_id != run_id or self.process is None or self.process.poll() is not None:
                return
        print(f"[runtime] run duration reached for run={run_id}; stopping arena", flush=True)
        try:
            self.stop(run_id)
        except Exception as exc:
            print(f"[runtime] watchdog failed to stop arena: {exc}", file=sys.stderr, flush=True)

    @staticmethod
    def _forward_logs(stream: Any, run_id: str) -> None:
        if stream is None:
            return
        log_path = DATA_DIR / "arena" / "arena.log"
        log_path.parent.mkdir(parents=True, exist_ok=True)
        with log_path.open("ab", buffering=0) as log:
            log.write(f"\n=== Rust arena process run={run_id} ===\n".encode())
            for line in iter(stream.readline, b""):
                sys.stdout.buffer.write(b"[arena] " + line)
                sys.stdout.buffer.flush()
                log.write(line)

    def status(self, run_id: str) -> dict[str, Any]:
        with self.lock:
            if run_id != self.run_id:
                raise LookupError("unknown arena run")
            if self.process is None:
                return {"run_id": run_id, "running": False, "exit_code": self.exit_code}
            code = self.process.poll()
            if code is not None:
                self.exit_code = code
            return {
                "run_id": run_id,
                "running": code is None,
                "exit_code": code,
                "pid": self.process.pid,
            }

    def stop(self, run_id: str) -> dict[str, Any]:
        with self.lock:
            if run_id != self.run_id or self.process is None:
                raise LookupError("unknown arena run")
            process = self.process
            self.stop_event.set()
            if process.poll() is None:
                print(f"[runtime] stopping Rust arena run={run_id}", flush=True)
                process.send_signal(signal.SIGINT)
        try:
            code = process.wait(timeout=10)
        except subprocess.TimeoutExpired:
            process.kill()
            code = process.wait(timeout=3)
        with self.lock:
            self.exit_code = code
        return {"run_id": run_id, "running": False, "exit_code": code}

    def stop_current(self) -> None:
        with self.lock:
            run_id = self.run_id
            active = self.process is not None and self.process.poll() is None
        if run_id and active:
            try:
                self.stop(run_id)
            except Exception as exc:  # shutdown must continue even if the child already exited
                print(f"[runtime] error stopping Rust arena: {exc}", file=sys.stderr, flush=True)


RUNTIME = ArenaRuntime()


class Handler(BaseHTTPRequestHandler):
    server_version = "StadMagicArenaRuntime/1.0"

    def log_message(self, fmt: str, *args: Any) -> None:
        print(f"[runtime-http] {self.address_string()} {fmt % args}", flush=True)

    def _json(self, status: int, payload: dict[str, Any]) -> None:
        body = json.dumps(payload, separators=(",", ":")).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json; charset=utf-8")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def _authorized(self) -> bool:
        supplied = self.headers.get("X-Arena-Control-Token", "")
        if CONTROL_TOKEN and hmac.compare_digest(supplied, CONTROL_TOKEN):
            return True
        self._json(401, {"error": "unauthorized"})
        return False

    def do_GET(self) -> None:  # noqa: N802
        parsed = urlparse(self.path)
        if parsed.path == "/health":
            self._json(200, {"status": "ok"})
            return
        if parsed.path == "/internal/worlds":
            if not self._authorized():
                return
            env = os.environ.copy()
            env["DATS_WORLDS_PATH"] = str(WORLDS_PATH)
            try:
                result = subprocess.run(
                    [str(ARENA_BIN), "--world-catalog-json"], cwd=ROOT, env=env,
                    check=True, capture_output=True, text=True, timeout=20,
                )
                worlds = json.loads(result.stdout)
                if not isinstance(worlds, list):
                    raise RuntimeError("arena returned an invalid world catalog")
                self._json(200, {"worlds": worlds})
            except (OSError, subprocess.SubprocessError, json.JSONDecodeError, RuntimeError) as exc:
                self._json(500, {"error": f"could not load world configurations: {exc}"})
            return
        if parsed.path != "/internal/arena/status" or not self._authorized():
            if parsed.path != "/internal/arena/status":
                self._json(404, {"error": "not found"})
            return
        run_id = parse_qs(parsed.query).get("run_id", [""])[0]
        try:
            self._json(200, RUNTIME.status(run_id))
        except LookupError as exc:
            self._json(404, {"error": str(exc)})

    def do_POST(self) -> None:  # noqa: N802
        if not self._authorized():
            return
        length = int(self.headers.get("Content-Length", "0"))
        if length <= 0 or length > 16_384:
            self._json(413, {"error": "request body must be between 1 and 16384 bytes"})
            return
        try:
            body = json.loads(self.rfile.read(length))
            if not isinstance(body, dict):
                raise ValueError("request body must be a JSON object")
            if self.path == "/internal/arena/start":
                self._json(200, RUNTIME.start(body))
            elif self.path == "/internal/arena/stop":
                run_id = body.get("run_id")
                if not isinstance(run_id, str):
                    raise ValueError("run_id is required")
                self._json(200, RUNTIME.stop(run_id))
            else:
                self._json(404, {"error": "not found"})
        except LookupError as exc:
            self._json(404, {"error": str(exc)})
        except RuntimeError as exc:
            self._json(409, {"error": str(exc)})
        except (ValueError, OSError, json.JSONDecodeError) as exc:
            self._json(400, {"error": str(exc)})


def main() -> None:
    if not CONTROL_TOKEN:
        raise RuntimeError("ARENA_CONTROL_TOKEN must be configured")
    server = ThreadingHTTPServer((CONTROL_HOST, CONTROL_PORT), Handler)
    def shutdown(*_: Any) -> None:
        RUNTIME.stop_current()
        threading.Thread(target=server.shutdown, daemon=True).start()

    signal.signal(signal.SIGTERM, shutdown)
    print(f"StadMagic arena runtime listening on {CONTROL_HOST}:{CONTROL_PORT}; Rust game API port={GAME_PORT}", flush=True)
    try:
        server.serve_forever()
    finally:
        RUNTIME.stop_current()
        server.server_close()


if __name__ == "__main__":
    main()
