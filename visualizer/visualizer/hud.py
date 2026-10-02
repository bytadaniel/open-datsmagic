"""Отрисовка пользовательского интерфейса и телеметрии (HUD)."""

from typing import Optional
import pygame

from .config import VisualizerConfig
from .client import WorldSnapshot


class HudRenderer:
    """Отображение информационной панели телеметрии матча."""

    def __init__(self, config: VisualizerConfig):
        self.config = config
        self.colors = config.colors
        self._font_title = pygame.font.SysFont("Arial", 16, bold=True)
        self._font_text = pygame.font.SysFont("Arial", 13, bold=False)
        self._font_bold = pygame.font.SysFont("Arial", 13, bold=True)
        self._font_small = pygame.font.SysFont("Arial", 11, bold=True)
        self._font_banner_title = pygame.font.SysFont("Arial", 22, bold=True)
        self._font_banner_sub = pygame.font.SysFont("Arial", 13, bold=False)

    def draw(
        self,
        surface: pygame.Surface,
        snapshot: Optional[WorldSnapshot],
        is_connected: bool,
        fps: float,
        zoom: float,
        follow_mode: bool,
        selected_carpet_id: Optional[str] = None,
        show_scan_fan: bool = False,
        manual_control_error: Optional[str] = None,
        manual_control_enabled: bool = False,
        trajectory_fan_step_degrees: float = 6.0,
        trajectory_fan_max_routes: int = 5,
        current_forecast=None,
        fan_forecasts=None,
    ) -> None:
        """Отрисовывает информационную панель поверх игрового поля."""
        # 1. Верхняя левая панель: телеметрия игрока и матча
        self._draw_player_panel(surface, snapshot, selected_carpet_id)

        # 2. Верхняя правая панель: статус сети, FPS и управление
        self._draw_system_panel(
            surface,
            is_connected,
            fps,
            zoom,
            follow_mode,
            selected_carpet_id,
            show_scan_fan,
            manual_control_error,
            manual_control_enabled,
            trajectory_fan_step_degrees,
            trajectory_fan_max_routes,
        )

        self._draw_trajectory_scores(surface, current_forecast, fan_forecasts or [])

        # 3. Плашка потери связи (FE-005 раздел 5)
        if not is_connected:
            self._draw_connection_alert(surface)

        # 4. Подсказка по горячим клавишам внизу экрана
        self._draw_controls_help(surface)

        # Гибель отдельного ковра отражается в составе флота; модальное окно
        # проигрыша не выводится, так как сервер автоматически респавнит ковер.

    def _draw_player_panel(
        self, surface: pygame.Surface, snapshot: Optional[WorldSnapshot], selected_carpet_id: Optional[str]
    ) -> None:
        panel_w = 270
        panel_h = 295
        x = 15
        y = 15

        # Полупрозрачная подложка
        bg_surf = pygame.Surface((panel_w, panel_h), pygame.SRCALPHA)
        pygame.draw.rect(bg_surf, self.colors.HUD_BG, (0, 0, panel_w, panel_h), border_radius=8)
        surface.blit(bg_surf, (x, y))

        if not snapshot:
            title = self._font_title.render("DatsMagic Telemetry", True, self.colors.HUD_ACCENT)
            surface.blit(title, (x + 12, y + 10))
            waiting = self._font_text.render("Waiting for telemetry snapshot...", True, self.colors.HUD_TEXT)
            surface.blit(waiting, (x + 12, y + 40))
            return

        p = snapshot.player

        # Заголовок команды
        title = self._font_title.render(f"Team: {p.id}", True, self.colors.HUD_ACCENT)
        surface.blit(title, (x + 12, y + 10))

        # Статус игры и номер тика
        status_color = self.colors.HUD_SUCCESS if snapshot.game_status == "active" else self.colors.HUD_ALERT
        status_text = self._font_bold.render(f"[{snapshot.game_status.upper()}]", True, status_color)
        tick_text = self._font_text.render(f"Tick: {snapshot.tick}", True, self.colors.HUD_TEXT)
        surface.blit(status_text, (x + 12, y + 34))
        surface.blit(tick_text, (x + 90, y + 34))

        # Общий счет команды (баланс очков)
        score_lbl = self._font_text.render("Team Score:", True, self.colors.HUD_TEXT)
        score_val = self._font_bold.render(f"{p.score} pts", True, self.colors.HUD_ACCENT)
        surface.blit(score_lbl, (x + 12, y + 56))
        surface.blit(score_val, (x + 100, y + 56))

        # Состав флота (FE-012 Section 1 & 3)
        active_count = getattr(p, "active_carpets_count", 0 if p.is_destroyed else 1)
        total_carpets = len(p.carpets) if p.carpets else 1
        fleet_text = f"Fleet Status ({active_count}/{total_carpets} active):"
        fleet_lbl = self._font_text.render(fleet_text, True, self.colors.HUD_TEXT)
        surface.blit(fleet_lbl, (x + 12, y + 78))

        # Индикаторы ковров [#1] [#2] [#3] [#4] [#5] (FE-012 Section 3)
        badge_x = x + 12
        badge_y = y + 98
        badge_w = 42
        badge_h = 22

        carpets_to_show = p.carpets if p.carpets else []
        slots_count = max(5, len(carpets_to_show))
        for i in range(min(5, slots_count)):
            bx = badge_x + i * (badge_w + 6)
            carpet = carpets_to_show[i] if i < len(carpets_to_show) else None

            if carpet is None:
                status = p.status if i == 0 else "inactive"
            else:
                status = carpet.status

            if status == "destroyed":
                b_color = (100, 25, 25)
                border_c = (220, 50, 50)
                t_color = (255, 120, 120)
                icon = "✕"
            elif status == "stunned":
                b_color = (130, 85, 20)
                border_c = (245, 180, 30)
                t_color = (255, 230, 100)
                icon = "!"
            elif status == "normal":
                b_color = (25, 80, 45)
                border_c = (46, 204, 113)
                t_color = (160, 255, 190)
                icon = "✓"
            else:
                b_color = (40, 45, 55)
                border_c = (70, 80, 95)
                t_color = (140, 150, 165)
                icon = "-"

            badge_surf = pygame.Surface((badge_w, badge_h), pygame.SRCALPHA)
            pygame.draw.rect(badge_surf, (*b_color, 230), (0, 0, badge_w, badge_h), border_radius=4)
            pygame.draw.rect(badge_surf, (*border_c, 210), (0, 0, badge_w, badge_h), width=1, border_radius=4)
            surface.blit(badge_surf, (bx, badge_y))

            badge_label = f"#{i+1} {icon}"
            t_surf = self._font_small.render(badge_label, True, t_color)
            surface.blit(t_surf, (bx + (badge_w - t_surf.get_width()) // 2, badge_y + 3))

        # Позиция и скорость основного ковра
        pos_lbl = self._font_text.render(f"Lead Pos: ({p.position.x:.1f}, {p.position.y:.1f})", True, self.colors.HUD_TEXT)
        surface.blit(pos_lbl, (x + 12, y + 130))

        speed = p.velocity.length()
        spd_lbl = self._font_text.render(f"Speed: {speed:.1f} / {p.max_velocity:.1f}", True, self.colors.HUD_TEXT)
        surface.blit(spd_lbl, (x + 12, y + 152))

        # Статистика карты (сокровища, вихри, враги)
        attract_count = sum(1 for a in snapshot.anomalies if getattr(a, "is_attracting", False))
        repel_count = sum(1 for a in snapshot.anomalies if getattr(a, "is_repelling", False))
        if attract_count > 0 or repel_count > 0:
            anom_stat = f"Anomalies: {len(snapshot.anomalies)} (A:{attract_count}|R:{repel_count})"
        else:
            anom_stat = f"Anomalies: {len(snapshot.anomalies)}"

        counts = f"Coins: {len(snapshot.treasures)} | {anom_stat} | Enemies: {len(snapshot.enemies)}"
        counts_lbl = self._font_text.render(counts, True, (180, 190, 200))
        surface.blit(counts_lbl, (x + 12, y + 174))

        selected = next((c for c in p.carpets if c.id == selected_carpet_id), None)
        if selected:
            header = self._font_bold.render(f"Selected: {selected.id}", True, self.colors.HUD_ACCENT)
            surface.blit(header, (x + 12, y + 202))
            status = self._font_text.render(f"Status: {selected.status}", True, self.colors.HUD_TEXT)
            speed = self._font_text.render(
                f"V: ({selected.velocity.x:.1f}, {selected.velocity.y:.1f}) | {selected.velocity.length():.1f}",
                True, self.colors.HUD_TEXT,
            )
            own = self._font_text.render(
                f"S: ({selected.acceleration.x:.1f}, {selected.acceleration.y:.1f}) | {selected.acceleration.length():.1f}",
                True, self.colors.ACCELERATION_ARROW,
            )
            anomaly = selected.anomaly_acceleration
            env = self._font_text.render(
                f"W: ({anomaly.x:.1f}, {anomaly.y:.1f}) | {anomaly.length():.1f}",
                True, self.colors.ANOMALY_ACCELERATION_ARROW,
            )
            surface.blit(status, (x + 12, y + 224))
            surface.blit(speed, (x + 12, y + 244))
            surface.blit(own, (x + 12, y + 264))
            surface.blit(env, (x + 12, y + 282))

    def _draw_system_panel(
        self,
        surface: pygame.Surface,
        is_connected: bool,
        fps: float,
        zoom: float,
        follow_mode: bool,
        selected_carpet_id: Optional[str],
        show_scan_fan: bool,
        manual_control_error: Optional[str],
        manual_control_enabled: bool,
        trajectory_fan_step_degrees: float,
        trajectory_fan_max_routes: int,
    ) -> None:
        panel_w = 230
        panel_h = 235
        x = self.config.screen_width - panel_w - 15
        y = 15

        bg_surf = pygame.Surface((panel_w, panel_h), pygame.SRCALPHA)
        pygame.draw.rect(bg_surf, self.colors.HUD_BG, (0, 0, panel_w, panel_h), border_radius=8)
        surface.blit(bg_surf, (x, y))

        # Статус подключения
        conn_text = "ONLINE" if is_connected else "DISCONNECTED"
        conn_color = self.colors.HUD_SUCCESS if is_connected else self.colors.HUD_ALERT
        conn_lbl = self._font_bold.render(f"Server: {conn_text}", True, conn_color)
        surface.blit(conn_lbl, (x + 12, y + 12))

        # FPS и Zoom
        fps_lbl = self._font_text.render(f"FPS: {fps:.0f} (Target 60)", True, self.colors.HUD_TEXT)
        zoom_lbl = self._font_text.render(f"Zoom: {zoom:.2f}x", True, self.colors.HUD_TEXT)
        surface.blit(fps_lbl, (x + 12, y + 36))
        surface.blit(zoom_lbl, (x + 12, y + 56))

        # Режим слежения
        follow_str = "Follow: ON" if follow_mode else "Follow: OFF"
        follow_color = self.colors.HUD_ACCENT if follow_mode else (160, 170, 180)
        follow_lbl = self._font_bold.render(follow_str, True, follow_color)
        surface.blit(follow_lbl, (x + 12, y + 76))
        watch_label = f"Watch: {selected_carpet_id[-12:]}" if selected_carpet_id else "Watch: none"
        watch = self._font_text.render(watch_label, True, self.colors.HUD_TEXT)
        if manual_control_error and manual_control_enabled:
            control_label = "Manual LOCK ERROR"
            control_color = self.colors.HUD_ALERT
        elif manual_control_enabled:
            control_label = "Manual ON | bot paused"
            control_color = self.colors.HUD_ACCENT
        else:
            control_label = "Manual OFF [M]"
            control_color = self.colors.HUD_TEXT
        control = self._font_bold.render(control_label, True, control_color)
        fan = self._font_text.render(f"Trajectory fan [P]: {'ON' if show_scan_fan else 'OFF'}", True, self.colors.HUD_TEXT)
        surface.blit(watch, (x + 12, y + 98))
        surface.blit(control, (x + 12, y + 118))
        surface.blit(fan, (x + 12, y + 138))
        if manual_control_error and manual_control_enabled:
            warning = self._font_small.render("Bot may overwrite controls", True, self.colors.HUD_ALERT)
            surface.blit(warning, (x + 12, y + 155))

        self._draw_slider(surface, x, y + 176, "Scan step", "scan_step", trajectory_fan_step_degrees, 1, 20)
        self._draw_slider(surface, x, y + 207, "Top routes", "route_count", trajectory_fan_max_routes, 1, 10)

    def _slider_track(self, panel_x: int, row_y: int) -> pygame.Rect:
        return pygame.Rect(panel_x + 112, row_y + 7, 98, 4)

    def slider_at(self, position: tuple[int, int]) -> Optional[str]:
        panel_x = self.config.screen_width - 230 - 15
        for name, row_y in (("scan_step", 15 + 176), ("route_count", 15 + 207)):
            hit_rect = pygame.Rect(panel_x + 102, row_y - 4, 116, 24)
            if hit_rect.collidepoint(position):
                return name
        return None

    def slider_value_at(self, name: str, mouse_x: int) -> float | int:
        panel_x = self.config.screen_width - 230 - 15
        row_y = 15 + (176 if name == "scan_step" else 207)
        track = self._slider_track(panel_x, row_y)
        fraction = max(0.0, min(1.0, (mouse_x - track.left) / track.width))
        if name == "scan_step":
            return round(1.0 + fraction * 19.0, 1)
        return max(1, min(10, round(1.0 + fraction * 9.0)))

    def _draw_slider(self, surface, panel_x, row_y, label, name, value, minimum, maximum) -> None:
        label_surface = self._font_small.render(f"{label}: {value:g}{'°' if name == 'scan_step' else ''}", True, self.colors.HUD_TEXT)
        surface.blit(label_surface, (panel_x + 12, row_y + 1))
        track = self._slider_track(panel_x, row_y)
        pygame.draw.line(surface, (120, 135, 155), track.midleft, track.midright, 3)
        fraction = (float(value) - minimum) / (maximum - minimum)
        knob_x = int(track.left + fraction * track.width)
        pygame.draw.circle(surface, self.colors.HUD_ACCENT, (knob_x, track.centery), 6)

    def _draw_trajectory_scores(self, surface, current_forecast, fan_forecasts) -> None:
        if current_forecast is None and not fan_forecasts:
            return
        rows = []
        if current_forecast is not None:
            rows.append(("Current", current_forecast))
        rows.extend((f"{item.angle_degrees:.1f}°", item) for item in fan_forecasts)
        rows = rows[:11]
        row_h = 17
        panel_w = 385
        panel_h = 28 + row_h * len(rows)
        x = 15
        y = self.config.screen_height - panel_h - 34
        panel = pygame.Surface((panel_w, panel_h), pygame.SRCALPHA)
        pygame.draw.rect(panel, self.colors.HUD_BG, (0, 0, panel_w, panel_h), border_radius=7)
        surface.blit(panel, (x, y))
        title = self._font_bold.render("Trajectory score / time (points/s)", True, self.colors.HUD_ACCENT)
        surface.blit(title, (x + 10, y + 6))
        for index, (label, forecast) in enumerate(rows):
            if forecast.time_to_last_bounty is None:
                ratio = "no bounty"
            else:
                denominator = max(0.2, forecast.time_to_last_bounty)
                ratio = (
                    f"{forecast.route_score:.0f} / {denominator:.1f}s = "
                    f"{forecast.score_rate:.1f}/s (last {forecast.time_to_last_bounty:.1f}s)"
                )
            if forecast.death_time is not None:
                ratio += f" | death {forecast.death_time:.1f}s"
            color = self.colors.TRAJECTORY_DEATH if forecast.death_position is not None else self.colors.HUD_TEXT
            text = self._font_small.render(f"{label}: {ratio}", True, color)
            surface.blit(text, (x + 10, y + 25 + index * row_h))

    def _draw_connection_alert(self, surface: pygame.Surface) -> None:
        """Плашка при разрыве или ожидании соединения (FE-005 раздел 5)."""
        banner_w = 340
        banner_h = 44
        bx = (self.config.screen_width - banner_w) // 2
        by = 20

        bg_surf = pygame.Surface((banner_w, banner_h), pygame.SRCALPHA)
        pygame.draw.rect(bg_surf, (192, 57, 43, 230), (0, 0, banner_w, banner_h), border_radius=6)
        surface.blit(bg_surf, (bx, by))

        msg = self._font_bold.render("Connecting to server...", True, (255, 255, 255))
        surface.blit(msg, (bx + (banner_w - msg.get_width()) // 2, by + 12))

    def _draw_controls_help(self, surface: pygame.Surface) -> None:
        """Строка подсказок управления внизу экрана."""
        help_text = "[Click / 1-5] Watch  |  [M] Manual on/off  |  [P] Fan  |  [Space] Follow  |  [Arrows] Pan  |  [+/-] Zoom"
        text_surf = self._font_text.render(help_text, True, (110, 100, 90))
        surface.blit(text_surf, (20, self.config.screen_height - 24))

    def _draw_destroyed_banner(self, surface: pygame.Surface) -> None:
        """Отрисовывает аварийный баннер CARPET DESTROYED BY ANOMALY CORE по центру экрана (FE-008 раздел 4.3)."""
        banner_w = 660
        banner_h = 96
        bx = (self.config.screen_width - banner_w) // 2
        by = (self.config.screen_height - banner_h) // 2

        bg_surf = pygame.Surface((banner_w, banner_h), pygame.SRCALPHA)
        pygame.draw.rect(bg_surf, self.colors.DESTROYED_BANNER_BG, (0, 0, banner_w, banner_h), border_radius=10)
        pygame.draw.rect(bg_surf, self.colors.DESTROYED_BANNER_BORDER, (0, 0, banner_w, banner_h), width=3, border_radius=10)
        surface.blit(bg_surf, (bx, by))

        # Главный заголовок цвета глубокого алого огня
        title_surf = self._font_banner_title.render(
            "CARPET DESTROYED BY ANOMALY CORE", True, self.colors.DESTROYED_BANNER_TEXT
        )
        surface.blit(title_surf, (bx + (banner_w - title_surf.get_width()) // 2, by + 18))

        # Поясняющий подзаголовок
        sub_surf = self._font_banner_sub.render(
            "Lethal core contact detected  |  Carpet disintegrated  |  Telemetry stopped",
            True,
            (240, 180, 180),
        )
        surface.blit(sub_surf, (bx + (banner_w - sub_surf.get_width()) // 2, by + 54))
