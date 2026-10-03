---
id: FE-001
title: "Нативное приложение и realtime-сетевой worker"
module: "lib/arena-visualizer::app, lib/arena-visualizer::network"
author: "Codex"
created_at: "2026-10-02"
updated_at: "2026-10-02"
status: "approved"
version: 1.2
tags: [rust, eframe, network, websocket, api]
related_domain_records: [DR-001]
related_test_cases: [TC-V2-API-01, TC-V2-API-02]
---

# FE-001: Нативное приложение и Desert API worker

## 1. Контекст и цель

Новый клиент должен рисовать кадры независимо от сети, получать состояние через realtime-канал Hub/Arena и не расходовать игровой REST-лимит бота пустыми запросами.

## 2. Архитектура

- `eframe::App` владеет окном, камерой, snapshot буфером, UI состоянием и scene painter.
- Один Rust worker выполняет сетевой обмен вне UI event loop. Он запрашивает у Hub одноразовый ticket (`POST {hub_url}/api/visualizer/ticket`, `X-Auth-Token`, `{}`), подключается к `websocketUrl` с подпротоколами `stadmagic.v1` и `stadmagic-ticket.<ticket>`, затем получает realtime snapshots. Ticket короткоживущий и одноразовый; игровой токен не передаётся Arena по WebSocket.
- Состояние приходит сообщениями `{ "type":"snapshot", "tick":N, "state":<Desert> }`; `state` десериализуется по схеме из `lib/arena-server/src/api/dto.rs`. В UI публикуется только последний snapshot.
- В ручном режиме тот же WebSocket отправляет `{ "type":"commands", "transports":[...] }` с ускорением выбранного транспорта. Пустые `/move` запросы не выполняются; REST API ботов и его rate limit остаются без изменений.
- `--url` используется для определения адреса Hub, если `--hub-url` не задан. За публичным reverse proxy с нестандартной маршрутизацией Hub URL следует указать явно.
- Совместимый CLI-параметр `--poll-ms` задаёт задержку presentation interpolation между snapshots, а не частоту сетевых запросов; reconnect использует отдельный экспоненциальный backoff.
- Игровой токен обязателен и читается только из `DATS_PLAYER_TOKEN`; передача токена в CLI запрещена, чтобы он не попадал в список процессов. При пустом значении приложение не стартует и не ищет token file. URL арены и Hub задаются через `STADMAGIC_ARENA_URL` и `STADMAGIC_HUB_URL` либо соответствующие CLI-параметры.
- В `lib/arena-visualizer/Cargo.toml` включены только зависимости нового executable. Серверный crate не зависит от графического stack.

## 3. DTO и соединение

Все координаты finite `f64`; отсутствующая optional telemetry имеет безопасное нулевое значение только если это допустимо контрактом. Ошибки соединения сохраняются отдельно от последнего валидного snapshot. При разрыве worker запрашивает новый ticket и переподключается с backoff; он не переключается на polling `/move`.

## 4. Ошибки

| Случай | Поведение |
|---|---|
| Ticket endpoint вернул 401/503 | Показать auth/arena status; не перетирать последний валидный snapshot, повторить с backoff. |
| WebSocket handshake/stream разорван | Показать reconnect status, запросить новый ticket после backoff; REST polling не включать. |
| Ошибка WebSocket команды | Показать ошибку, сохранить connected status/snapshot; повторять команду на следующем snapshot только пока ручной ввод включён. |
| Невалидный JSON | Зафиксировать ошибку разбора, продолжить процесс. |
| `DATS_PLAYER_TOKEN` пуст или отсутствует | Ошибка CLI до создания сетевого worker. |

## 5. План и проверки

- [ ] Создать отдельный crate и точку запуска с обязательным `DATS_PLAYER_TOKEN`.
- [ ] Реализовать выдачу ticket, WebSocket snapshots и realtime-команды в фоне.
- [ ] Проверить ticket auth/body, подпротоколы WebSocket, reconnect с новым ticket и разбор Desert fixture из `datsmagic/map.json`.
- [ ] Убедиться, что за пределами leaderboard и ticket/lease control plane визуализатор не вызывает игровой `/move` endpoint.
- [ ] Проверить, что window loop продолжает кадры при задержке ответа.

## История изменений

| Версия | Дата | Автор | Изменение |
|---|---|---|---|
| 1.0 | 2026-10-02 | Codex | Первоначальная спецификация сетевого клиента. |
| 1.1 | 2026-10-03 | Codex | Состояние и ручные команды перенесены с игрового REST polling на Hub ticket + Arena WebSocket; fallback на `/move` запрещён. |
| 1.2 | 2026-10-03 | Codex | Скрипт использует переменные окружения для адресов и обязательного токена; токен нельзя передавать аргументом процесса. |
