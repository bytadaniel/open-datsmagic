"""Сетевой клиент и структуры данных для взаимодействия с API симулятора DatsMagic."""

from dataclasses import dataclass, field, replace
import math
import threading
import time
from typing import Any, Dict, List, Optional
import requests


@dataclass(frozen=True)
class Vector2D:
    """Двумерный вектор с компонентами (x, y)."""
    x: float
    y: float

    def length(self) -> float:
        """Евклидова длина вектора."""
        return math.sqrt(self.x * self.x + self.y * self.y)

    def distance(self, other: "Vector2D") -> float:
        """Расстояние до другой точки."""
        dx = self.x - other.x
        dy = self.y - other.y
        return math.sqrt(dx * dx + dy * dy)

    def to_tuple(self) -> tuple[float, float]:
        """Преобразование в кортеж."""
        return self.x, self.y

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> "Vector2D":
        """Создание вектора из JSON-словаря."""
        return cls(x=float(data.get("x", 0.0)), y=float(data.get("y", 0.0)))


@dataclass
class CarpetState:
    """Состояние отдельного ковра флота (FE-010 / FE-012)."""
    id: str
    status: str
    position: Vector2D
    velocity: Vector2D
    acceleration: Vector2D = field(default_factory=lambda: Vector2D(0.0, 0.0))
    max_acceleration: float = 5.0
    max_velocity: float = 20.0
    anomaly_acceleration: Vector2D = field(default_factory=lambda: Vector2D(0.0, 0.0))

    @property
    def is_stunned(self) -> bool:
        """Флаг оглушения ковра."""
        return self.status == "stunned"

    @property
    def is_destroyed(self) -> bool:
        """Флаг уничтожения ковра."""
        return self.status == "destroyed"

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> "CarpetState":
        return cls(
            id=str(data.get("id", "")),
            status=str(data.get("status", "normal")),
            position=Vector2D.from_dict(data.get("position", {})),
            velocity=Vector2D.from_dict(data.get("velocity", {})),
            acceleration=Vector2D.from_dict(data.get("acceleration", {})),
            max_acceleration=float(data.get("max_acceleration", 5.0)),
            max_velocity=float(data.get("max_velocity", 20.0)),
        )


@dataclass
class EnemyCarpetState:
    """Состояние ковра противника (FE-010 / FE-012)."""
    id: str
    position: Vector2D
    velocity: Vector2D
    acceleration: Vector2D = field(default_factory=lambda: Vector2D(0.0, 0.0))
    status: str = "normal"

    @property
    def is_destroyed(self) -> bool:
        """Флаг уничтожения ковра."""
        return self.status == "destroyed"

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> "EnemyCarpetState":
        return cls(
            id=str(data.get("id", "")),
            position=Vector2D.from_dict(data.get("position", {})),
            velocity=Vector2D.from_dict(data.get("velocity", {})),
            acceleration=Vector2D.from_dict(data.get("acceleration", {})),
            status=str(data.get("status", "normal")),
        )


@dataclass
class PlayerState:
    """Состояние управляемого ковра игрока и его флота (FE-010 / FE-012)."""
    id: str
    score: int
    status: str
    position: Vector2D
    velocity: Vector2D
    acceleration: Vector2D = field(default_factory=lambda: Vector2D(0.0, 0.0))
    max_acceleration: float = 5.0
    max_velocity: float = 20.0
    carpets: List[CarpetState] = field(default_factory=list)
    anomaly_acceleration: Vector2D = field(default_factory=lambda: Vector2D(0.0, 0.0))

    @property
    def is_stunned(self) -> bool:
        """Флаг оглушения ковра."""
        return self.status == "stunned"

    @property
    def is_destroyed(self) -> bool:
        """Флаг уничтожения ковра (FE-008)."""
        return self.status == "destroyed"

    @property
    def active_carpets_count(self) -> int:
        """Количество активных живых ковров во флоте."""
        if not self.carpets:
            return 0 if self.is_destroyed else 1
        return sum(1 for c in self.carpets if not c.is_destroyed)

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> "PlayerState":
        carpets = [CarpetState.from_dict(c) for c in data.get("carpets", [])]
        return cls(
            id=str(data.get("id", "")),
            score=int(data.get("score", 0)),
            status=str(data.get("status", "normal")),
            position=Vector2D.from_dict(data.get("position", {})),
            velocity=Vector2D.from_dict(data.get("velocity", {})),
            acceleration=Vector2D.from_dict(data.get("acceleration", {})),
            max_acceleration=float(data.get("max_acceleration", 5.0)),
            max_velocity=float(data.get("max_velocity", 20.0)),
            carpets=carpets,
        )


