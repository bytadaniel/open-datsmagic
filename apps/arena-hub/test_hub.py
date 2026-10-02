import json
import tempfile
import unittest
from pathlib import Path

from hub import (
    HubState,
    Store,
    TeamRegistry,
    home_html,
    leaderboard_html,
    load_world_catalog,
    next_minute_boundary,
    pick_world,
    token_id,
)


class HubTests(unittest.TestCase):
    def test_hub_pages_use_live_layout_and_requested_leaderboard_labels(self):
        state = HubState.__new__(HubState)
        state.lock = __import__("threading").RLock()
        state.arena = {
            "status": "running", "world_number": None, "world_id": None,
            "world_name": None, "arena_name": "world-demo-1-1", "port": 8080,
            "run_id": None, "started_at": None, "ends_at": None,
            "next_start_at": None, "pid": None, "error": None,
        }
        home = home_html(state).decode("utf-8")
        leaderboard = leaderboard_html().decode("utf-8")
        self.assertIn("/static/hub.css", home)
        self.assertIn("refreshHome();setInterval(refreshHome,5000)", home)
        for label in ("Золото", "Собрано золота", "Потеряно ковров от аварий", "Пройденное расстояние"):
            self.assertIn(label, leaderboard)
        self.assertNotIn("Золота на руках", leaderboard)
        self.assertNotIn("Всего собрано золота", leaderboard)

    def test_world_catalog_and_no_immediate_repeat(self):
        worlds = load_world_catalog()
        self.assertGreaterEqual(len(worlds), 1)
        selected = pick_world(worlds, worlds[0]["id"])
        self.assertNotEqual(selected["id"], worlds[0]["id"])
        self.assertEqual(pick_world([worlds[0]], worlds[0]["id"])["id"], worlds[0]["id"])

    def test_world_catalog_size_comes_from_json(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "worlds.json"
            path.write_text(json.dumps({"worlds": [
                {"id": "a", "name": "A"},
                {"id": "b", "name": "B"},
            ]}), encoding="utf-8")
            self.assertEqual(len(load_world_catalog(path)), 2)

    def test_next_arena_starts_at_minute_boundary(self):
        self.assertEqual(next_minute_boundary(1200.0), 1200.0)
        self.assertEqual(next_minute_boundary(1200.1), 1260.0)
        self.assertEqual(next_minute_boundary(1259.9), 1260.0)

    def test_registry_upserts_name_but_only_returns_public_identity(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "registry.json"
            registry = TeamRegistry(path)
            first = registry.register("private-secret", "First Name")
            second = registry.register("private-secret", "Renamed")
            self.assertEqual(first["team_id"], token_id("private-secret"))
            self.assertEqual(second["name"], "Renamed")
            self.assertNotIn("token", second)
            self.assertEqual(registry.names()[first["team_id"]], "Renamed")
            self.assertEqual(registry.resolve_token("private-secret"), first["team_id"])
            self.assertIsNone(registry.resolve_token("not-registered"))
            self.assertIn("private-secret", path.read_text(encoding="utf-8"))

    def test_votes_are_one_per_team_changeable_and_consumed_for_next_world(self):
        state = HubState.__new__(HubState)
        state.world_configs = [
            {"id": "a", "name": "A"}, {"id": "b", "name": "B"},
        ]
        state.lock = __import__("threading").RLock()
        state.votes = {}
        state.arena = {"run_id": "active-run"}
        first_team = token_id("first")
        second_team = token_id("second")

        self.assertEqual(state.set_vote(first_team, "a"), {"a": 1})
        self.assertEqual(state.set_vote(first_team, "b"), {"b": 1})
        self.assertEqual(state.set_vote(second_team, "b"), {"b": 2})
        self.assertEqual(state.consume_vote_winner(), "b")
        self.assertEqual(state.vote_results(), {})

    def test_world_selection_prefers_vote_and_falls_back_to_random(self):
        worlds = [{"id": "a"}, {"id": "b"}]
        self.assertEqual(pick_world(worlds, "a", "b")["id"], "b")
        self.assertIn(pick_world(worlds, "a")["id"], {"a", "b"})

    def test_run_names_world_filter_and_totals(self):
        with tempfile.TemporaryDirectory() as directory:
            store = Store(Path(directory) / "hub.sqlite3")
            team = token_id("team-token")
            world = {"world_number": 1, "id": "quiet-harbor-01", "name": "Тихая бухта"}
            run_a, name_a = store.start_run(world, 8080)
            run_b, name_b = store.start_run(world, 8080)
            self.assertEqual(name_a, "world_quiet-harbor-01_1_1")
            self.assertEqual(name_b, "world_quiet-harbor-01_2_2")
            store.ingest(run_a, [{"team_id": team, "gold": 40, "gold_collected": 100,
                                  "carpets_lost": 1, "distance_travelled": 12}], {})
            store.ingest(run_b, [{"team_id": team, "gold": 20, "gold_collected": 150,
                                  "carpets_lost": 2, "distance_travelled": 25}], {})

            result = store.leaderboard("world", {team: "Team One"}, world_number=1)["teams"][0]
            self.assertEqual(result["name"], "Team One")
            self.assertEqual(result["top"], {
                "gold": 20, "gold_collected": 150, "carpets_lost": 2,
                "distance_travelled": 25.0,
            })
            self.assertEqual(result["total"], {
                "gold": 60, "gold_collected": 250, "carpets_lost": 3,
                "distance_travelled": 37.0,
            })
            self.assertEqual(result["attempts"], 2)
            page = store.runs(world_number=1, limit=1, offset=0)
            self.assertEqual(page["total"], 2)
            self.assertEqual(page["runs"][0]["id"], run_b)
            older_page = store.runs(world_number=1, limit=1, offset=1)
            self.assertEqual(older_page["runs"][0]["id"], run_a)
            all_time = store.leaderboard("all", {team: "Team One"})["teams"][0]
            self.assertEqual(all_time["total"], result["total"])
            run_result = store.leaderboard("run", {team: "Team One"}, run_id=run_a)
            self.assertEqual(run_result["teams"][0]["gold_collected"], 100)
            run_c, name_c = store.start_run({"world_number": 2, "id": "storm-belt-01", "name": "Грозовой пояс"}, 8080)
            self.assertEqual(name_c, "world_storm-belt-01_1_3")
            self.assertEqual(store.leaderboard("run", {}, run_id=run_c)["teams"], [])
            store.db.close()

    def test_empty_aggregate_leaderboards_return_empty_team_lists(self):
        with tempfile.TemporaryDirectory() as directory:
            store = Store(Path(directory) / "hub.sqlite3")
            store.start_run({"world_number": 1, "id": "quiet-harbor-01", "name": "Тихая бухта"}, 8080)
            self.assertEqual(store.leaderboard("all", {})["teams"], [])
            self.assertEqual(store.leaderboard("world", {}, world_number=1)["teams"], [])
            store.db.close()

    def test_aggregate_rank_uses_total_collected_gold(self):
        with tempfile.TemporaryDirectory() as directory:
            store = Store(Path(directory) / "hub.sqlite3")
            stronger_single = token_id("single")
            consistent_team = token_id("consistent")
            world = {"world_number": 1, "id": "quiet-harbor-01", "name": "Тихая бухта"}
            first, _ = store.start_run(world, 8080)
            second, _ = store.start_run(world, 8080)
            store.ingest(first, [
                {"team_id": stronger_single, "gold_collected": 180},
                {"team_id": consistent_team, "gold_collected": 120},
            ], {})
            store.ingest(second, [
                {"team_id": consistent_team, "gold_collected": 120},
            ], {})
            teams = store.leaderboard("world", {}, world_number=1)["teams"]
            self.assertEqual(teams[0]["team_id"], consistent_team)
            self.assertEqual(teams[0]["top"]["gold_collected"], 120)
            self.assertEqual(teams[0]["total"]["gold_collected"], 240)
            store.db.close()

    def test_registry_file_is_not_part_of_public_reports(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "registry.json"
            registry = TeamRegistry(path)
            public = registry.register("do-not-publish", "Visible Name")
            self.assertEqual(set(public), {"team_id", "name"})
            self.assertEqual(json.loads(path.read_text(encoding="utf-8"))["teams"][0]["token"], "do-not-publish")

    def test_legacy_run_database_migrates_without_world_seed(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "hub.sqlite3"
            import sqlite3
            legacy = sqlite3.connect(path)
            legacy.executescript("""
                CREATE TABLE runs (
                    id TEXT PRIMARY KEY, world_number INTEGER NOT NULL,
                    world_seed INTEGER NOT NULL, seed_occurrence INTEGER NOT NULL DEFAULT 1,
                    run_number INTEGER NOT NULL DEFAULT 1, arena_name TEXT NOT NULL,
                    port INTEGER NOT NULL, started_at REAL NOT NULL, ended_at REAL,
                    status TEXT NOT NULL, error TEXT
                );
                CREATE TABLE run_team_stats (
                    run_id TEXT NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
                    team_id TEXT NOT NULL, fallback_name TEXT NOT NULL, gold INTEGER NOT NULL,
                    gold_collected INTEGER NOT NULL, carpets_lost INTEGER NOT NULL,
                    distance_travelled REAL NOT NULL, updated_at REAL NOT NULL,
                    PRIMARY KEY(run_id, team_id)
                );
                INSERT INTO runs VALUES ('old', 1, 1042, 1, 1, 'world_seed_1042_1_1', 8080, 1, NULL, 'complete', NULL);
                INSERT INTO run_team_stats VALUES ('old', '0123456789abcdef', 'Old', 10, 20, 1, 30, 1);
            """)
            legacy.close()
            store = Store(path)
            columns = {row[1] for row in store.db.execute("PRAGMA table_info(runs)")}
            self.assertNotIn("world_seed", columns)
            self.assertIn("world_id", columns)
            self.assertEqual(store.runs()["runs"][0]["arena_name"], "world_legacy_1_1_1")
            self.assertEqual(store.leaderboard("run", {}, run_id="old")["teams"][0]["gold_collected"], 20)
            self.assertEqual(store.db.execute("PRAGMA foreign_key_check").fetchall(), [])
            store.db.close()


if __name__ == "__main__":
    unittest.main()
