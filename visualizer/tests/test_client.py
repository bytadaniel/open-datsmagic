"""Unit-тесты для клиента и камеры визуализатора (FE-005)."""

import pytest
import pygame
from visualizer.client import (
    AnomalyState,
    CarpetState,
    EnemyCarpetState,
    EnemyState,
    PlayerState,
    TreasureState,
    Vector2D,
    WorldSnapshot,
    ThreadSafeSnapshotBuffer,
    interpolate_snapshot,
)
from visualizer.renderer import Camera
from visualizer.renderer import SceneRenderer
from visualizer.config import VisualizerConfig


def test_world_to_screen_inversion():
    """FE-005 TC-VIS-01: Проверка того, что точка с большим Y отображается выше по экрану (с меньшим экранным y)."""
    camera = Camera(offset_x=0.0, offset_y=0.0, zoom=1.0, screen_width=1000, screen_height=800)

    # Точка 1 в начале координат
    sx0, sy0 = camera.world_to_screen(0.0, 0.0)
    assert sx0 == 500
    assert sy0 == 400

    # Точка 2 выше по Y (Y = 100)
    sx_high, sy_high = camera.world_to_screen(0.0, 100.0)
    assert sx_high == 500
    # В экранных координатах точка выше должна иметь меньший y
    assert sy_high < sy0
    assert sy_high == 300  # 400 - 100 = 300

    # Точка 3 ниже по Y (Y = -100)
    sx_low, sy_low = camera.world_to_screen(0.0, -100.0)
    assert sx_low == 500
    assert sy_low > sy0
    assert sy_low == 500  # 400 - (-100) = 500


def test_camera_zoom():
    """FE-005 TC-VIS-01: Проверка масштабирования расстояний при зуме."""
    camera_normal = Camera(offset_x=0.0, offset_y=0.0, zoom=1.0, screen_width=1000, screen_height=800)
    camera_zoomed = Camera(offset_x=0.0, offset_y=0.0, zoom=2.0, screen_width=1000, screen_height=800)

    p1 = (0.0, 0.0)
    p2 = (50.0, 0.0)

    s1_norm = camera_normal.world_to_screen(*p1)
    s2_norm = camera_normal.world_to_screen(*p2)
    dist_norm = abs(s2_norm[0] - s1_norm[0])

    s1_zoom = camera_zoomed.world_to_screen(*p1)
    s2_zoom = camera_zoomed.world_to_screen(*p2)
    dist_zoom = abs(s2_zoom[0] - s1_zoom[0])

    # При 2-кратном зуме расстояние на экране должно увеличиться ровно в 2 раза
    assert dist_norm == 50
    assert dist_zoom == 100
    assert dist_zoom == dist_norm * 2


def test_screen_to_world_roundtrip():
    """Проверка взаимно-однозначного преобразования world <-> screen."""
    camera = Camera(offset_x=123.45, offset_y=-67.89, zoom=1.5, screen_width=1280, screen_height=720)

    orig_wx = 450.25
    orig_wy = 890.75

    sx, sy = camera.world_to_screen(orig_wx, orig_wy)
    back_wx, back_wy = camera.screen_to_world(sx, sy)

    assert abs(back_wx - orig_wx) < 1.0  # погрешность из-за дискретизации в целые пиксели
    assert abs(back_wy - orig_wy) < 1.0


def test_snapshot_interpolation_uses_midpoint_for_matching_carpet():
    """FE-014: между снапшотами ковер плавно проходит промежуточную позицию."""
    previous_player = PlayerState(
        id="team", score=0, status="normal", position=Vector2D(0.0, 0.0), velocity=Vector2D(10.0, 0.0),
        carpets=[CarpetState("team_0", "normal", Vector2D(0.0, 0.0), Vector2D(10.0, 0.0))],
    )
    current_player = PlayerState(
        id="team", score=0, status="normal", position=Vector2D(10.0, 0.0), velocity=Vector2D(10.0, 0.0),
        carpets=[CarpetState("team_0", "normal", Vector2D(10.0, 0.0), Vector2D(10.0, 0.0))],
    )
    frame = interpolate_snapshot(
        WorldSnapshot(tick=1, game_status="active", player=previous_player),
        WorldSnapshot(tick=2, game_status="active", player=current_player),
        0.5,
    )

    assert frame.player.position == Vector2D(5.0, 0.0)
    assert frame.player.carpets[0].position == Vector2D(5.0, 0.0)


