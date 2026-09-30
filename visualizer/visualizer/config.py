"""Конфигурация графического клиента визуализации DatsMagic."""

from dataclasses import dataclass, field
from typing import Tuple
import os


@dataclass
class Colors:
    """Цветовая палитра визуализатора (пустынный антураж)."""
    # Фон пустыни и сетка
    BACKGROUND: Tuple[int, int, int] = (244, 226, 198)       # Песочно-бежевый (внутри арены)
    BACKGROUND_OUTSIDE: Tuple[int, int, int] = (210, 190, 160) # Буферная зона за пределами арены
    GRID: Tuple[int, int, int] = (228, 208, 178)             # Светлые линии сетки внутри арены
    ARENA_BORDER: Tuple[int, int, int] = (140, 100, 60)      # Четкая граница игровой арены
    ARENA_CENTER: Tuple[int, int, int] = (160, 110, 70)      # Маркер центра арены

    # Ковер управляемого игрока
    PLAYER_BODY: Tuple[int, int, int] = (41, 128, 185)       # Синий ковер
    PLAYER_BORDER: Tuple[int, int, int] = (241, 196, 15)     # Золотая бахрома
    PLAYER_TEXT: Tuple[int, int, int] = (26, 82, 118)        # Текст метки игрока

    # Ковры соперников
    ENEMY_BODY: Tuple[int, int, int] = (231, 76, 60)         # Красный ковер
    ENEMY_BORDER: Tuple[int, int, int] = (146, 43, 33)       # Темно-красная кайма

    # Вектор скорости
    VELOCITY_ARROW: Tuple[int, int, int] = (39, 174, 96)     # Зеленая стрелка

    # Сокровища и динамическая градация монет (FE-012 Section 2.1)
    TREASURE_CHEST: Tuple[int, int, int] = (212, 175, 55)    # Золотой сундук
    TREASURE_BORDER: Tuple[int, int, int] = (184, 134, 11)   # Темное золото
    TREASURE_TEXT: Tuple[int, int, int] = (90, 60, 10)       # Номинал сокровища
    COIN_COMMON: Tuple[int, int, int] = (190, 110, 60)       # Бронзовая монета (V < 100)
    COIN_SILVER: Tuple[int, int, int] = (210, 220, 230)     # Серебряная монета (100 <= V < 250)
    COIN_GOLD: Tuple[int, int, int] = (255, 215, 0)          # Золотая монета (250 <= V < 500)
    COIN_LEGENDARY: Tuple[int, int, int] = (220, 20, 60)     # Пурпурно-рубиновый артефакт (V >= 500)
    COIN_LEGENDARY_HALO: Tuple[int, int, int, int] = (220, 20, 60, 90) # Пульсирующий рубиновый ореол
    COIN_SHINE: Tuple[int, int, int] = (255, 255, 240)       # Блик на монете

    # Анимация взрыва и обломков (FE-012 Section 2)
    EXPLOSION_FIRE: Tuple[int, int, int] = (255, 100, 30)
    EXPLOSION_SPARK: Tuple[int, int, int] = (255, 220, 50)
    EXPLOSION_SMOKE: Tuple[int, int, int] = (80, 50, 40)

    # Аномалии (вихри)
    ANOMALY_OUTER: Tuple[int, int, int, int] = (211, 84, 0, 75)     # Полупрозрачный оранжевый (fallback)
    ANOMALY_INNER: Tuple[int, int, int, int] = (230, 126, 34, 120)  # Спираль вихря
    ANOMALY_EPICENTER: Tuple[int, int, int, int] = (192, 57, 43, 160) # Эпицентр оглушения

    # Динамические аномалии (FE-008)
    ANOMALY_CORE_FILL: Tuple[int, int, int] = (30, 20, 40)             # Темно-фиолетовое смертоносное ядро
    ANOMALY_CORE_BORDER: Tuple[int, int, int] = (220, 20, 60)          # Алая пульсирующая граница опасности
    ANOMALY_CORE_MARKER: Tuple[int, int, int] = (255, 60, 60)          # Маркер предупреждения внутри ядра
    ANOMALY_ATTRACT_OUTER: Tuple[int, int, int, int] = (70, 30, 90, 65) # Песчано-фиолетовая зона притяжения
    ANOMALY_ATTRACT_SPIRAL: Tuple[int, int, int] = (142, 68, 173)      # Фиолетовые закручивающиеся спирали
    ANOMALY_REPEL_OUTER: Tuple[int, int, int, int] = (230, 126, 34, 65) # Янтарно-оранжевая зона отталкивания
    ANOMALY_REPEL_WAVE: Tuple[int, int, int] = (243, 156, 18)          # Янтарные концентрические волны
    ANOMALY_VELOCITY: Tuple[int, int, int] = (231, 76, 60)             # Стрелка вектора скорости аномалии

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
    auth_token: str = os.getenv("DATS_AUTH_TOKEN", "team_visualizer")
    poll_interval: float = 0.2          # Период опроса API (с)
    reconnect_interval: float = 1.0     # Период повторной попытки при потере связи (с)

    # Параметры графического окна
    screen_width: int = 1280
    screen_height: int = 720
    fps: int = 60
    window_title: str = "DatsMagic 2D Simulator Visualizer"

    # Геометрия игрового мира и арены
    arena_width: float = 1000.0         # Ширина игровой арены (X в [0, 1000])
    arena_height: float = 1000.0        # Высота игровой арены (Y в [0, 1000])
    carpet_radius: float = 12.0         # Радиус ковра (нематериальная точка-масса с ореолом взаимодействия)

    # Параметры отрисовки
    velocity_scale: float = 1.8         # Масштаб отображения стрелки скорости
    carpet_width: float = 24.0          # Для обратной совместимости
    carpet_height: float = 14.0         # Для обратной совместимости
    min_zoom: float = 0.2               # Минимальный зум
    max_zoom: float = 4.0               # Максимальный зум
    zoom_step: float = 1.15             # Коэффициент шага зума
    pan_speed: float = 450.0            # Скорость перемещения камеры (пикс/с)

    # Цветовая палитра
    colors: Colors = field(default_factory=Colors)
