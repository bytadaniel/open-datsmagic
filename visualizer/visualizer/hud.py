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
    ) -> None:
        """Отрисовывает информационную панель поверх игрового поля."""
        # 1. Верхняя левая панель: телеметрия игрока и матча
        self._draw_player_panel(surface, snapshot)

        # 2. Верхняя правая панель: статус сети, FPS и управление
        self._draw_system_panel(surface, is_connected, fps, zoom, follow_mode)

        # 3. Плашка потери связи (FE-005 раздел 5)
        if not is_connected:
            self._draw_connection_alert(surface)

        # 4. Подсказка по горячим клавишам внизу экрана
        self._draw_controls_help(surface)

        # 5. Аварийный баннер уничтожения ковра (FE-008 раздел 4.3)
        if snapshot and getattr(snapshot.player, "is_destroyed", False):
            self._draw_destroyed_banner(surface)

    def _draw_player_panel(self, surface: pygame.Surface, snapshot: Optional[WorldSnapshot]) -> None:
        panel_w = 270
        panel_h = 225
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

    def _draw_system_panel(
        self,
        surface: pygame.Surface,
        is_connected: bool,
        fps: float,
        zoom: float,
        follow_mode: bool,
    ) -> None:
        panel_w = 210
        panel_h = 100
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
        help_text = "[Space] Follow Player  |  [+/- / Wheel] Zoom  |  [Arrows / WASD] Pan  |  [R] Reset  |  [Esc] Exit"
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

