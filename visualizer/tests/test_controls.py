"""Tests for observation/manual-control mode separation."""

import pygame

from visualizer.client import WorldSnapshot
from visualizer.config import VisualizerConfig
from visualizer.main import VisualizerApp


def test_selecting_carpet_only_watches_until_manual_key_is_pressed(tmp_path, monkeypatch):
    monkeypatch.setenv("SDL_VIDEODRIVER", "dummy")
    manual_file = tmp_path / "manual.json"
    monkeypatch.setenv("DATS_MANUAL_CONTROL_FILE", str(manual_file))
    pygame.init()
    app = VisualizerApp(VisualizerConfig(auth_token="test", screen_width=320, screen_height=240))
    snapshot = WorldSnapshot.from_dict({
        "name": "team",
        "maxAccel": 40,
        "maxSpeed": 110,
        "transports": [
            {"id": f"team_{index}", "x": 100 + index * 20, "y": 100, "status": "alive"}
            for index in range(5)
        ],
    })
    app.buffer.update(snapshot)

    try:
        pygame.event.post(pygame.event.Event(pygame.KEYDOWN, key=pygame.K_3))
        assert app._handle_events(0.016)
        assert app.selected_carpet_id == "team_2"
        assert not app.manual_control_enabled
        assert not manual_file.exists()

        pygame.event.post(pygame.event.Event(pygame.KEYDOWN, key=pygame.K_m))
        assert app._handle_events(0.016)
        assert app.manual_control_enabled
        assert manual_file.exists()

        pygame.event.post(pygame.event.Event(pygame.KEYDOWN, key=pygame.K_m))
        assert app._handle_events(0.016)
        assert not app.manual_control_enabled
        assert not manual_file.exists()
    finally:
        if app._manual_control_id:
            app._refresh_manual_control_lease(None)
        pygame.quit()


def test_visualization_sliders_adjust_fan_density(tmp_path, monkeypatch):
    monkeypatch.setenv("SDL_VIDEODRIVER", "dummy")
    monkeypatch.setenv("DATS_MANUAL_CONTROL_FILE", str(tmp_path / "manual.json"))
    pygame.init()
    app = VisualizerApp(VisualizerConfig(auth_token="test", screen_width=800, screen_height=600))
    panel_x = app.config.screen_width - 230 - 15
    track = app.hud._slider_track(panel_x, 15 + 176)

    try:
        pygame.event.post(pygame.event.Event(
            pygame.MOUSEBUTTONDOWN, button=1, pos=(track.left, track.centery)
        ))
        assert app._handle_events(0.016)
        assert app.config.trajectory_fan_step_degrees == 1.0

        pygame.event.post(pygame.event.Event(
            pygame.MOUSEMOTION, pos=(track.right, track.centery), rel=(track.width, 0), buttons=(1, 0, 0)
        ))
        assert app._handle_events(0.016)
        assert app.config.trajectory_fan_step_degrees == 20.0

        pygame.event.post(pygame.event.Event(pygame.MOUSEBUTTONUP, button=1, pos=(track.right, track.centery)))
        assert app._handle_events(0.016)

        route_track = app.hud._slider_track(panel_x, 15 + 207)
        pygame.event.post(pygame.event.Event(
            pygame.MOUSEBUTTONDOWN, button=1, pos=(route_track.left, route_track.centery)
        ))
        assert app._handle_events(0.016)
        assert app.config.trajectory_fan_max_routes == 1
        pygame.event.post(pygame.event.Event(
            pygame.MOUSEMOTION,
            pos=(route_track.right, route_track.centery),
            rel=(route_track.width, 0),
            buttons=(1, 0, 0),
        ))
        assert app._handle_events(0.016)
        assert app.config.trajectory_fan_max_routes == 10
        pygame.event.post(pygame.event.Event(
            pygame.MOUSEBUTTONUP, button=1, pos=(route_track.right, route_track.centery)
        ))
        assert app._handle_events(0.016)
    finally:
        pygame.quit()
