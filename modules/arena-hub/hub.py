#!/usr/bin/env python3
"""StadMagic control plane: arena lifecycle, private team registry and aggregated results."""

from __future__ import annotations

import asyncio
import datetime as dt
import gzip
import html
import json
import math
import os
import random
import re
import secrets
import signal
import sqlite3
import subprocess
import sys
import threading
import time
import uuid
import urllib.error
import urllib.request
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Any
from urllib.parse import parse_qs, urlparse


ROOT = Path(__file__).resolve().parents[2]
HUB_DIR = Path(__file__).resolve().parent
DOCS_DIR = ROOT / "docs" / "components" / "arena-hub"
DATA_DIR = Path(os.environ.get("HUB_DATA_DIR", HUB_DIR / "data")).resolve()
ARENA_DATA_DIR = DATA_DIR / "arena"
REGISTRY_PATH = DATA_DIR / "registry.json"
DB_PATH = DATA_DIR / "hub.sqlite3"
WORLDS_PATH = Path(os.environ.get("DATS_WORLDS_PATH", ROOT / "assets/worlds.json"))
ARENA_BIN = Path(os.environ.get("DATS_ARENA_BIN", ROOT / "lib/arena-server/target/release/server"))
HUB_HOST = os.environ.get("HUB_HOST", "127.0.0.1")
HUB_PORT = int(os.environ.get("HUB_PORT", "8090"))
ARENA_HOST = os.environ.get("ARENA_HOST", "127.0.0.1")
ARENA_PORT = int(os.environ.get("ARENA_PORT", "8080"))
ARENA_PUBLIC_HOST = os.environ.get("ARENA_PUBLIC_HOST", "127.0.0.1")
ARENA_PUBLIC_URL = os.environ.get("ARENA_PUBLIC_URL", "").rstrip("/")
RUN_SECONDS = int(os.environ.get("HUB_RUN_SECONDS", "1200"))
POLL_SECONDS = float(os.environ.get("HUB_POLL_SECONDS", "1"))
FIXED_WORLD_ID = os.environ.get("HUB_FIXED_WORLD_ID")
ARENA_LIFECYCLE_MODE = os.environ.get("ARENA_LIFECYCLE_MODE", "process")
ARENA_CONTROL_URL = os.environ.get("ARENA_CONTROL_URL", "http://127.0.0.1:9001").rstrip("/")
ARENA_CONTROL_TOKEN = os.environ.get("ARENA_CONTROL_TOKEN", "")
METRICS_FIELDS = ("gold", "gold_collected", "carpets_lost", "distance_travelled")


def utc_now() -> str:
    return dt.datetime.now(dt.timezone.utc).isoformat(timespec="seconds")


