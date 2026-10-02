# DatsMagic 🧞‍♂️✨

> Высокопроизводительный пошагово-непрерывный симулятор управления ковром-самолетом и автономный 2D-визуализатор телеметрии.

Проект разработан по методологии **Spec-Driven Design (SDD)**. Корневой [`docs/`](docs/) задает механику игры и API сервера; изолированный клиент ведет собственные спецификации в [`visualizer_2/docs/`](visualizer_2/docs/).

---

## Архитектура проекта

Проект организован по принципу монорепозитория со строгим разделением ответственности:

```text
datsmagic/
├── docs/             # [SSOT] Спецификации: mechanics.md, adr/, domain/, features/
├── server/           # Высокопроизводительный Rust-сервер симулятора (Axum + Tokio)
├── visualizer_2/     # Основной автономный Rust-визуализатор (egui/glow)
├── visualizer/       # Legacy Python-клиент (fallback)
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
   - **REST API ([FE-015](docs/features/FE-015-legacy-desert-api-compatibility.md))**: Канонический Desert API `POST /play/magcarp/player/move`, авторизация через `X-Auth-Token`.

2. **Rust-визуализатор (`visualizer_2/`)**:
   - Нативное окно и scene painter на `eframe/egui`, отдельный REST-поток, интерактивное наблюдение, ручное управление, HUD, прогноз маршрутов и регулируемый веер. Спецификация: [`visualizer_2/docs/`](visualizer_2/docs/).

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

### 2. Запуск визуализатора на Rust
В отдельном окне терминала выполните:
```bash
cargo run --manifest-path visualizer_2/Cargo.toml -- --url http://127.0.0.1:8080 --token dev-token
```

`--token` обязателен. Для более частого/редкого запроса можно задать `--poll-ms 200`.

Параметры запуска:
```bash
cargo run --manifest-path visualizer_2/Cargo.toml -- --url http://127.0.0.1:8080 --token dev-token
```

#### Управление в визуализаторе:
- **Клик по ковру / `1`–`5`**: Выбрать ковер для наблюдения.
- **`M`**: Включить/выключить ручное управление мышью.
- **`P`**: Показать/скрыть веер прогнозных маршрутов.
- **`Пробел`**: Включить/выключить слежение камеры за выбранным ковром.
- **`+` / `-` / Колесо мыши**: Приблизить / отдалить масштаб сцены (Zoom).
- **Drag / стрелки**: Панорамирование сцены.
- **`R`**: Сброс камеры и масштаба.

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

### Проверка Rust-визуализатора
```bash
cargo check --manifest-path visualizer_2/Cargo.toml
```

Python fallback по-прежнему запускается через `./scripts/run_visualizer.sh`.

---

## Спецификации и документация (Docs-First)

В соответствии с правилами в [AGENTS.md](AGENTS.md):
- [docs/mechanics.md](docs/mechanics.md) — Базовая игровая механика и математические формулы.
- [docs/adr/](docs/adr/) — Архитектурные решения (ADR-001, ADR-002).
- [docs/domain/](docs/domain/) и [docs/features/](docs/features/) — игровые правила и серверные фичи.
- [visualizer_2/docs/](visualizer_2/docs/) — домен, архитектура и фичи Rust-визуализатора.
