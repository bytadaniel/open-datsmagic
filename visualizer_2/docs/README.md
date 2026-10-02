# Rust visualizer documentation

Эта папка — локальный источник правды для Rust-клиента `visualizer_2`. Нумерация записей локальная и начинается с 1; она не продолжает серии сервера или `players/player_2`.

- [`adr/ADR-001-rust-native-visualizer.md`](adr/ADR-001-rust-native-visualizer.md): стек и границы проекта.
- [`domain/DR-001-observation-and-visualization.md`](domain/DR-001-observation-and-visualization.md): пользовательские правила и сценарии наблюдения.
- [`features/FE-001-native-client-and-network.md`](features/FE-001-native-client-and-network.md): окно, цикл кадров и REST-клиент.
- [`features/FE-002-world-rendering.md`](features/FE-002-world-rendering.md): камера, сущности, палитра и HUD.
- [`features/FE-003-interaction-and-interpolation.md`](features/FE-003-interaction-and-interpolation.md): интерполяция, камера и ручное управление.
- [`features/FE-004-trajectory-analysis.md`](features/FE-004-trajectory-analysis.md): прогноз, score-rate, веер кандидатов и настройка плотности.

Серверные физические и сетевые контракты остаются в корневом [`docs/`](../../docs/): особенно `mechanics.md`, `DR-009` и `FE-004`. Клиент не вводит альтернативный API и всегда отправляет `X-Auth-Token` в `POST /play/magcarp/player/move`.

Запуск: `cargo run --manifest-path visualizer_2/Cargo.toml -- --url http://127.0.0.1:8080 --token player_2`.