def test_camera_centering_and_panning():
    """Проверка центрирования и панорамирования камеры."""
    camera = Camera(offset_x=0.0, offset_y=0.0, zoom=1.0, screen_width=1000, screen_height=800)

    camera.center_on(250.0, 350.0)
    assert camera.offset_x == 250.0
    assert camera.offset_y == 350.0

    # Центр мира должен проецироваться ровно в центр экрана
    sx, sy = camera.world_to_screen(250.0, 350.0)
    assert sx == 500
    assert sy == 400

    # Панорамирование
    camera.pan(100.0, 50.0)
    assert camera.offset_x == 150.0  # 250 - 100/1.0
    assert camera.offset_y == 400.0  # 350 + 50/1.0


def test_observed_trajectory_is_detailed_and_other_paths_are_sparse(monkeypatch):
    pygame.font.init()
    config = VisualizerConfig(
        auth_token="test",
        trajectory_horizon_seconds=2.0,
        trajectory_step_seconds=0.2,
        trajectory_background_step_seconds=1.0,
    )
    renderer = SceneRenderer(config)
    camera = Camera(100.0, 100.0, 1.0, 320, 240)
    carpet = CarpetState("carpet", "normal", Vector2D(100.0, 100.0), Vector2D(0.0, 0.0))
    colors = []
    monkeypatch.setattr(pygame.draw, "circle", lambda _surface, color, *_args, **_kwargs: colors.append(color))

    renderer.draw_trajectory(pygame.Surface((320, 240)), camera, carpet, is_active=False)
    assert colors == []
    assert config.colors.TRAJECTORY_BACKGROUND != config.colors.BACKGROUND

    colors.clear()
    renderer.draw_trajectory(pygame.Surface((320, 240)), camera, carpet, is_active=True)
    assert len(colors) == 10
    assert set(colors) == {config.colors.TRAJECTORY_DOT}


def test_trajectory_crossing_coin_uses_third_color(monkeypatch):
    pygame.font.init()
    config = VisualizerConfig(auth_token="test", trajectory_horizon_seconds=1.0, trajectory_step_seconds=1.0)
    renderer = SceneRenderer(config)
    camera = Camera(0.0, 0.0, 1.0, 320, 240)
    carpet = CarpetState("carpet", "normal", Vector2D(0.0, 0.0), Vector2D(10.0, 0.0))
    coin_index = renderer.build_treasure_index([
        TreasureState("coin", "bounty", Vector2D(8.0, 1.0), 50),
    ])
    colors = []
    monkeypatch.setattr(pygame.draw, "circle", lambda _surface, color, *_args, **_kwargs: colors.append(color))

    renderer.draw_trajectory(pygame.Surface((320, 240)), camera, carpet, treasure_index=coin_index)

    assert colors
    assert set(colors) == {config.colors.TRAJECTORY_COIN}


def test_coin_remains_visible_with_dark_rim_at_minimum_zoom(monkeypatch):
    pygame.font.init()
    config = VisualizerConfig(auth_token="test")
    renderer = SceneRenderer(config)
    camera = Camera(0.0, 0.0, 0.1, 320, 240)
    coin = TreasureState("coin", "bounty", Vector2D(20.0, 20.0), 50)
    circles = []
    monkeypatch.setattr(
        pygame.draw,
        "circle",
        lambda _surface, color, _center, radius, *args, **kwargs: circles.append((color, radius)),
    )

    renderer.draw_treasure(pygame.Surface((320, 240)), camera, coin)

    assert circles == [
        (config.colors.TREASURE_BORDER, 3),
        (config.colors.COIN_GOLD, 2),
    ]


