"""Точка входа и главный цикл приложения визуализации (FE-005)."""

import argparse
import sys
import time
from pathlib import Path
import pygame

if __package__ is None or __package__ == "":
    sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
    from visualizer.config import VisualizerConfig
    from visualizer.client import ApiClient, NetworkPoller, ThreadSafeSnapshotBuffer
    from visualizer.renderer import Camera, SceneRenderer
    from visualizer.hud import HudRenderer
else:
    from .config import VisualizerConfig
    from .client import ApiClient, NetworkPoller, ThreadSafeSnapshotBuffer
    from .renderer import Camera, SceneRenderer
    from .hud import HudRenderer



def parse_args() -> VisualizerConfig:
    """Парсинг аргументов командной строки."""
    parser = argparse.ArgumentParser(description="DatsMagic 2D Client Visualizer")
    parser.add_argument(
        "--url",
        type=str,
        default="http://127.0.0.1:8080",
        help="Базовый URL игрового сервера REST API (по умолчанию: http://127.0.0.1:8080)",
    )
    parser.add_argument(
        "--token",
        type=str,
        default="team_visualizer",
        help="Токен авторизации X-Auth-Token команды (по умолчанию: team_visualizer)",
    )
    parser.add_argument(
        "--width",
        type=int,
        default=1280,
        help="Ширина графического окна в пикселях (по умолчанию: 1280)",
    )
    parser.add_argument(
        "--height",
        type=int,
        default=720,
        help="Высота графического окна в пикселях (по умолчанию: 720)",
    )
    parser.add_argument(
        "--fps",
        type=int,
        default=60,
        help="Целевая частота кадров FPS (по умолчанию: 60)",
    )

    args = parser.parse_args()
    return VisualizerConfig(
        server_url=args.url,
        auth_token=args.token,
        screen_width=args.width,
        screen_height=args.height,
        fps=args.fps,
    )


