"""Конфигурация графического клиента визуализации DatsMagic."""

from dataclasses import dataclass, field
from pathlib import Path
from typing import Tuple
import os


def _load_auth_token(token_file: str | Path | None = None) -> str:
    """Читает token-файл; явный CLI-токен должен обходить эту функцию целиком."""
    configured_path = token_file or os.getenv("DATS_PLAYER_TOKEN_FILE")
    if configured_path:
        token_path = Path(configured_path).expanduser()
    else:
        project_root = Path(__file__).resolve().parents[2]
        token_path = project_root / "players" / "player_1" / "token.txt"
    try:
        token = token_path.read_text(encoding="utf-8").strip()
    except OSError as exc:
        raise RuntimeError(f"Не удалось прочитать токен визуализатора: {token_path}") from exc
    if not token:
        raise RuntimeError(f"Файл токена визуализатора пуст: {token_path}")
    return token


@dataclass
class Colors:
    """Контрастная светлая палитра визуализатора."""
    # Светлая арена и сдержанная сетка
    BACKGROUND: Tuple[int, int, int] = (248, 250, 252)      # Почти белая арена
    BACKGROUND_OUTSIDE: Tuple[int, int, int] = (226, 232, 240) # Светло-серая внешняя область
    GRID: Tuple[int, int, int] = (226, 232, 240)            # Ненавязчивая сетка
    ARENA_BORDER: Tuple[int, int, int] = (51, 65, 85)       # Контрастная граница арены
    ARENA_CENTER: Tuple[int, int, int] = (100, 116, 139)    # Нейтральный центр

    # Ковер управляемого игрока
    PLAYER_BODY: Tuple[int, int, int] = (37, 99, 235)        # Синий ковер
    PLAYER_AURA: Tuple[int, int, int, int] = (245, 158, 11, 48) # Полупрозрачная янтарная аура
    PLAYER_BORDER: Tuple[int, int, int] = (180, 83, 9)      # Янтарная бахрома
    PLAYER_TEXT: Tuple[int, int, int] = (30, 64, 120)       # Текст метки игрока

    # Ковры соперников
    ENEMY_BODY: Tuple[int, int, int] = (220, 38, 38)         # Красный ковер
    ENEMY_AURA: Tuple[int, int, int, int] = (22, 163, 74, 42) # Полупрозрачная зеленая аура
    ENEMY_BORDER: Tuple[int, int, int] = (153, 27, 27)      # Темно-красная кайма

    # Три телеметрических вектора транспорта: V, S и W
    VELOCITY_ARROW: Tuple[int, int, int] = (5, 150, 105)      # Зеленая скорость V
    ACCELERATION_ARROW: Tuple[int, int, int] = (217, 119, 6)  # Янтарное собственное ускорение S
    ANOMALY_ACCELERATION_ARROW: Tuple[int, int, int] = (126, 34, 206) # Фиолетовое влияние аномалий W
    AIM_LINE: Tuple[int, int, int] = (71, 85, 105)            # Темно-серый прицельный отрезок мыши
    TRAJECTORY_DOT: Tuple[int, int, int] = (0, 74, 135)       # Текущая траектория выбранного ковра
    ENEMY_TRAJECTORY_DOT: Tuple[int, int, int] = (0, 130, 70) # Точки прогноза врагов
    TRAJECTORY_BACKGROUND: Tuple[int, int, int] = (34, 75, 110) # Неактивные прогнозы
    TRAJECTORY_COIN: Tuple[int, int, int] = (176, 35, 160)    # Пурпурный: маршруты через монеты
    TRAJECTORY_DEATH: Tuple[int, int, int] = (198, 40, 40)   # Смертельные маршруты

    # Сокровища и динамическая градация монет (FE-012 Section 2.1)
    TREASURE_CHEST: Tuple[int, int, int] = (212, 175, 55)    # Золотой сундук
    TREASURE_BORDER: Tuple[int, int, int] = (120, 73, 8)    # Темно-коричневый контур на светлом фоне
    TREASURE_TEXT: Tuple[int, int, int] = (90, 60, 10)       # Номинал сокровища
    COIN_COMMON: Tuple[int, int, int] = (190, 110, 60)       # Бронзовая монета (V < 100)
    COIN_SILVER: Tuple[int, int, int] = (210, 220, 230)     # Серебряная монета (100 <= V < 250)
    COIN_GOLD: Tuple[int, int, int] = (255, 215, 0)         # Золотая монета (250 <= V < 500)
    COIN_LEGENDARY: Tuple[int, int, int] = (220, 20, 60)     # Пурпурно-рубиновый артефакт (V >= 500)
    COIN_LEGENDARY_HALO: Tuple[int, int, int, int] = (220, 20, 60, 90) # Пульсирующий рубиновый ореол
    COIN_SHINE: Tuple[int, int, int] = (255, 255, 240)       # Блик на монете

    # Анимация взрыва и обломков (FE-012 Section 2)
    EXPLOSION_FIRE: Tuple[int, int, int] = (255, 100, 30)
    EXPLOSION_SPARK: Tuple[int, int, int] = (255, 220, 50)
    EXPLOSION_SMOKE: Tuple[int, int, int] = (80, 50, 40)

    # Аномалии (вихри)
    ANOMALY_OUTER: Tuple[int, int, int, int] = (100, 116, 139, 45)  # Нейтральный полупрозрачный fallback
    ANOMALY_INNER: Tuple[int, int, int, int] = (51, 65, 85, 100)    # Внутренний пояс
    ANOMALY_EPICENTER: Tuple[int, int, int, int] = (30, 41, 59, 140) # Эпицентр оглушения

    # Динамические аномалии (FE-008)
    ANOMALY_CORE_FILL: Tuple[int, int, int] = (30, 41, 59)              # Fallback для заливки ядра
    ANOMALY_CORE_BORDER: Tuple[int, int, int] = (153, 27, 27)           # Рамка ядра притяжения
    ANOMALY_CORE_MARKER: Tuple[int, int, int] = (153, 27, 27)           # Статичный маркер предупреждения
    ANOMALY_ATTRACT_OUTER: Tuple[int, int, int, int] = (220, 38, 38, 42) # Полупрозрачная красная зона притяжения
    ANOMALY_ATTRACT_SPIRAL: Tuple[int, int, int] = (185, 28, 45)        # Насыщенно-красное ядро притяжения
    ANOMALY_REPEL_OUTER: Tuple[int, int, int, int] = (37, 99, 235, 42) # Полупрозрачная синяя зона отталкивания
    ANOMALY_REPEL_WAVE: Tuple[int, int, int] = (29, 78, 216)           # Насыщенно-синее ядро отталкивания
    ANOMALY_VELOCITY: Tuple[int, int, int] = (71, 85, 105)             # Стрелка вектора скорости аномалии

    # Уничтожение ковра (FE-008)
    DESTROYED_BANNER_BG: Tuple[int, int, int, int] = (15, 10, 15, 235)  # Темная подложка баннера гибели
    DESTROYED_BANNER_BORDER: Tuple[int, int, int] = (200, 25, 35)       # Рамка глубокого огня
    DESTROYED_BANNER_TEXT: Tuple[int, int, int] = (255, 45, 45)         # Текст гибели цвета алого огня
    DESTROYED_MARKER: Tuple[int, int, int] = (192, 57, 43)              # Перечеркнутый маркер погибшего ковра

    # Статусы и HUD
    STUNNED_HALO: Tuple[int, int, int] = (231, 76, 60)       # Мигающий красный ореол
    HUD_BG: Tuple[int, int, int, int] = (20, 24, 30, 210)    # Полупрозрачная плашка HUD
    HUD_TEXT: Tuple[int, int, int] = (236, 240, 241)         # Белый текст
    HUD_ACCENT: Tuple[int, int, int] = (241, 196, 15)        # Желтый акцент
    HUD_SUCCESS: Tuple[int, int, int] = (46, 204, 113)       # Зеленый статус
    HUD_ALERT: Tuple[int, int, int] = (231, 76, 60)          # Красный статус