def test_doomed_trajectory_is_drawn_red(monkeypatch):
    pygame.font.init()
    config = VisualizerConfig(
        auth_token="test", arena_width=5.0, trajectory_horizon_seconds=1.0,
        trajectory_background_step_seconds=1.0,
    )
    renderer = SceneRenderer(config)
    camera = Camera(0.0, 0.0, 1.0, 320, 240)
    carpet = CarpetState("carpet", "normal", Vector2D(0.0, 0.0), Vector2D(10.0, 0.0))
    colors = []
    lines = []
    monkeypatch.setattr(pygame.draw, "circle", lambda _surface, color, *_args, **_kwargs: colors.append(color))
    monkeypatch.setattr(
        pygame.draw, "line",
        lambda _surface, color, start, end, width=1: lines.append((color, start, end, width)),
    )

    renderer.draw_trajectory(pygame.Surface((320, 240)), camera, carpet)

    assert colors
    assert set(colors) == {config.colors.TRAJECTORY_DEATH}
    assert len(lines) == 2
    assert all(line[3] == 2 for line in lines)
    assert all(max(abs(line[2][axis] - line[1][axis]) for axis in (0, 1)) == 8 for line in lines)


def test_scan_fan_keeps_best_safe_score_rate_routes(monkeypatch):
    pygame.font.init()
    config = VisualizerConfig(
        auth_token="test", arena_width=10000.0, arena_height=10000.0,
        trajectory_horizon_seconds=2.0, trajectory_command_lead_seconds=0.0,
        trajectory_fan_step_degrees=90.0, trajectory_fan_max_routes=1,
    )
    renderer = SceneRenderer(config)
    camera = Camera(5000.0, 5000.0, 1.0, 320, 240)
    carpet = CarpetState("carpet", "normal", Vector2D(5000.0, 5000.0), Vector2D(0.0, 0.0), max_acceleration=40.0, max_velocity=110.0)
    coin_index = renderer.build_treasure_index([
        TreasureState("coin", "bounty", Vector2D(5015.0, 5000.0), 100),
    ])
    colors = []
    monkeypatch.setattr(pygame.draw, "circle", lambda _surface, color, *_args, **_kwargs: colors.append(color))

    selected = renderer.draw_scan_fan(
        pygame.Surface((320, 240)), camera, carpet, treasure_index=coin_index, max_routes=1
    )

    assert len(selected) == 1
    assert selected[0].angle_degrees == 0.0
    assert selected[0].route_score == 100
    assert selected[0].time_to_last_bounty is not None
    assert selected[0].score_rate == pytest.approx(
        selected[0].route_score / max(0.2, selected[0].time_to_last_bounty)
    )
    assert set(colors) == {config.colors.TRAJECTORY_COIN}


def test_snapshot_deserialization_from_desert_api_format():
    """Визуализатор читает канонический ответ Desert, а не переходный API."""
    json_data = {
        "tick": 1042,
        "name": "team_20",
        "points": 1420,
        "maxAccel": 5.0,
        "maxSpeed": 20.0,
        "mapSize": {"x": 12000.0, "y": 8000.0},
        "transportRadius": 7.0,
        "transports": [{
            "id": "team_20_0", "x": 412.5, "y": 890.2, "status": "alive",
            "velocity": {"x": 8.5, "y": -4.1},
            "selfAcceleration": {"x": 1.0, "y": 0.0},
            "anomalyAcceleration": {"x": -0.5, "y": 0.25},
        }],
        "bounties": [{"x": 450.0, "y": 920.0, "points": 50, "radius": 5}],
        "anomalies": [
            {
                "id": "a_3", "x": 400.0, "y": 900.0, "radius": 15.0,
                "effectiveRadius": 40.0, "strength": -3.5, "velocity": {"x": 0.0, "y": 0.0},
            }
        ],
        "enemies": [{"x": 430.0, "y": 910.0, "velocity": {"x": 12.0, "y": 2.1}}],
    }

    snapshot = WorldSnapshot.from_dict(json_data)

    assert snapshot.tick == 1042
    assert snapshot.game_status == "active"
    assert snapshot.map_size == Vector2D(12000.0, 8000.0)
    assert snapshot.transport_radius == 7.0

    # Игрок
    assert snapshot.player.id == "team_20"
    assert snapshot.player.score == 1420
    assert snapshot.player.status == "normal"
    assert not snapshot.player.is_stunned
    assert snapshot.player.position == Vector2D(412.5, 890.2)
    assert snapshot.player.velocity == Vector2D(8.5, -4.1)

    # Сокровища
    assert len(snapshot.treasures) == 1
    assert snapshot.treasures[0].id == "bounty-0"
    assert snapshot.treasures[0].value == 50

    # Аномалии
    assert len(snapshot.anomalies) == 1
    assert snapshot.anomalies[0].id == "a_3"
    assert snapshot.anomalies[0].core_radius == 15.0
    assert snapshot.anomalies[0].radius == 40.0
    assert snapshot.anomalies[0].force == 3.5
    assert snapshot.anomalies[0].is_repelling

    # Противники
    assert len(snapshot.enemies) == 1
    assert snapshot.enemies[0].id == "enemy-0"
    assert snapshot.enemies[0].velocity == Vector2D(12.0, 2.1)