class VisualizerApp:
    """Главный класс графического клиента визуализации DatsMagic."""

    def __init__(self, config: VisualizerConfig):
        self.config = config
        pygame.init()
        pygame.font.init()

        self.screen = pygame.display.set_mode((config.screen_width, config.screen_height))
        pygame.display.set_caption(config.window_title)
        self.clock = pygame.time.Clock()

        # Камера, рендереры и сеть
        self.camera = Camera(
            offset_x=config.arena_width / 2.0,
            offset_y=config.arena_height / 2.0,
            zoom=0.6,
            screen_width=config.screen_width,
            screen_height=config.screen_height,
        )
        self.renderer = SceneRenderer(config)
        self.hud = HudRenderer(config)

        self.buffer = ThreadSafeSnapshotBuffer()
        self.api_client = ApiClient(config.server_url, config.auth_token)
        self.poller = NetworkPoller(
            self.api_client,
            self.buffer,
            poll_interval=config.poll_interval,
            reconnect_interval=config.reconnect_interval,
        )

        self.follow_player = True
        self.is_dragging = False
        self.drag_start = (0, 0)
        self.camera_start_offset = (0.0, 0.0)

    def run(self) -> None:
        """Запуск главного цикла приложения."""
        self.poller.start()
        running = True
        prev_time = time.time()

        try:
            while running:
                current_time = time.time()
                dt = current_time - prev_time
                prev_time = current_time

                # 1. Обработка событий
                running = self._handle_events(dt)

                # 2. Обновление камеры (слежение за игроком)
                snapshot = self.buffer.get_latest()
                if self.follow_player and snapshot:
                    p = snapshot.player
                    # Плавная интерполяция к позиции игрока
                    self.camera.offset_x += (p.position.x - self.camera.offset_x) * min(1.0, dt * 10.0)
                    self.camera.offset_y += (p.position.y - self.camera.offset_y) * min(1.0, dt * 10.0)

                # 3. Отрисовка кадра
                self._render_frame(snapshot, current_time)

                # 4. Ограничение FPS
                self.clock.tick(self.config.fps)

        finally:
            self.poller.stop()
            pygame.quit()

    def _handle_events(self, dt: float) -> bool:
        """Обработка пользовательского ввода."""
        for event in pygame.event.get():
            if event.type == pygame.QUIT:
                return False

            elif event.type == pygame.KEYDOWN:
                if event.key == pygame.K_ESCAPE:
                    return False
                elif event.key == pygame.K_SPACE:
                    self.follow_player = not self.follow_player
                elif event.key in (pygame.K_PLUS, pygame.K_EQUALS, pygame.K_KP_PLUS):
                    self.camera.zoom_at(self.config.zoom_step)
                elif event.key in (pygame.K_MINUS, pygame.K_KP_MINUS):
                    self.camera.zoom_at(1.0 / self.config.zoom_step)
                elif event.key == pygame.K_r:
                    # Сброс камеры к центру арены
                    self.camera.center_on(self.config.arena_width / 2.0, self.config.arena_height / 2.0)
                    self.camera.zoom = 0.6
                    self.follow_player = True

            elif event.type == pygame.MOUSEWHEEL:
                mouse_pos = pygame.mouse.get_pos()
                if event.y > 0:
                    self.camera.zoom_at(self.config.zoom_step, anchor_screen=mouse_pos)
                elif event.y < 0:
                    self.camera.zoom_at(1.0 / self.config.zoom_step, anchor_screen=mouse_pos)

            elif event.type == pygame.MOUSEBUTTONDOWN and event.button == 1:
                self.is_dragging = True
                self.drag_start = pygame.mouse.get_pos()
                self.camera_start_offset = (self.camera.offset_x, self.camera.offset_y)
                self.follow_player = False

            elif event.type == pygame.MOUSEBUTTONUP and event.button == 1:
                self.is_dragging = False

            elif event.type == pygame.MOUSEMOTION and self.is_dragging:
                cur_pos = pygame.mouse.get_pos()
                dx = cur_pos[0] - self.drag_start[0]
                dy = cur_pos[1] - self.drag_start[1]
                self.camera.offset_x = self.camera_start_offset[0] - dx / self.camera.zoom
                self.camera.offset_y = self.camera_start_offset[1] + dy / self.camera.zoom

        # Непрерывное перемещение камеры стрелками или WASD
        keys = pygame.key.get_pressed()
        pan_dx = 0.0
        pan_dy = 0.0
        if keys[pygame.K_LEFT] or keys[pygame.K_a]:
            pan_dx -= self.config.pan_speed * dt
            self.follow_player = False
        if keys[pygame.K_RIGHT] or keys[pygame.K_d]:
            pan_dx += self.config.pan_speed * dt
            self.follow_player = False
        if keys[pygame.K_UP] or keys[pygame.K_w]:
            pan_dy -= self.config.pan_speed * dt
            self.follow_player = False
        if keys[pygame.K_DOWN] or keys[pygame.K_s]:
            pan_dy += self.config.pan_speed * dt
            self.follow_player = False

        if pan_dx != 0.0 or pan_dy != 0.0:
            self.camera.pan(pan_dx, pan_dy)

        return True

    def _render_frame(self, snapshot, current_time: float, dt: float = 0.016) -> None:
        """Отрисовывает полный графический кадр."""
        # 1. Сетка и фон
        self.renderer.draw_grid(self.screen, self.camera)

        # 2. Сущности мира при наличии снимка
        if snapshot:
            # Аномалии
            for anomaly in snapshot.anomalies:
                self.renderer.draw_anomaly(self.screen, self.camera, anomaly, current_time)

            # Сокровища с динамической градацией цвета и размера (FE-012)
            for treasure in snapshot.treasures:
                self.renderer.draw_treasure(self.screen, self.camera, treasure, current_time)

            # Противники с поддержкой флота ковров (FE-012)
            for enemy in snapshot.enemies:
                self.renderer.draw_enemy_fleet(self.screen, self.camera, enemy, current_time)

            # Флот управляемого игрока (FE-012)
            self.renderer.draw_player_fleet(self.screen, self.camera, snapshot.player, current_time)

            # Анимация взрывов и обломков (FE-012)
            self.renderer.draw_explosions(self.screen, self.camera, dt)

        # 3. HUD телеметрии
        is_connected, _, _ = self.buffer.get_status()
        self.hud.draw(
            self.screen,
            snapshot,
            is_connected=is_connected,
            fps=self.clock.get_fps(),
            zoom=self.camera.zoom,
            follow_mode=self.follow_player,
        )

        # 4. Обновление экрана
        pygame.display.flip()


def main() -> None:
    """Точка входа запуска визуализатора."""
    config = parse_args()
    app = VisualizerApp(config)
    app.run()


if __name__ == "__main__":
    main()