@dataclass
class EnemyState:
    """Состояние соперника и его флота в зоне видимости (FE-010 / FE-012)."""
    id: str
    position: Vector2D
    velocity: Vector2D
    acceleration: Vector2D = field(default_factory=lambda: Vector2D(0.0, 0.0))
    carpets: List[EnemyCarpetState] = field(default_factory=list)

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> "EnemyState":
        carpets = [EnemyCarpetState.from_dict(c) for c in data.get("carpets", [])]
        return cls(
            id=str(data.get("id", "")),
            position=Vector2D.from_dict(data.get("position", {})),
            velocity=Vector2D.from_dict(data.get("velocity", {})),
            acceleration=Vector2D.from_dict(data.get("acceleration", {})),
            carpets=carpets,
        )


@dataclass
class TreasureState:
    """Сокровище на карте."""
    id: str
    type: str
    position: Vector2D
    value: int

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> "TreasureState":
        return cls(
            id=str(data.get("id", "")),
            type=str(data.get("type", "chest")),
            position=Vector2D.from_dict(data.get("position", {})),
            value=int(data.get("value", 0)),
        )


@dataclass(frozen=True, init=False)
class AnomalyState:
    """Аномалия / Песчаный вихрь (FE-008)."""
    id: str
    anomaly_type: str  # "attracting" | "repelling"
    position: Vector2D
    velocity: Vector2D
    core_radius: float
    effect_radius: float
    force: float

    def __init__(
        self,
        id: str = "",
        anomaly_type: Optional[str] = None,
        position: Optional[Vector2D] = None,
        velocity: Optional[Vector2D] = None,
        core_radius: Optional[float] = None,
        effect_radius: Optional[float] = None,
        force: float = 0.0,
        radius: Optional[float] = None,
        type: Optional[str] = None,
        **kwargs: Any,
    ):
        if radius is None and "radius" in kwargs:
            radius = kwargs["radius"]
        if type is None and "type" in kwargs:
            type = kwargs["type"]
        eff_r = effect_radius if effect_radius is not None else (radius if radius is not None else 0.0)
        c_r = core_radius if core_radius is not None else eff_r * 0.2
        a_type = anomaly_type or type or "attracting"
        vel = velocity if velocity is not None else Vector2D(0.0, 0.0)
        pos = position if position is not None else Vector2D(0.0, 0.0)

        object.__setattr__(self, "id", str(id))
        object.__setattr__(self, "anomaly_type", str(a_type))
        object.__setattr__(self, "position", pos)
        object.__setattr__(self, "velocity", vel)
        object.__setattr__(self, "core_radius", float(c_r))
        object.__setattr__(self, "effect_radius", float(eff_r))
        object.__setattr__(self, "force", float(force))

    @property
    def radius(self) -> float:
        """Для обратной совместимости с FE-005."""
        return self.effect_radius

    @property
    def type(self) -> str:
        """Синоним anomaly_type."""
        return self.anomaly_type

    @property
    def is_attracting(self) -> bool:
        """Флаг притягивающего вихря."""
        return self.anomaly_type == "attracting"

    @property
    def is_repelling(self) -> bool:
        """Флаг отталкивающего вихря."""
        return self.anomaly_type == "repelling"

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> "AnomalyState":
        radius = float(data.get("radius", data.get("effect_radius", 0.0)))

        # fallback: core_radius = radius * 0.2
        raw_core = data.get("core_radius")
        if raw_core is not None:
            core_radius = float(raw_core)
        else:
            core_radius = radius * 0.2

        # fallback: type = "attracting"
        raw_type = data.get("type", data.get("anomaly_type", "attracting"))
        anomaly_type = str(raw_type) if raw_type else "attracting"

        # fallback: velocity = Vector2D(0.0, 0.0)
        vel_data = data.get("velocity")
        if isinstance(vel_data, dict):
            velocity = Vector2D.from_dict(vel_data)
        else:
            velocity = Vector2D(0.0, 0.0)

        pos_data = data.get("position")
        if isinstance(pos_data, dict):
            position = Vector2D.from_dict(pos_data)
        else:
            position = Vector2D(0.0, 0.0)

        force = float(data.get("force", 0.0))

        return cls(
            id=str(data.get("id", "")),
            anomaly_type=anomaly_type,
            position=position,
            velocity=velocity,
            core_radius=core_radius,
            effect_radius=radius,
            force=force,
        )