def test_stunned_flag_property():
    """Проверка свойства is_stunned у игрока."""
    player_normal = PlayerState(
        id="p1", score=0, status="normal",
        position=Vector2D(0, 0), velocity=Vector2D(0, 0),
        max_acceleration=5.0, max_velocity=20.0,
    )
    assert not player_normal.is_stunned

    player_stunned = PlayerState(
        id="p1", score=0, status="stunned",
        position=Vector2D(0, 0), velocity=Vector2D(0, 0),
        max_acceleration=5.0, max_velocity=20.0,
    )
    assert player_stunned.is_stunned


def test_thread_safe_buffer():
    """Проверка работы потокобезопасного буфера снимков."""
    buf = ThreadSafeSnapshotBuffer()
    assert buf.get_latest() is None
    connected, err, _ = buf.get_status()
    assert not connected
    assert err is None

    # Запись ошибки
    buf.record_error("Connection refused")
    connected, err, _ = buf.get_status()
    assert not connected
    assert err == "Connection refused"

    # Запись снимка
    player = PlayerState("p1", 100, "normal", Vector2D(1, 2), Vector2D(3, 4), 5.0, 20.0)
    snap = WorldSnapshot(tick=1, game_status="active", player=player)
    buf.update(snap)

    latest = buf.get_latest()
    assert latest is not None
    assert latest.tick == 1
    connected, err, _ = buf.get_status()
    assert connected
    assert err is None


def test_api_client_mock_fetch():
    """Проверка ApiClient с моком ответа от сервера."""
    from unittest.mock import MagicMock
    from visualizer.client import ApiClient

    client = ApiClient(base_url="http://mock-server:8080", token="test_token")
    mock_resp = MagicMock()
    mock_resp.status_code = 200
    mock_resp.json.return_value = {
        "name": "team_test", "points": 500, "maxAccel": 5.0, "maxSpeed": 20.0,
        "transports": [{
            "id": "team_test_0", "x": 10.0, "y": 20.0, "status": "alive",
            "velocity": {"x": 1.0, "y": -1.0}, "selfAcceleration": {"x": 0.0, "y": 0.0},
            "anomalyAcceleration": {"x": 0.0, "y": 0.0},
        }],
        "bounties": [],
        "anomalies": [],
        "enemies": [],
    }

    client.session.post = MagicMock(return_value=mock_resp)

    snapshot = client.get_desert()
    assert snapshot.tick == 1
    assert snapshot.player.id == "team_test"
    client.session.post.assert_called_once_with(
        "http://mock-server:8080/play/magcarp/player/move",
        json={"transports": []},
        timeout=1.0,
    )


