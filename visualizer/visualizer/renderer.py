"""Графический конвейер отрисовки сущностей и камера (FE-005)."""

import math
import time
from dataclasses import dataclass
from typing import Dict, List, Optional, Set, Tuple
import pygame

from .config import VisualizerConfig, Colors
from .client import (
    AnomalyState,
    CarpetState,
    EnemyCarpetState,
    EnemyState,
    PlayerState,
    TreasureState,
    Vector2D,
)


@dataclass
class ExplosionParticle:
    """Частица взрыва при столкновении ковров (FE-012)."""
    wx: float
    wy: float
    vx: float
    vy: float
    color: Tuple[int, int, int]
    life: float
    max_life: float
    size: float


class ExplosionEffect:
    """Анимационный эффект взрыва и рассеивающихся обломков (FE-012 Section 2)."""

    def __init__(self, wx: float, wy: float, color: Tuple[int, int, int] = Colors.EXPLOSION_FIRE, count: int = 20):
        self.particles: List[ExplosionParticle] = []
        for i in range(count):
            angle = (2.0 * math.pi / count) * i + (hash((wx, wy, i)) % 100) / 100.0
            speed = 25.0 + (hash((wy, wx, i)) % 40)
            vx = speed * math.cos(angle)
            vy = speed * math.sin(angle)
            p_color = (
                color if i % 3 == 0
                else (Colors.EXPLOSION_SPARK if i % 3 == 1 else Colors.EXPLOSION_SMOKE)
            )
            max_life = 0.5 + (i % 5) * 0.1
            self.particles.append(
                ExplosionParticle(
                    wx=wx,
                    wy=wy,
                    vx=vx,
                    vy=vy,
                    color=p_color,
                    life=max_life,
                    max_life=max_life,
                    size=max(2.0, 5.0 - (i % 3)),
                )
            )

    @property
    def is_finished(self) -> bool:
        return all(p.life <= 0.0 for p in self.particles)

    def update(self, dt: float) -> None:
        for p in self.particles:
            p.wx += p.vx * dt
            p.wy += p.vy * dt
            p.life -= dt
            p.vx *= 0.94
            p.vy *= 0.94


@dataclass
class TrajectoryForecast:
    """Результат интегрирования кандидата, используемый для отбора и отрисовки."""
    angle_degrees: Optional[float]
    points: List[Tuple[Vector2D, int]]
    death_position: Optional[Vector2D]
    death_time: Optional[float]
    route_score: float
    time_to_last_bounty: Optional[float]

    @property
    def score_rate(self) -> float:
        if self.time_to_last_bounty is None or self.time_to_last_bounty <= 0.0:
            return 0.0
        return self.route_score / max(0.2, self.time_to_last_bounty)


