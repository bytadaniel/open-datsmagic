"""Точка входа и главный цикл приложения визуализации (FE-005)."""

import argparse
import json
import os
import sys
import time
from pathlib import Path
import pygame

if __package__ is None or __package__ == "":
    sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
    from visualizer.config import VisualizerConfig
    from visualizer.client import ApiClient, NetworkPoller, ThreadSafeSnapshotBuffer, Vector2D
    from visualizer.renderer import Camera, SceneRenderer
    from visualizer.hud import HudRenderer
else:
    from .config import VisualizerConfig
    from .client import ApiClient, NetworkPoller, ThreadSafeSnapshotBuffer, Vector2D
    from .renderer import Camera, SceneRenderer
    from .hud import HudRenderer


def token_control_hash(token: str) -> int:
    """FNV-1a 64-bit suffix shared with player_2's manual-control lease path."""
    value = 0xCBF29CE484222325
    for byte in token.encode("utf-8"):
        value = ((value ^ byte) * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return value



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
        required=True,
        help="Обязательный токен X-Auth-Token",
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
        default=800,
        help="Высота графического окна в пикселях (по умолчанию: 800)",
    )
    parser.add_argument(
        "--fps",
        type=int,
        default=60,
        help="Целевая частота кадров FPS (по умолчанию: 60)",
    )

    args = parser.parse_args()
    auth_token = args.token.strip()
    if not auth_token:
        parser.error("Токен визуализатора не должен быть пустым")

    return VisualizerConfig(
        server_url=args.url,
        auth_token=auth_token,
        screen_width=args.width,
        screen_height=args.height,
        fps=args.fps,
    )