def test_headless_rendering_pipeline():
    """Проверка отрисовки сущностей и HUD на Pygame Surface в headless режиме."""
    import os
    import pygame
    from visualizer.config import VisualizerConfig
    from visualizer.renderer import SceneRenderer, Camera
    from visualizer.hud import HudRenderer

    os.environ["SDL_VIDEODRIVER"] = "dummy"
    pygame.init()
    pygame.font.init()

    config = VisualizerConfig(screen_width=800, screen_height=600)
    screen = pygame.display.set_mode((800, 600))
    camera = Camera(offset_x=0.0, offset_y=0.0, zoom=1.0, screen_width=800, screen_height=600)
    renderer = SceneRenderer(config)
    hud = HudRenderer(config)

    # Отрисовка сетки
    renderer.draw_grid(screen, camera)

    # Отрисовка аномалии
    anomaly = AnomalyState(id="a1", position=Vector2D(50.0, 50.0), radius=40.0, force=5.0)
    renderer.draw_anomaly(screen, camera, anomaly, current_time=1.0)

    # Отрисовка сокровища
    treasure = TreasureState(id="t1", type="chest", position=Vector2D(100.0, 100.0), value=150)
    renderer.draw_treasure(screen, camera, treasure)

    # Отрисовка игрока в оглушении
    player = PlayerState(
        id="player_hero", score=1000, status="stunned",
        position=Vector2D(0.0, 0.0), velocity=Vector2D(5.0, 0.0),
        max_acceleration=5.0, max_velocity=20.0,
    )
    renderer.draw_carpet(screen, camera, player, is_local_player=True, current_time=0.5)
    renderer.draw_trajectory(screen, camera, player)

    # Отрисовка соперника
    enemy = EnemyState(id="enemy_1", position=Vector2D(-50.0, 20.0), velocity=Vector2D(0.0, 2.0))
    renderer.draw_carpet(screen, camera, enemy, is_local_player=False, current_time=0.5)

    # Отрисовка HUD
    snap = WorldSnapshot(tick=100, game_status="active", player=player, treasures=[treasure], anomalies=[anomaly], enemies=[enemy])
    hud.draw(screen, snap, is_connected=True, fps=60.0, zoom=1.0, follow_mode=True)

    pygame.quit()


@pytest.mark.skip(reason="переходный /api/game/state удален; Desert проверяется выше")
def test_deserialize_dynamic_anomalies():
    """FE-008 TC-VIS-ANOM-01: Проверка десериализации расширенного состояния динамических аномалий."""
    json_data = {
        "tick": 420,
        "game_status": "active",
        "player": {
            "id": "team_1",
            "score": 120,
            "status": "destroyed",
            "position": {"x": 340.5, "y": 720.0},
            "velocity": {"x": 0.0, "y": 0.0},
            "max_acceleration": 5.0,
            "max_velocity": 20.0,
        },
        "treasures": [],
        "anomalies": [
            {
                "id": "a_dyn_1",
                "type": "attracting",
                "position": {"x": 350.0, "y": 725.0},
                "velocity": {"x": 5.0, "y": -1.2},
                "core_radius": 15.0,
                "radius": 80.0,
                "force": 4.5,
            },
            {
                "id": "a_dyn_2",
                "type": "repelling",
                "position": {"x": 100.0, "y": 900.0},
                "velocity": {"x": -2.0, "y": 3.0},
                "core_radius": 12.0,
                "radius": 60.0,
                "force": 3.0,
            },
        ],
        "enemies": [],
    }

    snapshot = WorldSnapshot.from_dict(json_data)

    # Игрок
    assert snapshot.player.id == "team_1"
    assert snapshot.player.score == 120
    assert snapshot.player.status == "destroyed"
    assert snapshot.player.is_destroyed
    assert not snapshot.player.is_stunned

    # Аномалии
    assert len(snapshot.anomalies) == 2

    a1 = snapshot.anomalies[0]
    assert a1.id == "a_dyn_1"
    assert a1.anomaly_type == "attracting"
    assert a1.is_attracting
    assert not a1.is_repelling
    assert a1.position == Vector2D(350.0, 725.0)
    assert a1.velocity == Vector2D(5.0, -1.2)
    assert a1.core_radius == 15.0
    assert a1.effect_radius == 80.0
    assert a1.radius == 80.0
    assert a1.force == 4.5

    a2 = snapshot.anomalies[1]
    assert a2.id == "a_dyn_2"
    assert a2.anomaly_type == "repelling"
    assert a2.is_repelling
    assert not a2.is_attracting
    assert a2.position == Vector2D(100.0, 900.0)
    assert a2.velocity == Vector2D(-2.0, 3.0)
    assert a2.core_radius == 12.0
    assert a2.effect_radius == 60.0
    assert a2.radius == 60.0
    assert a2.force == 3.0