class Camera:
    """Камера с декартовой трансформацией: Y растет вверх -> экранный Y растет вниз."""

    def __init__(
        self,
        offset_x: float = 0.0,
        offset_y: float = 0.0,
        zoom: float = 1.0,
        screen_width: int = 1280,
        screen_height: int = 720,
    ):
        self.offset_x = offset_x
        self.offset_y = offset_y
        self.zoom = zoom
        self.screen_width = screen_width
        self.screen_height = screen_height

    def world_to_screen(self, wx: float, wy: float) -> Tuple[int, int]:
        """Преобразование мировой координаты (Y вверх) в экранную (Y вниз) по FE-005."""
        sx = int((wx - self.offset_x) * self.zoom + self.screen_width / 2.0)
        sy = int((self.screen_height / 2.0) - (wy - self.offset_y) * self.zoom)
        return sx, sy

    def screen_to_world(self, sx: int, sy: int) -> Tuple[float, float]:
        """Обратное преобразование из экранных координат в мировые."""
        wx = (sx - self.screen_width / 2.0) / self.zoom + self.offset_x
        wy = self.offset_y - (sy - self.screen_height / 2.0) / self.zoom
        return wx, wy

    def center_on(self, wx: float, wy: float) -> None:
        """Центрирует камеру на мировых координатах."""
        self.offset_x = wx
        self.offset_y = wy

    def pan(self, dx_screen: float, dy_screen: float) -> None:
        """Смещает камеру на величину экранных пикселей."""
        self.offset_x -= dx_screen / self.zoom
        self.offset_y += dy_screen / self.zoom

    def zoom_at(
        self,
        factor: float,
        anchor_screen: Optional[Tuple[int, int]] = None,
        min_zoom: float = 0.1,
        max_zoom: float = 5.0,
    ) -> None:
        """Масштабирует зум относительно опорной экранной точки."""
        if anchor_screen is None:
            anchor_screen = (self.screen_width // 2, self.screen_height // 2)

        anchor_wx, anchor_wy = self.screen_to_world(*anchor_screen)
        new_zoom = max(min_zoom, min(max_zoom, self.zoom * factor))

        if abs(new_zoom - self.zoom) > 1e-6:
            self.zoom = new_zoom
            # Корректируем смещение, чтобы точка под курсором осталась на месте
            new_wx, new_wy = self.screen_to_world(*anchor_screen)
            self.offset_x += anchor_wx - new_wx
            self.offset_y += anchor_wy - new_wy


class SceneRenderer:
    """Отрисовщик элементов игрового мира DatsMagic."""

    _TREASURE_GRID_CELL_SIZE = 128.0
    _BOUNTY_CAPTURE_RADIUS = 10.0

    def __init__(self, config: VisualizerConfig):
        self.config = config
        self.colors = config.colors
        self.explosions: List[ExplosionEffect] = []
        self._known_destroyed: Set[str] = set()
        self._aura_surfaces: dict[Tuple[int, Tuple[int, int, int, int]], pygame.Surface] = {}
        self._anomaly_overlay: Optional[pygame.Surface] = None
        # Кэш шрифтов и полупрозрачных поверхностей для производительности
        self._font_small = pygame.font.SysFont("Arial", 12, bold=True)
        self._font_medium = pygame.font.SysFont("Arial", 14, bold=True)
        self._font_large = pygame.font.SysFont("Arial", 18, bold=True)

    @classmethod
    def build_treasure_index(cls, treasures: Optional[List[TreasureState]]) -> Dict[Tuple[int, int], List[TreasureState]]:
        """Пространственный индекс монет, общий для прогнозов текущего кадра."""
        index: Dict[Tuple[int, int], List[TreasureState]] = {}
        for treasure in treasures or []:
            cell = (
                math.floor(treasure.position.x / cls._TREASURE_GRID_CELL_SIZE),
                math.floor(treasure.position.y / cls._TREASURE_GRID_CELL_SIZE),
            )
            index.setdefault(cell, []).append(treasure)
        return index

    @classmethod
    def _segment_treasure_entries(
        cls,
        start: Vector2D,
        end: Vector2D,
        treasure_index: Dict[Tuple[int, int], List[TreasureState]],
        hit_radius: float,
    ) -> List[Tuple[TreasureState, float]]:
        """Возвращает монеты и долю отрезка входа в радиус захвата."""
        cell_size = cls._TREASURE_GRID_CELL_SIZE
        min_cell_x = math.floor((min(start.x, end.x) - hit_radius) / cell_size)
        max_cell_x = math.floor((max(start.x, end.x) + hit_radius) / cell_size)
        min_cell_y = math.floor((min(start.y, end.y) - hit_radius) / cell_size)
        max_cell_y = math.floor((max(start.y, end.y) + hit_radius) / cell_size)
        dx = end.x - start.x
        dy = end.y - start.y
        length_sq = dx * dx + dy * dy
        radius_sq = hit_radius * hit_radius
        hits: List[Tuple[TreasureState, float]] = []
        for cell_x in range(min_cell_x, max_cell_x + 1):
            for cell_y in range(min_cell_y, max_cell_y + 1):
                for treasure in treasure_index.get((cell_x, cell_y), ()):
                    rx = start.x - treasure.position.x
                    ry = start.y - treasure.position.y
                    c = rx * rx + ry * ry - radius_sq
                    if c <= 0.0:
                        hits.append((treasure, 0.0))
                        continue
                    if length_sq <= 1e-12:
                        continue
                    b = 2.0 * (rx * dx + ry * dy)
                    discriminant = b * b - 4.0 * length_sq * c
                    if discriminant < 0.0:
                        continue
                    fraction = (-b - math.sqrt(discriminant)) / (2.0 * length_sq)
                    if 0.0 <= fraction <= 1.0:
                        hits.append((treasure, fraction))
        hits.sort(key=lambda item: item[1])
        return hits

    def draw_grid(self, surface: pygame.Surface, camera: Camera) -> None:
        """Отрисовывает границы арены, внешнюю буферную зону, сетку и центр."""
        # 1. Заливка внешней буферной зоны за пределами арены
        surface.fill(self.colors.BACKGROUND_OUTSIDE)

        # 2. Экранный прямоугольник игровой арены [0, arena_width] x [0, arena_height]
        top_left_sx, top_left_sy = camera.world_to_screen(0.0, self.config.arena_height)
        bot_right_sx, bot_right_sy = camera.world_to_screen(self.config.arena_width, 0.0)
        arena_rect = pygame.Rect(
            top_left_sx,
            top_left_sy,
            bot_right_sx - top_left_sx,
            bot_right_sy - top_left_sy,
        )

        # Заливка территории внутри арены песочным цветом
        pygame.draw.rect(surface, self.colors.BACKGROUND, arena_rect)

        # 3. Линии координатной сетки строго внутри арены
        screen_rect = pygame.Rect(0, 0, camera.screen_width, camera.screen_height)
        clipped_arena = arena_rect.clip(screen_rect)
        if self.config.show_grid and clipped_arena.width > 0 and clipped_arena.height > 0:
            old_clip = surface.get_clip()
            surface.set_clip(clipped_arena)

            if camera.zoom < 0.3:
                grid_step = 250.0
            elif camera.zoom < 0.7:
                grid_step = 100.0
            elif camera.zoom > 2.0:
                grid_step = 25.0
            else:
                grid_step = 50.0

            # Вертикальные линии сетки внутри арены
            curr_x = grid_step
            while curr_x < self.config.arena_width:
                sx, _ = camera.world_to_screen(curr_x, 0.0)
                pygame.draw.line(surface, self.colors.GRID, (sx, top_left_sy), (sx, bot_right_sy), 1)
                curr_x += grid_step

            # Горизонтальные линии сетки внутри арены
            curr_y = grid_step
            while curr_y < self.config.arena_height:
                _, sy = camera.world_to_screen(0.0, curr_y)
                pygame.draw.line(surface, self.colors.GRID, (top_left_sx, sy), (bot_right_sx, sy), 1)
                curr_y += grid_step

            surface.set_clip(old_clip)

        # 4. Четкая граница игровой арены
        pygame.draw.rect(surface, self.colors.ARENA_BORDER, arena_rect, width=3)

    def draw_anomalies(
        self,
        surface: pygame.Surface,
        camera: Camera,
        anomalies: List[AnomalyState],
        current_time: float,
    ) -> None:
        """Рисует зоны влияния пачкой на одной экранной поверхности, затем ядра и векторы."""
        if not anomalies:
            return
        width, height = surface.get_size()
        if self._anomaly_overlay is None or self._anomaly_overlay.get_size() != (width, height):
            self._anomaly_overlay = pygame.Surface((width, height), pygame.SRCALPHA)

        overlay = self._anomaly_overlay
        clip_rect = surface.get_clip().clip(overlay.get_rect())

        for anomaly in anomalies:
            radius = max(1, int(anomaly.effect_radius * camera.zoom))
            if radius <= 2:
                continue
            center = camera.world_to_screen(anomaly.position.x, anomaly.position.y)
            bounds = pygame.Rect(center[0] - radius, center[1] - radius, radius * 2 + 1, radius * 2 + 1)
            visible_bounds = bounds.clip(clip_rect)
            if visible_bounds.width <= 0 or visible_bounds.height <= 0:
                continue
            color = self.colors.ANOMALY_REPEL_OUTER if anomaly.is_repelling else self.colors.ANOMALY_ATTRACT_OUTER
            farthest_corner_sq = max(
                (center[0] - x) ** 2 + (center[1] - y) ** 2
                for x in (clip_rect.left, clip_rect.right - 1)
                for y in (clip_rect.top, clip_rect.bottom - 1)
            )
            overlay.fill((0, 0, 0, 0), visible_bounds)
            overlay.set_clip(visible_bounds)
            if radius * radius >= farthest_corner_sq:
                overlay.fill(color, visible_bounds)
            else:
                pygame.draw.circle(overlay, color, center, radius)
            overlay.set_clip(None)
            # Alpha-blit каждой зоны поверх уже нарисованных — пересечения смешивают цвета.
            surface.blit(overlay, visible_bounds.topleft, area=visible_bounds)

        for anomaly in anomalies:
            self.draw_anomaly(surface, camera, anomaly, current_time, draw_zone=False)

    def draw_anomaly(
        self,
        surface: pygame.Surface,
        camera: Camera,
        anomaly: AnomalyState,
        current_time: float,
        draw_zone: bool = True,
    ) -> None:
        """Отрисовывает динамическую аномалию с ядром, силовым полем и вектором скорости (FE-008)."""
        center_sx, center_sy = camera.world_to_screen(anomaly.position.x, anomaly.position.y)
        r_effect = max(1, int(anomaly.effect_radius * camera.zoom))
        r_core = max(3, int(anomaly.core_radius * camera.zoom))

        if r_effect <= 2:
            return

        is_repelling = anomaly.is_repelling

        # 1. Полупрозрачная зона влияния, ограниченная видимой областью экрана.
        if draw_zone:
            outer_color = self.colors.ANOMALY_REPEL_OUTER if is_repelling else self.colors.ANOMALY_ATTRACT_OUTER
            visible_bounds = pygame.Rect(
                center_sx - r_effect,
                center_sy - r_effect,
                r_effect * 2 + 1,
                r_effect * 2 + 1,
            ).clip(surface.get_clip())
            if visible_bounds.width > 0 and visible_bounds.height > 0:
                zone_surface = pygame.Surface(visible_bounds.size, pygame.SRCALPHA)
                local_center = (center_sx - visible_bounds.left, center_sy - visible_bounds.top)
                farthest_corner_sq = max(
                    (local_center[0] - x) ** 2 + (local_center[1] - y) ** 2
                    for x in (0, visible_bounds.width - 1)
                    for y in (0, visible_bounds.height - 1)
                )
                if r_effect * r_effect >= farthest_corner_sq:
                    zone_surface.fill(outer_color)
                else:
                    pygame.draw.circle(zone_surface, outer_color, local_center, r_effect)
                surface.blit(zone_surface, visible_bounds.topleft)

        # 2. Статичная рамка зоны: без волн, спиралей и пульсации.
        field_color = self.colors.ANOMALY_REPEL_WAVE if is_repelling else self.colors.ANOMALY_ATTRACT_SPIRAL

        # 3. Смертоносное ядро: сплошная заливка цвета типа без маркера X.
        core_surf = pygame.Surface((r_core * 2 + 6, r_core * 2 + 6), pygame.SRCALPHA)
        pygame.draw.circle(
            core_surf,
            field_color,
            (r_core + 3, r_core + 3),
            r_core,
        )

        pygame.draw.circle(
            core_surf,
            field_color,
            (r_core + 3, r_core + 3),
            r_core,
            width=2,
        )

        surface.blit(core_surf, (center_sx - r_core - 3, center_sy - r_core - 3))

        # 4. Вектор постоянной линейной скорости перемещения аномалии (FE-008 раздел 4.2)
        vel = anomaly.velocity
        if vel.length() > 0.1:
            self._draw_anomaly_velocity_arrow(surface, camera, anomaly.position, vel, field_color)

    def _draw_anomaly_velocity_arrow(
        self,
        surface: pygame.Surface,
        camera: Camera,
        pos: Vector2D,
        vel: Vector2D,
        color: Tuple[int, int, int],
    ) -> None:
        """Отрисовывает стрелку вектора скорости перемещения аномалии (FE-008)."""
        scale = self.config.velocity_scale * 1.5
        start_sx, start_sy = camera.world_to_screen(pos.x, pos.y)
        end_sx, end_sy = camera.world_to_screen(pos.x + vel.x * scale, pos.y + vel.y * scale)

        arrow_len = math.hypot(end_sx - start_sx, end_sy - start_sy)
        if arrow_len < 4:
            return

        arrow_color = color

        # Линия вектора скорости
        pygame.draw.line(surface, arrow_color, (start_sx, start_sy), (end_sx, end_sy), 2)

        # Наконечник стрелки
        angle = math.atan2(end_sy - start_sy, end_sx - start_sx)
        head_len = min(9.0, max(4.0, arrow_len * 0.35))
        left_angle = angle + math.pi * 0.85
        right_angle = angle - math.pi * 0.85

        left_pt = (
            end_sx + int(head_len * math.cos(left_angle)),
            end_sy + int(head_len * math.sin(left_angle)),
        )
        right_pt = (
            end_sx + int(head_len * math.cos(right_angle)),
            end_sy + int(head_len * math.sin(right_angle)),
        )

        pygame.draw.polygon(surface, arrow_color, [(end_sx, end_sy), left_pt, right_pt])

    @staticmethod
    def get_coin_tier_color(value: int) -> Tuple[int, int, int]:
        """Возвращает RGB цвет монеты по ее номиналу в соответствии с FE-012 Section 2.1."""
        if value < 100:
            return Colors.COIN_COMMON  # Bronze (190, 110, 60)
        elif value < 250:
            return Colors.COIN_SILVER  # Silver (210, 220, 230)
        elif value < 500:
            return Colors.COIN_GOLD  # Gold (255, 215, 0)
        else:
            return Colors.COIN_LEGENDARY  # Ruby / Legendary (220, 20, 60)

    def _get_coin_tier_color(self, value: int) -> Tuple[int, int, int]:
        """Приватный метод для соответствия архитектурному плану FE-012 Section 3."""
        return self.get_coin_tier_color(value)

    def draw_treasure(
        self,
        surface: pygame.Surface,
        camera: Camera,
        treasure: TreasureState,
        current_time: float = 0.0,
    ) -> None:
        """Отрисовывает компактную золотистую монету (FE-017)."""
        sx, sy = camera.world_to_screen(treasure.position.x, treasure.position.y)
        tier_color = self.colors.COIN_GOLD
        val = treasure.value

        # Радиус bounty в legacy API — 5 мировых единиц; компактный диск чуть меньше bounty.
        if val < 100:
            base_r = 2.5
            border_c = self.colors.TREASURE_BORDER
        elif val < 250:
            base_r = 2.75
            border_c = self.colors.TREASURE_BORDER
        elif val < 500:
            base_r = 3.0
            border_c = self.colors.TREASURE_BORDER
        else:
            base_r = 3.5
            border_c = self.colors.TREASURE_BORDER

        r = max(2, int(round(base_r * camera.zoom)))
        pygame.draw.circle(surface, border_c, (sx, sy), r + 1)
        pygame.draw.circle(surface, tier_color, (sx, sy), r)

    def draw_carpet(
        self,
        surface: pygame.Surface,
        camera: Camera,
        player_or_enemy: PlayerState | EnemyState | CarpetState | EnemyCarpetState,
        is_local_player: bool,
        current_time: float,
        carpet_index: Optional[int] = None,
    ) -> None:
        """Отрисовывает ковер-самолет с номером [#1]..[#5] и аурой взаимодействия (FE-012)."""
        pos = player_or_enemy.position
        vel = player_or_enemy.velocity
        sx, sy = camera.world_to_screen(pos.x, pos.y)

        radius = max(2, int(self.config.carpet_radius * camera.zoom))

        is_destroyed = (
            getattr(player_or_enemy, "is_destroyed", False)
            or getattr(player_or_enemy, "status", "") == "destroyed"
        )
        if is_destroyed:
            # Анимация взрыва при уничтожении (FE-012)
            if player_or_enemy.id not in self._known_destroyed:
                self._known_destroyed.add(player_or_enemy.id)
                self.add_explosion(pos.x, pos.y)

            self._draw_destroyed_carpet_marker(
                surface, sx, sy, radius * 2, radius * 2, player_or_enemy.id, current_time, carpet_index
            )
            return
        elif player_or_enemy.id in self._known_destroyed:
            self._known_destroyed.remove(player_or_enemy.id)

        is_stunned = getattr(player_or_enemy, "is_stunned", False)

        # 1. Индикатор оглушения (FE-005 раздел 4.3): мигающий красный ореол и надпись STUNNED
        if is_stunned:
            flash = (math.sin(current_time * 10.0) + 1.0) / 2.0  # [0, 1]
            halo_radius = int(radius * 1.35 + flash * 6.0)
            halo_surface = pygame.Surface((halo_radius * 2 + 2, halo_radius * 2 + 2), pygame.SRCALPHA)
            alpha = int(120 + flash * 100)
            pygame.draw.circle(
                halo_surface,
                (*self.colors.STUNNED_HALO, alpha),
                (halo_radius + 1, halo_radius + 1),
                halo_radius,
                3,
            )
            surface.blit(halo_surface, (sx - halo_radius - 1, sy - halo_radius - 1))

            # Надпись STUNNED
            stun_label = self._font_medium.render("STUNNED", True, self.colors.STUNNED_HALO)
            surface.blit(stun_label, (sx - stun_label.get_width() // 2, sy - radius - 26))

        # 2. Большая полупрозрачная аура: своя — желтая, вражеская — зеленая.
        max_velocity = float(getattr(player_or_enemy, "max_velocity", self.config.max_velocity))
        aura_radius = max(6, int(max_velocity * camera.zoom))
        aura_color = self.colors.PLAYER_AURA if is_local_player else self.colors.ENEMY_AURA
        aura_key = (aura_radius, aura_color)
        aura_surf = self._aura_surfaces.get(aura_key)
        if aura_surf is None:
            aura_size = aura_radius * 2 + 4
            aura_surf = pygame.Surface((aura_size, aura_size), pygame.SRCALPHA)
            pygame.draw.circle(
                aura_surf,
                aura_color,
                (aura_radius + 2, aura_radius + 2),
                aura_radius,
            )
            self._aura_surfaces[aura_key] = aura_surf
        surface.blit(aura_surf, (sx - aura_radius - 2, sy - aura_radius - 2))

        # 3. Нематериальный ковер-самолет (малый ореол корпуса + центральная точка)
        body_color = self.colors.PLAYER_BODY if is_local_player else self.colors.ENEMY_BODY
        border_color = self.colors.PLAYER_BORDER if is_local_player else self.colors.ENEMY_BORDER

        # Круговой полупрозрачный ореол области ковра / сбора
        aura_surf = pygame.Surface((radius * 2 + 4, radius * 2 + 4), pygame.SRCALPHA)
        pygame.draw.circle(aura_surf, (*body_color, 50), (radius + 2, radius + 2), radius)
        surface.blit(aura_surf, (sx - radius - 2, sy - radius - 2))

        # Центральная нематериальная материальная точка ковра
        pt_r = max(1, int(4.0 * camera.zoom))
        pygame.draw.circle(surface, body_color, (sx, sy), pt_r)
        if camera.zoom >= 0.5 and pt_r >= 2:
            pygame.draw.circle(surface, (255, 255, 255), (sx, sy), 1)

        # Цифровой номер внутри точки ковра при достаточном увеличении (FE-012 Section 2.2)
        if pt_r >= 8 and carpet_index is not None:
            num_surf = self._font_small.render(str(carpet_index), True, (20, 20, 20))
            surface.blit(num_surf, (sx - num_surf.get_width() // 2, sy - num_surf.get_height() // 2))

        # 4. Метка ковра: идентификатор и цифровой номер [#1]..[#5] (FE-012 Section 2.2)
        idx_str = f"[#{carpet_index}]" if carpet_index is not None else ""
        label_text = f"{player_or_enemy.id} {idx_str}".strip()
        id_text = self._font_small.render(
            label_text, True, self.colors.PLAYER_TEXT if is_local_player else self.colors.ENEMY_BORDER
        )
        surface.blit(id_text, (sx - id_text.get_width() // 2, sy - radius - 15))

        # 5. Три вектора внешнего контракта: V, S и W начинаются у ковра.
        speed = vel.length()
        if speed > 0.1:
            self._draw_velocity_arrow(surface, camera, pos, vel)
        accel = getattr(player_or_enemy, "acceleration", Vector2D(0.0, 0.0))
        if accel.length() > 0.1:
            # S использует те же экранные единицы, что и мышиный прицел.
            end = (int(sx + accel.x), int(sy - accel.y))
            pygame.draw.line(surface, self.colors.ACCELERATION_ARROW, (sx, sy), end, 2)
            angle = math.atan2(end[1] - sy, end[0] - sx)
            head_size = 6
            left = (
                int(end[0] - head_size * math.cos(angle - math.pi / 6)),
                int(end[1] - head_size * math.sin(angle - math.pi / 6)),
            )
            right = (
                int(end[0] - head_size * math.cos(angle + math.pi / 6)),
                int(end[1] - head_size * math.sin(angle + math.pi / 6)),
            )
            pygame.draw.polygon(surface, self.colors.ACCELERATION_ARROW, [end, left, right])
            label = self._font_small.render("S", True, self.colors.ACCELERATION_ARROW)
            surface.blit(label, (end[0] + 3, end[1] - label.get_height() // 2))
        anomaly_accel = getattr(player_or_enemy, "anomaly_acceleration", Vector2D(0.0, 0.0))
        if anomaly_accel.length() > 0.1:
            self._draw_world_vector_arrow(
                surface,
                camera,
                pos,
                Vector2D(
                    pos.x + anomaly_accel.x * self.config.acceleration_scale,
                    pos.y + anomaly_accel.y * self.config.acceleration_scale,
                ),
                self.colors.ANOMALY_ACCELERATION_ARROW,
                "W",
                2,
            )

    def draw_player_fleet(
        self,
        surface: pygame.Surface,
        camera: Camera,
        player: PlayerState,
        current_time: float,
        anomalies: Optional[List[AnomalyState]] = None,
        moving_bodies: Optional[List[Tuple[str, Vector2D, Vector2D]]] = None,
        observed_carpet_id: Optional[str] = None,
        treasure_index: Optional[Dict[Tuple[int, int], List[TreasureState]]] = None,
    ) -> Optional[TrajectoryForecast]:
        """Отрисовывает все ковры флота игрока с номерами #1..#5 (FE-012)."""
        current_forecast = None
        if player.carpets:
            ordered_carpets = [
                carpet for carpet in player.carpets if carpet.id != observed_carpet_id
            ] + [
                carpet for carpet in player.carpets if carpet.id == observed_carpet_id
            ]
            for carpet in ordered_carpets:
                forecast = self.draw_trajectory(
                    surface,
                    camera,
                    carpet,
                    anomalies,
                    is_local_player=True,
                    moving_bodies=moving_bodies,
                    is_active=carpet.id == observed_carpet_id,
                    treasure_index=treasure_index,
                )
                if carpet.id == observed_carpet_id:
                    current_forecast = forecast
            for i, carpet in enumerate(player.carpets, start=1):
                self.draw_carpet(
                    surface, camera, carpet, is_local_player=True, current_time=current_time, carpet_index=i
                )
        else:
            current_forecast = self.draw_trajectory(
                surface, camera, player, anomalies, is_local_player=True, moving_bodies=moving_bodies,
                is_active=True,
                treasure_index=treasure_index,
            )
            self.draw_carpet(surface, camera, player, is_local_player=True, current_time=current_time)
        return current_forecast

    def draw_trajectory(
        self,
        surface: pygame.Surface,
        camera: Camera,
        carpet: CarpetState | EnemyCarpetState | PlayerState | EnemyState,
        anomalies: Optional[List[AnomalyState]] = None,
        is_local_player: bool = True,
        candidate_acceleration: Optional[Vector2D] = None,
        include_command_lead: bool = False,
        moving_bodies: Optional[List[Tuple[str, Vector2D, Vector2D]]] = None,
        is_active: bool = False,
        step_seconds: Optional[float] = None,
        treasure_index: Optional[Dict[Tuple[int, int], List[TreasureState]]] = None,
        render: bool = True,
        angle_degrees: Optional[float] = None,
    ) -> TrajectoryForecast:
        """Прогноз по физике player_2: тик 0,2 с, Euler, трение, скорость и аномалии."""
        if getattr(carpet, "is_destroyed", False) or getattr(carpet, "status", "") == "destroyed":
            return TrajectoryForecast(angle_degrees, [], None, None, 0.0, None)
        applied_acceleration = getattr(carpet, "acceleration", Vector2D(0.0, 0.0))
        if getattr(carpet, "is_stunned", False):
            applied_acceleration = Vector2D(0.0, 0.0)
        max_acceleration = float(getattr(carpet, "max_acceleration", self.config.max_acceleration))
        if candidate_acceleration is None:
            candidate_acceleration = applied_acceleration
        accel_length = candidate_acceleration.length()
        if accel_length > max_acceleration > 0.0:
            ratio = max_acceleration / accel_length
            candidate_acceleration = Vector2D(candidate_acceleration.x * ratio, candidate_acceleration.y * ratio)

        position = carpet.position
        velocity = carpet.velocity
        max_velocity = max(0.0, float(getattr(carpet, "max_velocity", self.config.max_velocity)))
        horizon = max(0.01, self.config.trajectory_horizon_seconds)
        tick_seconds = max(
            0.01,
            step_seconds
            if step_seconds is not None
            else self.config.trajectory_step_seconds if is_active
            else self.config.trajectory_background_step_seconds,
        )
        friction_base = max(0.0, self.config.friction)
        future_anomalies = anomalies or []
        elapsed = 0.0
        death_position: Optional[Vector2D] = None
        death_time: Optional[float] = None
        lead_remaining = self.config.trajectory_command_lead_seconds if include_command_lead else 0.0
        prediction_duration = horizon + lead_remaining
        index = 0
        predicted_points: List[Tuple[Vector2D, int]] = []
        route_score = 0.0
        time_to_last_bounty: Optional[float] = None
        collected_treasures: Set[str] = set()
        while elapsed + 1e-9 < prediction_duration:
            index += 1
            dt = min(tick_seconds, prediction_duration - elapsed)
            if dt <= 1e-9:
                break
            command = applied_acceleration if lead_remaining > 1e-9 else candidate_acceleration
            if lead_remaining > 1e-9:
                dt = min(dt, lead_remaining)
            friction_per_step = friction_base ** (dt / tick_seconds)
            environmental_x = 0.0
            environmental_y = 0.0
            for anomaly in future_anomalies:
                anomaly_x = anomaly.position.x + anomaly.velocity.x * elapsed
                anomaly_y = anomaly.position.y + anomaly.velocity.y * elapsed
                dx = anomaly_x - position.x
                dy = anomaly_y - position.y
                distance_sq = dx * dx + dy * dy
                effect_radius_sq = anomaly.effect_radius * anomaly.effect_radius
                if distance_sq <= effect_radius_sq and distance_sq > 1e-12:
                    inv_distance = 1.0 / math.sqrt(distance_sq)
                    direction = -1.0 if anomaly.is_repelling else 1.0
                    magnitude = anomaly.force * direction * inv_distance
                    environmental_x += dx * magnitude
                    environmental_y += dy * magnitude

            velocity_x = velocity.x * friction_per_step + (command.x + environmental_x) * dt
            velocity_y = velocity.y * friction_per_step + (command.y + environmental_y) * dt
            speed = math.hypot(velocity_x, velocity_y)
            if speed > max_velocity > 0.0:
                velocity_x *= max_velocity / speed
                velocity_y *= max_velocity / speed
            velocity = Vector2D(velocity_x, velocity_y)
            previous_position = position
            position = Vector2D(position.x + velocity.x * dt, position.y + velocity.y * dt)

            # Проверяем непрерывный отрезок за подшаг: быстрый ковер может пересечь
            # ядро или границу арены, не оказавшись внутри на самих точках дискретизации.
            collision_position: Optional[Vector2D] = None
            collision_fraction = 1.0
            if death_position is None:
                impact_fraction: Optional[float] = None
                for anomaly in future_anomalies:
                    relative_start_x = anomaly.position.x + anomaly.velocity.x * elapsed - previous_position.x
                    relative_start_y = anomaly.position.y + anomaly.velocity.y * elapsed - previous_position.y
                    relative_end_x = anomaly.position.x + anomaly.velocity.x * (elapsed + dt) - position.x
                    relative_end_y = anomaly.position.y + anomaly.velocity.y * (elapsed + dt) - position.y
                    segment_x = relative_end_x - relative_start_x
                    segment_y = relative_end_y - relative_start_y
                    segment_length_sq = segment_x * segment_x + segment_y * segment_y
                    lethal_radius = max(0.0, anomaly.core_radius) + self.config.carpet_radius
                    c = relative_start_x * relative_start_x + relative_start_y * relative_start_y - lethal_radius * lethal_radius
                    if c <= 0.0:
                        fraction = 0.0
                    elif segment_length_sq > 1e-12:
                        b = 2.0 * (relative_start_x * segment_x + relative_start_y * segment_y)
                        discriminant = b * b - 4.0 * segment_length_sq * c
                        if discriminant < 0.0:
                            continue
                        fraction = (-b - math.sqrt(discriminant)) / (2.0 * segment_length_sq)
                        if not 0.0 <= fraction <= 1.0:
                            continue
                    else:
                        continue
                    if impact_fraction is None or fraction < impact_fraction:
                        impact_fraction = fraction

                # Player_2 учитывает столкновение с остальными коврами/врагами,
                # экстраполируя их по текущей скорости без будущей команды.
                collision_radius = 2.0 * self.config.carpet_radius
                for body_id, body_position, body_velocity in moving_bodies or []:
                    if body_id == getattr(carpet, "id", None):
                        continue
                    relative_start_x = body_position.x + body_velocity.x * elapsed - previous_position.x
                    relative_start_y = body_position.y + body_velocity.y * elapsed - previous_position.y
                    relative_end_x = body_position.x + body_velocity.x * (elapsed + dt) - position.x
                    relative_end_y = body_position.y + body_velocity.y * (elapsed + dt) - position.y
                    segment_x = relative_end_x - relative_start_x
                    segment_y = relative_end_y - relative_start_y
                    segment_length_sq = segment_x * segment_x + segment_y * segment_y
                    c = relative_start_x * relative_start_x + relative_start_y * relative_start_y - collision_radius * collision_radius
                    if c <= 0.0:
                        fraction = 0.0
                    elif segment_length_sq > 1e-12:
                        b = 2.0 * (relative_start_x * segment_x + relative_start_y * segment_y)
                        discriminant = b * b - 4.0 * segment_length_sq * c
                        if discriminant < 0.0:
                            continue
                        fraction = (-b - math.sqrt(discriminant)) / (2.0 * segment_length_sq)
                        if not 0.0 <= fraction <= 1.0:
                            continue
                    else:
                        continue
                    if impact_fraction is None or fraction < impact_fraction:
                        impact_fraction = fraction

                # Граница смертельна при выходе центра ковра за пределы [0, W] x [0, H],
                # как в WorldSpatialEngine. Найдем первый край, пересеченный за этот шаг.
                width = self.config.arena_width
                height = self.config.arena_height
                dx = position.x - previous_position.x
                dy = position.y - previous_position.y
                boundary_fractions: List[float] = []
                if position.x < 0.0 and dx < 0.0:
                    boundary_fractions.append((0.0 - previous_position.x) / dx)
                elif position.x > width and dx > 0.0:
                    boundary_fractions.append((width - previous_position.x) / dx)
                if position.y < 0.0 and dy < 0.0:
                    boundary_fractions.append((0.0 - previous_position.y) / dy)
                elif position.y > height and dy > 0.0:
                    boundary_fractions.append((height - previous_position.y) / dy)
                if previous_position.x < 0.0 or previous_position.x > width or previous_position.y < 0.0 or previous_position.y > height:
                    boundary_fractions.append(0.0)
                if boundary_fractions:
                    boundary_fraction = min(min(boundary_fractions), 1.0)
                    if impact_fraction is None or boundary_fraction < impact_fraction:
                        impact_fraction = boundary_fraction

                if impact_fraction is not None:
                    collision_fraction = impact_fraction
                    collision_position = Vector2D(
                        previous_position.x + dx * impact_fraction,
                        previous_position.y + dy * impact_fraction,
                    )
                    death_position = collision_position
                    death_time = elapsed + dt * impact_fraction

            if treasure_index:
                reward_end = collision_position or position
                for treasure, fraction in self._segment_treasure_entries(
                    previous_position,
                    reward_end,
                    treasure_index,
                    self._BOUNTY_CAPTURE_RADIUS,
                ):
                    if treasure.id in collected_treasures:
                        continue
                    collected_treasures.add(treasure.id)
                    route_score += max(0, treasure.value)
                    time_to_last_bounty = elapsed + dt * collision_fraction * fraction

            elapsed += dt
            lead_remaining = max(0.0, lead_remaining - dt)
            radius = 2 if index <= 3 else 1
            predicted_points.append((collision_position or position, radius))
            if collision_position is not None:
                break

        forecast = TrajectoryForecast(
            angle_degrees=angle_degrees,
            points=predicted_points,
            death_position=death_position,
            death_time=death_time,
            route_score=route_score,
            time_to_last_bounty=time_to_last_bounty,
        )
        if render:
            self.draw_trajectory_forecast(surface, camera, forecast, is_active=is_active)
        return forecast

    def draw_trajectory_forecast(
        self,
        surface: pygame.Surface,
        camera: Camera,
        forecast: TrajectoryForecast,
        is_active: bool = False,
    ) -> None:
        """Рисует текущий, доходный или смертельный прогноз; пустые кандидаты скрыты."""
        if forecast.death_position is not None:
            color = self.colors.TRAJECTORY_DEATH
        elif is_active:
            color = self.colors.TRAJECTORY_DOT
        elif forecast.route_score > 0:
            color = self.colors.TRAJECTORY_COIN
        else:
            return

        for point, radius in forecast.points:
            sx, sy = camera.world_to_screen(point.x, point.y)
            pygame.draw.circle(surface, color, (sx, sy), radius)

        if forecast.death_position is not None:
            sx, sy = camera.world_to_screen(forecast.death_position.x, forecast.death_position.y)
            cross_radius = 4
            pygame.draw.line(surface, self.colors.TRAJECTORY_DEATH,
                             (sx - cross_radius, sy - cross_radius), (sx + cross_radius, sy + cross_radius), 2)
            pygame.draw.line(surface, self.colors.TRAJECTORY_DEATH,
                             (sx - cross_radius, sy + cross_radius), (sx + cross_radius, sy - cross_radius), 2)

    def draw_scan_fan(
        self,
        surface: pygame.Surface,
        camera: Camera,
        carpet: CarpetState,
        anomalies: Optional[List[AnomalyState]] = None,
        moving_bodies: Optional[List[Tuple[str, Vector2D, Vector2D]]] = None,
        treasure_index: Optional[Dict[Tuple[int, int], List[TreasureState]]] = None,
        max_routes: Optional[int] = None,
    ) -> List[TrajectoryForecast]:
        """Сканирует направления, ранжирует по score/time и рисует только лучшие."""
        step = max(1.0, self.config.trajectory_fan_step_degrees)
        direction_count = max(1, round(360.0 / step))
        max_acceleration = max(0.0, carpet.max_acceleration)
        forecasts: List[TrajectoryForecast] = []
        for index in range(direction_count):
            angle_degrees = index * 360.0 / direction_count
            angle = math.radians(angle_degrees)
            candidate = Vector2D(max_acceleration * math.cos(angle), max_acceleration * math.sin(angle))
            forecast = self.draw_trajectory(
                surface,
                camera,
                carpet,
                anomalies,
                candidate_acceleration=candidate,
                include_command_lead=True,
                moving_bodies=moving_bodies,
                is_active=False,
                step_seconds=self.config.physics_tick_seconds,
                treasure_index=treasure_index,
                render=False,
                angle_degrees=angle_degrees,
            )
            forecasts.append(forecast)

        safe_profit = [
            item for item in forecasts
            if item.death_position is None and item.route_score > 0 and item.score_rate > 0.0
        ]
        route_limit = max_routes if max_routes is not None else self.config.trajectory_fan_max_routes
        if safe_profit:
            safe_profit.sort(key=lambda item: (item.score_rate, item.route_score), reverse=True)
            selected = safe_profit[:max(1, route_limit)]
        else:
            deaths = [item for item in forecasts if item.death_position is not None]
            deaths.sort(key=lambda item: (item.death_time or 0.0, item.route_score), reverse=True)
            selected = deaths[:max(1, route_limit)]

        for item in selected:
            self.draw_trajectory_forecast(surface, camera, item)
        return selected

    def draw_enemy_fleet(
        self,
        surface: pygame.Surface,
        camera: Camera,
        enemy: EnemyState,
        current_time: float,
        anomalies: Optional[List[AnomalyState]] = None,
        treasure_index: Optional[Dict[Tuple[int, int], List[TreasureState]]] = None,
    ) -> None:
        """Отрисовывает все ковры флота противника с номерами #1..#5 (FE-012)."""
        if enemy.carpets:
            for carpet in enemy.carpets:
                self.draw_trajectory(surface, camera, carpet, anomalies, is_local_player=False, treasure_index=treasure_index)
            for i, carpet in enumerate(enemy.carpets, start=1):
                self.draw_carpet(
                    surface, camera, carpet, is_local_player=False, current_time=current_time, carpet_index=i
                )
        else:
            self.draw_trajectory(surface, camera, enemy, anomalies, is_local_player=False, treasure_index=treasure_index)
            self.draw_carpet(surface, camera, enemy, is_local_player=False, current_time=current_time)

    def add_explosion(
        self,
        wx: float,
        wy: float,
        color: Tuple[int, int, int] = Colors.EXPLOSION_FIRE,
        count: int = 24,
    ) -> None:
        """Добавляет анимацию взрыва ковра в мировых координатах (FE-012)."""
        self.explosions.append(ExplosionEffect(wx, wy, color, count))

    def draw_explosions(self, surface: pygame.Surface, camera: Camera, dt: float = 0.016) -> None:
        """Обновляет физику и отрисовывает активные частицы взрывов (FE-012 Section 2)."""
        active_explosions = []
        for explosion in self.explosions:
            explosion.update(dt)
            for p in explosion.particles:
                if p.life > 0.0:
                    sx, sy = camera.world_to_screen(p.wx, p.wy)
                    alpha = int(255 * max(0.0, p.life / p.max_life))
                    p_size = max(1, int(p.size * camera.zoom))
                    part_surf = pygame.Surface((p_size * 2 + 2, p_size * 2 + 2), pygame.SRCALPHA)
                    pygame.draw.circle(part_surf, (*p.color, alpha), (p_size + 1, p_size + 1), p_size)
                    surface.blit(part_surf, (sx - p_size - 1, sy - p_size - 1))
            if not explosion.is_finished:
                active_explosions.append(explosion)
        self.explosions = active_explosions

    def _draw_destroyed_carpet_marker(
        self,
        surface: pygame.Surface,
        sx: int,
        sy: int,
        c_w: int,
        c_h: int,
        player_id: str,
        current_time: float,
        carpet_index: Optional[int] = None,
    ) -> None:
        """Отрисовывает перечеркнутый маркер гибели ковра (FE-008 / FE-012)."""
        marker_color = self.colors.DESTROYED_MARKER
        cross_r = max(8, c_w // 2)

        # Рассеивающиеся обломки / искры
        debris_surf = pygame.Surface((cross_r * 4, cross_r * 4), pygame.SRCALPHA)
        center_d = cross_r * 2
        for i in range(8):
            d_angle = (math.pi / 4.0) * i + (current_time * 0.8)
            dist = cross_r * 0.7 + (math.sin(current_time * 4.0 + i) + 1.0) * 4.0
            dx = int(center_d + dist * math.cos(d_angle))
            dy = int(center_d + dist * math.sin(d_angle))
            spark_col = Colors.EXPLOSION_FIRE if i % 2 == 0 else Colors.EXPLOSION_SPARK
            pygame.draw.circle(debris_surf, (*spark_col, 200), (dx, dy), 2)
        surface.blit(debris_surf, (sx - center_d, sy - center_d))

        # Перечеркнутый маркер X
        pygame.draw.line(surface, marker_color, (sx - cross_r, sy - cross_r // 2), (sx + cross_r, sy + cross_r // 2), 3)
        pygame.draw.line(surface, marker_color, (sx - cross_r, sy + cross_r // 2), (sx + cross_r, sy - cross_r // 2), 3)

        # Текст DESTROYED с номером ковра
        idx_str = f"[#{carpet_index}]" if carpet_index is not None else ""
        dest_label = self._font_small.render(f"{player_id} {idx_str} [DESTROYED]".strip(), True, marker_color)
        surface.blit(dest_label, (sx - dest_label.get_width() // 2, sy - cross_r - 12))

    def _draw_velocity_arrow(
        self,
        surface: pygame.Surface,
        camera: Camera,
        pos: Vector2D,
        vel: Vector2D,
    ) -> None:
        """Отрисовывает направленную стрелку вектора скорости от центра ковра."""
        scale = self.config.velocity_scale
        self._draw_world_vector_arrow(
            surface,
            camera,
            pos,
            Vector2D(pos.x + vel.x * scale, pos.y + vel.y * scale),
            self.colors.VELOCITY_ARROW,
            "V",
            2,
        )

    def _draw_world_vector_arrow(
        self,
        surface: pygame.Surface,
        camera: Camera,
        start: Vector2D,
        end: Vector2D,
        color: Tuple[int, int, int],
        label: str,
        width: int,
    ) -> None:
        """Рисует подписанную стрелку между двумя мировыми координатами."""
        start_sx, start_sy = camera.world_to_screen(start.x, start.y)
        end_sx, end_sy = camera.world_to_screen(end.x, end.y)

        arrow_len = math.hypot(end_sx - start_sx, end_sy - start_sy)
        if arrow_len < 3:
            return

        pygame.draw.line(surface, color, (start_sx, start_sy), (end_sx, end_sy), width)

        # Наконечник стрелки
        angle = math.atan2(end_sy - start_sy, end_sx - start_sx)
        head_len = min(10.0, max(5.0, arrow_len * 0.3))
        left_angle = angle + math.pi * 0.85
        right_angle = angle - math.pi * 0.85

        left_pt = (
            end_sx + int(head_len * math.cos(left_angle)),
            end_sy + int(head_len * math.sin(left_angle)),
        )
        right_pt = (
            end_sx + int(head_len * math.cos(right_angle)),
            end_sy + int(head_len * math.sin(right_angle)),
        )

        pygame.draw.polygon(surface, color, [(end_sx, end_sy), left_pt, right_pt])
        label_surface = self._font_small.render(label, True, color)
        surface.blit(label_surface, (end_sx + 4, end_sy + 2))