@dataclass
class WorldSnapshot:
    """Неизменяемый снимок состояния мира от сервера."""
    tick: int
    game_status: str
    player: PlayerState
    treasures: List[TreasureState] = field(default_factory=list)
    anomalies: List[AnomalyState] = field(default_factory=list)
    enemies: List[EnemyState] = field(default_factory=list)
    map_size: Vector2D = field(default_factory=lambda: Vector2D(0.0, 0.0))
    transport_radius: float = 0.0
    received_at: float = field(default_factory=time.time)

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> "WorldSnapshot":
        """Парсинг канонического ответа `Desert` внешнего API."""
        def status(value: Any) -> str:
            return "destroyed" if value == "dead" else "normal"

        carpets = [
            CarpetState(
                id=str(transport.get("id", "")),
                status=status(transport.get("status")),
                position=Vector2D(float(transport.get("x", 0.0)), float(transport.get("y", 0.0))),
                velocity=Vector2D.from_dict(transport.get("velocity", {})),
                acceleration=Vector2D.from_dict(transport.get("selfAcceleration", {})),
                anomaly_acceleration=Vector2D.from_dict(transport.get("anomalyAcceleration", {})),
                max_acceleration=float(data.get("maxAccel", 0.0)),
                max_velocity=float(data.get("maxSpeed", 0.0)),
            )
            for transport in data.get("transports", [])
        ]
        primary = carpets[0] if carpets else CarpetState(
            id="", status="normal", position=Vector2D(0.0, 0.0), velocity=Vector2D(0.0, 0.0)
        )
        anomalies = [
            AnomalyState(
                id=str(anomaly.get("id", "")),
                anomaly_type="repelling" if float(anomaly.get("strength", 0.0)) < 0 else "attracting",
                position=Vector2D(float(anomaly.get("x", 0.0)), float(anomaly.get("y", 0.0))),
                velocity=Vector2D.from_dict(anomaly.get("velocity", {})),
                core_radius=float(anomaly.get("radius", 0.0)),
                effect_radius=float(anomaly.get("effectiveRadius", 0.0)),
                force=abs(float(anomaly.get("strength", 0.0))),
            )
            for anomaly in data.get("anomalies", [])
        ]
        return cls(
            tick=int(data.get("tick", 0)),
            game_status="active",
            player=PlayerState(
                id=str(data.get("name", "")),
                score=int(data.get("points", 0)),
                status=primary.status,
                position=primary.position,
                velocity=primary.velocity,
                acceleration=primary.acceleration,
                anomaly_acceleration=primary.anomaly_acceleration,
                max_acceleration=float(data.get("maxAccel", 0.0)),
                max_velocity=float(data.get("maxSpeed", 0.0)),
                carpets=carpets,
            ),
            treasures=[
                TreasureState(
                    id=f"bounty-{index}", type="bounty",
                    position=Vector2D(float(bounty.get("x", 0.0)), float(bounty.get("y", 0.0))),
                    value=int(bounty.get("points", 0)),
                )
                for index, bounty in enumerate(data.get("bounties", []))
            ],
            anomalies=anomalies,
            enemies=[
                EnemyState(
                    id=f"enemy-{index}",
                    position=Vector2D(float(enemy.get("x", 0.0)), float(enemy.get("y", 0.0))),
                    velocity=Vector2D.from_dict(enemy.get("velocity", {})),
                )
                for index, enemy in enumerate(data.get("enemies", []))
            ],
            map_size=Vector2D.from_dict(data.get("mapSize", {})),
            transport_radius=float(data.get("transportRadius", 0.0)),
            received_at=time.time(),
        )


class ApiClient:
    """Сетевой HTTP-клиент опроса игрового сервера DatsMagic."""

    def __init__(self, base_url: str, token: str, timeout: float = 1.0):
        self.base_url = base_url.rstrip("/")
        self.token = token
        self.timeout = timeout
        self.session = requests.Session()
        self.session.headers.update({
            "X-Auth-Token": self.token,
            "Accept": "application/json",
        })
        self._snapshot_tick = 0
        self._selected_command: Optional[Dict[str, Any]] = None
        self._command_lock = threading.Lock()

    def set_selected_acceleration(self, carpet_id: Optional[str], acceleration: Vector2D) -> None:
        """Задает команду выбранному ковру для следующего опроса Desert."""
        with self._command_lock:
            self._selected_command = (
                {"id": carpet_id, "acceleration": {"x": acceleration.x, "y": acceleration.y}}
                if carpet_id else None
            )

    def get_desert(self) -> WorldSnapshot:
        """Опросить единственный внешний API пустым пакетом команд."""
        url = f"{self.base_url}/play/magcarp/player/move"
        with self._command_lock:
            selected_command = self._selected_command
            payload = {"transports": [selected_command] if selected_command else []}
        response = self.session.post(url, json=payload, timeout=self.timeout)
        response.raise_for_status()
        data = response.json()
        self._snapshot_tick += 1
        data["tick"] = self._snapshot_tick
        return WorldSnapshot.from_dict(data)