def test_backward_compatible_anomaly_parsing():
    """FE-008: Проверка безопасных fallback-значений при парсинге старого формата API."""
    data = {
        "id": "old_anomaly",
        "position": {"x": 200.0, "y": 300.0},
        "radius": 50.0,
        "force": 4.0,
    }
    anomaly = AnomalyState.from_dict(data)

    assert anomaly.id == "old_anomaly"
    assert anomaly.position == Vector2D(200.0, 300.0)
    assert anomaly.effect_radius == 50.0
    assert anomaly.radius == 50.0
    # Fallback: core_radius = radius * 0.2
    assert anomaly.core_radius == pytest.approx(10.0)
    # Fallback: type = "attracting"
    assert anomaly.anomaly_type == "attracting"
    assert anomaly.is_attracting
    # Fallback: velocity = Vector2D(0.0, 0.0)
    assert anomaly.velocity == Vector2D(0.0, 0.0)
    assert anomaly.force == 4.0


def test_render_dynamic_anomaly_headless():
    """FE-008 TC-VIS-ANOM-02: Headless рендеринг притягивающей и отталкивающей аномалий, векторов скорости и гибели ковра."""
    import os
    import pygame
    from visualizer.config import VisualizerConfig
    from visualizer.renderer import SceneRenderer, Camera
    from visualizer.hud import HudRenderer

    os.environ["SDL_VIDEODRIVER"] = "dummy"
    pygame.init()
    pygame.font.init()

    config = VisualizerConfig(screen_width=800, screen_height=600)
    screen = pygame.display.set_mode((800, 600))
    camera = Camera(offset_x=200.0, offset_y=200.0, zoom=1.0, screen_width=800, screen_height=600)
    renderer = SceneRenderer(config)
    hud = HudRenderer(config)

    # 1. Притягивающая аномалия с вектором скорости
    a_attract = AnomalyState(
        id="dyn_attract",
        anomaly_type="attracting",
        position=Vector2D(200.0, 200.0),
        velocity=Vector2D(10.0, -5.0),
        core_radius=15.0,
        effect_radius=60.0,
        force=3.5,
    )
    renderer.draw_anomaly(screen, camera, a_attract, current_time=0.5)

    # 2. Отталкивающая аномалия с вектором скорости
    a_repel = AnomalyState(
        id="dyn_repel",
        anomaly_type="repelling",
        position=Vector2D(350.0, 200.0),
        velocity=Vector2D(-4.0, 8.0),
        core_radius=12.0,
        effect_radius=50.0,
        force=2.0,
    )
    renderer.draw_anomaly(screen, camera, a_repel, current_time=1.2)

    # 3. Уничтоженный ковер игрока
    destroyed_player = PlayerState(
        id="team_rip",
        score=250,
        status="destroyed",
        position=Vector2D(200.0, 200.0),
        velocity=Vector2D(0.0, 0.0),
        max_acceleration=5.0,
        max_velocity=20.0,
    )
    renderer.draw_carpet(screen, camera, destroyed_player, is_local_player=True, current_time=1.0)

    # 4. HUD с аварийным баннером CARPET DESTROYED
    snap = WorldSnapshot(
        tick=500,
        game_status="active",
        player=destroyed_player,
        anomalies=[a_attract, a_repel],
    )
    hud.draw(screen, snap, is_connected=True, fps=60.0, zoom=1.0, follow_mode=False)

    pygame.quit()


