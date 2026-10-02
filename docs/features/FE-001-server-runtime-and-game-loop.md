---
id: FE-001
title: "Ядро игрового сервера и тактовый генератор тиков на Rust"
module: "server::engine"
author: "AI Agent & Systems Architect"
created_at: "2026-09-30"
updated_at: "2026-09-30"
status: "approved"
version: 1.0
tags:
  - rust
  - tokio
  - game-loop
  - engine
  - solid
related_domain_records:
  - DR-001
related_test_cases:
  - TC-ENG-01
  - TC-ENG-02
---

# FE-001: Ядро игрового сервера и тактовый генератор тиков на Rust

> Описание технического дизайна ядра сервера, управления игровыми тиками и атомарной синхронизации состояния.

---

## 1. Контекст и бизнес-цель

Фича реализует основу игрового сервера симулятора DatsMagic на языке Rust. Сервер должен гарантировать непрерывный и стабильный игровой цикл с периодом $\Delta t = 200$ мс (5 тиков в секунду), потокобезопасный сбор команд игроков через буфер тика и атомарное продвижение симуляции с генерацией неизменяемого снапшота для клиентов.

Основано на требованиях предметной области [`DR-001`](file:///Users/d.byta/Documents/Code/dats/datsmagic/docs/domain/DR-001-game-loop-and-state.md).

---

## 2. Архитектурное решение

Игровой сервер построен на базе асинхронного рантайма `tokio`. Цикл симулятора выполняется в выделенной фоновой задаче, изолированной от потоков обработки HTTP-запросов.

### Схема взаимодействия компонентов

```mermaid
sequenceDiagram
    participant Timer as Tokio Interval (200ms)
    participant Loop as GameEngineLoop
    participant InBuf as InputCommandBuffer
    participant Physics as PhysicsSolver
    participant Spatial as SpatialCollisionManager
    participant State as SharedGameState

    Timer->>Loop: Tick event (Δt = 0.2s)
    Loop->>InBuf: Drain commands for current tick
    InBuf-->>Loop: Map<PlayerId, Command>
    Loop->>Physics: Update motion (commands, friction, dt)
    Physics-->>Loop: Updated positions & velocities
    Loop->>Spatial: Check collisions & apply anomalies
    Spatial-->>Loop: Applied forces, collected treasures, stuns
    Loop->>State: Atomically publish new WorldSnapshot (tick++)
    State-->>Loop: Published
```

---

## 3. Модели данных и структуры в ОЗУ (Rust)

Сервер хранит состояние исключительно в оперативной памяти (без постоянной БД), обеспечивая доступ через потокобезопасный примитив `Arc<RwLock<GameState>>`.

```rust
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Идентификатор команды игрока
pub type PlayerId = String;

/// Буфер команд, накопленных за текущий тик
#[derive(Default, Debug)]
pub struct InputCommandBuffer {
    pub pending_commands: HashMap<PlayerId, PlayerCommand>,
}

/// Команда игрока с вектором ускорения
#[derive(Clone, Copy, Debug)]
pub struct PlayerCommand {
    pub acceleration: (f64, f64),
}

/// Состояние игровой сессии
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionStatus {
    Active,
    Paused,
    Finished,
}

/// Полный контекст симуляции
pub struct GameState {
    pub tick: u64,
    pub status: SessionStatus,
    pub world: WorldData,
}

pub type SharedGameState = Arc<RwLock<GameState>>;
```

---

## 4. Алгоритмы и вспомогательные функции

### 4.1. Главный цикл симуляции (`GameEngineLoop::run`)
1. Инициализация интервала: `let mut interval = tokio::time::interval(Duration::from_millis(200));`.
2. Режим пропуска задержек: `interval.set_missed_tick_behavior(MissedTickBehavior::Skip);`.
3. В начале каждого шага цикла:
   - Захват блокировки на запись `InputCommandBuffer` с помощью метода `std::mem::take`, очищающего буфер для следующего тика.
   - Вызов фазы физического движения (`PhysicsSolver::step`).
   - Вызов фазы коллизий и сбора сокровищ (`SpatialManager::resolve`).
   - Инкремент `tick += 1` и публикация обновленного снапшота в `SharedGameState`.

---

## 5. API-контракты и DTO

Компонент ядра предоставляет интерфейс управления игровым циклом для веб-слоя:

| Метод Rust | Назначение |
| :--- | :--- |
| `GameEngine::register_command(player_id, command)` | Регистрация команды в буфере текущего тика |
| `GameEngine::get_snapshot() -> Arc<WorldSnapshot>` | Получение актуального снапшота для чтения без блокировки симулятора |

Принятое ускорение удерживается между тиками до замены новой командой. Если в буфере конкретного ковра на тик нет записи (пакет опоздал, отклонен rate limit-ом или не содержал этот ковер), физика повторно использует его последнее применённое ускорение. Явная команда `(0, 0)` останавливает тягу; при уничтожении и респавне сохранённое ускорение сбрасывается.

---

## 6. Обработка ошибок (Error Handling)

| Исключительная ситуация | Реакция ядра | Причина |
| :--- | :--- | :--- |
| Повторная отправка команды за один тик | Возврат ошибки `CommandError::AlreadySubmitted` | Превышение лимита в 1 команду за такт |
| Слишком долгий шаг физики ($> 200$ мс) | Предупреждение `tracing::warn!` и переход к следующему такту | Временная пиковая нагрузка CPU |

---

## 7. План реализации

- [x] **Шаг 1:** Реализовать структуру `InputCommandBuffer` с потокобезопасным методом `drain`.
- [x] **Шаг 2:** Создать абстракцию игрового цикла `GameEngine` с использованием `tokio::time::interval`.
- [x] **Шаг 3:** Реализовать конвейер фаз тика: `drain_input -> physics_step -> spatial_step -> publish_snapshot`.
- [x] **Шаг 4:** Покрыть unit-тестами точность интервалов и очистку буфера команд.

---

## 8. Тестирование

### Unit-тесты
- `test_command_buffer_drain`: проверка извлечения команд и очистки буфера для нового тика.
- `test_rate_limit_per_tick`: проверка отклонения повторной команды от того же `PlayerId` в рамках одного такта.

### Интеграционные тесты
- `test_game_loop_advances_ticks`: запуск цикла на 1 секунду с проверкой, что счетчик тиков увеличился ровно на 5.

---

## История изменений

| Версия | Дата | Автор | Изменение |
| :--- | :--- | :--- | :--- |
| 1.0 | 2026-09-30 | AI Agent | Первоначальное описание технической архитектуры игрового цикла |
| 1.1 | 2026-10-02 | Codex | Зафиксировано удержание последней принятой команды между тиками и сброс при гибели/респавне. |
