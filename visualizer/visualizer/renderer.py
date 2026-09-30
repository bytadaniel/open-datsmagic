"""Графический конвейер отрисовки сущностей и камера (FE-005)."""

import math
import time
from dataclasses import dataclass
from typing import List, Optional, Set, Tuple
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

    def __init__(self, config: VisualizerConfig):
        self.config = config
        self.colors = config.colors
        self.explosions: List[ExplosionEffect] = []
        self._known_destroyed: Set[str] = set()
        # Кэш полупрозрачных поверхностей для производительности
        self._font_small = pygame.font.SysFont("Arial", 12, bold=True)
        self._font_medium = pygame.font.SysFont("Arial", 14, bold=True)
        self._font_large = pygame.font.SysFont("Arial", 18, bold=True)

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
        if clipped_arena.width > 0 and clipped_arena.height > 0:
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

        # 5. Угловые маркеры и координаты вершин арены
        w = self.config.arena_width
        h = self.config.arena_height
        corners = [
            (0.0, 0.0, "(0, 0)", -1, 1),
            (w, 0.0, f"({int(w)}, 0)", 1, 1),
            (0.0, h, f"(0, {int(h)})", -1, -1),
            (w, h, f"({int(w)}, {int(h)})", 1, -1),
        ]
        for wx, wy, label_text, align_x, align_y in corners:
            csx, csy = camera.world_to_screen(wx, wy)
            pygame.draw.rect(surface, self.colors.ARENA_BORDER, pygame.Rect(csx - 3, csy - 3, 6, 6))
            lbl = self._font_small.render(label_text, True, self.colors.ARENA_BORDER)
            lx = csx + 5 if align_x > 0 else csx - lbl.get_width() - 5
            ly = csy + 4 if align_y > 0 else csy - lbl.get_height() - 4
            surface.blit(lbl, (lx, ly))

        # 6. Четкий маркер центра арены (W/2, H/2)
        cx, cy = camera.world_to_screen(w / 2.0, h / 2.0)
        ch_size = 9
        pygame.draw.line(surface, self.colors.ARENA_CENTER, (cx - ch_size, cy), (cx + ch_size, cy), 2)
        pygame.draw.line(surface, self.colors.ARENA_CENTER, (cx, cy - ch_size), (cx, cy + ch_size), 2)
        pygame.draw.circle(surface, self.colors.ARENA_CENTER, (cx, cy), 5, width=1)
        center_lbl = self._font_small.render(f"CENTER ({int(w / 2.0)}, {int(h / 2.0)})", True, self.colors.ARENA_CENTER)
        surface.blit(center_lbl, (cx - center_lbl.get_width() // 2, cy + 8))

    def draw_anomaly(self, surface: pygame.Surface, camera: Camera, anomaly: AnomalyState, current_time: float) -> None:
        """Отрисовывает динамическую аномалию с ядром, силовым полем и вектором скорости (FE-008)."""
        center_sx, center_sy = camera.world_to_screen(anomaly.position.x, anomaly.position.y)
        r_effect = max(1, int(anomaly.effect_radius * camera.zoom))
        r_core = max(3, int(anomaly.core_radius * camera.zoom))

        if r_effect <= 2:
            return

        is_repelling = anomaly.is_repelling

        # 1. Полупрозрачная внешняя зона влияния
        temp_surface = pygame.Surface((r_effect * 2 + 6, r_effect * 2 + 6), pygame.SRCALPHA)
        outer_color = self.colors.ANOMALY_REPEL_OUTER if is_repelling else self.colors.ANOMALY_ATTRACT_OUTER
        pygame.draw.circle(
            temp_surface,
            outer_color,
            (r_effect + 3, r_effect + 3),
            r_effect,
        )
        surface.blit(temp_surface, (center_sx - r_effect - 3, center_sy - r_effect - 3))

        # 2. Анимация силового поля (FE-008 раздел 4.1):
        if is_repelling:
            # Отталкивающий вихрь: концентрические ударные волны, расходящиеся наружу от ядра
            num_waves = 3
            wave_phase = (current_time * 1.5) % 1.0
            wave_color = self.colors.ANOMALY_REPEL_WAVE
            wave_surface = pygame.Surface((r_effect * 2 + 6, r_effect * 2 + 6), pygame.SRCALPHA)
            for i in range(num_waves):
                phase_i = (wave_phase + i / num_waves) % 1.0
                ring_r = int(r_core + (r_effect - r_core) * phase_i)
                if r_core < ring_r <= r_effect:
                    alpha = int(180 * (1.0 - phase_i * 0.7))
                    pygame.draw.circle(
                        wave_surface,
                        (*wave_color, alpha),
                        (r_effect + 3, r_effect + 3),
                        ring_r,
                        width=2,
                    )
            surface.blit(wave_surface, (center_sx - r_effect - 3, center_sy - r_effect - 3))
        else:
            # Притягивающий вихрь: 4 спирали, закручивающиеся по часовой стрелке внутрь к ядру
            num_arms = 4
            rotation_offset = current_time * 2.5
            spiral_color = self.colors.ANOMALY_ATTRACT_SPIRAL
            for arm in range(num_arms):
                arm_angle = (2.0 * math.pi / num_arms) * arm + rotation_offset
                points = []
                steps = 10
                for s in range(steps):
                    frac = (s + 1) / steps
                    r = r_core + (r_effect - r_core) * frac
                    theta = arm_angle + (frac * 1.2)
                    px = center_sx + int(r * math.cos(theta))
                    py = center_sy + int(r * math.sin(theta))
                    points.append((px, py))
                if len(points) > 1:
                    pygame.draw.lines(surface, spiral_color, False, points, 2)

        # 3. Смертоносное ядро (FE-008 раздел 4.1):
        core_surf = pygame.Surface((r_core * 2 + 6, r_core * 2 + 6), pygame.SRCALPHA)
        # Залитый круг темно-фиолетового цвета
        pygame.draw.circle(
            core_surf,
            self.colors.ANOMALY_CORE_FILL,
            (r_core + 3, r_core + 3),
            r_core,
        )

        # Контрастная пульсирующая граница опасности (алый огонь)
        pulse = (math.sin(current_time * 8.0) + 1.0) / 2.0
        border_alpha = int(180 + pulse * 75)
        border_w = max(2, int(2 + pulse * 1.5))
        pygame.draw.circle(
            core_surf,
            (*self.colors.ANOMALY_CORE_BORDER, border_alpha),
            (r_core + 3, r_core + 3),
            r_core,
            width=border_w,
        )

        # Предупреждающий маркер X внутри ядра
        if r_core >= 6:
            cross_r = max(2, int(r_core * 0.45))
            cx, cy = r_core + 3, r_core + 3
            marker_color = self.colors.ANOMALY_CORE_MARKER
            pygame.draw.line(core_surf, marker_color, (cx - cross_r, cy - cross_r), (cx + cross_r, cy + cross_r), 2)
            pygame.draw.line(core_surf, marker_color, (cx - cross_r, cy + cross_r), (cx + cross_r, cy - cross_r), 2)

        surface.blit(core_surf, (center_sx - r_core - 3, center_sy - r_core - 3))

        # 4. Вектор постоянной линейной скорости перемещения аномалии (FE-008 раздел 4.2)
        vel = anomaly.velocity
        if vel.length() > 0.1:
            self._draw_anomaly_velocity_arrow(surface, camera, anomaly.position, vel)

        # 5. Метка типа и силы вихря
        type_str = "REPEL" if is_repelling else "ATTRACT"
        label_color = self.colors.ANOMALY_REPEL_WAVE if is_repelling else self.colors.ANOMALY_ATTRACT_SPIRAL
        label = self._font_small.render(f"{type_str} F={anomaly.force:.1f}", True, label_color)
        surface.blit(label, (center_sx - label.get_width() // 2, center_sy - r_effect - 15))

    def _draw_anomaly_velocity_arrow(
        self,
        surface: pygame.Surface,
        camera: Camera,
        pos: Vector2D,
        vel: Vector2D,
    ) -> None:
        """Отрисовывает стрелку вектора скорости перемещения аномалии (FE-008)."""
        scale = self.config.velocity_scale * 1.5
        start_sx, start_sy = camera.world_to_screen(pos.x, pos.y)
        end_sx, end_sy = camera.world_to_screen(pos.x + vel.x * scale, pos.y + vel.y * scale)

        arrow_len = math.hypot(end_sx - start_sx, end_sy - start_sy)
        if arrow_len < 4:
            return

        arrow_color = self.colors.ANOMALY_VELOCITY

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
        """Отрисовывает сокровище с цветовой градацией по номиналу (FE-012)."""
        sx, sy = camera.world_to_screen(treasure.position.x, treasure.position.y)
        tier_color = self._get_coin_tier_color(treasure.value)
        val = treasure.value

        # Динамический радиус монеты в зависимости от тира
        if val < 100:
            base_r = 7.0
            border_c = (130, 70, 30)
        elif val < 250:
            base_r = 8.5
            border_c = (150, 160, 175)
        elif val < 500:
            base_r = 10.0
            border_c = (184, 134, 11)
        else:
            base_r = 12.0
            border_c = (140, 10, 30)

        r = max(5, int(base_r * camera.zoom))

        # Для Legendary (V >= 500): пульсирующий рубиновый ореол (FE-012 Section 2.1)
        if val >= 500:
            pulse = (math.sin(current_time * 6.0) + 1.0) / 2.0
            halo_r = int(r * 1.5 + pulse * 5.0)
            halo_surf = pygame.Surface((halo_r * 2 + 4, halo_r * 2 + 4), pygame.SRCALPHA)
            halo_alpha = int(70 + pulse * 80)
            pygame.draw.circle(
                halo_surf,
                (*self.colors.COIN_LEGENDARY, halo_alpha),
                (halo_r + 2, halo_r + 2),
                halo_r,
            )
            surface.blit(halo_surf, (sx - halo_r - 2, sy - halo_r - 2))

        # 1. Внешняя кайма монеты
        pygame.draw.circle(surface, border_c, (sx, sy), r + 1, width=2)

        # 2. Тело монеты с цветом тира
        pygame.draw.circle(surface, tier_color, (sx, sy), r)

        # 3. Внутренний блик
        if r >= 6:
            pygame.draw.circle(surface, self.colors.COIN_SHINE, (sx - r // 3, sy - r // 3), max(1, r // 4))

        # 4. Текст номинала сокровища
        val_text = self._font_small.render(f"+{val}", True, self.colors.TREASURE_TEXT)
        surface.blit(val_text, (sx - val_text.get_width() // 2, sy + r + 2))

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

        radius = max(8, int(self.config.carpet_radius * camera.zoom))

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

        # 2. Нематериальный ковер-самолет (круговой ореол сбора + центральная материальная точка)
        body_color = self.colors.PLAYER_BODY if is_local_player else self.colors.ENEMY_BODY
        border_color = self.colors.PLAYER_BORDER if is_local_player else self.colors.ENEMY_BORDER

        # Круговой полупрозрачный ореол области ковра / сбора
        aura_surf = pygame.Surface((radius * 2 + 4, radius * 2 + 4), pygame.SRCALPHA)
        pygame.draw.circle(aura_surf, (*body_color, 50), (radius + 2, radius + 2), radius)
        pygame.draw.circle(aura_surf, (*border_color, 200), (radius + 2, radius + 2), radius, width=2)
        surface.blit(aura_surf, (sx - radius - 2, sy - radius - 2))

        # Центральная нематериальная материальная точка ковра
        pt_r = max(3, int(4.0 * camera.zoom))
        pygame.draw.circle(surface, border_color, (sx, sy), pt_r + 1)
        pygame.draw.circle(surface, body_color, (sx, sy), pt_r)
        pygame.draw.circle(surface, (255, 255, 255), (sx, sy), max(1, pt_r - 2))

        # Цифровой номер внутри точки ковра при достаточном увеличении (FE-012 Section 2.2)
        if pt_r >= 8 and carpet_index is not None:
            num_surf = self._font_small.render(str(carpet_index), True, (20, 20, 20))
            surface.blit(num_surf, (sx - num_surf.get_width() // 2, sy - num_surf.get_height() // 2))

        # 3. Метка ковра: идентификатор и цифровой номер [#1]..[#5] (FE-012 Section 2.2)
        idx_str = f"[#{carpet_index}]" if carpet_index is not None else ""
        label_text = f"{player_or_enemy.id} {idx_str}".strip()
        id_text = self._font_small.render(
            label_text, True, self.colors.PLAYER_TEXT if is_local_player else self.colors.ENEMY_BORDER
        )
        surface.blit(id_text, (sx - id_text.get_width() // 2, sy - radius - 15))

        # 4. Вектор текущей скорости (FE-005 раздел 4.1)
        speed = vel.length()
        if speed > 0.1:
            self._draw_velocity_arrow(surface, camera, pos, vel)

    def draw_player_fleet(
        self,
        surface: pygame.Surface,
        camera: Camera,
        player: PlayerState,
        current_time: float,
    ) -> None:
        """Отрисовывает все ковры флота игрока с номерами #1..#5 (FE-012)."""
        if player.carpets:
            for i, carpet in enumerate(player.carpets, start=1):
                self.draw_carpet(
                    surface, camera, carpet, is_local_player=True, current_time=current_time, carpet_index=i
                )
        else:
            self.draw_carpet(surface, camera, player, is_local_player=True, current_time=current_time)

    def draw_enemy_fleet(
        self,
        surface: pygame.Surface,
        camera: Camera,
        enemy: EnemyState,
        current_time: float,
    ) -> None:
        """Отрисовывает все ковры флота противника с номерами #1..#5 (FE-012)."""
        if enemy.carpets:
            for i, carpet in enumerate(enemy.carpets, start=1):
                self.draw_carpet(
                    surface, camera, carpet, is_local_player=False, current_time=current_time, carpet_index=i
                )
        else:
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
        start_sx, start_sy = camera.world_to_screen(pos.x, pos.y)
        end_sx, end_sy = camera.world_to_screen(pos.x + vel.x * scale, pos.y + vel.y * scale)

        arrow_len = math.hypot(end_sx - start_sx, end_sy - start_sy)
        if arrow_len < 3:
            return

        # Основная линия вектора
        pygame.draw.line(surface, self.colors.VELOCITY_ARROW, (start_sx, start_sy), (end_sx, end_sy), 2)

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

        pygame.draw.polygon(surface, self.colors.VELOCITY_ARROW, [(end_sx, end_sy), left_pt, right_pt])