def test_arena_boundaries_and_circular_entities_render_headless():
    """Тестирование отрисовки границ арены, центра (500, 500) и круговых сущностей в headless-режиме."""
    import os
    os.environ["SDL_VIDEODRIVER"] = "dummy"
    import pygame
    from visualizer.config import VisualizerConfig
    from visualizer.renderer import Camera, SceneRenderer

    pygame.init()
    pygame.font.init()

    config = VisualizerConfig(
        arena_width=1000.0,
        arena_height=1000.0,
        carpet_radius=12.0,
        screen_width=1280,
        screen_height=720,
    )
    screen = pygame.display.set_mode((1280, 720))
    camera = Camera(
        offset_x=500.0,
        offset_y=500.0,
        zoom=0.6,
        screen_width=1280,
        screen_height=720,
    )
    renderer = SceneRenderer(config)

    # 1. Отрисовка границ арены и координатной сетки
    renderer.draw_grid(screen, camera)

    # 2. Отрисовка кругового ковра игрока в центре (500, 500)
    player = PlayerState(
        id="player_center",
        score=100,
        status="normal",
        position=Vector2D(500.0, 500.0),
        velocity=Vector2D(5.0, 2.0),
        max_acceleration=5.0,
        max_velocity=20.0,
    )
    renderer.draw_carpet(screen, camera, player, is_local_player=True, current_time=0.1)

    # 3. Отрисовка соперника
    enemy = EnemyState(
        id="enemy_1",
        position=Vector2D(400.0, 450.0),
        velocity=Vector2D(-2.0, 3.0),
    )
    renderer.draw_carpet(screen, camera, enemy, is_local_player=False, current_time=0.1)

    # 4. Отрисовка оглушенного игрока
    stunned_player = PlayerState(
        id="player_stunned",
        score=50,
        status="stunned",
        position=Vector2D(600.0, 550.0),
        velocity=Vector2D(0.0, 0.0),
        max_acceleration=5.0,
        max_velocity=20.0,
    )
    renderer.draw_carpet(screen, camera, stunned_player, is_local_player=True, current_time=0.1)

    # 5. Отрисовка круглого сокровища
    treasure = TreasureState(
        id="t_gold",
        type="chest",
        position=Vector2D(500.0, 400.0),
        value=150,
    )
    renderer.draw_treasure(screen, camera, treasure)

    # Пиксель в самом центре (640, 360) не должен быть пустой буферной зоной
    center_color = screen.get_at((640, 360))[:3]
    assert center_color != config.colors.BACKGROUND_OUTSIDE

    # Пиксель в крайнем левом углу экрана (50, 50) при zoom=0.6 и центре (500, 500) находится за пределами арены
    corner_color = screen.get_at((50, 50))[:3]
    assert corner_color == config.colors.BACKGROUND_OUTSIDE

    pygame.quit()


