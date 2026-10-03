# StadMagic

StadMagic — неофициальная самостоятельная игра, вдохновлённая [DatsMagic от Dats.Team](https://gamethon.datsteam.dev/datsmagic). Оригинальные игровые серверы закрыты; StadMagic создан, чтобы дать людям возможность ещё немного поиграть в похожий мир. Проект не связан с Dats.Team, не заявляет никаких прав или претензий и будет закрыт вместе с доступом по просьбе Dats.Team.

Симуляция включает ковры-самолёты, золото и аномалии. Сервер и визуализатор написаны на Rust; Hub управляет аренами, командами, голосованием и лидербордом. Правила проекта ведутся по Docs-First / Spec-Driven Design: исходные спецификации находятся в [`docs/`](docs/).

## Структура

```text
apps/arena-hub/                 Python control plane и веб-приложение
lib/arena-server/               Rust-симулятор и API одной арены
lib/arena-visualizer/            Нативный Rust/egui визуализатор
lib/bot-variants/player_1/       Старая реализация игрока (TypeScript)
lib/bot-variants/player_2/       Автономный игрок со стратегиями (Rust)
assets/worlds.json               Каталог профилей миров; число профилей свободное
docs/                            Единый каталог ADR, домена, фич и документов компонентов
scripts/                         Поддерживаемые команды запуска
```

Сервер, визуализатор и каждый бот — независимые Cargo-пакеты со своими lock-файлами. Hub — самостоятельное Python-приложение. Графические зависимости визуализатора не попадают в бинарник арены.

## Быстрый старт

Нужны Rust toolchain и Python 3.10+. Из корня репозитория:

```bash
# Одна арена, без управления Hub
./scripts/run_server.sh

# Hub: сайт на :8090 и одна управляемая арена на :8080
./scripts/run_hub.sh

# Нативная визуализация (токен обязателен)
./scripts/run_visualizer.sh --url http://127.0.0.1:8080 --token player_2
```

Откройте Hub на [http://127.0.0.1:8090](http://127.0.0.1:8090). Там доступны регистрация команды, каталог миров, голосование за следующую арену, документация и лидерборд. Голосовать можно зарегистрированным токеном; команда может изменить голос до выбора мира. Арена меняется после 20-минутного запуска с паузой до минутной границы. Для локальной отладки выбор можно закрепить через `HUB_FIXED_WORLD_ID`.

`assets/worlds.json` — источник профилей: там настраиваются размеры карты, флот, респавн, аномалии и квота золота. Hub и сервер читают один каталог; фиксированного количества миров нет. Runtime-данные находятся в `apps/arena-hub/data/`, а локальные токены — в файлах `token.txt`; эти данные не следует публиковать.

## Игроки

Варианты игроков хранятся отдельно от симуляции:

```bash
# Игрок 2: установите/передайте токен, затем выберите стратегии переменными окружения
cd lib/bot-variants/player_2
DATS_PLAYER_TOKEN='your-token' DATS_PLAYER_STRATEGY=agile-top1 DATS_MOVEMENT_STRATEGY=survival cargo run --release
```

Не коммитьте токены. Player 1 содержит прежнюю клиентскую реализацию и её собственный `token.txt`; Player 2 — отдельный Rust-пакет и отдельная логика поведения.

## Визуализатор

Выбор ковра и ручное управление разделены. Визуализатор получает игровое состояние с арены и лидерборд/миры через Hub:

```bash
./scripts/run_visualizer.sh --url http://127.0.0.1:8080 --hub-url http://127.0.0.1:8090 --token player_2
```

Справка по клавишам и настройкам: [`docs/components/arena-visualizer/README.md`](docs/components/arena-visualizer/README.md).

## Проверки

```bash
cargo test --manifest-path lib/arena-server/Cargo.toml
cargo test --manifest-path lib/arena-visualizer/Cargo.toml
cargo fmt --manifest-path lib/arena-server/Cargo.toml --check
cargo fmt --manifest-path lib/arena-visualizer/Cargo.toml --check
cargo clippy --manifest-path lib/arena-server/Cargo.toml --all-targets -- -D warnings
python3 -m unittest discover -s apps/arena-hub -v
```

Player 2 проверяется независимо:

```bash
cargo test --manifest-path lib/bot-variants/player_2/Cargo.toml
```

## Документация

- [`docs/mechanics.md`](docs/mechanics.md) — игровая механика и API.
- [`docs/domain/`](docs/domain/) и [`docs/features/`](docs/features/) — доменные правила и технические спецификации сервера/платформы.
- [`docs/adr/`](docs/adr/) — архитектурные решения, включая текущую [структуру репозитория](docs/adr/ADR-003-repository-layout.md).
- [`docs/components/`](docs/components/) — спецификации Hub, визуализатора и автономного игрока.
- [`AGENTS.md`](AGENTS.md) — обязательный процесс изменения проекта.
