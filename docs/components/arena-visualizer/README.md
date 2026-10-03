# Rust visualizer documentation

Эта папка в общем `docs/` содержит спецификации Rust-клиента `lib/arena-visualizer`. Нумерация записей локальная и начинается с 1; она не продолжает серии сервера или `lib/arena-bots/rust_bytadaniel`.

- [`adr/ADR-001-rust-native-visualizer.md`](adr/ADR-001-rust-native-visualizer.md): стек и границы проекта.
- [`domain/DR-001-observation-and-visualization.md`](domain/DR-001-observation-and-visualization.md): пользовательские правила и сценарии наблюдения.
- [`features/FE-001-native-client-and-network.md`](features/FE-001-native-client-and-network.md): окно, цикл кадров и realtime WebSocket-клиент.
- [`features/FE-002-world-rendering.md`](features/FE-002-world-rendering.md): камера, сущности, палитра и HUD.
- [`features/FE-003-interaction-and-interpolation.md`](features/FE-003-interaction-and-interpolation.md): интерполяция, камера и ручное управление.
- [`features/FE-004-trajectory-analysis.md`](features/FE-004-trajectory-analysis.md): прогноз, score-rate, веер кандидатов и настройка плотности.
- [`features/FE-007-world-information-window.md`](features/FE-007-world-information-window.md): ID и профиль активного мира.

Серверные физические и сетевые контракты остаются в общей документации, особенно [`mechanics.md`](../../mechanics.md), [`DR-009`](../../domain/DR-009-legacy-world-contract.md) и [`FE-026`](../../features/FE-026-web-visualizer-realtime-channel.md). Rust-визуализатор получает одноразовый ticket у Hub, затем читает snapshots и отправляет ручные команды по WebSocket; он не опрашивает игровой `/move` и не расходует REST rate limit бота. Токен передаётся только в заголовке `X-Auth-Token` при выдаче ticket.

Запуск для публичного сервера: задайте `DATS_PLAYER_TOKEN` и выполните `./scripts/run_visualizer.sh`. Для локальной арены при необходимости задайте `STADMAGIC_ARENA_URL=http://127.0.0.1:8080/play/magcarp/player/move` и `STADMAGIC_HUB_URL=http://127.0.0.1:8090`. Токен принимается только через окружение, не через CLI.