def test_tc_vis_fleet_01_render_player_and_enemy_fleets():
    """FE-012 TC-VIS-FLEET-01: Headless-рендер корректно отображает 5 ковров игрока и ковры соперников."""
    import os
    os.environ["SDL_VIDEODRIVER"] = "dummy"
    import pygame
    from visualizer.config import VisualizerConfig
    from visualizer.renderer import Camera, SceneRenderer
    from visualizer.hud import HudRenderer

    pygame.init()
    pygame.font.init()

    config = VisualizerConfig(screen_width=1280, screen_height=720)
    screen = pygame.display.set_mode((1280, 720))
    camera = Camera(offset_x=500.0, offset_y=500.0, zoom=1.0, screen_width=1280, screen_height=720)
    renderer = SceneRenderer(config)
    hud = HudRenderer(config)

    # 1. Создаем игрока с флотом из 5 ковров (один из них оглушен, один уничтожен)
    carpets = [
        CarpetState(id="team_alpha_0", status="normal", position=Vector2D(500.0, 500.0), velocity=Vector2D(2.0, 1.0)),
        CarpetState(id="team_alpha_1", status="stunned", position=Vector2D(470.0, 470.0), velocity=Vector2D(0.0, 0.0)),
        CarpetState(id="team_alpha_2", status="normal", position=Vector2D(530.0, 470.0), velocity=Vector2D(-1.0, 2.0)),
        CarpetState(id="team_alpha_3", status="destroyed", position=Vector2D(470.0, 530.0), velocity=Vector2D(0.0, 0.0)),
        CarpetState(id="team_alpha_4", status="normal", position=Vector2D(530.0, 530.0), velocity=Vector2D(1.0, -1.0)),
    ]
    player = PlayerState(
        id="team_alpha",
        score=350,
        status="normal",
        position=Vector2D(500.0, 500.0),
        velocity=Vector2D(2.0, 1.0),
        max_acceleration=5.0,
        max_velocity=20.0,
        carpets=carpets,
    )
    assert player.active_carpets_count == 4

    # 2. Создаем противника с флотом ковров
    enemy_carpets = [
        EnemyCarpetState(id="enemy_omega_0", position=Vector2D(700.0, 700.0), velocity=Vector2D(-3.0, 0.0)),
        EnemyCarpetState(id="enemy_omega_1", position=Vector2D(730.0, 700.0), velocity=Vector2D(-2.0, 1.0)),
    ]
    enemy = EnemyState(
        id="enemy_omega",
        position=Vector2D(700.0, 700.0),
        velocity=Vector2D(-3.0, 0.0),
        carpets=enemy_carpets,
    )

    # 3. Отрисовка флота игрока и флота противника
    renderer.draw_player_fleet(screen, camera, player, current_time=0.5)
    renderer.draw_enemy_fleet(screen, camera, enemy, current_time=0.5)

    # 4. Проверка и отрисовка анимации взрыва:
    # Разрушенный ковер team_alpha_3 автоматически породил 1 взрыв
    assert len(renderer.explosions) == 1
    # Добавляем еще один взрыв вручную
    renderer.add_explosion(700.0, 700.0)
    assert len(renderer.explosions) == 2
    renderer.draw_explosions(screen, camera, dt=0.016)

    # 5. Отрисовка расширенного HUD с метриками состава флота
    snap = WorldSnapshot(tick=120, game_status="active", player=player, enemies=[enemy])
    hud.draw(screen, snap, is_connected=True, fps=60.0, zoom=1.0, follow_mode=False)

    pygame.quit()


def test_tc_vis_coins_01_progressive_coin_palette():
    """FE-012 TC-VIS-COINS-01: Монеты с номиналами 50, 150, 300, 750 отрисовываются цветами палитры."""
    import os
    os.environ["SDL_VIDEODRIVER"] = "dummy"
    import pygame
    from visualizer.config import VisualizerConfig, Colors
    from visualizer.renderer import Camera, SceneRenderer

    pygame.init()
    pygame.font.init()

    config = VisualizerConfig(screen_width=800, screen_height=600)
    screen = pygame.display.set_mode((800, 600))
    camera = Camera(offset_x=250.0, offset_y=250.0, zoom=1.0, screen_width=800, screen_height=600)
    renderer = SceneRenderer(config)

    # 1. Проверка соответствия цветов градаций номиналов
    assert renderer.get_coin_tier_color(50) == (190, 110, 60)       # Bronze (Common)
    assert renderer.get_coin_tier_color(150) == (210, 220, 230)     # Silver
    assert renderer.get_coin_tier_color(300) == (255, 215, 0)       # Gold
    assert renderer.get_coin_tier_color(750) == (220, 20, 60)       # Legendary Ruby

    # Проверка приватного метода _get_coin_tier_color
    assert renderer._get_coin_tier_color(50) == Colors.COIN_COMMON
    assert renderer._get_coin_tier_color(150) == Colors.COIN_SILVER
    assert renderer._get_coin_tier_color(300) == Colors.COIN_GOLD
    assert renderer._get_coin_tier_color(750) == Colors.COIN_LEGENDARY

    # 2. Отрисовка монет каждого тира на Surface без падений
    coins = [
        TreasureState(id="coin_bronze", type="common", position=Vector2D(100.0, 100.0), value=50),
        TreasureState(id="coin_silver", type="silver", position=Vector2D(200.0, 100.0), value=150),
        TreasureState(id="coin_gold", type="gold", position=Vector2D(300.0, 100.0), value=300),
        TreasureState(id="coin_ruby", type="legendary", position=Vector2D(400.0, 100.0), value=750),
    ]

    for coin in coins:
        renderer.draw_treasure(screen, camera, coin, current_time=0.5)

    pygame.quit()