class ThreadSafeSnapshotBuffer:
    """Потокобезопасный буфер для обмена снимками мира между сетью и графикой."""

    def __init__(self):
        self._lock = threading.Lock()
        self._snapshot: Optional[WorldSnapshot] = None
        self._prev_snapshot: Optional[WorldSnapshot] = None
        self._is_connected: bool = False
        self._last_error: Optional[str] = None
        self._last_success_time: float = 0.0

    def update(self, snapshot: WorldSnapshot) -> None:
        """Записывает новый снимок мира."""
        with self._lock:
            self._prev_snapshot = self._snapshot
            self._snapshot = snapshot
            self._is_connected = True
            self._last_error = None
            self._last_success_time = time.time()

    def record_error(self, error_msg: str) -> None:
        """Фиксирует сетевой сбой без сброса последнего кадра."""
        with self._lock:
            self._is_connected = False
            self._last_error = error_msg

    def get_latest(self) -> Optional[WorldSnapshot]:
        """Возвращает актуальный снимок мира."""
        with self._lock:
            return self._snapshot

    def get_interpolated(self, interval: float) -> Optional[WorldSnapshot]:
        """Возвращает плавный кадр между двумя последними снимками сервера."""
        with self._lock:
            current = self._snapshot
            previous = self._prev_snapshot
            if current is None or previous is None or interval <= 0.0:
                return current
            alpha = max(0.0, min(1.0, (time.time() - current.received_at) / interval))
            return interpolate_snapshot(previous, current, alpha)

    def get_status(self) -> tuple[bool, Optional[str], float]:
        """Возвращает (is_connected, last_error, last_success_time)."""
        with self._lock:
            return self._is_connected, self._last_error, self._last_success_time


def _lerp_vector(previous: Vector2D, current: Vector2D, alpha: float) -> Vector2D:
    return Vector2D(
        previous.x + (current.x - previous.x) * alpha,
        previous.y + (current.y - previous.y) * alpha,
    )


def _interpolate_by_id(previous: List[Any], current: List[Any], alpha: float) -> List[Any]:
    previous_by_id = {entity.id: entity for entity in previous}
    return [
        replace(entity, position=_lerp_vector(previous_by_id[entity.id].position, entity.position, alpha))
        if entity.id in previous_by_id
        else entity
        for entity in current
    ]


def interpolate_snapshot(previous: WorldSnapshot, current: WorldSnapshot, alpha: float) -> WorldSnapshot:
    """Линейно интерполирует только визуальные позиции совпадающих сущностей."""
    alpha = max(0.0, min(1.0, alpha))
    player = replace(
        current.player,
        position=_lerp_vector(previous.player.position, current.player.position, alpha),
        carpets=_interpolate_by_id(previous.player.carpets, current.player.carpets, alpha),
    )
    previous_enemies = {enemy.id: enemy for enemy in previous.enemies}
    enemies = [
        replace(
            enemy,
            position=_lerp_vector(previous_enemies[enemy.id].position, enemy.position, alpha),
            carpets=_interpolate_by_id(previous_enemies[enemy.id].carpets, enemy.carpets, alpha),
        )
        if enemy.id in previous_enemies
        else enemy
        for enemy in current.enemies
    ]
    return replace(
        current,
        player=player,
        anomalies=_interpolate_by_id(previous.anomalies, current.anomalies, alpha),
        enemies=enemies,
    )


class NetworkPoller:
    """Фоновый воркер опроса сервера симуляции."""

    def __init__(
        self,
        client: ApiClient,
        buffer: ThreadSafeSnapshotBuffer,
        poll_interval: float = 0.2,
        reconnect_interval: float = 1.0,
    ):
        self.client = client
        self.buffer = buffer
        self.poll_interval = poll_interval
        self.reconnect_interval = reconnect_interval
        self._running = False
        self._thread: Optional[threading.Thread] = None

    def start(self) -> None:
        """Запускает фоновый поток опроса."""
        self._running = True
        self._thread = threading.Thread(target=self._run_loop, daemon=True, name="NetworkPoller")
        self._thread.start()

    def stop(self) -> None:
        """Останавливает фоновый поток."""
        self._running = False
        if self._thread and self._thread.is_alive():
            self._thread.join(timeout=1.0)

    def _run_loop(self) -> None:
        while self._running:
            start_time = time.time()
            try:
                snapshot = self.client.get_desert()
                self.buffer.update(snapshot)
                sleep_time = max(0.01, self.poll_interval - (time.time() - start_time))
            except Exception as e:
                self.buffer.record_error(str(e))
                sleep_time = self.reconnect_interval

            time.sleep(sleep_time)
