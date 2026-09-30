# DatsMagic 🧞‍♂️✨

> Высокопроизводительный пошагово-непрерывный симулятор управления ковром-самолетом и автономный 2D-визуализатор телеметрии.

Проект разработан по методологии **Spec-Driven Design (SDD)**, где каталог [`docs/`](docs/) является **единственным источником правды (Single Source of Truth, SSOT)**.

---

## Архитектура проекта

Проект организован по принципу монорепозитория со строгим разделением ответственности:

```text
datsmagic/
├── docs/             # [SSOT] Спецификации: mechanics.md, adr/, domain/, features/
├── server/           # Высокопроизводительный Rust-сервер симулятора (Axum + Tokio)
├── visualizer/       # Графический 2D-клиент телеметрии на Python (Pygame)
├── scripts/          # Скрипты локального запуска (run_server.sh, run_visualizer.sh)
├── AGENTS.md         # Руководство и правила Spec-Driven Design для AI-агентов
├── Cargo.toml        # Корневой манифест Cargo Workspace
└── README.md         # Документация проекта
```

### Компоненты системы

1. **Rust Симулятор (`server/`)**:
   - **Engine ([FE-001](docs/features/FE-001-server-runtime-and-game-loop.md))**: Тактовый таймер $\Delta t = 200\text{ ms}$ (5 Hz), буфер команд с защитой от race conditions и rate-limiting.
   - **Physics ([FE-002](docs/features/FE-002-euler-physics-engine.md))**: 2D векторный интегратор Эйлера, затухание вязкого трения ($k_f = 0.98$), ограничение ускорения ($A_{max}$) и предельной скорости ($V_{max}$).
   - **Spatial & Collisions ([FE-003](docs/features/FE-003-spatial-entities-and-collisions.md))**: Взаимодействие сущностей, захват сокровищ в радиусе $R_{capture}$, суперпозиция гравитационных аномалий $F_{pull}$, механика оглушения (`stunned`).
   - **REST API ([FE-004](docs/features/FE-004-simulation-rest-api.md))**: Асинхронный сервер Axum (`GET /api/game/state`, `POST /api/carpet/command`), авторизация через `X-Auth-Token`.

2. **Python Визуализатор (`visualizer/`)**:
   - **Visualizer ([FE-005](docs/features/FE-005-python-2d-visualizer.md))**: 60 FPS интерфейс на Pygame с инверсией оси $Y$ (`world_to_screen`), плавным панорамированием, масштабированием (zoom), слежением за игроком (follow camera), векторными стрелками скорости, анимированными вихрями и HUD-телеметрией.

---

## Быстрый старт

### Требования
- **Rust**: 1.80+ (`cargo`, `rustc`)
- **Python**: 3.10+ (рекомендуется 3.12–3.14)

### 1. Запуск игрового сервера
```bash
./scripts/run_server.sh
# Или напрямую через cargo:
cargo run --bin server
```
Сервер будет доступен по адресу `http://127.0.0.1:8080`.

### 2. Запуск 2D-визуализатора
В отдельном окне терминала выполните:
```bash
./scripts/run_visualizer.sh
```
Скрипт автоматически создаст виртуальное окружение `visualizer/.venv`, установит зависимости из `requirements.txt` и запустит графический интерфейс.

Параметры запуска:
```bash
./scripts/run_visualizer.sh --url http://127.0.0.1:8080 --token dev-token --width 1280 --height 720
```

#### Управление в визуализаторе:
- **`Пробел`**: Включить/выключить режим автоматического слежения камеры за игроком.
- **`+` / `-` / Колесо мыши**: Приблизить / отдалить масштаб сцены (Zoom).
- **`W`, `A`, `S`, `D` / Стрелки**: Ручное перемещение камеры по карте.
- **Зажатая ЛКМ**: Панорамирование сцены перетаскиванием мыши.
- **`R`**: Сброс позиции камеры и масштаба в начальное состояние.
- **`Escape`**: Выход из визуализатора.

---

## Запуск тестов

### Тесты Rust-сервера (Unit & Integration)
```bash
cargo test
```
Запуск статического анализатора кода:
```bash
cargo clippy --all-targets -- -D warnings
```

### Тесты Python-визуализатора
```bash
visualizer/.venv/bin/pytest visualizer/tests
```

---

## Спецификации и документация (Docs-First)

В соответствии с правилами в [AGENTS.md](AGENTS.md):
- [docs/mechanics.md](docs/mechanics.md) — Базовая игровая механика и математические формулы.
- [docs/adr/](docs/adr/) — Архитектурные решения (ADR-001, ADR-002).
- [docs/domain/](docs/domain/) — Записи предметных областей (DR-001 — DR-005).
- [docs/features/](docs/features/) — Технические спецификации фичей (FE-001 — FE-008).