def token_id(token: str) -> str:
    value = 0xCBF29CE484222325
    for byte in token.encode("utf-8"):
        value = ((value ^ byte) * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return f"{value:016x}"


def next_minute_boundary(timestamp: float) -> float:
    return math.ceil(timestamp / 60.0) * 60.0


def load_world_catalog(path: Path = WORLDS_PATH) -> list[dict[str, Any]]:
    data = json.loads(path.read_text(encoding="utf-8"))
    worlds = data.get("worlds")
    if not isinstance(worlds, list) or not worlds:
        raise RuntimeError(f"{path} must contain at least one world")
    ids = [world.get("id") for world in worlds if isinstance(world, dict)]
    if len(ids) != len(worlds) or any(not isinstance(value, str) or not value.strip() for value in ids):
        raise RuntimeError("worlds must have non-empty string IDs")
    if len(set(ids)) != len(worlds):
        raise RuntimeError("world IDs must be unique")
    return worlds


def load_world_configs(world_catalog: list[dict[str, Any]]) -> list[dict[str, Any]]:
    if ARENA_LIFECYCLE_MODE == "runtime-api":
        worlds = arena_runtime_request("GET", "/internal/worlds").get("worlds")
    else:
        env = os.environ.copy()
        env["DATS_WORLDS_PATH"] = str(WORLDS_PATH)
        result = subprocess.run(
            [str(ARENA_BIN), "--world-catalog-json"], cwd=ROOT, env=env,
            check=True, capture_output=True, text=True, timeout=20,
        )
        worlds = json.loads(result.stdout)
    if not isinstance(worlds, list) or len(worlds) != len(world_catalog):
        raise RuntimeError("server returned an invalid world configuration catalog")
    if [world.get("id") for world in worlds] != [world.get("id") for world in world_catalog]:
        raise RuntimeError("server world configurations do not match worlds.json")
    return worlds


class TeamRegistry:
    """Private token/name file. Public responses only contain the stable token fingerprint."""

    def __init__(self, path: Path):
        self.path = path
        self.lock = threading.RLock()
        self.path.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
        if os.name != "nt":
            self.path.parent.chmod(0o700)
            if self.path.exists():
                self.path.chmod(0o600)
        self.teams: dict[str, dict[str, str]] = {}
        try:
            raw = json.loads(path.read_text(encoding="utf-8"))
            for record in raw.get("teams", []):
                token = str(record.get("token", ""))
                name = str(record.get("name", ""))
                if token and name:
                    self.teams[token_id(token)] = {"token": token, "name": name}
        except FileNotFoundError:
            self._save_locked()
        except (OSError, json.JSONDecodeError, AttributeError, TypeError):
            raise RuntimeError(f"private team registry is unreadable: {path}")

    def _save_locked(self) -> None:
        records = [
            {"team_id": key, "token": value["token"], "name": value["name"]}
            for key, value in sorted(self.teams.items())
        ]
        temporary = self.path.with_suffix(self.path.suffix + ".tmp")
        temporary.write_text(json.dumps({"version": 1, "teams": records}, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
        if os.name != "nt":
            temporary.chmod(0o600)
        os.replace(temporary, self.path)

    def register(self, token: str, name: str) -> dict[str, str]:
        token = token.strip()
        name = name.strip()
        if not 1 <= len(token) <= 128 or any(ord(ch) < 32 for ch in token):
            raise ValueError("token must be 1–128 printable characters")
        if not 1 <= len(name) <= 48 or any(ord(ch) < 32 for ch in name):
            raise ValueError("name must be 1–48 printable characters")
        key = token_id(token)
        with self.lock:
            existing = self.teams.get(key)
            if existing and existing["token"] != token:
                raise ValueError("token fingerprint collision; choose another token")
            if any(
                other_key != key and team["name"].casefold() == name.casefold()
                for other_key, team in self.teams.items()
            ):
                raise ValueError("team name is already taken")
            self.teams[key] = {"token": token, "name": name}
            self._save_locked()
        return {"team_id": key, "name": name}

    def create(self, name: str) -> dict[str, str]:
        name = name.strip()
        if not 1 <= len(name) <= 48 or any(ord(ch) < 32 for ch in name):
            raise ValueError("name must be 1–48 printable characters")
        with self.lock:
            if any(team["name"].casefold() == name.casefold() for team in self.teams.values()):
                raise ValueError("team name is already taken")
            while True:
                token = secrets.token_urlsafe(32)
                team_id = token_id(token)
                if team_id not in self.teams:
                    break
            result = self.register(token, name)
            return {**result, "token": token}

    def names(self) -> dict[str, str]:
        with self.lock:
            return {key: team["name"] for key, team in self.teams.items()}

    def resolve_token(self, token: str) -> str | None:
        key = token_id(token.strip())
        with self.lock:
            team = self.teams.get(key)
            return key if team is not None and team["token"] == token.strip() else None


class Store:
    def __init__(self, path: Path):
        path.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
        self.db = sqlite3.connect(path, check_same_thread=False, isolation_level=None)
        if os.name != "nt" and path.exists():
            path.chmod(0o600)
        self.db.row_factory = sqlite3.Row
        self.lock = threading.RLock()
        with self.lock:
            self.db.execute("PRAGMA journal_mode=WAL")
            self.db.executescript(
                """
                CREATE TABLE IF NOT EXISTS runs (
                    id TEXT PRIMARY KEY,
                    world_number INTEGER NOT NULL,
                    world_id TEXT NOT NULL,
                    world_name TEXT NOT NULL,
                    world_occurrence INTEGER NOT NULL DEFAULT 1,
                    run_number INTEGER NOT NULL DEFAULT 1,
                    arena_name TEXT NOT NULL,
                    port INTEGER NOT NULL,
                    started_at REAL NOT NULL,
                    ended_at REAL,
                    status TEXT NOT NULL,
                    error TEXT
                );
                CREATE TABLE IF NOT EXISTS run_team_stats (
                    run_id TEXT NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
                    team_id TEXT NOT NULL,
                    fallback_name TEXT NOT NULL,
                    gold INTEGER NOT NULL,
                    gold_collected INTEGER NOT NULL,
                    carpets_lost INTEGER NOT NULL,
                    distance_travelled REAL NOT NULL,
                    updated_at REAL NOT NULL,
                    PRIMARY KEY(run_id, team_id)
                );
                CREATE INDEX IF NOT EXISTS run_stats_team ON run_team_stats(team_id);
                """
            )
            columns = {row[1] for row in self.db.execute("PRAGMA table_info(runs)")}
            if "world_id" not in columns:
                self.db.execute("PRAGMA foreign_keys=OFF")
                self.db.executescript(
                    """
                    CREATE TABLE runs_new (
                        id TEXT PRIMARY KEY, world_number INTEGER NOT NULL,
                        world_id TEXT NOT NULL, world_name TEXT NOT NULL,
                        world_occurrence INTEGER NOT NULL DEFAULT 1,
                        run_number INTEGER NOT NULL DEFAULT 1,
                        arena_name TEXT NOT NULL, port INTEGER NOT NULL,
                        started_at REAL NOT NULL, ended_at REAL, status TEXT NOT NULL, error TEXT
                    );
                    """
                )
                old_columns = {row[1] for row in self.db.execute("PRAGMA table_info(runs)")}
                occurrence = "seed_occurrence" if "seed_occurrence" in old_columns else "1"
                run_number = "run_number" if "run_number" in old_columns else "rowid"
                self.db.execute(
                    f"""INSERT INTO runs_new
                    (id, world_number, world_id, world_name, world_occurrence, run_number,
                     arena_name, port, started_at, ended_at, status, error)
                    SELECT id, world_number, 'legacy-world-' || world_number,
                     'Мир ' || world_number, {occurrence}, {run_number},
                     'world_legacy_' || world_number || '_' || {occurrence} || '_' || {run_number},
                     port, started_at, ended_at, status, error FROM runs"""
                )
                self.db.executescript(
                    "DROP TABLE runs; ALTER TABLE runs_new RENAME TO runs; PRAGMA foreign_keys=ON;"
                )
            self.db.execute("CREATE INDEX IF NOT EXISTS runs_world_started ON runs(world_number, started_at DESC)")
            self.db.execute("CREATE INDEX IF NOT EXISTS runs_world_id ON runs(world_id, started_at DESC)")

    def start_run(self, world: dict[str, Any], port: int) -> tuple[str, str]:
        run_id = uuid.uuid4().hex
        with self.lock, self.db:
            run_number = self.db.execute("SELECT count(*) + 1 FROM runs").fetchone()[0]
            world_number = int(world["world_number"])
            world_id = str(world["id"])
            world_name = str(world["name"])
            world_occurrence = self.db.execute(
                "SELECT count(*) + 1 FROM runs WHERE world_id=?", (world_id,)
            ).fetchone()[0]
            arena_name = f"world_{world_id}_{world_occurrence}_{run_number}"
            self.db.execute(
                """INSERT INTO runs
                (id, world_number, world_id, world_name, world_occurrence, run_number, arena_name,
                 port, started_at, ended_at, status, error)
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, NULL, 'starting', NULL)""",
                (run_id, world_number, world_id, world_name, world_occurrence, run_number, arena_name, port, time.time()),
            )
        return run_id, arena_name

    def set_run(self, run_id: str, status: str, error: str | None = None) -> None:
        ended_at = time.time() if status in {"complete", "failed", "interrupted"} else None
        with self.lock, self.db:
            self.db.execute("UPDATE runs SET status=?, ended_at=COALESCE(?, ended_at), error=? WHERE id=?", (status, ended_at, error, run_id))

    def ingest(self, run_id: str, entries: list[dict[str, Any]], names: dict[str, str]) -> None:
        now = time.time()
        rows = []
        for entry in entries:
            team_id = str(entry.get("team_id", ""))
            if not re.fullmatch(r"[0-9a-f]{16}", team_id):
                continue
            fallback = str(entry.get("team", f"Team-{team_id[:6]}"))[:48]
            rows.append((
                run_id,
                team_id,
                fallback,
                max(0, int(entry.get("gold", 0))),
                max(0, int(entry.get("gold_collected", 0))),
                max(0, int(entry.get("carpets_lost", 0))),
                max(0.0, float(entry.get("distance_travelled", 0.0))),
                now,
            ))
        with self.lock, self.db:
            self.db.executemany(
                """INSERT INTO run_team_stats VALUES (?, ?, ?, ?, ?, ?, ?, ?)
                ON CONFLICT(run_id, team_id) DO UPDATE SET fallback_name=excluded.fallback_name,
                gold=excluded.gold, gold_collected=excluded.gold_collected, carpets_lost=excluded.carpets_lost,
                distance_travelled=excluded.distance_travelled, updated_at=excluded.updated_at""",
                rows,
            )

    @staticmethod
    def _run_json(row: sqlite3.Row) -> dict[str, Any]:
        value = dict(row)
        value["started_at"] = dt.datetime.fromtimestamp(value["started_at"], dt.timezone.utc).isoformat(timespec="seconds")
        if value["ended_at"] is not None:
            value["ended_at"] = dt.datetime.fromtimestamp(value["ended_at"], dt.timezone.utc).isoformat(timespec="seconds")
        return value

    def runs(
        self, world_number: int | None = None, limit: int = 50, offset: int = 0
    ) -> dict[str, Any]:
        with self.lock:
            if world_number is None:
                total = self.db.execute("SELECT count(*) FROM runs").fetchone()[0]
                rows = self.db.execute(
                    "SELECT * FROM runs ORDER BY run_number DESC LIMIT ? OFFSET ?",
                    (limit, offset),
                ).fetchall()
            else:
                total = self.db.execute(
                    "SELECT count(*) FROM runs WHERE world_number=?", (world_number,)
                ).fetchone()[0]
                rows = self.db.execute(
                    "SELECT * FROM runs WHERE world_number=? ORDER BY run_number DESC LIMIT ? OFFSET ?",
                    (world_number, limit, offset),
                ).fetchall()
        return {
            "runs": [self._run_json(row) for row in rows],
            "total": total,
            "limit": limit,
            "offset": offset,
        }

    def leaderboard(self, scope: str, registry_names: dict[str, str], world_number: int | None = None, run_id: str | None = None) -> dict[str, Any]:
        with self.lock:
            if scope == "run":
                if not run_id:
                    raise ValueError("run_id is required for scope=run")
                run_row = self.db.execute("SELECT * FROM runs WHERE id=?", (run_id,)).fetchone()
                if run_row is None:
                    raise KeyError("run not found")
                raw_rows = self.db.execute(
                    "SELECT * FROM run_team_stats WHERE run_id=?", (run_id,)
                ).fetchall()
                teams = []
                for row in raw_rows:
                    data = {field: row[field] for field in METRICS_FIELDS}
                    teams.append({"team_id": row["team_id"], "name": registry_names.get(row["team_id"], row["fallback_name"]), **data})
                teams.sort(key=lambda item: (-item["gold_collected"], -item["gold"], item["name"].casefold()))
                for rank, item in enumerate(teams, 1):
                    item["rank"] = rank
                return {"scope": scope, "run": self._run_json(run_row), "teams": teams}
            if scope == "world":
                if world_number is None or world_number < 1:
                    raise ValueError("world must be a positive ordinal")
                raw_rows = self.db.execute(
                    "SELECT r.id AS run_id, r.world_number, r.world_id, r.world_name, r.arena_name, r.started_at, r.status AS run_status, s.* FROM runs r JOIN run_team_stats s ON s.run_id=r.id WHERE r.world_number=? ORDER BY r.started_at",
                    (world_number,),
                ).fetchall()
            elif scope == "all":
                raw_rows = self.db.execute(
                    "SELECT r.id AS run_id, r.world_number, r.world_id, r.world_name, r.arena_name, r.started_at, r.status AS run_status, s.* FROM runs r JOIN run_team_stats s ON s.run_id=r.id ORDER BY r.started_at",
                ).fetchall()
            else:
                raise ValueError("scope must be run, world, or all")
        grouped: dict[str, list[sqlite3.Row]] = {}
        for row in raw_rows:
            grouped.setdefault(row["team_id"], []).append(row)
        teams = []
        for team_id, attempts in grouped.items():
            best = max(attempts, key=lambda row: (row["gold_collected"], row["gold"], -row["started_at"]))
            total = {field: sum(row[field] for row in attempts) for field in METRICS_FIELDS}
            top = {field: best[field] for field in METRICS_FIELDS}
            teams.append({
                "team_id": team_id,
                "name": registry_names.get(team_id, best["fallback_name"]),
                "attempts": len(attempts),
                "top": top,
                "total": total,
                "best_run": {"run_id": best["run_id"], "world_number": best["world_number"], "world_id": best["world_id"], "world_name": best["world_name"], "started_at": dt.datetime.fromtimestamp(best["started_at"], dt.timezone.utc).isoformat(timespec="seconds")},
            })
        teams.sort(key=lambda item: (-item["total"]["gold_collected"], -item["total"]["gold"], item["name"].casefold()))
        for rank, item in enumerate(teams, 1):
            item["rank"] = rank
        return {"scope": scope, "world_number": world_number, "teams": teams}

    def counts(self) -> dict[str, int]:
        with self.lock:
            return {
                "runs": self.db.execute("SELECT count(*) FROM runs").fetchone()[0],
                "teams": self.db.execute("SELECT count(DISTINCT team_id) FROM run_team_stats").fetchone()[0],
            }


class HubState:
    def __init__(self, world_configs: list[dict[str, Any]]):
        DATA_DIR.mkdir(parents=True, exist_ok=True, mode=0o700)
        ARENA_DATA_DIR.mkdir(parents=True, exist_ok=True, mode=0o700)
        if os.name != "nt":
            DATA_DIR.chmod(0o700)
            ARENA_DATA_DIR.chmod(0o700)
        self.world_configs = world_configs
        self.registry = TeamRegistry(REGISTRY_PATH)
        self.observer_token = secrets.token_urlsafe(32)
        self.control_token = ARENA_CONTROL_TOKEN or secrets.token_urlsafe(32)
        self.store = Store(DB_PATH)
        self.lock = threading.RLock()
        self.realtime_tickets: dict[str, dict[str, Any]] = {}
        self.votes: dict[str, str] = {}
        self.arena: dict[str, Any] = {
            "status": "starting", "world_number": None, "world_id": None, "world_name": None,
            "arena_name": None, "port": ARENA_PORT, "run_id": None,
            "started_at": None, "ends_at": None, "next_start_at": None,
            "pid": None, "error": None,
        }

    def set_arena(self, **values: Any) -> None:
        with self.lock:
            self.arena.update(values)

    def set_vote(self, team_id: str, world_id: str) -> dict[str, Any]:
        if not any(world["id"] == world_id for world in self.world_configs):
            raise ValueError("world_id is not present in worlds.json")
        with self.lock:
            if self.arena.get("run_id") is None:
                raise ValueError("the first arena is starting; vote after it becomes active")
            self.votes[team_id] = world_id
            return self.vote_results_locked()

    def vote_results_locked(self) -> dict[str, int]:
        counts: dict[str, int] = {}
        for world_id in self.votes.values():
            counts[world_id] = counts.get(world_id, 0) + 1
        return counts

    def vote_results(self) -> dict[str, int]:
        with self.lock:
            return self.vote_results_locked()

    def consume_vote_winner(self) -> str | None:
        with self.lock:
            counts = self.vote_results_locked()
            self.votes.clear()
        if not counts:
            return None
        highest = max(counts.values())
        tied = [world_id for world_id, count in counts.items() if count == highest]
        return random.SystemRandom().choice(tied)

    def current_arena(self) -> dict[str, Any]:
        with self.lock:
            result = dict(self.arena)
        if result["world_number"] is not None:
            result["url"] = ARENA_PUBLIC_URL or f"http://{ARENA_PUBLIC_HOST}:{ARENA_PORT}"
            result["seconds_remaining"] = max(0, int((result.get("ends_at") or time.time()) - time.time()))
        return result


def arena_runtime_request(method: str, path: str, payload: dict[str, Any] | None = None) -> dict[str, Any]:
    if not ARENA_CONTROL_TOKEN:
        raise RuntimeError("ARENA_CONTROL_TOKEN must be configured for runtime-api mode")
    body = json.dumps(payload).encode("utf-8") if payload is not None else None
    request = urllib.request.Request(
        f"{ARENA_CONTROL_URL}{path}", data=body,
        headers={"X-Arena-Control-Token": ARENA_CONTROL_TOKEN, "Content-Type": "application/json"},
        method=method,
    )
    try:
        with urllib.request.urlopen(request, timeout=5.0) as response:
            result = json.loads(response.read())
    except urllib.error.HTTPError as exc:
        try:
            detail = json.loads(exc.read()).get("error", str(exc))
        except (json.JSONDecodeError, AttributeError):
            detail = str(exc)
        raise RuntimeError(f"arena runtime returned HTTP {exc.code}: {detail}") from exc
    except (urllib.error.URLError, TimeoutError, OSError) as exc:
        raise RuntimeError(f"arena runtime is unavailable: {exc}") from exc
    if not isinstance(result, dict):
        raise RuntimeError("arena runtime returned an invalid response")
    return result


class ArenaRuntimeProcess:
    """Popen-compatible handle for a Rust arena process managed by the runtime container."""

    def __init__(self, run_id: str, pid: int):
        self.run_id = run_id
        self.pid = pid
        self.returncode: int | None = None

    def poll(self) -> int | None:
        status = arena_runtime_request("GET", f"/internal/arena/status?run_id={self.run_id}")
        if status.get("running"):
            return None
        code = status.get("exit_code")
        self.returncode = int(code) if code is not None else 1
        return self.returncode

    def send_signal(self, sig: int) -> None:
        if sig not in (signal.SIGINT, signal.SIGTERM):
            raise ValueError("arena runtime only accepts graceful stop signals")
        arena_runtime_request("POST", "/internal/arena/stop", {"run_id": self.run_id})

    def wait(self, timeout: float | None = None) -> int:
        deadline = None if timeout is None else time.monotonic() + timeout
        while True:
            result = self.poll()
            if result is not None:
                return result
            if deadline is not None and time.monotonic() >= deadline:
                raise subprocess.TimeoutExpired("arena runtime", timeout)
            time.sleep(0.1)

    def kill(self) -> None:
        try:
            arena_runtime_request("POST", "/internal/arena/stop", {"run_id": self.run_id})
        except RuntimeError:
            pass


def start_runtime_arena(world_id: str, arena_name: str, run_id: str, observer_token: str) -> ArenaRuntimeProcess:
    result = arena_runtime_request("POST", "/internal/arena/start", {
        "run_id": run_id,
        "world_id": world_id,
        "arena_name": arena_name,
        "observer_token": observer_token,
        "duration_sec": RUN_SECONDS,
    })
    if result.get("status") != "running" or int(result.get("pid", 0)) <= 0:
        raise RuntimeError("arena runtime did not confirm process startup")
    return ArenaRuntimeProcess(run_id, int(result["pid"]))


def pick_world(
    worlds: list[dict[str, Any]], previous_id: str | None, voted_world_id: str | None = None
) -> dict[str, Any]:
    if FIXED_WORLD_ID:
        world = next((item for item in worlds if item["id"] == FIXED_WORLD_ID), None)
        if world is None:
            raise RuntimeError("HUB_FIXED_WORLD_ID is not present in worlds.json")
        return world
    if voted_world_id:
        world = next((item for item in worlds if item["id"] == voted_world_id), None)
        if world is not None:
            return world
    candidates = [world for world in worlds if world["id"] != previous_id] if previous_id else worlds
    if not candidates:
        candidates = worlds
    return random.SystemRandom().choice(candidates)


def read_report(path: Path) -> list[dict[str, Any]] | None:
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
        return data.get("teams", [])
    except (OSError, json.JSONDecodeError, AttributeError, TypeError):
        return None


async def stop_process(process: Any) -> None:
    if process.poll() is not None:
        return
    try:
        process.send_signal(signal.SIGINT if os.name != "nt" else signal.SIGTERM)
        await asyncio.wait_for(asyncio.to_thread(process.wait), timeout=10)
    except (asyncio.TimeoutError, ProcessLookupError):
        process.kill()
        await asyncio.to_thread(process.wait)


async def run_arena_loop(state: HubState) -> None:
    previous_world_id: str | None = None
    while True:
        voted_world_id = state.consume_vote_winner()
        world = pick_world(state.world_configs, previous_world_id, voted_world_id)
        previous_world_id = world["id"]
        world_number = world["world_number"]
        world_id = world["id"]
        run_id, arena_name = state.store.start_run(world, ARENA_PORT)
        run_dir = ARENA_DATA_DIR / f"run_{run_id}"
        run_dir.mkdir(parents=True, exist_ok=True)
        report_path = run_dir / "metrics.json"
        status_path = DATA_DIR / "current_world.json"
        log_path = ARENA_DATA_DIR / "arena.log"
        start_epoch = time.time()
        ends_epoch = start_epoch + RUN_SECONDS
        state.set_arena(
            status="starting", world_number=world_number, world_id=world_id, world_name=world["name"],
            arena_name=arena_name, port=ARENA_PORT, run_id=run_id,
            started_at=start_epoch, ends_at=ends_epoch, next_start_at=None,
            pid=None, error=None,
        )
        env = os.environ.copy()
        env.update({
            "HOST": ARENA_HOST,
            "PORT": str(ARENA_PORT),
            "DATS_WORLD_ID": str(world_id),
            "DATS_WORLD_RUN_NAME": arena_name,
            "DATS_WORLDS_PATH": str(WORLDS_PATH),
            "DATS_TOKEN_REGISTRY_PATH": str(REGISTRY_PATH),
            "DATS_OBSERVER_TOKEN": state.observer_token,
            "DATS_LEADERBOARD_PATH": str(report_path),
            "DATS_WORLD_STATUS_PATH": str(status_path),
            "DATS_HUB_INTERNAL_URL": os.environ.get("DATS_HUB_INTERNAL_URL", f"http://{HUB_HOST}:{HUB_PORT}"),
            "DATS_HUB_CONTROL_TOKEN": state.control_token,
        })
        process: subprocess.Popen[bytes] | None = None
        planned_stop = False
        error: str | None = None
        try:
            with log_path.open("ab") as log:
                log.write(f"\n=== {utc_now()} start {arena_name} world={world_id} run={run_id} ===\n".encode())
                log.flush()
                if ARENA_LIFECYCLE_MODE == "runtime-api":
                    process = start_runtime_arena(world_id, arena_name, run_id, state.observer_token)
                elif ARENA_LIFECYCLE_MODE == "process":
                    process = subprocess.Popen(
                        [str(ARENA_BIN)], cwd=ROOT, env=env,
                        stdout=log, stderr=subprocess.STDOUT, start_new_session=(os.name != "nt"),
                    )
                else:
                    raise RuntimeError(f"unsupported ARENA_LIFECYCLE_MODE: {ARENA_LIFECYCLE_MODE}")
                state.store.set_run(run_id, "running")
                state.set_arena(status="running", pid=process.pid)
                while time.time() < ends_epoch:
                    if process.poll() is not None:
                        error = f"arena exited early with code {process.returncode}"
                        break
                    report = read_report(report_path)
                    if report is not None:
                        state.store.ingest(run_id, report, state.registry.names())
                    await asyncio.sleep(min(POLL_SECONDS, max(0.05, ends_epoch - time.time())))
                if process.poll() is None:
                    planned_stop = True
                    state.set_arena(status="stopping")
                    await stop_process(process)
                final_report = read_report(report_path)
                if final_report is not None:
                    state.store.ingest(run_id, final_report, state.registry.names())
                log.write(f"=== {utc_now()} stop {arena_name} code={process.returncode} ===\n".encode())
                log.flush()
        except FileNotFoundError:
            error = f"arena binary not found: {ARENA_BIN}; run scripts/run_hub.sh to build it"
        except asyncio.CancelledError:
            if process is not None:
                await stop_process(process)
            state.store.set_run(run_id, "interrupted", "control plane stopped")
            state.set_arena(status="stopped", pid=None)
            raise
        except Exception as exc:  # keep supervisor alive and make the failure visible
            error = f"{type(exc).__name__}: {exc}"
            if process is not None:
                await stop_process(process)

        status = "complete" if planned_stop else "failed"
        state.store.set_run(run_id, status, error)
        previous_world_id = world_id
        next_start = next_minute_boundary(time.time())
        state.set_arena(
            status="cooldown", pid=None, error=error,
            ended_at=time.time(), next_start_at=next_start,
        )
        await asyncio.sleep(max(0, next_start - time.time()))


def esc(value: str) -> str:
    return html.escape(value, quote=True)


def markdown_html(source: str) -> str:
    output: list[str] = []
    in_code = False
    in_list = False
    in_table = False
    for line in source.splitlines():
        if line.strip().startswith("```"):
            if in_list:
                output.append("</ul>")
                in_list = False
            if in_table:
                output.append("</pre>")
                in_table = False
            output.append("<pre><code>" if not in_code else "</code></pre>")
            in_code = not in_code
            continue
        if in_code:
            output.append(esc(line) + "\n")
            continue
        if line.startswith("|"):
            if in_list:
                output.append("</ul>")
                in_list = False
            if not in_table:
                output.append('<pre class="table">')
                in_table = True
            output.append(esc(line) + "\n")
            continue
        if in_table:
            output.append("</pre>")
            in_table = False
        if line.startswith("- "):
            if not in_list:
                output.append("<ul>")
                in_list = True
            output.append(f"<li>{esc(line[2:])}</li>")
        else:
            if in_list:
                output.append("</ul>")
                in_list = False
            if line.startswith("### "):
                output.append(f"<h3>{esc(line[4:])}</h3>")
            elif line.startswith("## "):
                output.append(f"<h2>{esc(line[3:])}</h2>")
            elif line.startswith("# "):
                output.append(f"<h1>{esc(line[2:])}</h1>")
            elif line.strip():
                output.append(f"<p>{esc(line)}</p>")
    if in_list:
        output.append("</ul>")
    if in_table:
        output.append("</pre>")
    if in_code:
        output.append("</code></pre>")
    return "\n".join(output)


STYLE = '<link rel="icon" type="image/svg+xml" href="/static/stadmagic-mark.svg"><link rel="stylesheet" href="/static/hub.css">'


def page(title: str, body: str) -> bytes:
    nav = '<header><a class="brand" href="/" aria-label="StadMagic — на главную"><img src="/static/stadmagic-mark.svg" alt=""><strong>StadMagic</strong></a><a href="/">Обзор</a><a href="/arena">Арена</a><a href="/worlds">Миры</a><a href="/leaderboard">Рейтинг</a><a href="/docs">Документы</a><a href="/register">Команда</a></header>'
    footer = '<footer class=site-footer>StadMagic · Мир меняется. Команды остаются.</footer>'
    return (f"<!doctype html><html lang=ru><head><meta charset=utf-8><meta name=viewport content='width=device-width, initial-scale=1'><meta name=theme-color content='#08111d'><title>{esc(title)}</title>{STYLE}</head><body>{nav}<main>{body}</main>{footer}</body></html>").encode("utf-8")


def home_html(state: HubState) -> bytes:
    arena = state.current_arena()
    body = """
    <section class="card hero"><span class=eyebrow><i class=live-dot></i>Живая арена</span><h1>Здесь решается,<br>какой мир будет дальше.</h1><p>Следи за матчами, выбирай следующий мир голосом команды и смотри, кто лучше справился с хаосом.</p><div class=hero-actions><a class=button href="/worlds">Выбрать следующий мир <span aria-hidden=true>↗</span></a><a class="button secondary" href="/leaderboard">Смотреть рейтинг</a></div>
      <div class=metrics><div class=metric><small>Сейчас играют</small><strong id=home-world>__WORLD_NAME__</strong></div><div class=metric><small>Запуск</small><strong id=home-run>__ARENA_NAME__</strong></div><div class=metric><small>Статус</small><strong id=home-status>__ARENA_STATUS__</strong></div><div class=metric><small>До смены мира</small><strong id=home-countdown>__COUNTDOWN__ сек.</strong></div></div>
      <p><a id=home-arena-link class="button secondary" href="__ARENA_URL__">Открыть API арены</a> <span id=home-updated class=muted>обновление состояния каждые 5 секунд</span></p>
    </section>
    <section class="card inspiration-card"><div><span class=eyebrow>Откуда появился StadMagic</span><h2>С благодарностью к Dats.Team</h2><p>Я вдохновился <a href="https://gamethon.datsteam.dev/datsmagic" target="_blank" rel="noopener noreferrer">DatsMagic от Dats.Team</a> и сделал очень похожую самостоятельную реализацию. Официальные игровые серверы закрыты, а мне захотелось дать людям возможность ещё немного поиграть в этот мир.</p><p>StadMagic — неофициальный проект, не связанный с Dats.Team. У меня нет к команде претензий и я ничего от неё не требую. Если Dats.Team попросит, я закрою серверы и доступ к игре.</p></div><span class="inspiration-mark" aria-hidden=true>✦</span></section>
    <section class=grid><a class="card quick-link" href="/arena"><span class=eyebrow>01 · Наблюдай</span><h2>Живая арена</h2><p>Ковры, золото, аномалии и простое ручное управление прямо в браузере.</p><span class=status>Открыть визуализацию →</span></a><a class="card quick-link" href="/worlds"><span class=eyebrow>02 · Участвуй</span><h2>Голосуй за мир</h2><p>Один голос от команды. Меняй решение до старта следующей арены.</p><span class=status>Открыть каталог →</span></a><a class="card quick-link" href="/leaderboard"><span class=eyebrow>03 · Сравнивай</span><h2>Следи за командами</h2><p>Результаты активного запуска, отдельные арены и сводные итоги.</p><span class=status>Открыть лидерборд →</span></a><a class="card quick-link" href="/register"><span class=eyebrow>04 · Представься</span><h2>Создай команду</h2><p>Выбери уникальное имя и получи токен для игрового бота.</p><span class=status>Зарегистрировать команду →</span></a></section>
    <script>async function refreshHome(){try{const d=await(await fetch('/api/worlds')).json(),a=d.active||{};document.querySelector('#home-world').textContent=a.world_name||'Подготовка арены';document.querySelector('#home-run').textContent=a.arena_name||'Ожидание';document.querySelector('#home-status').textContent=a.status||'starting';document.querySelector('#home-countdown').textContent=`${a.seconds_remaining??0} сек.`;if(a.url)document.querySelector('#home-arena-link').href=a.url;document.querySelector('#home-updated').textContent=`${d.worlds?.length??0} миров · обновлено ${new Date().toLocaleTimeString()}`}catch(_){document.querySelector('#home-updated').textContent='Нет связи с Hub API' }}refreshHome();setInterval(refreshHome,5000)</script>
    """
    values = {
        "__WORLD_NAME__": esc(arena.get("world_name") or "Подготовка арены"),
        "__ARENA_NAME__": esc(arena.get("arena_name") or "Ожидание"),
        "__ARENA_STATUS__": esc(arena.get("status", "starting")),
        "__COUNTDOWN__": str(arena.get("seconds_remaining", 0)),
        "__ARENA_URL__": esc(arena.get("url", f"http://{ARENA_PUBLIC_HOST}:{ARENA_PORT}")),
    }
    for placeholder, value in values.items():
        body = body.replace(placeholder, value)
    return page("StadMagic", body)


def arena_visualizer_html() -> bytes:
    body = '''
    <link rel="stylesheet" href="/static/arena-visualizer.css">
    <section class="visualizer-shell">
      <div class="visualizer-top card">
        <div><span class="eyebrow">ЖИВАЯ АРЕНА</span><h1>Наблюдение</h1><p>Лёгкая карта мира. Выбор и слежение не включают управление.</p></div>
        <form id="connect-form" class="connect-form"><label for="viz-token">Токен команды <span class="muted">(необязательно для наблюдения)</span></label><div class="connect-row"><input id="viz-token" type="password" autocomplete="off" placeholder="Вставь зарегистрированный токен"><button type="submit">Мой флот</button><button id="observer-connect" type="button" class="secondary">Наблюдать</button></div></form>
      </div>
      <div class="visualizer-layout">
        <section class="visualizer-stage card">
          <div class="viz-toolbar">
            <label class="select-label">Следить за<select id="carpet-select" disabled><option>Подключись к арене</option></select></label>
            <button id="follow-toggle" class="secondary" disabled>◎ Следить</button>
            <button id="manual-toggle" class="secondary" disabled>Ручное управление: выкл.</button>
            <button id="zoom-out" class="secondary" aria-label="Уменьшить">−</button><button id="zoom-in" class="secondary" aria-label="Увеличить">+</button>
            <button id="camera-reset" class="secondary">Обзор</button><button id="fullscreen-toggle" class="secondary">⛶ На весь экран</button>
          </div>
          <div class="canvas-wrap"><canvas id="arena-canvas" aria-label="Карта арены"></canvas><div id="touch-stick" class="touch-stick" aria-label="Виртуальный стик"><div class="stick-base"><i></i></div><span>тяни для ускорения</span></div><div id="connection-badge" class="connection-badge">Нет подключения</div></div>
          <div class="viz-footer"><span id="world-label">Мир не загружен</span><span id="fps-label">— FPS</span><span>Колесо / щипок — зум · перетаскивание / стрелки — карта</span></div>
        </section>
        <aside class="viz-sidebar">
          <section class="card selected-panel"><span class="eyebrow">ВЫБРАННЫЙ КОВЁР</span><h2 id="selected-title">Ничего не выбрано</h2><div id="selected-stats" class="selected-stats muted">Наблюдай без токена или подключи свой флот.</div><div class="vector-legend"><span><i class="v-speed"></i>Скорость V</span><span><i class="v-self"></i>Ускорение A</span><span><i class="v-anomaly"></i>Силы аномалий W</span></div></section>
          <section class="card viz-help"><h2>Управление</h2><p><b>ПК:</b> колесо — масштаб, перетаскивание или стрелки — перемещение, клик по ковру — выбор. Включи слежение отдельно.</p><p><b>Телефон:</b> один палец — карта, два — масштаб. Для ручного режима выбери свой ковер и потяни виртуальный стик.</p><p>Ручное управление работает только для твоего живого ковра. При включении бот временно уступает ему управление.</p></section>
        </aside>
      </div>
      <p id="viz-message" class="viz-message" role="status"></p>
    </section>
    <script src="/static/arena-visualizer.js" defer></script>
    '''
    return page("Живая арена · StadMagic", body)


def docs_html() -> bytes:
    body = """
    <h1>Документация</h1><section class=card><p>Всё необходимое, чтобы зарегистрировать команду и подключить игрового бота.</p>
    <div class=tabs><a class=button href="/docs/api">Игровой API</a><a class=button href="/docs/world">Как играть и написать бота</a><a class=button href="/api/docs/mechanics">Техническая механика (Markdown)</a></div></section>
    """
    return page("Документация · StadMagic", body)


def register_html() -> bytes:
    body = """
    <h1>Регистрация команды</h1><section class=card><p>Придумайте уникальное имя команды. Hub создаст токен автоматически — сохраните его: повторно показать секрет нельзя. Токен нужен боту для каждого игрового запроса.</p>
    <form id=f><label for=n>Имя команды</label><input id=n maxlength=48 autocomplete=organization required><p><button>Создать команду и токен</button></p></form><div id=result role=status aria-live=polite></div></section>
    <script>document.querySelector('#f').addEventListener('submit',async e=>{e.preventDefault();const r=await fetch('/api/teams',{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify({name:document.querySelector('#n').value})});const d=await r.json();const out=document.querySelector('#result');out.replaceChildren();out.className=r.ok?'ok':'error';if(!r.ok){out.textContent=d.error||'Ошибка';return}const title=document.createElement('p');title.textContent=`Команда ${d.name} создана. Сохраните токен сейчас:`;const token=document.createElement('code');token.textContent=d.token;token.style='display:block;overflow-wrap:anywhere;padding:1rem;margin:.75rem 0';const copy=document.createElement('button');copy.type='button';copy.textContent='Скопировать токен';copy.onclick=async()=>{await navigator.clipboard.writeText(d.token);copy.textContent='Скопировано'};const warning=document.createElement('p');warning.textContent='Токен показывается только один раз. Не отправляйте его другим и не публикуйте.';out.append(title,token,copy,warning);document.querySelector('#f').reset()});</script>
    """
    return page("Регистрация · StadMagic", body)


def worlds_html() -> bytes:
    body = """
    <div class=eyebrow>Каталог миров</div><h1>Выбери, куда лететь дальше</h1>
    <section class=card><div class=grid><label>Изучить профиль<select id=world-select></select></label><div><p id=current class=status>Подключаемся к арене…</p><small id=catalog-summary>Загружаю каталог миров…</small></div></div></section>
    <section class=card><div class=eyebrow>Голосование команды</div><h2>Каким будет следующий мир?</h2><p>Один голос на зарегистрированную команду. Его можно поменять до выбора арены. При равенстве голосов победителя определит случай; без голосов мир тоже будет выбран случайно.</p>
      <form id=vote-form><div class=grid><label>Ваш выбор<select id=vote-world required></select></label><label>Токен команды<input id=vote-token type=password maxlength=128 autocomplete=current-password placeholder="Токен, зарегистрированный в Hub" required></label></div><p><button>Отдать голос <span aria-hidden=true>→</span></button> <span id=vote-result role=status aria-live=polite></span></p></form>
      <h3>Кандидаты и голоса</h3><div id=vote-list class=muted>Загружаю…</div>
    </section>
    <section class=card><div class=eyebrow>Профиль мира</div><h2 id=world-title>Загрузка настроек…</h2><p id=world-description></p><pre id=world-config></pre></section>
    <script>
    let worlds=[],selectedWorld=null,catalogSignature='';
    const select=document.querySelector('#world-select'),voteSelect=document.querySelector('#vote-world');
    select.onchange=()=>{selectedWorld=Number(select.value);showWorld()};
    function showWorld(){const w=worlds.find(x=>x.world_number===selectedWorld);if(!w)return;document.querySelector('#world-title').textContent=`${w.name} · ${w.id} · мир ${w.world_number}`;document.querySelector('#world-description').textContent=w.description;document.querySelector('#world-config').textContent=JSON.stringify(w.config,null,2)}
    async function refreshVotes(){const response=await fetch('/api/votes'),data=await response.json(),host=document.querySelector('#vote-list');if(!response.ok)throw Error(data.error||'Не удалось загрузить голоса');const rows=(data.worlds||[]).filter(item=>item.votes>0).sort((a,b)=>b.votes-a.votes||a.name.localeCompare(b.name,'ru'));host.replaceChildren();if(!rows.length){host.textContent='Пока ни одной команды не проголосовало.';return}const max=Math.max(...rows.map(item=>item.votes),1);for(const item of rows){const row=document.createElement('div');row.className='vote-row';const name=document.createElement('span');name.textContent=item.name;const track=document.createElement('div');track.className='vote-track';const fill=document.createElement('div');fill.className='vote-fill';fill.style.width=`${Math.max(5,item.votes/max*100)}%`;track.append(fill);const count=document.createElement('strong');count.textContent=`${item.votes}`;row.append(name,track,count);host.append(row)}}
    async function refresh(){try{const response=await fetch('/api/worlds'),data=await response.json();if(!response.ok)throw Error(data.error||'Не удалось загрузить миры');worlds=data.worlds||[];const active=data.active||{},signature=worlds.map(w=>w.id).join('|');document.querySelector('#catalog-summary').textContent=`${worlds.length} профилей · каталог assets/worlds.json`;if(signature!==catalogSignature){catalogSignature=signature;const previous=select.value;select.replaceChildren();voteSelect.replaceChildren();for(const w of worlds){select.add(new Option(`${w.world_number}. ${w.name} · ${w.id}`,w.world_number));voteSelect.add(new Option(`${w.name} · ${w.id}`,w.id))}selectedWorld=worlds.some(w=>String(w.world_number)===previous)?Number(previous):(active.world_number??worlds[0]?.world_number);if(selectedWorld!=null)select.value=selectedWorld}document.querySelector('#current').textContent=active.world_number?`Сейчас: ${active.arena_name} · ${active.world_name} · ${active.status} · осталось ${active.seconds_remaining??0} сек.`:'Арена сейчас перезапускается';showWorld();await refreshVotes()}catch(error){document.querySelector('#current').textContent=`Ошибка обновления: ${error.message}`}}
    document.querySelector('#vote-form').addEventListener('submit',async event=>{event.preventDefault();const button=event.currentTarget.querySelector('button'),out=document.querySelector('#vote-result');button.disabled=true;out.className='muted';out.textContent='Отправляю голос…';try{const response=await fetch('/api/votes',{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify({token:document.querySelector('#vote-token').value,world_id:voteSelect.value})}),data=await response.json();if(!response.ok)throw Error(data.error||'Голос не принят');out.className='ok';out.textContent='Голос принят. Его можно изменить до выбора следующего мира.';document.querySelector('#vote-token').value='';await refreshVotes()}catch(error){out.className='error';out.textContent=error.message}finally{button.disabled=false}});
    refresh();setInterval(refresh,5000)
    </script>
    """
    return page("Миры · StadMagic", body)


def leaderboard_html() -> bytes:
    body = """
    <h1>Лидерборд</h1><section class=card><div class=grid><label>Раздел<select id=scope><option value=current>Текущая арена</option><option value=library>Библиотека арен</option><option value=world>Итоги одного мира</option><option value=all>Итоги всех миров</option></select></label><label id=world-label>Фильтр по миру<select id=world></select></label></div><p id=summary class=muted>Загружаю лидерборд…</p>
    <div id=library class=card hidden><div class=grid><p id=library-summary class=muted></p><div><button id=prev-page>← Новее</button> <button id=next-page>Старее →</button></div></div><div id=run-list class=run-library></div></div>
    <div style="overflow:auto"><table id=table></table></div><p id=totals-note class=muted hidden>В сводке по миру и всем мирам показатели суммируются по попыткам. «Золото» — сумма остатков на конец запусков, а не текущий баланс.</p></section>
    <script>
    const world=document.querySelector('#world'),scope=document.querySelector('#scope');
    const runLimit=30;let worldCatalog=[],runOffset=0,selectedRunId=new URLSearchParams(location.search).get('run_id');
    if(selectedRunId)scope.value='library';
    function esc(s){return String(s??'').replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]))}
    async function loadWorlds(){
      const d=await(await fetch('/api/worlds')).json();worldCatalog=d.worlds;
      if(world.options.length===0){world.add(new Option('Все миры',''));for(const w of worldCatalog)world.add(new Option(`${w.name} · ${w.id}`,w.world_number));world.value=''}
      return d;
    }
    async function loadRuns(active){
      const query=new URLSearchParams({limit:String(runLimit),offset:String(runOffset)});if(world.value)query.set('world',world.value);
      const d=await(await fetch('/api/runs?'+query.toString())).json();const runs=d.runs||[];
      const start=d.total?d.offset+1:0;document.querySelector('#library-summary').textContent=`Арены ${start}–${d.offset+runs.length} из ${d.total||0}`;
      document.querySelector('#prev-page').disabled=runOffset===0;document.querySelector('#next-page').disabled=runOffset+runLimit>=d.total;
      const entries=[];
      if(active.run_id&&!runs.some(item=>item.id===active.run_id))entries.push(`<a class="run-link active-run" href="/leaderboard?run_id=${encodeURIComponent(active.run_id)}">● Сейчас: ${esc(active.arena_name)} · ${esc(active.world_name)}</a>`);
      for(const item of runs){const isActive=item.id===active.run_id;entries.push(`<a class="run-link ${isActive?'active-run':''}" href="/leaderboard?run_id=${encodeURIComponent(item.id)}">${isActive?'● СЕЙЧАС · ':''}${esc(item.arena_name)} · ${esc(item.world_name)} · ${new Date(item.started_at).toLocaleString()} · ${esc(item.status)}</a>`)}
      document.querySelector('#run-list').innerHTML=entries.join('')||'<p class=muted>Запусков пока нет.</p>';
      if(!selectedRunId)selectedRunId=active.run_id||(runs[0]?.id??null);
      return selectedRunId;
    }
    function showRunTable(d,summary){
      document.querySelector('#summary').textContent=summary;
      document.querySelector('#table').innerHTML='<thead><tr><th>#</th><th>Команда</th><th>Золото</th><th>Собрано золота</th><th>Потеряно ковров от аварий</th><th>Пройденное расстояние</th></tr></thead><tbody>'+d.teams.map(t=>`<tr><td>${t.rank}</td><td class=team>${esc(t.name)}</td><td>${t.gold}</td><td>${t.gold_collected}</td><td>${t.carpets_lost}</td><td>${Math.round(t.distance_travelled)}</td></tr>`).join('')+'</tbody>';
    }
    async function refresh(){
      const s=scope.value;document.querySelector('#world-label').style.display=['world','library'].includes(s)?'':'none';document.querySelector('#library').hidden=s!=='library';document.querySelector('#totals-note').hidden=!['world','all'].includes(s);
      const worldState=await loadWorlds();let url,summary='';
      if(s==='current'){
        if(!worldState.active.run_id){document.querySelector('#summary').textContent='Арена запускается; лидерборд появится после первого отчёта.';document.querySelector('#table').innerHTML='';return}
        url='/api/leaderboard?scope=run&run_id='+encodeURIComponent(worldState.active.run_id);summary=`${worldState.active.arena_name} · ${worldState.active.world_name} · текущая арена`;
      }else if(s==='world'){
        if(!world.value)world.value=String(worldState.active.world_number??1);
        url='/api/leaderboard?scope=world&world='+encodeURIComponent(world.value);
      }
      else if(s==='library'){
        const runId=await loadRuns(worldState.active);if(!runId){document.querySelector('#summary').textContent='Запусков пока нет.';document.querySelector('#table').innerHTML='';return}
        url='/api/leaderboard?scope=run&run_id='+encodeURIComponent(runId);
      }else url='/api/leaderboard?scope=all';
      const r=await fetch(url);const d=await r.json();if(!r.ok){document.querySelector('#summary').textContent=d.error||'Ошибка';return}
      if(s==='library'||s==='current'){if(s==='library')summary=`${d.run.arena_name} · ${d.run.world_name} · ${d.run.status}`;showRunTable(d,summary);return}
      const selectedWorld=worldCatalog.find(w=>String(w.world_number)===world.value);
      document.querySelector('#summary').textContent=s==='world'?`Суммарные результаты мира ${selectedWorld?.name} · ${selectedWorld?.id}`:'Суммарные результаты всех команд по всем мирам';
      document.querySelector('#table').innerHTML='<thead><tr><th>Место</th><th>Команда</th><th>Попыток</th><th>Золото</th><th>Собрано золота</th><th>Потеряно ковров от аварий</th><th>Пройденное расстояние</th></tr></thead><tbody>'+d.teams.map(t=>`<tr><td>${t.rank}</td><td class=team>${esc(t.name)}</td><td>${t.attempts}</td><td>${t.total.gold}</td><td>${t.total.gold_collected}</td><td>${t.total.carpets_lost}</td><td>${Math.round(t.total.distance_travelled)}</td></tr>`).join('')+'</tbody>';
    }
    scope.onchange=refresh;world.onchange=()=>{runOffset=0;refresh()};document.querySelector('#prev-page').onclick=()=>{runOffset=Math.max(0,runOffset-runLimit);refresh()};document.querySelector('#next-page').onclick=()=>{runOffset+=runLimit;refresh()};refresh();setInterval(refresh,5000)
    </script>
    """
    return page("Лидерборд · StadMagic", body)


class RequestHandler(BaseHTTPRequestHandler):
    state: HubState

    def log_message(self, fmt: str, *args: Any) -> None:
        print(f"[hub-http] {self.address_string()} {fmt % args}")

    def send_bytes(self, status: int, body: bytes, content_type: str, content_encoding: str | None = None) -> None:
        self.send_response(status)
        self.send_header("Content-Type", content_type)
        if content_encoding:
            self.send_header("Content-Encoding", content_encoding)
            self.send_header("Vary", "Accept-Encoding")
        self.send_header("Content-Length", str(len(body)))
        self.send_header("X-Content-Type-Options", "nosniff")
        self.end_headers()
        self.wfile.write(body)

    def send_json(self, status: int, body: Any) -> None:
        self.send_bytes(status, json.dumps(body, ensure_ascii=False).encode("utf-8"), "application/json; charset=utf-8")

    def do_GET(self) -> None:  # noqa: N802
        parsed = urlparse(self.path)
        query = parse_qs(parsed.query)
        state = self.state
        try:
            if parsed.path == "/static/stadmagic-mark.svg":
                self.send_bytes(200, (HUB_DIR / "static" / "stadmagic-mark.svg").read_bytes(), "image/svg+xml; charset=utf-8")
            elif parsed.path == "/static/hub.css":
                self.send_bytes(200, (HUB_DIR / "static" / "hub.css").read_bytes(), "text/css; charset=utf-8")
            elif parsed.path in {"/static/arena-visualizer.css", "/static/arena-visualizer.js"}:
                asset = "arena-visualizer.css" if parsed.path.endswith(".css") else "arena-visualizer.js"
                content_type = "text/css; charset=utf-8" if asset.endswith(".css") else "application/javascript; charset=utf-8"
                self.send_bytes(200, (HUB_DIR / "static" / asset).read_bytes(), content_type)
            elif parsed.path == "/" or parsed.path == "/index.html":
                self.send_bytes(200, home_html(state), "text/html; charset=utf-8")
            elif parsed.path == "/arena":
                self.send_bytes(200, arena_visualizer_html(), "text/html; charset=utf-8")
            elif parsed.path == "/docs":
                self.send_bytes(200, docs_html(), "text/html; charset=utf-8")
            elif parsed.path == "/leaderboard":
                self.send_bytes(200, leaderboard_html(), "text/html; charset=utf-8")
            elif parsed.path == "/register":
                self.send_bytes(200, register_html(), "text/html; charset=utf-8")
            elif parsed.path == "/worlds":
                self.send_bytes(200, worlds_html(), "text/html; charset=utf-8")
            elif parsed.path in {"/docs/api", "/docs/world"}:
                file_name = "api.md" if parsed.path.endswith("api") else "world-rules.md"
                source = (DOCS_DIR / file_name).read_text(encoding="utf-8")
                self.send_bytes(200, page("Документация", f'<article class=card>{markdown_html(source)}</article>'), "text/html; charset=utf-8")
            elif parsed.path in {"/api/docs/api", "/api/docs/world"}:
                file_name = "api.md" if parsed.path.endswith("api") else "world-rules.md"
                self.send_bytes(200, (DOCS_DIR / file_name).read_bytes(), "text/markdown; charset=utf-8")
            elif parsed.path == "/api/docs/mechanics":
                self.send_bytes(200, (ROOT / "docs" / "mechanics.md").read_bytes(), "text/markdown; charset=utf-8")
            elif parsed.path == "/api/worlds":
                active = state.current_arena()
                worlds = [
                    {**world_config,
                     "active": active.get("world_number") == world_config["world_number"]}
                    for world_config in state.world_configs
                ]
                self.send_json(200, {"active": active, "worlds": worlds})
            elif parsed.path == "/api/votes":
                counts = state.vote_results()
                worlds = [{"world_id": world["id"], "name": world["name"],
                           "votes": counts.get(world["id"], 0)} for world in state.world_configs]
                self.send_json(200, {"total_votes": sum(counts.values()), "worlds": worlds})
            elif parsed.path == "/api/runs":
                world = int(query["world"][0]) if query.get("world") else None
                limit = int(query.get("limit", ["50"])[0])
                offset = int(query.get("offset", ["0"])[0])
                if not 1 <= limit <= 200 or offset < 0:
                    raise ValueError("limit must be 1–200 and offset must be non-negative")
                self.send_json(200, state.store.runs(world, limit, offset))
            elif parsed.path == "/api/leaderboard":
                scope = query.get("scope", ["all"])[0]
                world = int(query["world"][0]) if query.get("world") else None
                run_id = query.get("run_id", [None])[0]
                self.send_json(200, state.store.leaderboard(scope, state.registry.names(), world, run_id))
            elif parsed.path == "/health":
                self.send_json(200, {"status": "ok", "active_arena": state.current_arena(), **state.store.counts()})
            else:
                self.send_json(404, {"error": "not found"})
        except (ValueError, KeyError) as exc:
            self.send_json(400 if isinstance(exc, ValueError) else 404, {"error": str(exc)})
        except (OSError, sqlite3.Error) as exc:
            self.send_json(500, {"error": str(exc)})
        except Exception as exc:
            self.log_error("Unhandled GET request error: %s", repr(exc))
            self.send_json(500, {"error": "internal server error"})

    def do_POST(self) -> None:  # noqa: N802
        path = urlparse(self.path).path
        if path not in {"/api/teams", "/api/votes", "/api/visualizer/move", "/api/visualizer/ticket", "/api/visualizer/lease", "/internal/visualizer/ticket/consume"}:
            self.send_json(404, {"error": "not found"})
            return
        try:
            length = int(self.headers.get("content-length", "0"))
            maximum_body = 16384 if path == "/api/visualizer/move" else 4096
            if not 1 <= length <= maximum_body:
                raise ValueError(f"request body must be between 1 and {maximum_body} bytes")
            body = json.loads(self.rfile.read(length))
            if path == "/api/visualizer/move":
                self.handle_visualizer_move(body)
                return
            if path == "/api/visualizer/ticket":
                self.handle_visualizer_ticket(body)
                return
            if path == "/api/visualizer/lease":
                self.handle_visualizer_lease(body)
                return
            if path == "/internal/visualizer/ticket/consume":
                self.handle_consume_visualizer_ticket(body)
                return
            if path == "/api/teams":
                name = str(body.get("name", ""))
                if body.get("token"):
                    token = str(body["token"])
                    if self.state.registry.resolve_token(token) is None:
                        self.send_json(403, {"error": "token is not registered"})
                        return
                    result = self.state.registry.register(token, name)
                else:
                    result = self.state.registry.create(name)
            else:
                token = str(body.get("token", ""))
                team_id = self.state.registry.resolve_token(token)
                if team_id is None:
                    self.send_json(403, {"error": "token is not registered"})
                    return
                world_id = str(body.get("world_id", ""))
                counts = self.state.set_vote(team_id, world_id)
                result = {"world_id": world_id, "votes": counts.get(world_id, 0),
                          "total_votes": sum(counts.values())}
            self.send_json(200, result)
        except (ValueError, json.JSONDecodeError, UnicodeDecodeError) as exc:
            self.send_json(400, {"error": str(exc)})
        except OSError as exc:
            self.send_json(500, {"error": str(exc)})

    def handle_visualizer_ticket(self, body: Any) -> None:
        if not isinstance(body, dict) or body:
            raise ValueError("ticket request body must be an empty JSON object")
        supplied_token = str(self.headers.get("X-Auth-Token", "")).strip()
        if supplied_token:
            team_id = self.state.registry.resolve_token(supplied_token)
            if team_id is None:
                self.send_json(401, {"error": "unknown team token"})
                return
            player_id = team_id
            mode = "player"
            name = self.state.registry.names().get(team_id, "Команда")
        else:
            player_id = "__dats_observer__"
            mode = "observer"
            name = "Наблюдатель"
        arena = self.state.current_arena()
        if arena.get("status") != "running" or not arena.get("run_id"):
            self.send_json(503, {"error": "active arena is not ready"})
            return
        ticket = secrets.token_urlsafe(32)
        now = time.time()
        with self.state.lock:
            self.state.realtime_tickets[ticket] = {
                "player_id": player_id, "name": name, "mode": mode,
                "run_id": arena["run_id"], "expires_at": now + 10,
            }
            self.state.realtime_tickets = {
                key: value for key, value in self.state.realtime_tickets.items()
                if value["expires_at"] > now
            }
        base = str(arena["url"])
        ws_base = base.replace("https://", "wss://", 1).replace("http://", "ws://", 1)
        self.send_json(200, {"ticket": ticket, "websocketUrl": f"{ws_base}/stream/visualizer",
                             "expiresInMs": 10000, "mode": mode, "name": name})

    def handle_consume_visualizer_ticket(self, body: Any) -> None:
        if self.headers.get("X-Arena-Control-Token") != self.state.control_token:
            self.send_json(401, {"error": "unauthorized"})
            return
        ticket = str(body.get("ticket", "")) if isinstance(body, dict) else ""
        now = time.time()
        with self.state.lock:
            claims = self.state.realtime_tickets.pop(ticket, None)
            active_run_id = self.state.arena.get("run_id")
        if not claims or claims["expires_at"] <= now or claims["run_id"] != active_run_id:
            self.send_json(401, {"error": "ticket expired, consumed, or for inactive arena"})
            return
        self.send_json(200, {key: claims[key] for key in ("player_id", "name", "mode", "run_id")})

    def handle_visualizer_lease(self, body: Any) -> None:
        if not isinstance(body, dict):
            raise ValueError("request body must be a JSON object")
        token = str(self.headers.get("X-Auth-Token", "")).strip()
        team_id = self.state.registry.resolve_token(token) if token else None
        if team_id is None:
            self.send_json(401, {"error": "unknown team token"})
            return
        lease_path = (Path(os.environ["DATS_MANUAL_CONTROL_FILE"]) if os.environ.get("DATS_MANUAL_CONTROL_FILE") else
                      ROOT / "lib" / "arena-bots" / "rust_bytadaniel" / f"manual_control_{token_id(token)}.json")
        release_id = str(body.get("releaseLeaseId", ""))
        if release_id:
            try:
                current = json.loads(lease_path.read_text(encoding="utf-8"))
                if current.get("leaseId") == release_id:
                    lease_path.unlink(missing_ok=True)
            except (OSError, json.JSONDecodeError):
                pass
            self.send_json(200, {"status": "released"})
            return
        carpet_id = body.get("carpetId")
        lease_id = str(body.get("leaseId", ""))
        suffix = carpet_id.removeprefix(f"{team_id}_") if isinstance(carpet_id, str) else ""
        if not suffix.isdecimal() or not lease_id or len(lease_id) > 100:
            self.send_json(403, {"error": "manual control is only allowed for your own carpet"})
            return
        if lease_path.is_file():
            try:
                current = json.loads(lease_path.read_text(encoding="utf-8"))
                if int(current.get("expiresAtUnixMs", 0)) > int(time.time() * 1000) and current.get("leaseId") != lease_id:
                    self.send_json(409, {"error": "this team already has an active manual-control session"})
                    return
            except (OSError, ValueError, json.JSONDecodeError):
                pass
        lease = {"carpetId": carpet_id, "leaseId": lease_id, "expiresAtUnixMs": int(time.time() * 1000) + 1500}
        lease_path.parent.mkdir(parents=True, exist_ok=True)
        temporary_path = lease_path.with_name(f".{lease_path.name}.{uuid.uuid4().hex}.tmp")
        temporary_path.write_text(json.dumps(lease), encoding="utf-8")
        os.replace(temporary_path, lease_path)
        self.send_json(200, {"status": "renewed", "expiresInMs": 1500})

    def handle_visualizer_move(self, body: Any) -> None:
        if not isinstance(body, dict):
            raise ValueError("request body must be a JSON object")
        if "token" in body:
            raise ValueError("token must be supplied in the X-Auth-Token header")
        commands = body.get("transports", [])
        if not isinstance(commands, list) or len(commands) > 64:
            raise ValueError("transports must be an array with at most 64 commands")
        supplied_token = str(self.headers.get("X-Auth-Token", "")).strip()
        observer = not supplied_token
        if observer:
            if commands or any(key in body for key in ("manualCarpetId", "leaseId", "releaseLeaseId")):
                self.send_json(403, {"error": "observer mode is read-only"})
                return
            token = self.state.observer_token
            team_id = None
        else:
            token = supplied_token
            team_id = self.state.registry.resolve_token(token)
            if team_id is None:
                self.send_json(401, {"error": "unknown team token"})
                return
        normalized_commands = []
        for command in commands:
            if not isinstance(command, dict) or not isinstance(command.get("id"), str):
                raise ValueError("each transport command must contain a string id")
            acceleration = command.get("acceleration")
            if not isinstance(acceleration, dict):
                raise ValueError("each transport command must contain acceleration")
            x, y = float(acceleration.get("x", 0)), float(acceleration.get("y", 0))
            if not math.isfinite(x) or not math.isfinite(y):
                raise ValueError("acceleration components must be finite numbers")
            normalized_commands.append({"id": command["id"], "acceleration": {"x": x, "y": y}})

        lease_id = str(body.get("leaseId", ""))
        manual_id = body.get("manualCarpetId")
        lease_path = None
        if team_id is not None:
            configured_lease_path = os.environ.get("DATS_MANUAL_CONTROL_FILE")
            lease_path = (Path(configured_lease_path) if configured_lease_path else
                          ROOT / "lib" / "arena-bots" / "rust_bytadaniel" / f"manual_control_{token_id(token)}.json")
        if manual_id is not None:
            assert team_id is not None and lease_path is not None
            suffix = manual_id.removeprefix(f"{team_id}_") if isinstance(manual_id, str) else ""
            if not suffix.isdecimal() or not lease_id or len(lease_id) > 100:
                self.send_json(403, {"error": "manual control is only allowed for your own carpet"})
                return
            if lease_path.is_file():
                try:
                    current_lease = json.loads(lease_path.read_text(encoding="utf-8"))
                    if (int(current_lease.get("expiresAtUnixMs", 0)) > int(time.time() * 1000)
                            and current_lease.get("leaseId") != lease_id):
                        self.send_json(409, {"error": "this team already has an active manual-control session"})
                        return
                except (OSError, ValueError, json.JSONDecodeError):
                    pass
            lease = {"carpetId": manual_id, "leaseId": lease_id,
                     "expiresAtUnixMs": int(time.time() * 1000) + 1000}
            lease_path.parent.mkdir(parents=True, exist_ok=True)
            temporary_path = lease_path.with_name(f".{lease_path.name}.{uuid.uuid4().hex}.tmp")
            temporary_path.write_text(json.dumps(lease), encoding="utf-8")
            os.replace(temporary_path, lease_path)

        try:
            arena_url = f"http://{ARENA_HOST}:{ARENA_PORT}/play/magcarp/player/move"
            upstream_body = json.dumps({"transports": normalized_commands}).encode("utf-8")
            request = urllib.request.Request(
                arena_url, data=upstream_body,
                headers={"X-Auth-Token": token, "Content-Type": "application/json", "Accept": "application/json"},
                method="POST",
            )
            request.add_header("Accept-Encoding", "gzip")
            with urllib.request.urlopen(request, timeout=2.0) as response:
                payload = response.read()
                content_type = response.headers.get("Content-Type", "application/json; charset=utf-8")
                content_encoding = response.headers.get("Content-Encoding")
                client_accepts_gzip = any(
                    encoding.split(";", 1)[0].strip().lower() == "gzip"
                    and not any(
                        parameter.strip().lower() in {"q=0", "q=0.0", "q=0.00"}
                        for parameter in encoding.split(";")[1:]
                    )
                    for encoding in self.headers.get("Accept-Encoding", "").split(",")
                )
                if content_encoding and content_encoding.lower() == "gzip" and not client_accepts_gzip:
                    payload = gzip.decompress(payload)
                    content_encoding = None
                self.send_bytes(response.status, payload, content_type, content_encoding)
        except urllib.error.HTTPError as exc:
            payload = exc.read()
            self.send_bytes(exc.code, payload or json.dumps({"error": "arena rejected request"}).encode("utf-8"),
                            exc.headers.get("Content-Type", "application/json; charset=utf-8"),
                            exc.headers.get("Content-Encoding"))
        except (urllib.error.URLError, TimeoutError, OSError) as exc:
            self.send_json(502, {"error": f"active arena is unavailable: {exc}"})
        finally:
            release_id = str(body.get("releaseLeaseId", ""))
            if release_id and lease_path is not None and lease_path.is_file():
                try:
                    current = json.loads(lease_path.read_text(encoding="utf-8"))
                    if current.get("leaseId") == release_id:
                        lease_path.unlink()
                except (OSError, json.JSONDecodeError):
                    pass


async def serve() -> None:
    world_catalog = load_world_catalog()
    if RUN_SECONDS <= 0 or POLL_SECONDS <= 0:
        raise RuntimeError("HUB_RUN_SECONDS and HUB_POLL_SECONDS must be > 0")
    if ARENA_LIFECYCLE_MODE not in {"process", "runtime-api"}:
        raise RuntimeError("ARENA_LIFECYCLE_MODE must be 'process' or 'runtime-api'")
    if ARENA_LIFECYCLE_MODE == "runtime-api" and not ARENA_CONTROL_TOKEN:
        raise RuntimeError("ARENA_CONTROL_TOKEN must be configured for runtime-api mode")
    if ARENA_LIFECYCLE_MODE == "process" and not ARENA_BIN.is_file():
        raise RuntimeError(f"Arena executable not found: {ARENA_BIN}. Build it with scripts/run_hub.sh")
    state = HubState(load_world_configs(world_catalog))
    handler = type("BoundRequestHandler", (RequestHandler,), {"state": state})
    httpd = ThreadingHTTPServer((HUB_HOST, HUB_PORT), handler)
    threading.Thread(target=httpd.serve_forever, name="hub-http", daemon=True).start()
    print(f"StadMagic Hub: http://{HUB_HOST}:{HUB_PORT} · arena={ARENA_HOST}:{ARENA_PORT} · {len(world_catalog)} worlds · run={RUN_SECONDS}s")
    supervisor = asyncio.create_task(run_arena_loop(state), name="arena-supervisor")
    try:
        await supervisor
    finally:
        httpd.shutdown()
        httpd.server_close()


if __name__ == "__main__":
    try:
        asyncio.run(serve())
    except KeyboardInterrupt:
        print("StadMagic Hub stopped")
    except Exception as exc:
        print(f"StadMagic Hub startup error: {exc}", file=sys.stderr)
        raise