class VisualizerApp:
    """Главный класс графического клиента визуализации DatsMagic."""

    def __init__(self, config: VisualizerConfig):
        self.config = config
        if not os.getenv("DATS_MANUAL_CONTROL_FILE"):
            self.config.manual_control_file = config.manual_control_file.with_name(
                f"manual_control_{token_control_hash(config.auth_token):016x}.json"
            )
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
        self.selected_carpet_id: str | None = None
        self.manual_control_enabled = False
        self.show_scan_fan = False
        self._active_slider: str | None = None
        self._manual_control_last_write = 0.0
        self._manual_control_id: str | None = None
        self._manual_control_error: str | None = None

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
                snapshot = self.buffer.get_interpolated(self.config.poll_interval)
                if self.follow_player and snapshot:
                    p = next(
                        (carpet for carpet in snapshot.player.carpets if carpet.id == self.selected_carpet_id),
                        snapshot.player,
                    )
                    # Плавная интерполяция к позиции игрока
                    self.camera.offset_x += (p.position.x - self.camera.offset_x) * min(1.0, dt * 10.0)
                    self.camera.offset_y += (p.position.y - self.camera.offset_y) * min(1.0, dt * 10.0)

                # 3. Отрисовка кадра
                self._render_frame(snapshot, current_time, dt)

                # 4. Ограничение FPS
                self.clock.tick(self.config.fps)

        finally:
            if self._manual_control_id:
                self._refresh_manual_control_lease(None)
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
                elif event.key == pygame.K_p:
                    self.show_scan_fan = not self.show_scan_fan
                elif event.key == pygame.K_m:
                    if self.selected_carpet_id and not self.manual_control_enabled:
                        snapshot = self.buffer.get_latest()
                        selected = next(
                            (
                                carpet for carpet in snapshot.player.carpets
                                if carpet.id == self.selected_carpet_id and not carpet.is_destroyed
                            ),
                            None,
                        ) if snapshot else None
                        self.manual_control_enabled = selected is not None
                    else:
                        self.manual_control_enabled = False
                elif pygame.K_1 <= event.key <= pygame.K_5:
                    snapshot = self.buffer.get_latest()
                    index = event.key - pygame.K_1
                    if snapshot and index < len(snapshot.player.carpets):
                        carpet = snapshot.player.carpets[index]
                        self.selected_carpet_id = None if carpet.is_destroyed else carpet.id
                        self.manual_control_enabled = False
                        self.follow_player = True
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
                slider_name = self.hud.slider_at(event.pos)
                if slider_name:
                    self._active_slider = slider_name
                    self._update_slider(slider_name, event.pos[0])
                    continue
                snapshot = self.buffer.get_latest()
                mouse = pygame.mouse.get_pos()
                selected = None
                if snapshot:
                    candidates = []
                    for carpet in snapshot.player.carpets:
                        if carpet.is_destroyed:
                            continue
                        sx, sy = self.camera.world_to_screen(carpet.position.x, carpet.position.y)
                        distance_sq = (sx - mouse[0]) ** 2 + (sy - mouse[1]) ** 2
                        candidates.append((distance_sq, carpet.id))
                    if candidates:
                        distance_sq, nearest_id = min(candidates)
                        if distance_sq <= 30 ** 2:
                            selected = nearest_id
                if selected:
                    self.selected_carpet_id = selected
                    self.manual_control_enabled = False
                    self.follow_player = True
                else:
                    self.selected_carpet_id = None
                    self.manual_control_enabled = False

            elif event.type == pygame.MOUSEBUTTONUP and event.button == 1:
                self._active_slider = None
                self.is_dragging = False

            elif event.type == pygame.MOUSEMOTION:
                if self._active_slider:
                    self._update_slider(self._active_slider, event.pos[0])
                elif self.is_dragging:
                    cur_pos = pygame.mouse.get_pos()
                    dx = cur_pos[0] - self.drag_start[0]
                    dy = cur_pos[1] - self.drag_start[1]
                    self.camera.offset_x = self.camera_start_offset[0] - dx / self.camera.zoom
                    self.camera.offset_y = self.camera_start_offset[1] + dy / self.camera.zoom

        # Стрелки перемещают камеру; выбранный ковер непрерывно следует за курсором.
        keys = pygame.key.get_pressed()
        pan_dx = 0.0
        pan_dy = 0.0
        if keys[pygame.K_LEFT]:
            pan_dx += self.config.pan_speed * dt
            self.follow_player = False
        if keys[pygame.K_RIGHT]:
            pan_dx -= self.config.pan_speed * dt
            self.follow_player = False
        if keys[pygame.K_UP]:
            pan_dy += self.config.pan_speed * dt
            self.follow_player = False
        if keys[pygame.K_DOWN]:
            pan_dy -= self.config.pan_speed * dt
            self.follow_player = False

        if pan_dx != 0.0 or pan_dy != 0.0:
            self.camera.pan(pan_dx, pan_dy)

        snapshot = self.buffer.get_latest()
        selected_carpet = None
        if snapshot and self.selected_carpet_id:
            selected_carpet = next(
                (
                    carpet for carpet in snapshot.player.carpets
                    if carpet.id == self.selected_carpet_id and not carpet.is_destroyed
                ),
                None,
            )
            if selected_carpet is None:
                self.selected_carpet_id = None
        if selected_carpet:
            if self.manual_control_enabled:
                manual_ready = self._refresh_manual_control_lease(selected_carpet.id)
                carpet_x, carpet_y = self.camera.world_to_screen(
                    selected_carpet.position.x, selected_carpet.position.y
                )
                mouse_x, mouse_y = pygame.mouse.get_pos()
                dx = mouse_x - carpet_x
                dy = carpet_y - mouse_y
                distance = (dx * dx + dy * dy) ** 0.5
                max_accel = selected_carpet.max_acceleration
                magnitude = min(max_accel, distance)
                acceleration = Vector2D(dx / distance * magnitude, dy / distance * magnitude) if distance else Vector2D(0.0, 0.0)
                if manual_ready:
                    self.api_client.set_selected_acceleration(selected_carpet.id, acceleration)
                else:
                    self.api_client.set_selected_acceleration(None, Vector2D(0.0, 0.0))
            else:
                if self._manual_control_id:
                    self._refresh_manual_control_lease(None)
                self.api_client.set_selected_acceleration(None, Vector2D(0.0, 0.0))
        else:
            self.manual_control_enabled = False
            if self._manual_control_id:
                self._refresh_manual_control_lease(None)
            self.api_client.set_selected_acceleration(None, Vector2D(0.0, 0.0))

        return True

    def _update_slider(self, name: str, mouse_x: int) -> None:
        value = self.hud.slider_value_at(name, mouse_x)
        if name == "scan_step":
            self.config.trajectory_fan_step_degrees = float(value)
        elif name == "route_count":
            self.config.trajectory_fan_max_routes = int(value)

    def _refresh_manual_control_lease(self, carpet_id: str | None) -> bool:
        """Обеспечивает эксклюзивный API-цикл визуализатору, пока бот запущен рядом."""
        path = self.config.manual_control_file
        if not carpet_id:
            if self._manual_control_id:
                try:
                    current = json.loads(path.read_text(encoding="utf-8"))
                    if current.get("carpetId") == self._manual_control_id:
                        path.unlink(missing_ok=True)
                except FileNotFoundError:
                    pass
                except (OSError, ValueError, AttributeError) as exc:
                    self._manual_control_error = str(exc)
            else:
                self._manual_control_error = None
            self._manual_control_id = None
            self._manual_control_last_write = 0.0
            return self._manual_control_error is None

        now = time.monotonic()
        if now - self._manual_control_last_write < 0.2 and self._manual_control_error is None:
            return True
        temporary_path = path.with_name(f"{path.name}.{os.getpid()}.tmp")
        try:
            path.parent.mkdir(parents=True, exist_ok=True)
            temporary_path.write_text(
                json.dumps({"carpetId": carpet_id, "expiresAtUnixMs": int(time.time() * 1000) + 1000}),
                encoding="utf-8",
            )
            os.replace(temporary_path, path)
            self._manual_control_error = None
            self._manual_control_id = carpet_id
            self._manual_control_last_write = now
            return True
        except OSError as exc:
            try:
                temporary_path.unlink(missing_ok=True)
            except OSError:
                pass
            self._manual_control_error = str(exc)
            return False

    def _render_frame(self, snapshot, current_time: float, dt: float = 0.016) -> None:
        """Отрисовывает полный графический кадр."""
        current_forecast = None
        fan_forecasts = []
        if snapshot:
            # Размеры мира и транспорта берём из того же Desert-снимка, что и player_2.
            if snapshot.map_size.x > 0 and snapshot.map_size.y > 0:
                self.config.arena_width = snapshot.map_size.x
                self.config.arena_height = snapshot.map_size.y
            if snapshot.transport_radius > 0:
                self.config.carpet_radius = snapshot.transport_radius
            if snapshot.player.max_velocity > 0:
                self.config.max_velocity = snapshot.player.max_velocity
            if snapshot.player.max_acceleration > 0:
                self.config.max_acceleration = snapshot.player.max_acceleration

        # 1. Сетка и фон
        self.renderer.draw_grid(self.screen, self.camera)

        # 2. Сущности мира при наличии снимка
        if snapshot:
            treasure_index = self.renderer.build_treasure_index(snapshot.treasures)
            # Аномалии
            self.renderer.draw_anomalies(self.screen, self.camera, snapshot.anomalies, current_time)

            # Сокровища с динамической градацией цвета и размера (FE-012)
            for treasure in snapshot.treasures:
                self.renderer.draw_treasure(self.screen, self.camera, treasure, current_time)

            # Противники с поддержкой флота ковров (FE-012)
            for enemy in snapshot.enemies:
                self.renderer.draw_enemy_fleet(
                    self.screen, self.camera, enemy, current_time, snapshot.anomalies, treasure_index
                )

            # Флот управляемого игрока (FE-012)
            moving_bodies = [
                (carpet.id, carpet.position, carpet.velocity)
                for carpet in snapshot.player.carpets
                if not carpet.is_destroyed
            ] + [
                (enemy.id, enemy.position, enemy.velocity)
                for enemy in snapshot.enemies
            ]
            current_forecast = self.renderer.draw_player_fleet(
                self.screen,
                self.camera,
                snapshot.player,
                current_time,
                snapshot.anomalies,
                moving_bodies,
                observed_carpet_id=self.selected_carpet_id,
                treasure_index=treasure_index,
            )

            selected = next(
                (
                    c for c in snapshot.player.carpets
                    if c.id == self.selected_carpet_id and not c.is_destroyed
                ),
                None,
            )
            if selected:
                sx, sy = self.camera.world_to_screen(selected.position.x, selected.position.y)
                pygame.draw.circle(
                    self.screen,
                    self.config.colors.PLAYER_BORDER,
                    (sx, sy),
                    max(12, int(self.config.carpet_radius * self.camera.zoom) + 8),
                    2,
                )
            if self.show_scan_fan and selected:
                fan_forecasts = self.renderer.draw_scan_fan(
                    self.screen,
                    self.camera,
                    selected,
                    snapshot.anomalies,
                    moving_bodies,
                    treasure_index,
                    max_routes=self.config.trajectory_fan_max_routes,
                )

            # Серый прицел: полное направление до курсора, не телеметрия ускорения.
            if selected and self.manual_control_enabled:
                sx, sy = self.camera.world_to_screen(selected.position.x, selected.position.y)
                mx, my = pygame.mouse.get_pos()
                pygame.draw.line(self.screen, self.config.colors.AIM_LINE, (sx, sy), (mx, my), 2)

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
            selected_carpet_id=self.selected_carpet_id,
            show_scan_fan=self.show_scan_fan,
            manual_control_error=self._manual_control_error,
            manual_control_enabled=self.manual_control_enabled,
            trajectory_fan_step_degrees=self.config.trajectory_fan_step_degrees,
            trajectory_fan_max_routes=self.config.trajectory_fan_max_routes,
            current_forecast=current_forecast,
            fan_forecasts=fan_forecasts,
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