@dataclass
class VisualizerConfig:
    """Параметры графического клиента."""
    # Сетевые настройки
    server_url: str = os.getenv("DATS_SERVER_URL", "http://127.0.0.1:8080")
    auth_token: str = field(default_factory=_load_auth_token)
    poll_interval: float = 0.2          # Период опроса API (с)
    reconnect_interval: float = 1.0     # Период повторной попытки при потере связи (с)

    # Параметры графического окна
    screen_width: int = 1280
    screen_height: int = 800
    fps: int = 60
    window_title: str = "DatsMagic 2D Simulator Visualizer"

    # Геометрия игрового мира и арены
    arena_width: float = 9000.0         # Ширина игровой арены (X в [0, 9000])
    arena_height: float = 9000.0        # Высота игровой арены (Y в [0, 9000])
    carpet_radius: float = 12.0         # Радиус ковра (нематериальная точка-масса с ореолом взаимодействия)
    max_velocity: float = 110.0         # maxSpeed; радиус ауры по умолчанию для вражеских ковров
    max_acceleration: float = 40.0      # maxAccel; предел для прогнозного ускорения
    physics_tick_seconds: float = 0.2   # Базовый тик сервера для аппроксимации трения в подшагах
    friction: float = 0.98              # Коэффициент трения по умолчанию сервера
    # player_2 default (DATS_PLAN_HORIZON_SECONDS) and planner tick.
    trajectory_horizon_seconds: float = field(
        default_factory=lambda: _positive_env_float("DATS_PLAN_HORIZON_SECONDS", 12.0)
    )
    trajectory_step_seconds: float = field(
        default_factory=lambda: _positive_env_float("DATS_VISUALIZER_STEP_SECONDS", 0.2)
    )
    trajectory_background_step_seconds: float = field(
        default_factory=lambda: _positive_env_float("DATS_VISUALIZER_BACKGROUND_STEP_SECONDS", 1.0)
    )
    trajectory_command_lead_seconds: float = field(
        default_factory=lambda: _positive_env_float("DATS_VISUALIZER_COMMAND_LEAD_SECONDS", 0.3)
    )
    trajectory_fan_step_degrees: float = field(
        default_factory=lambda: _positive_env_float("DATS_VISUALIZER_FAN_STEP_DEGREES", 6.0)
    )
    trajectory_fan_max_routes: int = field(
        default=5
    )
    manual_control_file: Path = field(
        default_factory=lambda: Path(
            os.getenv(
                "DATS_MANUAL_CONTROL_FILE",
                str(Path(__file__).resolve().parents[2] / "players" / "player_2" / "manual_control.json"),
            )
        ).expanduser()
    )

    # Параметры отрисовки
    velocity_scale: float = 1.8         # Масштаб отображения стрелки скорости
    position_scale: float = 0.05        # Масштаб перенесенного к ковру вектора положения
    acceleration_scale: float = 8.0     # Масштаб отображения стрелки ускорения
    carpet_width: float = 24.0          # Для обратной совместимости
    carpet_height: float = 14.0         # Для обратной совместимости
    min_zoom: float = 0.2               # Минимальный зум
    max_zoom: float = 4.0               # Максимальный зум
    zoom_step: float = 1.15             # Коэффициент шага зума
    pan_speed: float = 450.0            # Скорость перемещения камеры (пикс/с)
    show_grid: bool = False              # Координатная сетка выключена по умолчанию
    # Цветовая палитра
    colors: Colors = field(default_factory=Colors)


def _positive_env_float(name: str, default: float) -> float:
    try:
        value = float(os.getenv(name, str(default)))
    except ValueError:
        return default
    return value if value > 0.0 else default
