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
    # Keep the web/API shape stable across process and runtime-api modes. The
    # arena server owns effective gameplay values; the checked-in catalog owns
    # human-facing identity and copy.
    normalized: list[dict[str, Any]] = []
    for index, (metadata, runtime_world) in enumerate(zip(world_catalog, worlds), start=1):
        if not isinstance(runtime_world, dict):
            raise RuntimeError(f"server returned an invalid world at index {index - 1}")
        runtime_config = runtime_world.get("config")
        catalog_config = metadata.get("config")
        normalized.append({
            **runtime_world,
            "id": metadata["id"],
            "name": metadata.get("name") or f"Мир {index:02d}",
            "description": metadata.get("description") or "",
            "world_number": index,
            "config": runtime_config if isinstance(runtime_config, dict) else catalog_config,
        })
    return normalized


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

    def public_teams(self) -> list[dict[str, str]]:
        with self.lock:
            names = sorted((team["name"] for team in self.teams.values()), key=str.casefold)
        return [{"name": name} for name in names]

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
                    carpet_count INTEGER,
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
            if "carpet_count" not in columns:
                self.db.execute("ALTER TABLE runs ADD COLUMN carpet_count INTEGER")
            if "world_id" not in columns:
                self.db.execute("PRAGMA foreign_keys=OFF")
                self.db.executescript(
                    """
                    CREATE TABLE runs_new (
                        id TEXT PRIMARY KEY, world_number INTEGER NOT NULL,
                        world_id TEXT NOT NULL, world_name TEXT NOT NULL,
                        world_occurrence INTEGER NOT NULL DEFAULT 1,
                        run_number INTEGER NOT NULL DEFAULT 1,
                        carpet_count INTEGER,
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
                     carpet_count, arena_name, port, started_at, ended_at, status, error)
                    SELECT id, world_number, 'legacy-world-' || world_number,
                     'Мир ' || world_number, {occurrence}, {run_number},
                     NULL,
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
            config = world.get("config") if isinstance(world.get("config"), dict) else {}
            carpet_count = max(1, int(config.get("carpet_count", world.get("carpet_count", 5))))
            world_occurrence = self.db.execute(
                "SELECT count(*) + 1 FROM runs WHERE world_id=?", (world_id,)
            ).fetchone()[0]
            arena_name = f"world_{world_id}_{world_occurrence}_{run_number}"
            self.db.execute(
                """INSERT INTO runs
                (id, world_number, world_id, world_name, world_occurrence, run_number, carpet_count, arena_name,
                 port, started_at, ended_at, status, error)
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, NULL, 'starting', NULL)""",
                (run_id, world_number, world_id, world_name, world_occurrence, run_number, carpet_count, arena_name, port, time.time()),
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

    def leaderboard(self, scope: str, registry_names: dict[str, str], world_number: int | None = None, run_id: str | None = None, sort_by: str = "gold", period: str = "all") -> dict[str, Any]:
        window_runs = None
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
                carpet_count = int(run_row["carpet_count"] or 0)
                for item in teams:
                    item["gold_per_carpet"] = item["gold"] / carpet_count if carpet_count else None
                    distance = float(item["distance_travelled"] or 0)
                    item["gold_per_distance"] = item["gold"] / distance if distance > 0 else None
                if sort_by == "gold_per_distance":
                    teams = [item for item in teams if item["gold_per_distance"] is not None]
                    teams.sort(key=lambda item: (-item["gold_per_distance"], -item["gold"], item["name"].casefold()))
                else:
                    teams.sort(key=lambda item: (-item["gold"], -item["gold_collected"], item["name"].casefold()))
                for rank, item in enumerate(teams, 1):
                    item["rank"] = rank
                return {"scope": scope, "run": self._run_json(run_row), "teams": teams}
            if scope in {"world", "average"}:
                if scope == "average" and world_number is None:
                    raw_rows = self.db.execute(
                        "SELECT r.id AS run_id, r.world_number, r.world_id, r.world_name, r.arena_name, r.started_at, r.status AS run_status, r.carpet_count, s.* FROM runs r JOIN run_team_stats s ON s.run_id=r.id ORDER BY r.started_at"
                    ).fetchall()
                else:
                    if world_number is None or world_number < 1:
                        raise ValueError("world must be a positive ordinal")
                    if scope == "world" and period == "best":
                        raw_rows = self.db.execute(
                            "SELECT r.id AS run_id, r.world_number, r.world_id, r.world_name, r.arena_name, r.started_at, r.status AS run_status, r.carpet_count, s.* FROM runs r JOIN run_team_stats s ON s.run_id=r.id WHERE r.world_number=? ORDER BY r.run_number",
                            (world_number,),
                        ).fetchall()
                        window_runs = self.db.execute(
                            "SELECT count(*) FROM runs WHERE world_number=?", (world_number,)
                        ).fetchone()[0]
                    elif scope == "world" and period == "last_1":
                        query = "SELECT r.id AS run_id, r.world_number, r.world_id, r.world_name, r.arena_name, r.started_at, r.status AS run_status, r.carpet_count, s.* FROM runs r JOIN run_team_stats s ON s.run_id=r.id WHERE r.id=(SELECT id FROM runs WHERE world_number=? ORDER BY run_number DESC LIMIT 1) ORDER BY r.started_at"
                        raw_rows = self.db.execute(query, (world_number,)).fetchall()
                        window_runs = self.db.execute("SELECT count(*) FROM (SELECT id FROM runs WHERE world_number=? ORDER BY run_number DESC LIMIT 1)", (world_number,)).fetchone()[0]
                    elif scope == "world" and period == "last_10":
                        query = "SELECT r.id AS run_id, r.world_number, r.world_id, r.world_name, r.arena_name, r.started_at, r.status AS run_status, r.carpet_count, s.* FROM runs r JOIN run_team_stats s ON s.run_id=r.id WHERE r.world_number=? AND r.run_number IN (SELECT run_number FROM runs WHERE world_number=? ORDER BY run_number DESC LIMIT 10) ORDER BY r.started_at"
                        raw_rows = self.db.execute(query, (world_number, world_number)).fetchall()
                        window_runs = self.db.execute("SELECT count(*) FROM (SELECT id FROM runs WHERE world_number=? ORDER BY run_number DESC LIMIT 10)", (world_number,)).fetchone()[0]
                    else:
                        raw_rows = self.db.execute(
                            "SELECT r.id AS run_id, r.world_number, r.world_id, r.world_name, r.arena_name, r.started_at, r.status AS run_status, r.carpet_count, s.* FROM runs r JOIN run_team_stats s ON s.run_id=r.id WHERE r.world_number=? ORDER BY r.started_at",
                            (world_number,),
                        ).fetchall()
            elif scope == "all":
                raw_rows = self.db.execute(
                    "SELECT r.id AS run_id, r.world_number, r.world_id, r.world_name, r.arena_name, r.started_at, r.status AS run_status, r.carpet_count, s.* FROM runs r JOIN run_team_stats s ON s.run_id=r.id ORDER BY r.started_at",
                ).fetchall()
            elif scope == "last_10":
                window_runs = self.db.execute(
                    "SELECT count(*) FROM (SELECT run_number FROM runs ORDER BY run_number DESC LIMIT 10)"
                ).fetchone()[0]
                raw_rows = self.db.execute(
                    "SELECT r.id AS run_id, r.world_number, r.world_id, r.world_name, r.arena_name, r.started_at, r.status AS run_status, r.carpet_count, s.* FROM runs r JOIN run_team_stats s ON s.run_id=r.id WHERE r.run_number IN (SELECT run_number FROM runs ORDER BY run_number DESC LIMIT 10) ORDER BY r.started_at"
                ).fetchall()
            else:
                raise ValueError("scope must be run, world, average, last_10, or all")
        grouped: dict[str, list[sqlite3.Row]] = {}
        for row in raw_rows:
            grouped.setdefault(row["team_id"], []).append(row)
        teams = []
        for team_id, attempts in grouped.items():
            best = max(attempts, key=lambda row: (row["gold"], row["gold_collected"], -row["started_at"]))
            total = ({field: best[field] for field in METRICS_FIELDS}
                     if scope == "world" and period == "best"
                     else {field: sum(row[field] for row in attempts) for field in METRICS_FIELDS})
            top = {field: best[field] for field in METRICS_FIELDS}
            per_round = [row["gold"] / row["carpet_count"] for row in attempts if row["carpet_count"]]
            teams.append({
                "team_id": team_id,
                "name": registry_names.get(team_id, best["fallback_name"]),
                "attempts": len(attempts),
                "average_gold_per_carpet": sum(per_round) / len(per_round) if per_round else None,
                "average_rounds": len(per_round),
                "gold_per_distance": (best["gold"] / best["distance_travelled"] if best["distance_travelled"] > 0 else None)
                    if scope == "world" and period == "best"
                    else total["gold"] / total["distance_travelled"] if total["distance_travelled"] > 0 else None,
                "top": top,
                "total": total,
                "best_run": {"run_id": best["run_id"], "world_number": best["world_number"], "world_id": best["world_id"], "world_name": best["world_name"], "started_at": dt.datetime.fromtimestamp(best["started_at"], dt.timezone.utc).isoformat(timespec="seconds")},
            })
        if sort_by == "gold_per_distance":
            teams = [item for item in teams if item["gold_per_distance"] is not None]
            teams.sort(key=lambda item: (-item["gold_per_distance"], -item["total"]["gold"], item["name"].casefold()))
        elif scope == "average":
            teams = [item for item in teams if item["average_gold_per_carpet"] is not None]
            teams.sort(key=lambda item: (-item["average_gold_per_carpet"], -item["total"]["gold"], item["name"].casefold()))
        elif scope == "world" and period == "best":
            teams.sort(key=lambda item: (-item["total"]["gold"], -item["total"]["gold_collected"], item["name"].casefold()))
        else:
            teams.sort(key=lambda item: (-item["total"]["gold"], -item["total"]["gold_collected"], item["name"].casefold()))
        for rank, item in enumerate(teams, 1):
            item["rank"] = rank
        return {"scope": scope, "world_number": world_number, "period": period if scope == "world" else None, "window_runs": window_runs, "teams": teams}

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
            if result.get("status") == "cooldown":
                target = result.get("next_start_at")
                result["seconds_remaining"] = max(0, int((target or time.time()) - time.time()))
            else:
                target = result.get("ends_at")
                result["seconds_remaining"] = max(0, int((target or time.time()) - time.time()))
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
    source = re.sub(r"\A---\s*\n.*?\n---\s*\n", "", source, count=1, flags=re.S)

    def inline(text: str) -> str:
        text = esc(text)
        text = re.sub(r"`([^`]+)`", r"<code>\1</code>", text)
        text = re.sub(r"\*\*([^*]+)\*\*", r"<strong>\1</strong>", text)
        text = re.sub(r"(?<!\*)\*([^*]+)\*(?!\*)", r"<em>\1</em>", text)
        link_pattern = re.compile(r"\[([^\]]+)\]\(([^)\s]+)(?:\s+\"[^\"]*\")?\)")

        def make_link(match: re.Match[str]) -> str:
            label, href = match.group(1), match.group(2)
            if href.lower().startswith(("javascript:", "data:")):
                return label
            safe_href = href if href.startswith(("https://", "http://", "/", "#", "../", "./")) else "#"
            external = safe_href.startswith(("https://", "http://"))
            attrs = ' target="_blank" rel="noopener noreferrer"' if external else ""
            return f'<a href="{esc(safe_href)}"{attrs}>{label}</a>'

        return link_pattern.sub(make_link, text)

    def cells(line: str) -> list[str]:
        return [cell.strip() for cell in line.strip().strip("|").split("|")]

    def is_table_rule(line: str) -> bool:
        return bool(re.fullmatch(r"\s*\|?\s*:?-{3,}:?\s*(\|\s*:?-{3,}:?\s*)+\|?\s*", line))

    output: list[str] = []
    lines = source.splitlines()
    index = 0
    while index < len(lines):
        line = lines[index]
        stripped = line.strip()
        if not stripped:
            index += 1
            continue
        if stripped.startswith("```"):
            language = stripped[3:].strip()
            code: list[str] = []
            index += 1
            while index < len(lines) and not lines[index].strip().startswith("```"):
                code.append(lines[index])
                index += 1
            lang_class = f' class="language-{esc(language)}"' if language else ""
            output.append(f"<pre class=docs-code><code{lang_class}>{esc(chr(10).join(code))}</code></pre>")
            index += 1
            continue
        if line.startswith("|") and index + 1 < len(lines) and is_table_rule(lines[index + 1]):
            headers = cells(line)
            index += 2
            rows: list[list[str]] = []
            while index < len(lines) and lines[index].startswith("|"):
                rows.append(cells(lines[index]))
                index += 1
            table = ['<div class="docs-table-wrap"><table class="docs-table"><thead><tr>']
            table.extend(f"<th scope=col>{inline(value)}</th>" for value in headers)
            table.append("</tr></thead><tbody>")
            for row in rows:
                table.append("<tr>" + "".join(f"<td>{inline(value)}</td>" for value in row) + "</tr>")
            table.append("</tbody></table></div>")
            output.append("".join(table))
            continue
        heading = re.match(r"^(#{1,3})\s+(.+)$", line)
        if heading:
            level, title = len(heading.group(1)), heading.group(2).strip()
            slug = re.sub(r"[^a-z0-9а-яё]+", "-", title.lower()).strip("-")
            output.append(f'<h{level} id="{esc(slug)}">{inline(title)}</h{level}>')
            index += 1
            continue
        if stripped in {"---", "***", "___"}:
            output.append('<hr class="docs-divider">')
            index += 1
            continue
        if stripped.startswith("> "):
            quote: list[str] = []
            while index < len(lines) and lines[index].strip().startswith(">"):
                quote.append(inline(lines[index].strip().removeprefix(">").strip()))
                index += 1
            output.append(f'<blockquote class="docs-quote">{" ".join(quote)}</blockquote>')
            continue
        list_match = re.match(r"^\s*(-|\*)\s+(.+)$", line)
        ordered_match = re.match(r"^\s*\d+[.)]\s+(.+)$", line)
        if list_match or ordered_match:
            ordered = ordered_match is not None
            tag = "ol" if ordered else "ul"
            items: list[str] = []
            while index < len(lines):
                item_match = re.match(r"^\s*(?:-|\*)\s+(.+)$", lines[index]) if not ordered else re.match(r"^\s*\d+[.)]\s+(.+)$", lines[index])
                if not item_match:
                    break
                items.append(f"<li>{inline(item_match.group(1))}</li>")
                index += 1
            output.append(f'<{tag} class="docs-list">{"".join(items)}</{tag}>')
            continue
        paragraph = [stripped]
        index += 1
        while index < len(lines) and lines[index].strip() and not re.match(r"^(#{1,3}\s|\||```|>\s|\s*(?:[-*]\s+|\d+[.)]\s+))", lines[index]):
            paragraph.append(lines[index].strip())
            index += 1
        output.append(f'<p>{inline(" ".join(paragraph))}</p>')
    return "\n".join(output)


STYLE = '<link rel="icon" type="image/svg+xml" href="/static/stadmagic-mark.svg"><link rel="stylesheet" href="/static/hub.css">'


def page(title: str, body: str) -> bytes:
    nav = '<header class="site-header"><a class="brand" href="/" aria-label="StadMagic — на главную"><img src="/static/stadmagic-mark.svg" alt=""><span><strong>STADMAGIC</strong><small>ONLINE GAME JAM</small></span></a><nav class="primary-nav" aria-label="Основная навигация"><a class="nav-arena" href="/arena"><span class="nav-arena-pulse" aria-hidden="true"></span>Арена<span class="nav-live-tag">LIVE</span></a><a href="/register">Команды</a><a href="/leaderboard">Рейтинг</a><a href="/worlds">Миры</a><a href="/docs">Документация</a></nav><div class="nav-actions"><a class="nav-cta nav-source" href="https://github.com/bytadaniel/dats-magic" target="_blank" rel="noopener noreferrer" aria-label="Исходный код StadMagic на GitHub"><svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12 .9a11.1 11.1 0 0 0-3.51 21.63c.55.1.76-.24.76-.53v-2.04c-3.1.68-3.75-1.32-3.75-1.32-.5-1.3-1.24-1.65-1.24-1.65-1.01-.69.08-.68.08-.68 1.12.08 1.71 1.15 1.71 1.15 1 1.7 2.62 1.21 3.26.93.1-.72.39-1.21.71-1.49-2.47-.28-5.06-1.24-5.06-5.5 0-1.22.44-2.21 1.15-2.99-.12-.28-.5-1.42.11-2.95 0 0 .94-.3 3.05 1.14a10.6 10.6 0 0 1 5.55 0c2.12-1.44 3.05-1.14 3.05-1.14.61 1.53.23 2.67.11 2.95.72.78 1.15 1.77 1.15 2.99 0 4.27-2.59 5.21-5.07 5.49.4.35.76 1.02.76 2.06V22c0 .29.2.63.76.52A11.1 11.1 0 0 0 12 .9Z"/></svg><span>GitHub</span></a><button class="nav-profile" id="nav-profile" type="button">Войти <span aria-hidden="true">↗</span></button></div></header>'
    footer = '<footer class=site-footer>StadMagic · Мир меняется. Команды остаются.</footer>'
    motion = """<div class="page-ambient" aria-hidden="true"><i></i><i></i><i></i></div><dialog class="site-auth-dialog" id="site-auth-dialog"><form id="site-auth-form"><button type="button" class="auth-close" id="site-auth-close" aria-label="Закрыть">×</button><span class="eyebrow">ПРОФИЛЬ КОМАНДЫ</span><h2>Подключить команду</h2><p>Введи токен один раз. Он сохранится только в этом браузере и будет подставляться для голосования и игры.</p><label for="site-auth-token">Токен команды</label><input id="site-auth-token" type="password" autocomplete="current-password" required placeholder="Вставь токен команды"><p id="site-auth-status" class="auth-status" role="status" aria-live="polite"></p><div class="auth-actions"><button type="submit" id="site-auth-submit">Войти</button><button type="button" class="secondary" id="site-auth-logout" hidden>Выйти</button></div></form></dialog><script>
      (()=>{const profile=document.querySelector('#nav-profile'),dialog=document.querySelector('#site-auth-dialog'),form=document.querySelector('#site-auth-form'),tokenInput=document.querySelector('#site-auth-token'),status=document.querySelector('#site-auth-status'),logout=document.querySelector('#site-auth-logout');const read=key=>{try{return localStorage.getItem(key)||''}catch(_){return ''}};const syncProfile=()=>{const name=read('stadmagic-team-name'),token=read('stadmagic-team-token');if(profile){profile.firstChild.textContent=name||'Войти ';profile.classList.toggle('has-team',Boolean(token&&name));profile.setAttribute('aria-label',token&&name?`Команда ${name}`:'Войти с токеном команды')}if(tokenInput&&!dialog.open)tokenInput.value=token;if(logout)logout.hidden=!token};syncProfile();if(profile)profile.addEventListener('click',event=>{event.preventDefault();syncProfile();dialog.showModal()});document.querySelector('#site-auth-close').addEventListener('click',()=>dialog.close());dialog.addEventListener('click',event=>{if(event.target===dialog)dialog.close()});logout.addEventListener('click',()=>{try{localStorage.removeItem('stadmagic-team-token');localStorage.removeItem('stadmagic-team-name')}catch(_){}tokenInput.value='';window.dispatchEvent(new Event('stadmagic-profile-change'));status.textContent='Команда отключена в этом браузере.';syncProfile()});form.addEventListener('submit',async event=>{event.preventDefault();const token=tokenInput.value.trim(),submit=document.querySelector('#site-auth-submit');submit.disabled=true;status.textContent='Проверяем токен…';try{const response=await fetch('/api/teams/me',{headers:{'X-Auth-Token':token}}),data=await response.json();if(!response.ok)throw Error(data.error||'Токен не принят');try{localStorage.setItem('stadmagic-team-token',token);localStorage.setItem('stadmagic-team-name',data.name)}catch(_){throw Error('Браузер запретил сохранить профиль. Разреши локальное хранилище и повтори попытку.')}window.dispatchEvent(new Event('stadmagic-profile-change'));status.textContent=`Ты вошёл как «${data.name}».`;syncProfile();setTimeout(()=>dialog.close(),500)}catch(error){status.textContent=error.message}finally{submit.disabled=false}});window.addEventListener('storage',syncProfile);window.addEventListener('stadmagic-profile-change',syncProfile);const stored=read('stadmagic-team-token');if(stored)fetch('/api/teams/me',{headers:{'X-Auth-Token':stored}}).then(async response=>{if(!response.ok)throw Error('invalid token');const data=await response.json();try{localStorage.setItem('stadmagic-team-name',data.name)}catch(_){}window.dispatchEvent(new Event('stadmagic-profile-change'))}).catch(()=>{try{localStorage.removeItem('stadmagic-team-token');localStorage.removeItem('stadmagic-team-name')}catch(_){}syncProfile()});const ambient=document.querySelector('.page-ambient');let frame=0,point=null;window.addEventListener('pointermove',event=>{point={x:event.clientX,y:event.clientY};if(frame)return;frame=requestAnimationFrame(()=>{ambient.style.setProperty('--pointer-x',`${point.x}px`);ambient.style.setProperty('--pointer-y',`${point.y}px`);frame=0})},{passive:true});const targets=document.querySelectorAll('main .card,main > section:not(.card),main > details,main > .home-resources,.arena-context,.visualizer-layout');if(!('IntersectionObserver'in window)){targets.forEach(node=>node.classList.add('scroll-reveal','is-visible'));return}targets.forEach(node=>node.classList.add('scroll-reveal'));document.body.classList.add('scroll-motion-ready');const observer=new IntersectionObserver(entries=>{for(const entry of entries){if(entry.isIntersecting){entry.target.classList.add('is-visible');observer.unobserve(entry.target)}}},{threshold:.08,rootMargin:'0px 0px -36px 0px'});targets.forEach(node=>observer.observe(node))})();
    </script>"""
    interactions = """<script>
      (()=>{
        const pending=new WeakMap();
        document.addEventListener('click',event=>{
          const summary=event.target.closest('details > summary');if(!summary)return;
          const details=summary.parentElement,content=details.querySelector(':scope > .disclosure-content');if(!content)return;
          event.preventDefault();const reduced=matchMedia('(prefers-reduced-motion: reduce)').matches;
          const previous=pending.get(details);
          if(previous){clearTimeout(previous.timer);content.removeEventListener('transitionend',previous.finish);pending.delete(details);details.classList.remove('disclosure-closing');return}
          if(!details.open){details.open=true;details.classList.add('disclosure-closing');requestAnimationFrame(()=>requestAnimationFrame(()=>details.classList.remove('disclosure-closing')));return}
          if(reduced){details.open=false;details.classList.remove('disclosure-closing');return}
          const finish=event=>{if(event&&(event.target!==content||event.propertyName!=='grid-template-rows'))return;const active=pending.get(details);if(active){clearTimeout(active.timer);content.removeEventListener('transitionend',active.finish);pending.delete(details)}details.open=false;details.classList.remove('disclosure-closing')};
          const timer=setTimeout(()=>finish(),430);pending.set(details,{timer,finish});details.classList.add('disclosure-closing');content.addEventListener('transitionend',finish);
        });
      })();
    </script>"""
    return (f"<!doctype html><html lang=ru><head><meta charset=utf-8><meta name=viewport content='width=device-width, initial-scale=1'><meta name=theme-color content='#08111d'><title>{esc(title)}</title>{STYLE}</head><body>{nav}<main class=page-container>{body}</main>{footer}{motion}{interactions}</body></html>").encode("utf-8")


def home_html(state: HubState) -> bytes:
    arena = state.current_arena()
    body = """
    <section class="card hero hero-interactive"><span class=eyebrow><i class=live-dot></i>Живая арена StadMagic</span><p class="hero-format">БЕССРОЧНЫЙ · СОРЕВНОВАТЕЛЬНЫЙ · ОНЛАЙН-ГЕЙМТОН</p><h1>Запусти бота.<br>Войди в легенду.</h1><p>Собери команду ковров и отправь своего бота в живую гонку за золотом. Миры сменяются, соревнование не заканчивается — подключиться и побороться за рейтинг можно в любой момент. Или начни с наблюдения.</p><div class=hero-actions><a class="button hero-primary" href="/register"><span><strong>Создать команду</strong><small>Получить токен и выйти на арену</small></span><b aria-hidden=true>↗</b></a><a class="button secondary" href="/arena">Смотреть гонку</a></div><a class="hero-docs-link" href="/docs/world">Как играть и подключить бота <span aria-hidden="true">↗</span></a>
      <div class="metrics arena-metrics"><div class="metric arena-metric-world"><small>Текущий мир</small><strong id=home-world>__WORLD_NAME__</strong></div><div class="metric arena-metric-status"><small>Статус арены</small><strong><span id=home-status class="phase-badge phase-starting">Подключение…</span></strong></div><div class="metric arena-metric-players"><small>Игроков в матче</small><strong id=home-players>—</strong></div><div class="metric arena-metric-countdown"><small>До смены мира</small><strong id=home-countdown>—:—</strong></div></div>
    </section>
    <section class="card home-podium" aria-labelledby="home-podium-title">
      <div class="home-podium-heading"><div><span class="eyebrow">РЕЙТИНГ КОМАНД</span><h2 id="home-podium-title">Кто сейчас впереди?</h2><p id="home-podium-subtitle">Суммарные результаты команд за всю историю</p></div><div class="podium-switch" role="group" aria-label="Период рейтинга"><button type="button" class="podium-scope" data-scope="current" aria-pressed="false">Текущая арена</button><button type="button" class="podium-scope is-active" data-scope="all" aria-pressed="true">За всё время</button></div></div>
      <div class="home-podium-meta" id="home-podium-meta" aria-live="polite">Загружаем рейтинг…</div><div class="home-top-teams" id="home-top-teams" aria-live="polite"><div class="podium-message">Загружаем…</div></div><a class="podium-full-link" id="home-podium-full-link" href="/leaderboard?scope=all">Открыть полный рейтинг <span aria-hidden="true">↗</span></a>
    </section>
    <section class="home-live-observer" aria-labelledby="home-live-title">
      <div class="home-live-heading"><div><span class="eyebrow"><i class="live-dot"></i> ПУБЛИЧНОЕ НАБЛЮДЕНИЕ</span><h2 id="home-live-title">Арена — прямо сейчас</h2><p><span id="home-live-world">Загружаем мир…</span><span class="home-live-separator">·</span><span id="home-live-state">Подключаемся</span></p></div><span class="home-live-tag"><i></i> LIVE</span></div>
      <div class="home-live-screen"><iframe src="/arena?mode=observer&amp;embed=1" title="Живая арена StadMagic — публичное наблюдение" loading="lazy" referrerpolicy="same-origin"></iframe><span class="home-live-vignette" aria-hidden="true"></span><div class="home-screen-hud home-screen-hud-top" aria-hidden="true"><span class="home-screen-brand"><i>✦</i><span><b>STADMAGIC</b><small>ARENA FEED · 01</small></span></span><span class="home-screen-live"><i></i> LIVE <span>·</span> НАБЛЮДАТЕЛЬ</span></div><div class="home-screen-hud home-screen-hud-bottom" aria-hidden="true"><span>КОВРЫ <i>·</i> ЗОЛОТО <i>·</i> АНОМАЛИИ</span><span class="home-screen-crosshair">⌖</span></div></div>
      <div class="home-live-footer"><p>Смотри, как команды собирают золото и лавируют среди аномалий. На карте видны все участники — подключаться не нужно.</p><a class="home-arena-cta" href="/arena"><span class="home-arena-cta-icon" aria-hidden="true">↗</span><span><b>Открыть арену</b><small>Следить за командами или подключить свою</small></span><i aria-hidden="true">→</i></a></div>
    </section>
    <section class="card game-overview"><div class="game-overview-copy"><span class=eyebrow>Что это за игра?</span><h2>Программные пилоты.<br>Живая гонка без финального свистка.</h2><p class="game-lead"><b>StadMagic — бессрочный соревновательный онлайн-геймтон.</b> Здесь код становится пилотом: ты создаёшь команду и отправляешь бота в общую гонку, которая продолжается, пока сменяются арены.</p><p>Твой флот — ковры-самолёты. Они скользят по пустыне, маневрируют среди аномалий и подбирают золото. Синие поля выталкивают, красные затягивают к центру — ошибка в движении может стоить маршрута и ковра.</p><p>Условия каждый раз другие: меняются размер карты, трение, запас золота, скорость и сила аномалий. Можно наблюдать за соперниками или подключить своего бота и проверить идею в живом матче.</p><div class="game-loop"><span><i>01</i><b>Создай команду</b><small>получи игровой токен</small></span><span><i>02</i><b>Запусти бота</b><small>подключись к арене</small></span><span><i>03</i><b>Борись за рейтинг</b><small>собирай золото командой</small></span></div><a class="button secondary" href="/arena">Смотреть живую арену <span aria-hidden="true">↗</span></a></div><div class="home-legend"><span><i class="legend-chip home-gold"></i><b>Золото</b><small>монеты, которые собирают ковры</small></span><span><i class="legend-chip home-carpet"></i><b>Ковёр</b><small>цвет показывает команду</small></span><span><i class="legend-chip home-blue"></i><b>Синяя аномалия</b><small>отталкивает ковры</small></span><span><i class="legend-chip home-red"></i><b>Красная аномалия</b><small>притягивает ковры</small></span><span><i class="legend-chip home-core"></i><b>Ядро</b><small>опасная центральная область</small></span><span><i class="legend-chip home-arrow">→</i><b>V / A / W</b><small>скорость, управление, сила аномалий</small></span></div></section>
    <section class="card inspiration-card"><div><span class=eyebrow>Источник вдохновения</span><h2>С благодарностью к Dats.Team</h2><p>Я вдохновился <a href="https://gamethon.datsteam.dev/datsmagic" target="_blank" rel="noopener noreferrer">DatsMagic от Dats.Team</a> и сделал самостоятельную похожую реализацию. Официальные игровые серверы закрыты, и мне захотелось дать людям возможность ещё немного поиграть в этот мир.</p><p>StadMagic — неофициальный проект, не связанный с Dats.Team. У меня нет к команде претензий и я ничего от неё не требую. Если Dats.Team попросит, я закрою серверы и доступ к игре.</p></div><span class="inspiration-mark" aria-hidden=true>✦</span></section>
    <section class="card about-card"><div><span class=eyebrow>Обо мне · автор проекта</span><h2>Daniel Byta <span class="author-handle">@bytadaniel</span></h2><p>Я backend-разработчик: с 2019 года работаю с Node.js, а также разрабатываю сервисы на Go и Python. Мне интересны системы, где за кодом видно поведение целого мира — так и появился StadMagic: самостоятельная живая площадка для ботов и их авторов.</p><p class="about-community">Участвую в сообществе ClickHouse. В свободном доступе — код игры, устройство сервера и инструменты для подключения собственного бота.</p><div class="about-links"><a class="button github-link" href="https://github.com/bytadaniel/dats-magic" target="_blank" rel="noopener noreferrer"><svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12 .9a11.1 11.1 0 0 0-3.51 21.63c.55.1.76-.24.76-.53v-2.04c-3.1.68-3.75-1.32-3.75-1.32-.5-1.3-1.24-1.65-1.24-1.65-1.01-.69.08-.68.08-.68 1.12.08 1.71 1.15 1.71 1.15 1 1.7 2.62 1.21 3.26.93.1-.72.39-1.21.71-1.49-2.47-.28-5.06-1.24-5.06-5.5 0-1.22.44-2.21 1.15-2.99-.12-.28-.5-1.42.11-2.95 0 0 .94-.3 3.05 1.14a10.6 10.6 0 0 1 5.55 0c2.12-1.44 3.05-1.14 3.05-1.14.61 1.53.23 2.67.11 2.95.72.78 1.15 1.77 1.15 2.99 0 4.27-2.59 5.21-5.07 5.49.4.35.76 1.02.76 2.06V22c0 .29.2.63.76.52A11.1 11.1 0 0 0 12 .9Z"/></svg>Код StadMagic</a><a class="button secondary github-link" href="https://github.com/bytadaniel" target="_blank" rel="noopener noreferrer"><svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12 .9a11.1 11.1 0 0 0-3.51 21.63c.55.1.76-.24.76-.53v-2.04c-3.1.68-3.75-1.32-3.75-1.32-.5-1.3-1.24-1.65-1.24-1.65-1.01-.69.08-.68.08-.68 1.12.08 1.71 1.15 1.71 1.15 1 1.7 2.62 1.21 3.26.93.1-.72.39-1.21.71-1.49-2.47-.28-5.06-1.24-5.06-5.5 0-1.22.44-2.21 1.15-2.99-.12-.28-.5-1.42.11-2.95 0 0 .94-.3 3.05 1.14a10.6 10.6 0 0 1 5.55 0c2.12-1.44 3.05-1.14 3.05-1.14.61 1.53.23 2.67.11 2.95.72.78 1.15 1.77 1.15 2.99 0 4.27-2.59 5.21-5.07 5.49.4.35.76 1.02.76 2.06V22c0 .29.2.63.76.52A11.1 11.1 0 0 0 12 .9Z"/></svg>GitHub автора</a></div></div><span class="inspiration-mark" aria-hidden=true>⌘</span></section>
    <section class="home-resources" aria-label="Дополнительная информация"><a class="resource-link" href="/docs"><span><b>Документация</b><small>Правила мира и контракт игрового API</small></span><strong>Читать →</strong></a><a class="resource-link resource-worlds" href="/worlds"><span><b>Каталог миров</b><small>Профили, особенности и условия арен</small></span><strong>Посмотреть →</strong></a></section>
    <script>
      const homeCountdown=document.querySelector('#home-countdown');let homeDeadline=Date.now(),homeActive={},podiumRequest=0,podiumScope='all';
      function renderHomeCountdown(){const remaining=Math.max(0,Math.ceil((homeDeadline-Date.now())/1000)),minutes=Math.floor(remaining/60),seconds=remaining%60;homeCountdown.textContent=`${minutes}:${String(seconds).padStart(2,'0')}`}
      const podiumNumber=value=>Math.round(Number(value)||0).toLocaleString('ru-RU');
      function podiumDistance(value){return (Math.max(0,Number(value)||0)/1000).toLocaleString('ru-RU',{minimumFractionDigits:1,maximumFractionDigits:1})}
      function podiumStat(icon,value,label){const stat=document.createElement('span');stat.className='podium-stat';const mark=document.createElement('i');mark.setAttribute('aria-hidden','true');mark.textContent=icon;const copy=document.createElement('span');const amount=document.createElement('b');amount.textContent=value;const caption=document.createElement('small');caption.textContent=label;copy.append(amount,caption);stat.append(mark,copy);return stat}
      function makePodiumCard(team,rank){const row=document.createElement(team?'a':'div');row.className=`podium-team podium-place-${rank}${team?'':' podium-placeholder'}`;if(team){row.href=podiumScope==='current'?`/leaderboard?scope=history&run_id=${encodeURIComponent(homeActive.run_id)}`:'/leaderboard?scope=all';row.setAttribute('aria-label',`Место ${rank}: ${team.name}, ${podiumNumber(podiumScope==='current'?team.gold:team.total?.gold)} золота, ${podiumDistance(podiumScope==='current'?team.distance_travelled:team.total?.distance_travelled)} километров пройдено`)}else row.setAttribute('aria-label',`Место ${rank}: ожидает команду`);const badge=document.createElement('span');badge.className='podium-rank';badge.textContent=String(rank).padStart(2,'0');const identity=document.createElement('span');identity.className='podium-team-identity';const name=document.createElement('strong');name.className='podium-team-name';name.textContent=team?.name||'Место свободно';const note=document.createElement('small');note.textContent=team?(rank===1?'Лидер гонки':`Место в рейтинге · ${rank}`):'Ожидаем участника';identity.append(name,note);const metrics=team?(podiumScope==='current'?team:team.total||{}):{};const values=document.createElement('span');values.className='podium-values';values.append(podiumStat('✦',team?podiumNumber(metrics.gold):'—','золото'),podiumStat('↗',team?podiumDistance(metrics.distance_travelled):'—','пройдено, км'));const arrow=document.createElement('span');arrow.className='podium-card-arrow';arrow.setAttribute('aria-hidden','true');arrow.textContent='↗';row.append(badge,identity,values,arrow);return row}
      async function refreshHomePodium(animate=false){const request=++podiumRequest,host=document.querySelector('#home-top-teams'),meta=document.querySelector('#home-podium-meta'),subtitle=document.querySelector('#home-podium-subtitle'),fullLink=document.querySelector('#home-podium-full-link');document.querySelectorAll('.podium-scope').forEach(button=>{const active=button.dataset.scope===podiumScope;button.classList.toggle('is-active',active);button.setAttribute('aria-pressed',String(active))});subtitle.textContent=podiumScope==='current'?'Сохранённое золото в текущей арене':'Суммарные результаты команд за всю историю';fullLink.href=podiumScope==='current'?(homeActive.run_id?`/leaderboard?scope=history&run_id=${encodeURIComponent(homeActive.run_id)}`:'/leaderboard?scope=current'):'/leaderboard?scope=all';try{if(podiumScope==='current'&&!homeActive.run_id){meta.textContent='Команд в рейтинге: 0';host.replaceChildren(...[1,2,3].map(rank=>makePodiumCard(null,rank)));return}const apiScope=podiumScope==='current'?'run':podiumScope,query=new URLSearchParams({scope:apiScope});if(podiumScope==='current')query.set('run_id',homeActive.run_id);const response=await fetch('/api/leaderboard?'+query),data=await response.json();if(!response.ok)throw Error(data.error||'Не удалось загрузить рейтинг');if(request!==podiumRequest)return;const teams=data.teams||[];meta.textContent=`Команд в рейтинге: ${teams.length}`;const cards=[1,2,3].map(rank=>makePodiumCard(teams[rank-1]||null,rank));if(animate&&!matchMedia('(prefers-reduced-motion: reduce)').matches){try{await host.animate([{opacity:1,transform:'translateY(0)'},{opacity:0,transform:'translateY(5px)'}],{duration:110}).finished}catch(_){}}host.replaceChildren(...cards);if(animate&&!matchMedia('(prefers-reduced-motion: reduce)').matches)host.animate([{opacity:0,transform:'translateY(-4px)'},{opacity:1,transform:'translateY(0)'}],{duration:230,easing:'ease-out'})}catch(error){if(request===podiumRequest){meta.textContent='Не удалось обновить рейтинг';host.replaceChildren(...[1,2,3].map(rank=>makePodiumCard(null,rank)))}}}
      async function refreshHome(){try{const d=await(await fetch('/api/worlds')).json(),a=d.active||{},phases={running:['Матч идёт','phase-running'],cooldown:['Перерыв между аренами','phase-cooldown'],starting:['Арена запускается','phase-starting'],stopping:['Арена завершает работу','phase-cooldown'],error:['Ошибка запуска','phase-error'],idle:['Ожидание арены','phase-starting']},[label,phaseClass]=phases[a.status]||['Подготовка арены','phase-starting'],status=document.querySelector('#home-status'),liveState=document.querySelector('#home-live-state');homeActive=a;document.querySelector('#home-world').textContent=a.world_name||'Подготовка арены';document.querySelector('#home-live-world').textContent=a.world_name||'Мир готовится';liveState.textContent=label;liveState.dataset.phase=a.status||'idle';status.textContent=label;status.className=`phase-badge ${phaseClass}`;document.querySelector('#home-players').textContent=Number(a.active_players)||0;homeDeadline=Date.now()+Math.max(0,Number(a.seconds_remaining)||0)*1000;renderHomeCountdown();refreshHomePodium()}catch(_){document.querySelector('#home-status').textContent='Нет связи с ареной';document.querySelector('#home-status').className='phase-badge phase-error';document.querySelector('#home-live-state').textContent='Нет связи с ареной';document.querySelector('#home-live-state').dataset.phase='error' }}
      document.querySelectorAll('.podium-scope').forEach(button=>button.addEventListener('click',()=>{if(podiumScope===button.dataset.scope)return;podiumScope=button.dataset.scope;refreshHomePodium(true)}));refreshHome();setInterval(refreshHome,5000);setInterval(renderHomeCountdown,1000);
    </script>
    """
    values = {
        "__WORLD_NAME__": esc(arena.get("world_name") or "Подготовка арены"),
    }
    for placeholder, value in values.items():
        body = body.replace(placeholder, value)
    return page("StadMagic", body)


def arena_visualizer_html(embed_observer: bool = False) -> bytes:
    body = '''
    <link rel="stylesheet" href="/static/arena-visualizer.css?v=11.0">
    <section class="visualizer-shell">
      <header class="arena-page-heading"><h1>Арена</h1><div id="arena-connection-status" class="arena-connection-status connecting" aria-live="polite"><i aria-hidden="true"></i><span>Подключаемся</span></div></header>
      <section class="arena-context" aria-live="polite">
        <div class="arena-context-head"><div class="arena-context-copy"><span id="arena-phase" class="arena-phase">ЗАГРУЗКА АРЕНЫ</span><h2 id="arena-title">Подключаемся к Hub…</h2><p id="arena-description">Получаем описание текущего мира.</p></div><div class="context-time"><span id="arena-time-label">ДО СМЕНЫ МИРА</span><strong id="arena-countdown" class="countdown-calm">—</strong></div></div>
        <div class="arena-context-details"><div class="arena-context-facts"><span class="context-fact"><span class="context-fact-label"><svg viewBox="0 0 24 24" aria-hidden="true"><path d="M4 5.5 9 3l6 2.5L20 3v15.5L15 21l-6-2.5L4 21zM9 3v15.5m6-13V21"/></svg>Карта</span><b id="arena-size">—</b><small id="arena-size-note" class="context-fact-note"></small></span><span class="context-fact context-fact-gold"><span class="context-fact-label"><svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="12" r="8.5"/><circle cx="12" cy="12" r="5.5"/><path d="M12 8v8m2-6h-3a1.5 1.5 0 0 0 0 3h2a1.5 1.5 0 0 1 0 3h-3"/></svg><span>✦ Золото на карте</span></span><b id="arena-bounties">—</b><small id="arena-bounties-note" class="context-fact-note"></small></span><span class="context-fact"><span class="context-fact-label"><svg viewBox="0 0 24 24" aria-hidden="true"><path d="M13.5 2 5 13h6l-.5 9L19 10h-6z"/><path d="M3 7h2m14 9h2M5 19l1.5-1.5M18 5l1.5-1.5"/></svg>Аномалии</span><b id="arena-anomalies">—</b><small id="arena-anomalies-note" class="context-fact-note"></small></span></div><div class="arena-world-profile" id="arena-world-profile"><span class="arena-profile-label">АНОМАЛИИ · РАЗМЕР ОТНОСИТЕЛЬНО КАРТЫ</span><div id="arena-world-traits" class="arena-world-traits"></div></div><div class="arena-world-rules" aria-label="Правила текущего мира"><span><small>Ковров на команду</small><b id="world-carpet-count">—</b></span><span><small>Макс. скорость ковра</small><b id="world-carpet-speed">—</b></span><span><small>Макс. ускорение</small><b id="world-carpet-acceleration">—</b></span><span><small>Сохраняет скорость</small><b id="world-friction">—</b></span><span><small>Монеты появляются</small><b id="world-coin-spawn" class="world-boolean">—</b></span><span><small>Аномалии появляются</small><b id="world-anomaly-spawn" class="world-boolean">—</b></span><span><small>Респавн ковров</small><b id="world-carpet-respawn" class="world-boolean">—</b></span></div></div>
      </section>
      <details class="arena-voting"><summary><span><b>Выбрать следующий мир</b><small>Голос команды влияет на следующую арену</small></span><em id="arena-vote-total">Загрузка голосов…</em></summary><div class="disclosure-content"><div class="disclosure-inner"><div class="arena-vote-layout"><form id="arena-vote-form"><label>Мир<select id="arena-vote-world" required></select></label><p id="arena-vote-identity" class="vote-identity">Войди через кнопку профиля вверху сайта, чтобы проголосовать.</p><button type="submit">Проголосовать <span aria-hidden="true">→</span></button><small id="arena-vote-status" role="status" aria-live="polite"></small></form><div id="arena-vote-list" class="arena-vote-list"></div></div></div></div></details>
      <section class="visualizer-top card">
        <div class="arena-mode-card" id="arena-mode-card" data-mode="observer" aria-live="polite"><div class="arena-mode-mark" aria-hidden="true"><svg viewBox="0 0 24 24"><path d="M12 3.5 20.5 12 12 20.5 3.5 12 12 3.5Z"/><circle cx="12" cy="12" r="2.4"/></svg></div><div class="arena-mode-copy"><div class="arena-mode-meta"><span class="eyebrow"><i class="live-dot"></i>ЖИВАЯ АРЕНА</span><span id="arena-mode-badge" class="arena-mode-badge">НАБЛЮДАТЕЛЬ</span></div><h1 id="arena-entry-title">Наблюдаешь за ареной</h1><h2 id="arena-entry-name">Публичный просмотр</h2><p id="arena-entry-status">Карта открыта для всех.</p><small id="arena-entry-description">Выбор ковра не включает управление.</small></div><button id="arena-team-connect" class="secondary mode-switch" type="button">Играть <span aria-hidden="true">↗</span></button></div>
      </section>
      <section id="viz-gold-summary" class="viz-gold-summary without-own" aria-label="Золото и смерти команд арены"><div class="viz-gold-own" id="viz-own-summary" hidden><span>ВАША КОМАНДА</span><strong id="viz-own-gold">—</strong><small id="viz-own-name">Золото сейчас</small><small class="viz-own-deaths"><svg class="viz-death-icon" viewBox="0 0 24 24" aria-hidden="true"><path d="M12 3a8 8 0 0 0-8 8c0 3 1.5 4.7 3.4 5.8V21h9.2v-4.2C18.5 15.7 20 14 20 11a8 8 0 0 0-8-8ZM8.5 10h.01M15.5 10h.01M9 14c1.8 1.3 4.2 1.3 6 0M9 18v3m6-3v3"/></svg>Смерти <b id="viz-own-deaths">0</b></small></div><div class="viz-gold-leaders-wrap"><span class="eyebrow">ТОП-3 · ТЕКУЩАЯ АРЕНА</span><div id="viz-gold-leaders" class="viz-gold-leaders" data-count="0"><div class="viz-gold-leader viz-gold-placeholder rank-1" data-place="1"><b class="viz-gold-rank">#1</b><span class="viz-gold-team">Место свободно</span><div class="viz-gold-metrics"><strong class="viz-gold-value">—</strong><small class="viz-gold-deaths"><svg class="viz-death-icon" viewBox="0 0 24 24" aria-hidden="true"><path d="M12 3a8 8 0 0 0-8 8c0 3 1.5 4.7 3.4 5.8V21h9.2v-4.2C18.5 15.7 20 14 20 11a8 8 0 0 0-8-8ZM8.5 10h.01M15.5 10h.01M9 14c1.8 1.3 4.2 1.3 6 0M9 18v3m6-3v3"/></svg>—</small></div></div><div class="viz-gold-leader viz-gold-placeholder rank-2" data-place="2"><b class="viz-gold-rank">#2</b><span class="viz-gold-team">Место свободно</span><div class="viz-gold-metrics"><strong class="viz-gold-value">—</strong><small class="viz-gold-deaths"><svg class="viz-death-icon" viewBox="0 0 24 24" aria-hidden="true"><path d="M12 3a8 8 0 0 0-8 8c0 3 1.5 4.7 3.4 5.8V21h9.2v-4.2C18.5 15.7 20 14 20 11a8 8 0 0 0-8-8ZM8.5 10h.01M15.5 10h.01M9 14c1.8 1.3 4.2 1.3 6 0M9 18v3m6-3v3"/></svg>—</small></div></div><div class="viz-gold-leader viz-gold-placeholder rank-3" data-place="3"><b class="viz-gold-rank">#3</b><span class="viz-gold-team">Место свободно</span><div class="viz-gold-metrics"><strong class="viz-gold-value">—</strong><small class="viz-gold-deaths"><svg class="viz-death-icon" viewBox="0 0 24 24" aria-hidden="true"><path d="M12 3a8 8 0 0 0-8 8c0 3 1.5 4.7 3.4 5.8V21h9.2v-4.2C18.5 15.7 20 14 20 11a8 8 0 0 0-8-8ZM8.5 10h.01M15.5 10h.01M9 14c1.8 1.3 4.2 1.3 6 0M9 18v3m6-3v3"/></svg>—</small></div></div></div></div></section>
      <div class="visualizer-layout">
        <section class="visualizer-stage">
          <div class="viz-toolbar">
            <label class="select-label"><span>Ковёр</span><select id="carpet-select" disabled><option>Подключись к арене</option></select></label>
            <div class="viz-toolbar-actions" aria-label="Управление картой">
              <button id="follow-toggle" class="secondary" disabled title="Центрировать камеру на выбранном ковре">◎ Следить</button>
              <button id="manual-toggle" class="secondary" disabled title="Передать выбранный ковер под ручное управление">Ручное</button>
              <div class="camera-tools" aria-label="Масштаб и обзор"><button id="zoom-out" class="secondary" aria-label="Уменьшить">−</button><button id="zoom-in" class="secondary" aria-label="Увеличить">+</button><button id="camera-reset" class="secondary">Обзор</button><button id="orientation-toggle" class="secondary orientation-toggle" type="button" hidden aria-label="Включить альбомный режим">↻ Альбом</button></div>
              <button id="fullscreen-toggle" class="secondary" aria-label="Полноэкранный режим">⛶</button>
            </div>
          </div>
          <div class="canvas-wrap"><canvas id="arena-canvas" aria-label="Карта арены"></canvas><div id="fps-label" class="viz-fps" aria-live="off">— FPS</div><div id="touch-stick" class="touch-stick" aria-label="Виртуальный стик"><div class="stick-base"><i></i></div><span>тяни для ускорения</span></div><div id="connection-badge" class="connection-badge">Нет подключения</div></div>
          <aside class="arena-pilot-callout" aria-label="Ручное пилотирование и планирование маршрута"><span class="arena-pilot-mark" aria-hidden="true">⌖</span><div class="arena-pilot-copy"><span class="eyebrow">РУЧНОЙ ПОЛЁТ</span><strong>Пилотируй сам. Закрепляй путь до золота.</strong><p>Выбери свой живой ковер и включи «Ручное». Прицел задаёт ускорение; если прогноз проходит через монету, закрепи участок до сбора и добавь следующий.</p><small class="arena-pilot-mobile">Телефон: стик · двойное касание — закрепить · долгое нажатие — отменить</small></div><div class="arena-pilot-keys"><span><kbd>F</kbd> Закрепить участок</span><span><kbd>Z</kbd> Отменить последний</span></div></aside>
        </section>
      </div>
      <details class="viz-help-disclosure">
        <summary><span><b>Как пользоваться визуализацией</b><small>ПК: колесо и перетаскивание · телефон: жесты и стик</small></span></summary>
        <div class="disclosure-content"><div class="disclosure-inner"><section class="viz-help" aria-label="Подсказки управления"><div class="viz-help-heading"><span class="eyebrow">КОРОТКО ОБ УПРАВЛЕНИИ</span><h2>Карта и ручной режим</h2></div><div class="viz-help-grid"><div class="viz-help-item"><i class="help-icon"><svg viewBox="0 0 24 24" aria-hidden="true"><rect x="3" y="4" width="18" height="13" rx="2"/><path d="M8 21h8m-4-4v4M6 8h12"/></svg></i><span><b>Компьютер</b><small>Колесо — зум · перетаскивай карту · клик — выбрать ковер · стрелки — двигать обзор.</small></span></div><div class="viz-help-item"><i class="help-icon"><svg viewBox="0 0 24 24" aria-hidden="true"><rect x="6" y="2" width="12" height="20" rx="2.5"/><path d="M10 5h4m-3 14h2"/></svg></i><span><b>Телефон</b><small>Один палец — карта · два — зум. Стик появляется в ручном режиме.</small></span></div><div class="viz-help-item"><i class="help-icon"><svg viewBox="0 0 24 24" aria-hidden="true"><path d="M5 19 19 5m-8 0h8v8"/><circle cx="5" cy="19" r="2"/></svg></i><span><b>Вести ковер</b><small>Выбери свой ковер и включи ручной режим. Бот уступит управление.</small></span></div></div><div class="viz-help-legend"><h3>Что показывают линии и точки</h3><div class="vector-legend"><span><i class="v-speed"></i>Скорость V</span><span><i class="v-self"></i>Ускорение A</span><span><i class="v-anomaly"></i>Силы аномалий W</span><span><i class="v-trajectory"></i>Будущий путь · 20 с</span><span><i class="v-history"></i>Фактический путь · 10 с</span><span><i class="v-collected"></i>Собранная монета</span></div><h3>Объекты карты</h3><div class="map-guide-items"><span><i class="legend-dot legend-gold"></i>золотая монета — собрать золото</span><span><i class="legend-dot legend-own"></i>жёлтый ковер — ваша команда</span><span><i class="legend-dot legend-team"></i>цветной ковер — другая команда</span><span><i class="legend-ring legend-blue"></i>синяя область — отталкивание</span><span><i class="legend-ring legend-red"></i>красная область — притяжение</span><span><i class="legend-core"></i>залитое ядро аномалии — смертельно опасно</span></div></div></section></div></div>
      </details>
      <details class="world-settings"><summary>Технические параметры мира</summary><div class="disclosure-content"><div class="disclosure-inner"><dl id="arena-world-settings"><div><dt>Загрузка…</dt><dd>—</dd></div></dl></div></div></details>
      <section id="arena-leaderboard" class="card arena-ranking"><div class="arena-ranking-heading"><div><span class="eyebrow">РЕЙТИНГ КОМАНД</span><h2>Зачёты</h2></div><label class="arena-ranking-mode">Категория<select id="arena-ranking-mode"><option value="current">Текущая арена</option><option value="last_10">Последние 10 игр</option><option value="all">За всё время</option></select></label><span id="arena-ranking-updated" class="muted">Обновляется каждые 5 секунд</span></div><p id="arena-ranking-status" class="muted">Загружаю результаты…</p><div id="arena-ranking-table" class="arena-ranking-table"></div><a class="button secondary" href="/leaderboard">Все результаты и история →</a></section>
      <p id="viz-message" class="viz-message" role="status"></p>
      <details class="arena-starter card"><summary><span><b>Запусти первого бота</b><small>Готовый Python-клиент · без сторонних библиотек</small></span></summary><div class="disclosure-content"><div class="disclosure-inner"><p><a href="/register">Создай команду и получи токен</a>, затем выполни:</p><pre><code>git clone https://github.com/bytadaniel/dats-magic.git</code></pre><pre><code>cd dats-magic &amp;&amp; python3 examples/python-starter/main.py</code></pre><p>Токен вводится скрыто и не попадает в историю терминала. <a href="/docs/api">Контракт игрового API →</a> · <a href="https://github.com/bytadaniel/dats-magic">Исходный код на GitHub →</a></p></div></div></details>
    </section>
    <script>
      (()=>{const shell=document.querySelector('.visualizer-shell'),context=shell?.querySelector('.arena-context');if(!shell||!context)return;for(const selector of ['.visualizer-top','.visualizer-layout','#viz-gold-summary','#arena-leaderboard']){const node=shell.querySelector(selector);if(node)shell.insertBefore(node,context)}})();
    </script>
    <script>
      (()=>{const select=document.querySelector('#arena-vote-world'),form=document.querySelector('#arena-vote-form'),list=document.querySelector('#arena-vote-list'),total=document.querySelector('#arena-vote-total'),identity=document.querySelector('#arena-vote-identity'),status=document.querySelector('#arena-vote-status');let previousPositions=new Map();const storedToken=()=>{try{return localStorage.getItem('stadmagic-team-token')||''}catch(_){return ''}};function syncIdentity(){const name=(()=>{try{return localStorage.getItem('stadmagic-team-name')||''}catch(_){return ''}})();identity.textContent=name?`Голос от команды «${name}».`:'Войди через кнопку профиля вверху сайта, чтобы проголосовать.';form.querySelector('button[type=submit]').disabled=!storedToken()}window.addEventListener('stadmagic-profile-change',syncIdentity);window.addEventListener('storage',syncIdentity);syncIdentity();async function refreshVotes(){try{const [worldResponse,voteResponse]=await Promise.all([fetch('/api/worlds'),fetch('/api/votes')]),worldData=await worldResponse.json(),voteData=await voteResponse.json();if(!worldResponse.ok||!voteResponse.ok)throw Error('Не удалось загрузить голосование');const profiles=(Array.isArray(worldData.worlds)?worldData.worlds:[]).map((world,index)=>{const id=[world?.id,world?.world_id].find(value=>typeof value==='string'&&value.trim()&&value!=='undefined')||`world-${index+1}`;const name=[world?.name,world?.display_name,world?.title].find(value=>typeof value==='string'&&value.trim()&&value!=='undefined')||`Мир ${String(index+1).padStart(2,'0')}`;return {...world,id,name}});if(select.options.length!==profiles.length)select.replaceChildren(...profiles.map(world=>new Option(`${world.name} · ${world.id}`,world.id)));const votes=voteData.worlds||[],byId=new Map(votes.map(item=>[item.world_id,item.votes||0]));const ranked=[...profiles].map(world=>({...world,votes:byId.get(world.id)||0})).sort((a,b)=>b.votes-a.votes||a.name.localeCompare(b.name,'ru'));total.textContent=`${voteData.total_votes||0} голосов`;previousPositions=new Map([...list.children].map(row=>[row.dataset.worldId,row.getBoundingClientRect().top]));list.replaceChildren();if(!ranked.length){list.textContent='Нет доступных миров.';return}const max=Math.max(1,...ranked.map(item=>item.votes));for(const [index,world] of ranked.slice(0,6).entries()){const row=document.createElement('div');row.className=`arena-vote-row vote-place-${index+1}`;row.dataset.worldId=world.id;const rank=document.createElement('b');rank.className='vote-rank';rank.textContent=String(index+1).padStart(2,'0');const label=document.createElement('span');label.textContent=world.name;const bar=document.createElement('i');bar.style.setProperty('--vote-fill',`${world.votes/max*100}%`);const count=document.createElement('strong');count.textContent=String(world.votes);row.append(rank,label,bar,count);list.append(row)}for(const row of list.children){const oldTop=previousPositions.get(row.dataset.worldId);if(oldTop===undefined||matchMedia('(prefers-reduced-motion: reduce)').matches)continue;const dy=oldTop-row.getBoundingClientRect().top;if(Math.abs(dy)>1){row.animate([{transform:`translateY(${dy}px)`},{transform:'translateY(0)'}],{duration:520,easing:'cubic-bezier(.2,.75,.25,1)'})}}}catch(error){total.textContent=error.message}}form.addEventListener('submit',async event=>{event.preventDefault();const token=storedToken();if(!token){status.textContent='Сначала войди с токеном команды через профиль вверху сайта.';status.className='error';return}const button=form.querySelector('button[type=submit]');button.disabled=true;status.textContent='Отправляем голос…';try{const response=await fetch('/api/votes',{method:'POST',headers:{'content-type':'application/json','X-Auth-Token':token},body:JSON.stringify({world_id:select.value})}),data=await response.json();if(!response.ok)throw Error(data.error||'Голос не принят');status.textContent='Голос учтён. Его можно изменить до следующего запуска.';status.className='ok';await refreshVotes()}catch(error){status.textContent=error.message;status.className='error'}finally{syncIdentity()}});refreshVotes();setInterval(refreshVotes,10000)})();
    </script>
    <script src="/static/arena-visualizer.js?v=15.0" defer></script>
    '''
    body = body.replace('<details class="arena-voting">', '<details class="arena-voting" open>')
    body = body.replace(
        '<summary><span><b>Выбрать следующий мир</b><small>Голос команды влияет на следующую арену</small></span>',
        '<summary><span class="vote-summary-copy"><span class="vote-summary-closed"><b>Следующий мир · <i id="arena-vote-next">Случайный мир</i></b><small id="arena-vote-summary-note">Пока нет голосов · выбор из каталога</small></span><span class="vote-summary-open"><b>Голосование за следующий мир</b><small>Слева — рейтинг голосов, справа — выбор команды</small></span></span>',
    )
    body = body.replace(
        '<form id="arena-vote-form">',
        '<form id="arena-vote-form"><div class="vote-form-heading"><span class="eyebrow">ТВОЙ ВЫБОР</span><h3>Какой мир следующим?</h3><p>Один голос от команды. Его можно изменить до конца текущей арены.</p></div>',
    )
    countdown = '<div class="context-time"><span id="arena-time-label">ДО СМЕНЫ МИРА</span><strong id="arena-countdown" class="countdown-calm">—</strong></div>'
    body = body.replace(countdown, '')
    body = body.replace(
        '<button id="arena-team-connect"',
        countdown.replace('class="context-time"', 'class="mode-countdown"') + '<button id="arena-team-connect"',
    )
    mode_button = '<button id="arena-team-connect" class="secondary mode-switch" type="button">Играть <span aria-hidden="true">↗</span></button>'
    body = body.replace(
        countdown.replace('class="context-time"', 'class="mode-countdown"') + mode_button,
        '<div class="mode-side-actions">' + mode_button + countdown.replace('class="context-time"', 'class="mode-countdown"') + '</div>',
    )
    body = body.replace(
        '    <script src="/static/arena-visualizer.js?v=15.0" defer></script>',
        '''    <script>
      (()=>{const details=document.querySelector('.arena-voting'),list=document.querySelector('#arena-vote-list'),totalNode=document.querySelector('#arena-vote-total'),next=document.querySelector('#arena-vote-next'),note=document.querySelector('#arena-vote-summary-note'),closed=document.querySelector('.vote-summary-closed'),opened=document.querySelector('.vote-summary-open'),voteWord=count=>({one:'голос',few:'голоса',many:'голосов',other:'голосов'}[new Intl.PluralRules('ru-RU').select(count)]);if(details)details.open=true;const syncDisclosure=()=>{closed.setAttribute('aria-hidden',String(details.open));opened.setAttribute('aria-hidden',String(!details.open))};details.addEventListener('toggle',syncDisclosure);syncDisclosure();const refreshSummary=()=>{const rows=[...list.querySelectorAll('.arena-vote-row')].map(row=>({name:row.children[1]?.textContent||'',votes:Number(row.children[3]?.textContent)||0}));const total=Number((totalNode.textContent.match(/\\d+/)||[])[0])||0;const totalLabel=`${total} ${voteWord(total)}`;if(totalNode.textContent!==totalLabel)totalNode.textContent=totalLabel;if(!total||!rows.length){next.textContent='Случайный мир';note.textContent='Пока нет голосов · выбор из каталога';return}const max=Math.max(...rows.map(row=>row.votes)),leaders=rows.filter(row=>row.votes===max);if(leaders.length===1){next.textContent=leaders[0].name;note.textContent='Лидер голосования'}else{next.textContent='Жребий среди лидеров';note.textContent='Несколько миров набрали поровну'}};if(list){const observer=new MutationObserver(refreshSummary);observer.observe(list,{childList:true,subtree:true,characterData:true});observer.observe(totalNode,{childList:true,characterData:true,subtree:true});refreshSummary()}})();
    </script>
    <script src="/static/arena-visualizer.js?v=15.0" defer></script>''',
    )
    body = body.replace(
        'Колесо — зум · перетаскивай карту · клик — выбрать ковер · стрелки — двигать обзор.',
        'Колесо — зум · перетаскивай карту · клик — выбрать ковер · стрелки — обзор. Ручное: F / средняя кнопка — закрепить сегмент, Z / правая — отменить.',
    ).replace(
        'Один палец — карта · два — зум. Стик появляется в ручном режиме.',
        'Один палец — карта · два — зум. В ручном режиме двойное касание фиксирует сегмент, долгое нажатие отменяет; стик задаёт ускорение.',
    )
    if embed_observer:
        shell_start = body.index('<section class="visualizer-shell">')
        shell_end = body.rfind("</section>") + len("</section>")
        shell = body[shell_start:shell_end]
        embedded = f'''<!doctype html><html lang="ru"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><meta name="theme-color" content="#08111d"><title>Живая арена · StadMagic</title>{STYLE}<link rel="stylesheet" href="/static/arena-visualizer.css?v=11.0"><style>
          html,body{{width:100%;height:100%;margin:0;overflow:hidden;background:#08111d}}
          body.arena-embed-observer{{padding:0}}
          body.arena-embed-observer .embed-arena-root{{width:100%;height:100%;padding:0}}
          body.arena-embed-observer .visualizer-shell{{width:100%;height:100%;max-width:none;margin:0;padding:0;background:transparent}}
          body.arena-embed-observer .visualizer-shell>:not(.visualizer-layout){{display:none!important}}
          body.arena-embed-observer .visualizer-layout{{display:block;width:100%;height:100%;min-height:0;margin:0;overflow:hidden}}
          body.arena-embed-observer .visualizer-stage{{width:100%;height:100%;min-height:0;padding:0}}
          body.arena-embed-observer .viz-toolbar,body.arena-embed-observer .viz-fps,body.arena-embed-observer .touch-stick{{display:none!important}}
          body.arena-embed-observer .arena-pilot-callout{{display:none!important}}
          body.arena-embed-observer .canvas-wrap{{width:100%;height:100%;min-height:0;max-height:none;border:0;border-radius:0}}
        </style></head><body class="arena-embed-observer"><main class="embed-arena-root">{shell}</main><script src="/static/arena-visualizer.js?v=15.0" defer></script></body></html>'''
        return embedded.encode("utf-8")
    return page("Живая арена · StadMagic", body)


def docs_html() -> bytes:
    body = """
    <section class=docs-hero><span class=eyebrow>STADMAGIC · DOCS</span><h1>От первого запроса<br>до живой арены.</h1><p>Короткий маршрут для тех, кто хочет подключиться к игре: сначала разберись с игровым контрактом, затем изучи мир и устройство матча.</p></section>
    <section class=docs-cards aria-label="Документы"><a class=docs-card href="/docs/api"><span class=docs-card-icon aria-hidden=true>⌘</span><h2>Игровое API</h2><p>Формат запроса и ответа, авторизация, ошибки и готовый пример вызова.</p><strong>Открыть контракт →</strong></a><a class=docs-card href="/docs/world"><span class=docs-card-icon aria-hidden=true>◇</span><h2>Правила мира</h2><p>Сущности, физика, арены, ковры, золото и жизненный цикл игрового запроса.</p><strong>Изучить игру →</strong></a></section>
    <aside class=docs-more>Нужны детали внутренней симуляции? <a href="/api/docs/mechanics">Открыть техническую механику сервера</a>.</aside>
    """
    return page("Документация · StadMagic", body)


def register_html() -> bytes:
    body = """
    <h1>Создай команду</h1><p>Придумай имя, получи токен и подключи бота к соревнованию. Имя будет видно всем, токен — только тебе.</p>
    <div class="team-register-layout"><section class="card team-create-card"><span class=eyebrow>Начать играть</span><h2>Регистрация</h2><p>Токен понадобится боту для игровых запросов. Сохрани его сразу — повторно показать секрет нельзя.</p>
    <form id="team-create-form"><label for="team-name">Имя команды</label><div class="team-name-control"><input id="team-name" maxlength="48" autocomplete="organization" required placeholder="Например, Синий бархан"><button id="generate-team-name" class="secondary name-generate" type="button" aria-label="Сгенерировать другое имя">↻ <span>Сгенерировать</span></button></div><button class="team-submit" type="submit">Создать команду и получить токен <span aria-hidden="true">↗</span></button></form><div id="registration-result" class="registration-result" role="status" aria-live="polite"></div></section>
    <section class="card team-directory-card"><div class=team-directory-heading><div><span class=eyebrow>Участники</span><h2>Зарегистрированные команды</h2></div><span id=team-count class=team-count>—</span></div><p class=team-directory-note>Имена видны всем участникам. Секретные токены здесь не отображаются.</p><label class=visually-hidden for=team-search>Найти команду</label><input id=team-search type=search placeholder="Найти команду…"><p id=team-list-status class=team-list-status role=status>Загружаю команды…</p><ul id=registered-teams class=registered-teams aria-live=polite></ul></section></div>
    <script>
      const form=document.querySelector('#team-create-form'),nameInput=document.querySelector('#team-name'),result=document.querySelector('#registration-result'),teamList=document.querySelector('#registered-teams'),teamSearch=document.querySelector('#team-search'),teamCount=document.querySelector('#team-count'),teamStatus=document.querySelector('#team-list-status');
      const nameAdjectives=['Быстрый','Золотой','Дальний','Тихий','Северный','Лунный','Пыльный','Смелый','Упрямый','Летучий','Искристый','Скрытный','Медный','Вольный'];
      const nameNouns=['Бархан','Шакал','Вихрь','Оазис','Компас','Метеор','Мираж','Сокол','Фантом','Следопыт','Странник','Дракон','Караван','Скакун','Ракетчик','Пилигрим'];
      function randomItem(items){if(window.crypto?.getRandomValues){const n=new Uint32Array(1);window.crypto.getRandomValues(n);return items[n[0]%items.length]}return items[Math.floor(Math.random()*items.length)]}
      function generateName(){nameInput.value=`${randomItem(nameAdjectives)} ${randomItem(nameNouns)}`}
      document.querySelector('#generate-team-name').addEventListener('click',generateName);generateName();
      let teams=[],myTeamName='';try{myTeamName=localStorage.getItem('stadmagic-team-name')||''}catch(_){}
      function renderTeams(){const query=teamSearch.value.trim().toLocaleLowerCase('ru'),visible=teams.filter(t=>t.name.toLocaleLowerCase('ru').includes(query)).sort((a,b)=>Number(b.name===myTeamName)-Number(a.name===myTeamName)||a.name.localeCompare(b.name,'ru'));teamList.replaceChildren();for(const team of visible){const row=document.createElement('li');row.className='registered-team'+(team.name===myTeamName?' own-team':'');const name=document.createElement('span');name.textContent=team.name;row.append(name);if(team.name===myTeamName){const marker=document.createElement('b');marker.className='team-you-marker';marker.textContent='Ваша команда';row.append(marker)}teamList.append(row)}teamStatus.textContent=visible.length?`Показано: ${visible.length}`:'Команды не найдены';teamCount.textContent=String(teams.length)}
      async function refreshTeams(){try{const response=await fetch('/api/teams'),data=await response.json();if(!response.ok)throw Error(data.error||'Не удалось загрузить команды');teams=data.teams||[];renderTeams()}catch(error){teamStatus.textContent='Не удалось загрузить список команд'}}
      teamSearch.addEventListener('input',renderTeams);refreshTeams();setInterval(refreshTeams,15000);
      form.addEventListener('submit',async e=>{e.preventDefault();const submit=form.querySelector('.team-submit');submit.disabled=true;result.replaceChildren();result.className='registration-result';try{const response=await fetch('/api/teams',{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify({name:nameInput.value})}),data=await response.json();if(!response.ok){result.classList.add('error');result.textContent=String(data.error||'Не удалось создать команду').includes('already taken')?'Это имя уже занято — сгенерируй другое или введи своё.':data.error||'Не удалось создать команду';return}myTeamName=data.name;try{localStorage.setItem('stadmagic-team-token',data.token);localStorage.setItem('stadmagic-team-name',myTeamName);window.dispatchEvent(new Event('stadmagic-profile-change'))}catch(_){}result.classList.add('success');const title=document.createElement('p');title.className='token-title';title.textContent=`Команда «${data.name}» создана. Сохрани токен:`;const tokenRow=document.createElement('div');tokenRow.className='token-row';const token=document.createElement('code');token.className='token-value';token.textContent=data.token;const copy=document.createElement('button');copy.className='secondary token-copy';copy.type='button';copy.textContent='Копировать';copy.onclick=async()=>{try{await navigator.clipboard.writeText(data.token);copy.textContent='Скопировано'}catch(_){copy.textContent='Не удалось скопировать'}};const warning=document.createElement('small');warning.className='token-warning';warning.textContent='Токен сохранён в этом браузере и показан здесь один раз. Не отправляй и не публикуй его.';tokenRow.append(token,copy);result.append(title,tokenRow,warning);generateName();await refreshTeams()}catch(_){result.classList.add('error');result.textContent='Не удалось связаться с сервером. Попробуй ещё раз.'}finally{submit.disabled=false}});
    </script>
    """
    return page("Регистрация · StadMagic", body)


def worlds_html() -> bytes:
    body = """
    <section class="worlds-hero"><span class="eyebrow">АТЛАС АРЕН</span><h1>У каждого мира свой характер.</h1><p>Сравни условия, выбери профиль и посмотри, что ждёт команды. Каталог обновляется вместе с конфигурациями игры.</p><div class="worlds-current"><span class="live-dot"></span><span id=current>Подключаемся к арене…</span></div></section>
    <section class="world-browser card"><div class="world-browser-head"><div><span class="eyebrow">МИР <b id=world-number>—</b></span><h2 id=world-title>Загрузка миров…</h2></div><div class="world-switch"><button type=button id=world-prev class=secondary aria-label="Предыдущий мир">←</button><label class=visually-hidden for=world-select>Выбрать мир</label><select id=world-select></select><button type=button id=world-next class=secondary aria-label="Следующий мир">→</button></div></div><p id=world-description class=world-description></p><div id=world-traits class=world-traits></div><div id=world-metrics class=world-metrics></div><details class=world-config><summary>Технические параметры</summary><div class=disclosure-content><div class=disclosure-inner><dl id=world-config></dl></div></div></details></section>
    <section class="world-index"><div class=world-index-heading><div><span class=eyebrow>КАТАЛОГ</span><h2>Все миры</h2></div><span id=catalog-summary class=muted></span></div><div id=world-cards class=world-cards></div></section>
    <script>
    let worlds=[],selectedWorld=null,signature='';const select=document.querySelector('#world-select');
    const nf=new Intl.NumberFormat('ru-RU');
    function configValue(c,key,fallback='—'){const v=c[key];return v===undefined||v===null?fallback:(typeof v==='number'?nf.format(v):String(v))}
    function describeTraits(c){const speed=Number(c.anomaly_speed_max)||0,effect=Number(c.anomaly_effect_radius_max)||0,core=Number(c.anomaly_core_radius_max)||0,force=Number(c.anomaly_force_max)||0;return [`Аномалии ${speed>300?'очень быстрые':speed>180?'быстрые':'медленные'}`,`Зоны ${effect>1800?'дальние':effect>900?'широкие':'компактные'}`,`Ядра ${core>effect*.45?'крупные':core<effect*.2?'малые':'обычные'}`,force>55?'Сильное притяжение':'Умеренные силы']}
    function selectWorld(number){selectedWorld=Number(number);select.value=String(selectedWorld);renderWorld()}
    function renderWorld(){const w=worlds.find(item=>Number(item.world_number)===selectedWorld);if(!w)return;const c=w.config||{};document.querySelector('#world-number').textContent=String(w.world_number).padStart(2,'0');document.querySelector('#world-title').textContent=w.name;document.querySelector('#world-description').textContent=w.description||'У этого мира пока нет описания.';document.querySelector('#world-traits').replaceChildren(...describeTraits(c).map(text=>{const tag=document.createElement('span');tag.textContent=text;return tag}));
      const metrics=[['Арена',`${configValue(c,'arena_width')} × ${configValue(c,'arena_height')}`],['Золото',configValue(c,'bounty_quota')],['Аномалии',configValue(c,'anomaly_quota')],['Скорость ковра',configValue(c,'max_velocity')],['Скорость аномалий',`${configValue(c,'anomaly_speed_min')}–${configValue(c,'anomaly_speed_max')}`],['Зона воздействия',`${configValue(c,'anomaly_effect_radius_min')}–${configValue(c,'anomaly_effect_radius_max')}`],['Размер ядер',`${configValue(c,'anomaly_core_radius_min')}–${configValue(c,'anomaly_core_radius_max')}`],['Сила аномалий',`${configValue(c,'anomaly_force_min')}–${configValue(c,'anomaly_force_max')}`]];const host=document.querySelector('#world-metrics');host.replaceChildren(...metrics.map(([label,value])=>{const card=document.createElement('div');card.className='world-metric';const small=document.createElement('small');small.textContent=label;const strong=document.createElement('b');strong.textContent=value;card.append(small,strong);return card}));const settings=document.querySelector('#world-config');settings.replaceChildren(...Object.entries(c).filter(([key])=>key!=='runtime_entropy').map(([key,value])=>{const row=document.createElement('div'),dt=document.createElement('dt'),dd=document.createElement('dd');dt.textContent=key.replaceAll('_',' ');dd.textContent=typeof value==='number'?nf.format(value):String(value);row.append(dt,dd);return row}));document.querySelectorAll('.world-card').forEach(card=>card.classList.toggle('is-selected',Number(card.dataset.world)===selectedWorld))}
    function renderCards(){const host=document.querySelector('#world-cards');host.replaceChildren();if(!worlds.length){const empty=document.createElement('p');empty.className='world-catalog-state';empty.textContent='В каталоге пока нет доступных миров.';host.append(empty);return}for(const w of worlds){const button=document.createElement('button');button.type='button';button.className='world-card';button.dataset.world=String(w.world_number);const number=document.createElement('span');number.className='world-card-number';number.textContent=String(w.world_number).padStart(2,'0');const copy=document.createElement('span');copy.className='world-card-copy';const name=document.createElement('b');name.textContent=w.name;const description=document.createElement('small');description.textContent=w.description||`Профиль мира ${String(w.world_number).padStart(2,'0')}`;copy.append(name,description);const arrow=document.createElement('span');arrow.className='world-card-arrow';arrow.textContent='↗';button.append(number,copy,arrow);button.addEventListener('click',()=>{selectWorld(w.world_number);document.querySelector('.world-browser').scrollIntoView({behavior:'smooth',block:'start'})});host.append(button)}renderWorld()}
    function normalizeWorld(world,index){const worldNumber=Number.isInteger(Number(world?.world_number))&&Number(world.world_number)>0?Number(world.world_number):index+1;const id=[world?.id,world?.world_id].find(value=>typeof value==='string'&&value.trim()&&value!=='undefined')||`world-${String(worldNumber).padStart(2,'0')}`;const name=[world?.name,world?.display_name,world?.title].find(value=>typeof value==='string'&&value.trim()&&value!=='undefined')||`Мир ${String(worldNumber).padStart(2,'0')}`;const description=[world?.description,world?.summary].find(value=>typeof value==='string'&&value.trim()&&value!=='undefined')||'';return {...world,id,name,description,world_number:worldNumber,config:world?.config&&typeof world.config==='object'?world.config:{}}}
    async function refresh(){try{const response=await fetch('/api/worlds'),data=await response.json();if(!response.ok)throw Error(data.error||'Не удалось загрузить миры');if(!Array.isArray(data.worlds))throw Error('Ответ каталога имеет неверный формат');worlds=data.worlds.map(normalizeWorld);const active=data.active||{};document.querySelector('#current').textContent=active.world_number?`${active.world_name||'Текущий мир'} · ${active.status==='running'?'арена идёт':active.status} · осталось ${Math.floor((active.seconds_remaining||0)/60)}:${String(Math.ceil(active.seconds_remaining||0)%60).padStart(2,'0')}`:'Арена сейчас перезапускается';const nextSignature=worlds.map(w=>`${w.id}:${w.name}`).join('|');if(nextSignature!==signature){signature=nextSignature;const previous=selectedWorld;select.replaceChildren(...worlds.map(w=>new Option(`${String(w.world_number).padStart(2,'0')} · ${w.name}`,w.world_number)));selectedWorld=worlds.some(w=>Number(w.world_number)===Number(previous))?Number(previous):Number(active.world_number||worlds[0]?.world_number);if(selectedWorld)select.value=String(selectedWorld);renderCards()}document.querySelector('#catalog-summary').textContent=worlds.length?`${worlds.length} игровых профилей`:'Пока нет доступных миров';if(!worlds.length)renderCards()}catch(error){document.querySelector('#current').textContent=`Ошибка: ${error.message}`;document.querySelector('#catalog-summary').textContent='Каталог временно недоступен';const host=document.querySelector('#world-cards');host.replaceChildren();const failure=document.createElement('p');failure.className='world-catalog-state';failure.textContent=`Не удалось загрузить каталог: ${error.message}`;host.append(failure)}}
    select.addEventListener('change',()=>selectWorld(select.value));document.querySelector('#world-prev').addEventListener('click',()=>selectWorld(worlds[(worlds.findIndex(w=>Number(w.world_number)===selectedWorld)-1+worlds.length)%worlds.length]?.world_number));document.querySelector('#world-next').addEventListener('click',()=>selectWorld(worlds[(worlds.findIndex(w=>Number(w.world_number)===selectedWorld)+1)%worlds.length]?.world_number));refresh();setInterval(refresh,15000)
    </script>
    """
    return page("Миры · StadMagic", body)


def _leaderboard_html_legacy() -> bytes:
    body = """
    <div class="leaderboard-page"><div class="leaderboard-heading"><div><span class=eyebrow>ТАБЛИЦА РЕЗУЛЬТАТОВ</span><h1>Рейтинг команд</h1><p>Текущая гонка, архив арен и суммарные результаты.</p></div><span class="phase-badge phase-running">LIVE · обновление 5 с</span></div>
    <nav class="leaderboard-tabs" aria-label="Раздел рейтинга"><button data-section=top>Топ</button><button data-section=world>По миру</button><button data-section=history>История арен</button></nav>
    <nav class="leaderboard-periods" aria-label="Период топа"><button data-scope=current>Текущая игра</button><button data-scope=last_10>Последние 10 игр</button><button data-scope=all>За всё время</button></nav>
    <div class="leaderboard-world-filter" hidden><label>Мир<select id=world></select></label><label>Период<select id=world-period><option value=all>Все игры в мире</option><option value=last_10>Последние 10 игр</option><option value=last_1>Последняя игра</option></select></label></div>
    <div id="leaderboard-results" class="leaderboard-results">
      <div class="leaderboard-toolbar"><p id=summary class="leaderboard-summary" aria-live=polite>Загружаю результаты…</p></div>
      <section id=history-panel class="card history-browser" hidden><div class=history-browser-head><div><strong>Арены от новых к старым</strong><small>Название запуска и описание мира указаны в каждой записи</small></div><div class=leaderboard-pagination><span id=history-summary class=muted></span><button id=prev-page class=secondary aria-label="Новые арены">← Новее</button><button id=next-page class=secondary aria-label="Более ранние арены">Раньше →</button></div></div><div id=history-list class=history-browser-list></div></section>
      <section id="leaderboard-table-card" class="card leaderboard-table-card"><div class=leaderboard-table-head><h2 id=table-heading>Суммарный рейтинг</h2><span id=updated class=muted></span></div><div class=leaderboard-table-wrap><table id=table class=leaderboard-table></table></div><p id=empty class=history-empty hidden>Пока нет результатов для этого среза.</p></section>
      <p id=totals-note class=muted hidden>Суммарные показатели складываются по всем попыткам команды. «Золото» здесь означает сумму остатков в отчётах завершённых запусков.</p>
    </div></div>
    <script>
    const world=document.querySelector('#world'),worldPeriod=document.querySelector('#world-period'),table=document.querySelector('#table'),results=document.querySelector('#leaderboard-results'),runLimit=20;
    const params=new URLSearchParams(location.search);
    let scope=params.get('run_id')?'history':['current','world','all','history','average','last_10'].includes(params.get('scope'))?params.get('scope'):'current',section=scope==='history'?'history':scope==='world'?'world':'top';
    let offset=0,worlds=[],requestId=0,selectedRunId=params.get('run_id');
    function esc(value){return String(value??'').replace(/[&<>"']/g,char=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[char]))}
    const number=value=>Math.round(Number(value)||0).toLocaleString('ru-RU');
    function setSection(value){section=value;document.querySelectorAll('.leaderboard-tabs button').forEach(button=>{const active=button.dataset.section===section;button.classList.toggle('is-active',active);button.setAttribute('aria-pressed',String(active))});document.querySelector('.leaderboard-periods').hidden=section!=='top';document.querySelector('.leaderboard-world-filter').hidden=section!=='world';document.querySelector('#history-panel').hidden=section!=='history';document.querySelector('#leaderboard-table-card').hidden=section==='history'&&!selectedRunId;document.querySelector('#totals-note').hidden=section==='current';document.querySelector('#table-heading').textContent=section==='history'?'Результаты выбранной арены':section==='world'?'Топ выбранного мира':'Рейтинг команд'}
    function runStatus(status){return ({starting:['status-starting','◌','Запускается'],running:['status-running','●','Идёт'],complete:['status-complete','✓','Завершена'],failed:['status-failed','×','Сбой'],interrupted:['status-interrupted','Ⅱ','Прервана'],stopping:['status-stopping','◌','Завершается'],stopped:['status-interrupted','Ⅱ','Остановлена']})[status]||['status-unknown','•','Статус неизвестен']}
    function runAge(value){const date=new Date(value);if(Number.isNaN(date.getTime()))return 'Время неизвестно';const seconds=Math.max(0,Math.floor((Date.now()-date.getTime())/1000));const [amount,unit]=seconds<60?[seconds,'second']:seconds<3600?[Math.floor(seconds/60),'minute']:seconds<86400?[Math.floor(seconds/3600),'hour']:[Math.floor(seconds/86400),'day'];const relative=new Intl.RelativeTimeFormat('ru-RU',{numeric:'auto'}).format(-amount,unit);return `${relative} · ${date.toLocaleString('ru-RU',{dateStyle:'medium',timeStyle:'short'})}`}
    function renderRuns(runs,total,active){const host=document.querySelector('#history-list'),start=total?offset+1:0,pageCount=Math.max(1,Math.ceil(total/runLimit)),pageNumber=Math.floor(offset/runLimit)+1;document.querySelector('#history-summary').textContent=`${start}–${offset+(runs.length||0)} из ${total} · стр. ${pageNumber}/${pageCount}`;document.querySelector('#prev-page').disabled=offset===0;document.querySelector('#next-page').disabled=offset+runLimit>=total;const rows=[];if(active.run_id){const [statusClass,icon,label]=runStatus(active.status||'running');rows.push(`<a class="history-run is-current ${selectedRunId===active.run_id?'is-selected':''}" data-run-id="${esc(active.run_id)}" href="/leaderboard?scope=history&amp;run_id=${encodeURIComponent(active.run_id)}"><span class="history-run-icon ${statusClass}" aria-hidden="true">${icon}</span><strong>Текущая арена · ${esc(active.arena_name||'Арена')}</strong><small>${esc(active.world_name||'Текущий мир')} · ${runAge(active.started_at)}</small><b>${label}</b></a>`)}for(const run of runs){if(run.id===active.run_id)continue;const [statusClass,icon,label]=runStatus(run.status);rows.push(`<a class="history-run ${selectedRunId===run.id?'is-selected':''}" data-run-id="${esc(run.id)}" href="/leaderboard?scope=history&amp;run_id=${encodeURIComponent(run.id)}"><span class="history-run-icon ${statusClass}" aria-hidden="true">${icon}</span><strong>${esc(run.arena_name||'Арена')} · ${esc(run.world_name||'Мир')}</strong><small>${runAge(run.started_at)}</small><b>${label}</b></a>`)}host.innerHTML=rows.join('')||'<div class="history-empty">История появится после первого запуска.</div>'}
    function renderTable(data,aggregated){const teams=data.teams||[],isEmpty=!teams.length;document.querySelector('#empty').hidden=!isEmpty;table.hidden=isEmpty;if(isEmpty){table.replaceChildren();return}const average=scope==='average',headers=average?['#','Команда','Раундов со статистикой','Среднее золото / ковер за раунд']:aggregated?['#','Команда','Попыток','Золото','Собрано золота','Потеряно ковров от аварий','Пройденное расстояние']:['#','Команда','Золото','Собрано золота','Потеряно ковров от аварий','Пройденное расстояние'];table.innerHTML='<thead><tr>'+headers.map(header=>`<th>${header}</th>`).join('')+'</tr></thead><tbody>'+teams.map(team=>{const metrics=aggregated?(team.total||{}):team,values=average?[team.rank,esc(team.name),number(team.average_rounds),Number(team.average_gold_per_carpet||0).toLocaleString('ru-RU',{maximumFractionDigits:1})+' ✦']:[team.rank,esc(team.name),...(aggregated?[number(team.attempts)]:[]),number(metrics.gold),number(metrics.gold_collected),number(metrics.carpets_lost),number(metrics.distance_travelled)],goldColumns=average?[3]:aggregated?[3,4]:[2,3];return `<tr class="rank-${team.rank}">${values.map((value,index)=>`<td class="${index===1?'team':''} ${goldColumns.includes(index)?'gold-cell':''}">${value}${goldColumns.includes(index)&&!(average&&index===3)?' <span class="gold-symbol" aria-label="золота">✦</span>':''}</td>`).join('')}</tr>`}).join('')+'</tbody>'}
    async function refresh(){const request=++requestId;try{const stateResponse=await fetch('/api/worlds'),worldState=await stateResponse.json();if(!stateResponse.ok)throw Error(worldState.error||'Не удалось получить состояние арены');worlds=Array.isArray(worldState.worlds)?worldState.worlds:[];if(!world.options.length){world.replaceChildren(...worlds.map(item=>new Option(`${item.name||`Мир ${item.world_number}`} · мир ${item.world_number}`,item.world_number)));const preferred=String(worldState.active?.world_number||worlds[0]?.world_number||'');if([...world.options].some(option=>option.value===preferred))world.value=preferred}setScope(scope);let url,summary='',aggregated=false;if(scope==='current'){const active=worldState.active||{};if(!active.run_id){document.querySelector('#summary').textContent='Арена готовится к запуску.';renderTable({teams:[]},false);document.querySelector('#updated').textContent='';return}url='/api/leaderboard?scope=run&run_id='+encodeURIComponent(active.run_id);summary=`${active.arena_name||'Арена'} · ${active.world_name||'текущий мир'} · сейчас`}else if(scope==='history'){const historyResponse=await fetch(`/api/runs?limit=${runLimit}&offset=${offset}`),historyData=await historyResponse.json();if(!historyResponse.ok)throw Error(historyData.error||'Не удалось загрузить историю');renderRuns(historyData.runs||[],historyData.total||0,worldState.active||{});const runId=selectedRunId||(worldState.active||{}).run_id||(historyData.runs||[])[0]?.id;if(!runId){document.querySelector('#summary').textContent='Запусков ещё не было.';renderTable({teams:[]},false);return}url='/api/leaderboard?scope=run&run_id='+encodeURIComponent(runId)}else if(scope==='world'){if(!world.value&&world.options.length)world.value=world.options[0].value;url='/api/leaderboard?scope=world&world='+encodeURIComponent(world.value);const profile=worlds.find(item=>String(item.world_number)===world.value);summary=`Суммарный рейтинг · ${profile?.name||'выбранный мир'}`;aggregated=true}else if(scope==='average'){url='/api/leaderboard?scope=average';summary='Средний остаток золота на один ковер за раунд по всем мирам';aggregated=true}else if(scope==='last_10'){url='/api/leaderboard?scope=last_10';summary='Сумма золота на руках за последние 10 запусков арены';aggregated=true}else{url='/api/leaderboard?scope=all';summary='Суммарные остатки золота по всем запускам';aggregated=true}const response=await fetch(url),data=await response.json();if(request!==requestId)return;if(!response.ok)throw Error(data.error||'Не удалось загрузить рейтинг');if(scope==='history'&&data.run){const run=data.run;summary=`${run.arena_name||'Арена'} · ${run.world_name||'мир'} · ${runAge(run.started_at)} · ${runStatus(run.status)[2]}`}document.querySelector('#summary').textContent=summary;renderTable(data,aggregated);document.querySelector('#updated').textContent=`Обновлено ${new Date().toLocaleTimeString('ru-RU')}`}catch(error){if(request===requestId)document.querySelector('#summary').textContent=error.message}}
    async function animateResults(action){if(matchMedia('(prefers-reduced-motion: reduce)').matches){await action();return}try{await results.animate([{opacity:1,filter:'blur(0)',transform:'translateY(0)'},{opacity:0,filter:'blur(4px)',transform:'translateY(7px)'}],{duration:130,easing:'ease-in'}).finished}catch(_){}await action();results.animate([{opacity:0,filter:'blur(4px)',transform:'translateY(-6px)'},{opacity:1,filter:'blur(0)',transform:'translateY(0)'}],{duration:300,easing:'cubic-bezier(.2,.75,.25,1)'})}
    function updateUrl(){const query=new URLSearchParams({scope});if(scope==='history'&&selectedRunId)query.set('run_id',selectedRunId);window.history.replaceState({},'',`/leaderboard?${query}`)}
    document.querySelectorAll('.leaderboard-tabs button').forEach(button=>button.addEventListener('click',()=>{const next=button.dataset.scope;if(next===scope&&!selectedRunId)return;selectedRunId=null;offset=0;updateUrl();animateResults(async()=>{setScope(next);await refresh()})}));
    document.querySelector('#history-list').addEventListener('click',event=>{const link=event.target.closest('a.history-run');if(!link)return;event.preventDefault();const nextRunId=link.dataset.runId;if(!nextRunId)return;selectedRunId=nextRunId;scope='history';updateUrl();animateResults(async()=>{setScope('history');await refresh()}).then(()=>document.querySelector('#leaderboard-table-card').scrollIntoView({behavior:matchMedia('(prefers-reduced-motion: reduce)').matches?'instant':'smooth',block:'start'}))});
    world.addEventListener('change',()=>{offset=0;updateUrl();animateResults(refresh)});document.querySelector('#prev-page').addEventListener('click',()=>{offset=Math.max(0,offset-runLimit);animateResults(refresh)});document.querySelector('#next-page').addEventListener('click',()=>{offset+=runLimit;animateResults(refresh)});
    setScope(scope);refresh();setInterval(refresh,5000)
    </script>
    """
    return page("Лидерборд · StadMagic", body)


def leaderboard_html() -> bytes:
    body = """
    <div class="leaderboard-page"><header class="leaderboard-heading"><div><span class="eyebrow">РЕЗУЛЬТАТЫ</span><h1>Рейтинг команд</h1><p>Текущая гонка, зачёты по мирам и вся хронология арен.</p></div><span class="phase-badge phase-running">LIVE · обновление 5 с</span></header>
    <nav class="leaderboard-tabs" aria-label="Раздел рейтинга"><button data-section="top">Топ</button><button data-section="world">По миру</button><button data-section="history">История</button></nav>
    <nav class="leaderboard-periods" aria-label="Период рейтинга"><button data-scope="current">Текущая игра</button><button data-scope="last_10">Последние 10 игр</button><button data-scope="all">За всё время</button></nav>
    <div class="leaderboard-world-filter" hidden><label>Мир<select id="world"></select></label><label>Период<select id="world-period"><option value="best">Абсолютный топ · лучшие результаты</option><option value="all">Все игры в мире</option><option value="last_10">Последние 10 игр</option><option value="last_1">Последняя игра</option></select></label></div>
    <div id="leaderboard-results" class="leaderboard-results"><p id="summary" class="leaderboard-summary" aria-live="polite">Загружаю результаты…</p>
      <section id="history-panel" class="card history-browser" hidden><div class="history-browser-head"><div><strong>Арены от новых к старым</strong><small>Каждая запись содержит название запуска и описание мира</small></div><div class="leaderboard-pagination"><span id="history-summary" class="muted"></span><button id="prev-page" class="secondary" aria-label="К новым аренам">← Новее</button><button id="next-page" class="secondary" aria-label="К более ранним аренам">Раньше →</button></div></div><div id="history-list" class="history-browser-list"></div></section>
      <section id="leaderboard-table-card" class="card leaderboard-table-card"><div class="leaderboard-table-head"><h2 id="table-heading">Топ команд</h2><span id="updated" class="muted"></span></div><div class="leaderboard-table-wrap"><table id="table" class="leaderboard-table"></table></div><p id="empty" class="history-empty" hidden>Пока нет результатов для этого среза.</p></section></div></div>
    <script>
    const worldSelect=document.querySelector('#world'),periodSelect=document.querySelector('#world-period'),table=document.querySelector('#table'),resultPanel=document.querySelector('#leaderboard-results'),runLimit=20,query=new URLSearchParams(location.search);
    let section=query.get('run_id')?'history':query.get('scope')==='world'?'world':'top',scope=query.get('scope')||'current',rankingSort=query.get('sort')||'gold',offset=0,worlds=[],requestNo=0,selectedRunId=query.get('run_id');
    const esc=value=>String(value??'').replace(/[&<>"']/g,char=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[char])),number=value=>Math.round(Number(value)||0).toLocaleString('ru-RU');
    function setSection(){document.querySelectorAll('[data-section]').forEach(button=>{const on=button.dataset.section===section;button.classList.toggle('is-active',on);button.setAttribute('aria-pressed',String(on))});document.querySelector('.leaderboard-periods').hidden=section!=='top';document.querySelector('.leaderboard-world-filter').hidden=section!=='world';document.querySelector('#history-panel').hidden=section!=='history';document.querySelector('#leaderboard-table-card').hidden=section==='history'&&!selectedRunId}
    const runState=status=>({starting:['status-starting','◌','Запускается'],running:['status-running','●','Идёт'],complete:['status-complete','✓','Завершена'],failed:['status-failed','×','Сбой'],interrupted:['status-interrupted','Ⅱ','Прервана'],stopping:['status-stopping','◌','Завершается'],stopped:['status-interrupted','Ⅱ','Остановлена']}[status]||['status-unknown','•','Неизвестно']);
    function renderHistory(runs,total,active){const host=document.querySelector('#history-list'),pageCount=Math.max(1,Math.ceil(total/runLimit));document.querySelector('#history-summary').textContent=`${total?offset+1:0}–${offset+runs.length} из ${total} · ${Math.floor(offset/runLimit)+1}/${pageCount}`;document.querySelector('#prev-page').disabled=offset===0;document.querySelector('#next-page').disabled=offset+runLimit>=total;const ordered=active.run_id?[{...active,id:active.run_id,status:active.status||'running'},...runs.filter(run=>run.id!==active.run_id)]:runs;host.innerHTML=ordered.map((run,index)=>{const [statusClass,icon,status]=runState(run.status),profile=worlds.find(item=>item.id===run.world_id||Number(item.world_number)===Number(run.world_number)),description=profile?.description||'Описание мира недоступно';return `<a class="history-run ${run.id===active.run_id?'is-current ':''}${run.id===selectedRunId?'is-selected':''}" data-run-id="${esc(run.id)}" href="/leaderboard?scope=history&amp;run_id=${encodeURIComponent(run.id)}"><span class="history-run-icon ${statusClass}" aria-hidden="true">${icon}</span><strong>${index===0&&run.id===active.run_id?'ТЕКУЩАЯ АРЕНА':`АРЕНА №${String(run.run_number||'').padStart(3,'0')}`} · ${esc(run.arena_name||'Арена')}</strong><b>${status}</b><small>${esc(run.world_name||'Мир')} · ${new Date(run.started_at).toLocaleString('ru-RU',{dateStyle:'medium',timeStyle:'short'})}</small><em>${esc(description)}</em></a>`}).join('')||'<div class="history-empty">История появится после первого запуска.</div>'}
    function renderTable(data,runScope=false){const teams=data.teams||[],empty=!teams.length;document.querySelector('#empty').hidden=!empty;table.hidden=empty;if(empty){table.replaceChildren();return}const headings=['#','Команда','Золото','Собрано золота','Золото / м'];table.innerHTML='<thead><tr>'+headings.map(value=>`<th>${value}</th>`).join('')+'</tr></thead><tbody>'+teams.map(team=>{const metrics=runScope?team:team.total||{},ratio=team.gold_per_distance,ratioText=ratio==null?'—':`${Number(ratio).toLocaleString('ru-RU',{maximumFractionDigits:3})} <span class="gold-symbol" aria-label="золота">✦</span>/м`,values=[team.rank,esc(team.name),number(metrics.gold),number(metrics.gold_collected),ratioText];return `<tr class="rank-${team.rank}">${values.map((value,index)=>`<td class="${index===1?'team':''} ${index===2||index===3?'gold-cell':''}">${value}${index===2||index===3?' <span class="gold-symbol" aria-label="золота">✦</span>':''}</td>`).join('')}</tr>`}).join('')+'</tbody>'}
    async function refresh(){const ticket=++requestNo;try{const stateResponse=await fetch('/api/worlds'),state=await stateResponse.json();if(!stateResponse.ok)throw Error(state.error||'Не удалось получить состояние арены');worlds=Array.isArray(state.worlds)?state.worlds:[];if(!worldSelect.options.length)worldSelect.replaceChildren(...worlds.map(item=>new Option(item.name||`Мир ${item.world_number}`,item.world_number)));if(!worldSelect.value)worldSelect.value=String(state.active?.world_number||worlds[0]?.world_number||'');setSection();let url,summary='',runScope=false;if(section==='history'){const response=await fetch(`/api/runs?limit=${runLimit}&offset=${offset}`),history=await response.json();if(!response.ok)throw Error(history.error||'Не удалось загрузить историю');renderHistory(history.runs||[],history.total||0,state.active||{});selectedRunId=selectedRunId||state.active?.run_id||history.runs?.[0]?.id;if(!selectedRunId){document.querySelector('#leaderboard-table-card').hidden=true;document.querySelector('#summary').textContent='Запусков ещё не было.';return}document.querySelector('#leaderboard-table-card').hidden=false;url='/api/leaderboard?scope=run&run_id='+encodeURIComponent(selectedRunId);summary='Результаты выбранной арены';runScope=true}else if(section==='world'){scope='world';const profile=worlds.find(item=>String(item.world_number)===worldSelect.value),period=periodSelect.value;url=`/api/leaderboard?scope=world&world=${encodeURIComponent(worldSelect.value)}&period=${period}`;summary=`${profile?.name||'Выбранный мир'} · ${{best:'абсолютный топ · лучшие результаты',all:'все игры этого мира',last_10:'последние 10 игр этого мира',last_1:'последняя игра этого мира'}[period]}`}else if(scope==='current'){const active=state.active||{};if(!active.run_id){document.querySelector('#summary').textContent='Арена готовится к запуску.';renderTable({teams:[]});return}url='/api/leaderboard?scope=run&run_id='+encodeURIComponent(active.run_id);summary=`${active.arena_name||'Арена'} · ${active.world_name||'текущий мир'}`;runScope=true}else{url=`/api/leaderboard?scope=${scope}${rankingSort==='gold'?'':`&sort=${encodeURIComponent(rankingSort)}`}`;summary=scope==='last_10'?'Последние 10 запусков':'Вся история запусков'}const response=await fetch(url),data=await response.json();if(ticket!==requestNo)return;if(!response.ok)throw Error(data.error||'Не удалось загрузить рейтинг');document.querySelector('#summary').textContent=summary;document.querySelector('#table-heading').textContent=summary;document.querySelector('#updated').textContent=`Обновлено ${new Date().toLocaleTimeString('ru-RU')}`;renderTable(data,runScope)}catch(error){if(ticket===requestNo)document.querySelector('#summary').textContent=error.message}}
    async function animate(action){if(matchMedia('(prefers-reduced-motion: reduce)').matches){await action();return}try{await resultPanel.animate([{opacity:1,transform:'translateY(0)'},{opacity:0,transform:'translateY(6px)'}],{duration:120}).finished}catch(_){}await action();resultPanel.animate([{opacity:0,transform:'translateY(-5px)'},{opacity:1,transform:'translateY(0)'}],{duration:250,easing:'ease-out'})}
    function syncUrl(){const params=new URLSearchParams({scope:section==='world'?'world':section==='history'?'history':scope});if(section==='history'&&selectedRunId)params.set('run_id',selectedRunId);if(rankingSort!=='gold')params.set('sort',rankingSort);history.replaceState({},'',`/leaderboard?${params}`)}
    document.querySelectorAll('[data-section]').forEach(button=>button.addEventListener('click',()=>{section=button.dataset.section;selectedRunId=null;offset=0;if(section==='top'&&!['current','last_10','all'].includes(scope))scope='current';if(section==='world')scope='world';if(section==='history')scope='history';syncUrl();animate(refresh)}));document.querySelectorAll('.leaderboard-periods [data-scope]').forEach(button=>button.addEventListener('click',()=>{section='top';scope=button.dataset.scope;rankingSort='gold';document.querySelectorAll('.leaderboard-periods [data-scope]').forEach(item=>item.classList.toggle('is-active',item===button));syncUrl();animate(refresh)}));
    document.querySelector('#history-list').addEventListener('click',event=>{const link=event.target.closest('[data-run-id]');if(!link)return;event.preventDefault();selectedRunId=link.dataset.runId;syncUrl();animate(refresh)});worldSelect.addEventListener('change',()=>animate(refresh));periodSelect.addEventListener('change',()=>animate(refresh));document.querySelector('#prev-page').addEventListener('click',()=>{offset=Math.max(0,offset-runLimit);animate(refresh)});document.querySelector('#next-page').addEventListener('click',()=>{offset+=runLimit;animate(refresh)});
    if(section==='world')scope='world';else if(section==='top'&&!['current','last_10','all'].includes(scope))scope='current';document.querySelectorAll('.leaderboard-periods [data-scope]').forEach(button=>button.classList.toggle('is-active',button.dataset.scope===scope));syncUrl();refresh();setInterval(refresh,5000)
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
                self.send_bytes(200, arena_visualizer_html(embed_observer=query.get("mode") == ["observer"] and query.get("embed") == ["1"]), "text/html; charset=utf-8")
            elif parsed.path == "/docs":
                self.send_bytes(200, docs_html(), "text/html; charset=utf-8")
            elif parsed.path == "/leaderboard":
                self.send_bytes(200, leaderboard_html(), "text/html; charset=utf-8")
            elif parsed.path == "/register":
                self.send_bytes(200, register_html(), "text/html; charset=utf-8")
            elif parsed.path == "/api/teams":
                self.send_json(200, {"teams": state.registry.public_teams()})
            elif parsed.path == "/api/teams/me":
                token = str(self.headers.get("X-Auth-Token", "")).strip()
                team_id = state.registry.resolve_token(token) if token else None
                if team_id is None:
                    self.send_json(401, {"error": "unknown or missing team token"})
                else:
                    self.send_json(200, {"name": state.registry.names().get(team_id, "Команда")})
            elif parsed.path == "/worlds":
                self.send_bytes(200, worlds_html(), "text/html; charset=utf-8")
            elif parsed.path in {"/docs/api", "/docs/world"}:
                file_name = "api.md" if parsed.path.endswith("api") else "world-rules.md"
                source = (DOCS_DIR / file_name).read_text(encoding="utf-8")
                clean_source = re.sub(r"\A---\s*\n.*?\n---\s*\n", "", source, count=1, flags=re.S)
                headings = re.findall(r"^##\s+(.+)$", clean_source, flags=re.M)
                toc_items = []
                for heading in headings:
                    slug = re.sub(r"[^a-z0-9а-яё]+", "-", heading.lower()).strip("-")
                    toc_items.append(f'<a href="#{esc(slug)}">{esc(heading)}</a>')
                document = (
                    '<div class="docs-toolbar"><a href="/docs">← Все материалы</a>'
                    '<span>STADMAGIC · ДОКУМЕНТАЦИЯ</span></div>'
                    '<div class="docs-layout"><nav class="docs-toc" aria-label="Содержание документа">'
                    '<strong>Содержание</strong>' + "".join(toc_items) +
                    '</nav><article class="docs-prose">' + markdown_html(source) + '</article></div>'
                )
                self.send_bytes(200, page("Документация", f'<section class="docs-article">{document}</section>'), "text/html; charset=utf-8")
            elif parsed.path in {"/api/docs/api", "/api/docs/world"}:
                file_name = "api.md" if parsed.path.endswith("api") else "world-rules.md"
                self.send_bytes(200, (DOCS_DIR / file_name).read_bytes(), "text/markdown; charset=utf-8")
            elif parsed.path == "/api/docs/mechanics":
                self.send_bytes(200, (ROOT / "docs" / "mechanics.md").read_bytes(), "text/markdown; charset=utf-8")
            elif parsed.path == "/api/worlds":
                active = state.current_arena()
                active["active_players"] = 0
                if active.get("status") == "running" and active.get("run_id"):
                    try:
                        active["active_players"] = len(state.store.leaderboard("run", state.registry.names(), run_id=active["run_id"])["teams"])
                    except KeyError:
                        pass
                worlds = [
                    {**world_config,
                     "world_number": world_config.get("world_number") or index + 1,
                     "active": active.get("world_number") == (world_config.get("world_number") or index + 1)}
                    for index, world_config in enumerate(state.world_configs)
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
                sort_by = query.get("sort", ["gold"])[0]
                period = query.get("period", ["all"])[0]
                if sort_by not in {"gold", "gold_per_distance"}:
                    raise ValueError("sort must be gold or gold_per_distance")
                if period not in {"all", "last_10", "last_1", "best"}:
                    raise ValueError("period must be all, last_10, last_1, or best")
                self.send_json(200, state.store.leaderboard(scope, state.registry.names(), world, run_id, sort_by, period))
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
                token = str(self.headers.get("X-Auth-Token", "")).strip()
                team_id = self.state.registry.resolve_token(token)
                if team_id is None:
                    self.send_json(401, {"error": "unknown or missing team token"})
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
