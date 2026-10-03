# Node.js-клиент `nodejs_bytadaniel`

TypeScript-игрок подключен как Git submodule из `https://github.com/bytadaniel/datsmagic.git`. Его локальные изменения API-запуска находятся в submodule; алгоритм поведения не менялся.

- [`domain/DR-001-node-player-runtime.md`](domain/DR-001-node-player-runtime.md): режим запуска и подключение к арене.
- [`features/FE-001-node-player-api-runtime.md`](features/FE-001-node-player-api-runtime.md): env-конфигурация, игровой HTTP-контракт и запуск.

Из корня проекта: `DATS_PLAYER_TOKEN=... DATS_GAME_API_URL=http://stadmagic.strangled.net/play/magcarp/player/move ./scripts/run_nodejs_bytadaniel.sh`. Скрипт установит зависимости через `npm ci`, если они еще не установлены, и запустит сетевой actioner.
