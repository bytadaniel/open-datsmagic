# Java-клиент `java_sas4eka`

Этот компонент сохраняет предоставленную Java-реализацию игрока и её собственный поиск действия. Общие правила физики и единственный игровой API остаются в [`mechanics.md`](../../mechanics.md) и [`DR-009`](../../domain/DR-009-legacy-world-contract.md).

- [`domain/DR-001-java-player-runtime.md`](domain/DR-001-java-player-runtime.md): поведение при запуске как офлайн-просмотрщика или игрового клиента.
- [`features/FE-001-java-player-api-runtime.md`](features/FE-001-java-player-api-runtime.md): переменные окружения, HTTP-контракт, сборка и обработка ошибок.

Сборка и запуск из корня проекта: `DATS_GAME_MODE=api DATS_PLAYER_TOKEN=... DATS_GAME_API_URL=https://stadmagic.strangled.net/play/magcarp/player/move ./scripts/run_java_sas4eka.sh`.
