# StadMagic

StadMagic — неофициальная самостоятельная игра, вдохновлённая [DatsMagic от Dats.Team](https://gamethon.datsteam.dev/datsmagic). Оригинальные игровые серверы закрыты; StadMagic создан, чтобы дать людям возможность ещё немного поиграть в похожий мир. Проект не связан с Dats.Team, не заявляет никаких прав или претензий и будет закрыт вместе с доступом по просьбе Dats.Team.

Это симуляция арены: команды подключают ботов по HTTP API, управляют коврами-самолётами, собирают золото и соревнуются в меняющихся мирах с аномалиями. Rust-сервер исполняет физику и игровой цикл. Веб-Hub регистрирует команды, раздаёт токены, выбирает миры и собирает лидерборд. Есть браузерная карта для зрителей и ручной игры, а также отдельный нативный Rust-визуализатор.

## Содержание

- [Структура проекта](#структура)
- [Быстрый запуск через Docker Compose](#быстрый-старт)
- [Что запустится и где смотреть](#что-запустится-и-где-смотреть)
- [Остановка и сохранение данных](#данные-и-публикация)
- [Локальный запуск без Docker](#локальный-запуск-без-docker)
- [Игроки](#игроки)
- [Визуализатор](#визуализатор)
- [Проверки](#проверки)
- [Документация](#документация)

## Структура

```text
apps/arena-hub/                  Веб-сайт, регистрация, API Hub, лидерборд и расписание арен (Python)
apps/arena-runtime/              Внутренний lifecycle API: запускает/останавливает Rust-симуляцию
lib/arena-server/                Игровая физика, мир и единственный API арены (Rust)
lib/arena-visualizer/             Нативный визуализатор с ручным управлением (Rust/egui)
lib/bot-variants/player_1/        Прежняя клиентская реализация (TypeScript)
lib/bot-variants/player_2/        Автономный бот со стратегиями (Rust)
assets/worlds.json                Профили миров; монтируется в контейнеры только для чтения
docs/                             Спецификации, правила мира, API и архитектурные решения
scripts/                          Локальные команды запуска
apps/arena-hub/Dockerfile         Образ веб-Hub
apps/arena-runtime/Dockerfile     Runtime-контейнер с управлением arena-процессом
lib/arena-server/Dockerfile       Самостоятельный образ игрового Rust API
docker-compose.yaml               Запуск Hub и runtime; arena-процесс стартует по команде Hub
```

Hub и `arena-runtime` — отдельные Compose-сервисы из одного образа. Контейнер runtime остаётся запущенным, а Rust-процесс мира стартует по внутренней команде Hub и завершается при смене арены. Логи Rust-процесса доступны в `docker compose logs -f arena-runtime`. Сервисы не имеют доступа к Docker socket.

Сервер, визуализатор и каждый бот — независимые Cargo-пакеты со своими lock-файлами. Графические зависимости визуализатора не попадают в бинарник арены.

## Быстрый старт

### Docker Compose (рекомендуется)

Требуется запущенный Docker Desktop или OrbStack с поддержкой `docker compose`. Все команды выполняй из корня репозитория.

1. Создай локальный файл настроек:

   ```bash
   cp .env.example .env
   ```

2. Открой `.env` и задай `ARENA_CONTROL_TOKEN` — длинный случайный секрет для внутренней связи Hub с arena-контейнером. Например, сгенерируй строку командой `openssl rand -hex 32` и вставь её после `ARENA_CONTROL_TOKEN=`. Для локальной игры оставь `ARENA_PUBLIC_URL=http://localhost:8080` и остальные значения по умолчанию. `.env` исключён из Git.

3. Собери образы и запусти приложение:

   ```bash
   docker compose up -d --build
   ```

При первом запуске Docker скачает базовые образы и соберёт Rust-сервер, поэтому сборка может занять несколько минут.

### Что запустится и где смотреть

- Веб-приложение: <http://localhost:8090>
- Игровой API для ботов: `http://localhost:8080/play/magcarp/player/move`
- Статус контейнеров: `docker compose ps`
- Логи Hub и Rust-арены: `docker compose logs -f hub arena-runtime`

Hub запускает одну арену и управляет её жизненным циклом через закрытый внутренний API runtime-контейнера. Этот управляющий порт не публикуется на хост. По умолчанию веб и игровой API доступны только локально на компьютере. `ARENA_CONTROL_TOKEN` ботам не нужен: токен команды выдаётся отдельно на странице регистрации в Hub.

### Данные и публикация

Остановить контейнеры, сохранив данные:

```bash
docker compose down
```

Чтобы запустить снова: `docker compose up -d`. Реестр команд, лидерборд и данные арены хранятся в именованном Docker volume `stadmagic-data` и переживают обычный `down`. Не используй `docker compose down -v`, если хочешь их сохранить. Миры настраиваются в `assets/worlds.json`; Compose подключает файл в контейнеры только для чтения.

Порты привязаны к `127.0.0.1`, поэтому сервисы доступны только на этой машине. Публикация на удалённый сервер в эту инструкцию не входит.

### Локальный запуск без Docker

Нужны Rust toolchain и Python 3.10+. Из корня репозитория:

```bash
# Одна арена, без управления Hub
./scripts/run_server.sh

# Hub: сайт на :8090 и одна управляемая арена на :8080
./scripts/run_hub.sh

# Нативная визуализация (токен обязателен)
./scripts/run_visualizer.sh --url http://127.0.0.1:8080 --token player_2
```

Открой Hub на <http://127.0.0.1:8090>. Там доступны регистрация команды, каталог миров, голосование, документация и лидерборд. Для локальной отладки выбор мира можно закрепить через `HUB_FIXED_WORLD_ID`.

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
