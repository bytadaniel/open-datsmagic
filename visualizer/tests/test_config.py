"""Token configuration behavior for the Pygame visualizer."""

import sys

import pytest

from visualizer import main


def test_explicit_cli_token_is_used(monkeypatch):
    monkeypatch.setattr(sys, "argv", ["visualizer", "--token", "player_2"])
    config = main.parse_args()

    assert config.auth_token == "player_2"


def test_cli_requires_token_flag(monkeypatch):
    monkeypatch.setattr(sys, "argv", ["visualizer"])

    with pytest.raises(SystemExit) as error:
        main.parse_args()

    assert error.value.code == 2


def test_cli_rejects_empty_token(monkeypatch):
    monkeypatch.setattr(sys, "argv", ["visualizer", "--token", "   "])

    with pytest.raises(SystemExit) as error:
        main.parse_args()

    assert error.value.code == 2


def test_trajectory_defaults_match_player_planner(monkeypatch):
    monkeypatch.setenv("DATS_PLAN_HORIZON_SECONDS", "12")
    config = main.VisualizerConfig(auth_token="test")

    assert config.trajectory_horizon_seconds == 12.0
    assert config.trajectory_step_seconds == 0.2
    assert config.trajectory_background_step_seconds == 1.0
    assert config.trajectory_fan_step_degrees == 6.0
    assert config.trajectory_fan_max_routes == 5


def test_trajectory_horizon_uses_player_override(monkeypatch):
    monkeypatch.setenv("DATS_PLAN_HORIZON_SECONDS", "17.5")
    config = main.VisualizerConfig(auth_token="test")

    assert config.trajectory_horizon_seconds == 17.5


def test_manual_control_path_hash_matches_player_two_contract():
    assert main.token_control_hash("player_2") == 0x20252DF859219FAD
