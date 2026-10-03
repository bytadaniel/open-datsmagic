# Документация компонентов

Спецификации всех продуктов хранятся рядом в общем `docs/`, но разнесены по папкам компонентов. Эти документы дополняют, но не переопределяют общие правила мира и API.

- [`arena-hub/`](arena-hub/) — API control plane и правила миров.
- [`arena-visualizer/`](arena-visualizer/README.md) — архитектура и пользовательские сценарии Rust/egui визуализатора.
- [`java-sas4eka/`](java-sas4eka/README.md) — сохранённый Java-игрок, его offline fixture и конфигурация подключения к игровому API.
- [`nodejs-bytadaniel/`](nodejs-bytadaniel/README.md) — сохранённый TypeScript-игрок и настройки подключения к игровому API.
- [`player-2/`](player-2/domain/DR-001-autonomous-trajectory-player.md) — отдельные домен и алгоритмы поведения автономного Rust-игрока (`lib/arena-bots/rust_bytadaniel`, submodule).
- `lib/arena-bots/` — независимые Java, Node.js и Rust реализации клиентов.

Игровая физика и серверный сетевой контракт для всех клиентов определяются общими [`mechanics.md`](../mechanics.md), [`domain/`](../domain/) и [`features/`](../features/).
