# Документация компонентов

Спецификации всех продуктов хранятся рядом в общем `docs/`, но разнесены по папкам компонентов. Эти документы дополняют, но не переопределяют общие правила мира и API.

- [`arena-hub/`](arena-hub/) — API control plane и правила миров.
- [`arena-visualizer/`](arena-visualizer/README.md) — архитектура и пользовательские сценарии Rust/egui визуализатора.
- [`player-2/`](player-2/domain/DR-001-autonomous-trajectory-player.md) — отдельные домен и алгоритмы поведения автономного Rust-игрока.
- `lib/bot-variants/player_1/` — архивная TypeScript-реализация; её исходные `readme.md` и игровые данные сохранены рядом с кодом.

Игровая физика и серверный сетевой контракт для всех клиентов определяются общими [`mechanics.md`](../mechanics.md), [`domain/`](../domain/) и [`features/`](../features/).
