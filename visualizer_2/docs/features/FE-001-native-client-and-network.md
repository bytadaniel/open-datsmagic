---
id: FE-001
title: "Нативное приложение и Desert API worker"
module: "visualizer_2::app, visualizer_2::network"
author: "Codex"
created_at: "2026-10-02"
updated_at: "2026-10-02"
status: "approved"
version: 1.0
tags: [rust, eframe, network, api]
related_domain_records: [DR-001]
related_test_cases: [TC-V2-API-01, TC-V2-API-02]
---

# FE-001: Нативное приложение и Desert API worker

## 1. Контекст и цель

Новый клиент должен рисовать кадры независимо от частоты HTTP и использовать тот же внешний API, что сервер и `./datsmagic`.

## 2. Архитектура

- `eframe::App` владеет окном, камерой, snapshot буфером, UI состоянием и scene painter.
- Один Rust worker выполняет запросы вне UI event loop с таймаутом и регулируемым polling interval. Межпоточный обмен — bounded channel/latest-snapshot slot; устаревшие промежуточные состояния могут коалесцироваться.
- API: `POST {base_url}/play/magcarp/player/move`, `X-Auth-Token: <token>`, JSON `{ "transports": [] }`. В ручном режиме пакет содержит команды выбранного транспорта. В ответе десериализуется Desert schema из `server/src/api/dto.rs`; `mapSize`, `maxSpeed`, `maxAccel`, `transportRadius`, `transports`, `enemies`, `anomalies`, `bounties` используются как источники состояния/масштабов.
- `--token` обязателен; при пустом значении приложение не стартует и не ищет скрытый token file.
- В `visualizer_2/Cargo.toml` включены только зависимости нового executable. Серверный crate не зависит от графического stack.

## 3. DTO и polling

Все координаты finite `f64`; отсутствующая optional telemetry имеет безопасное нулевое значение только если это допустимо контрактом. HTTP status/error сохраняется отдельно от последнего валидного snapshot. Polling по умолчанию около 200 мс; ошибки используют backoff и видны в HUD.

## 4. Ошибки

| Случай | Поведение |
|---|---|
| 401/403 | Показать auth failure; не перетирать последний валидный snapshot. |
| 429/network timeout | Backoff, сохранить connected/error status. |
| Невалидный JSON | Зафиксировать ошибку разбора, продолжить процесс. |
| Пустой токен | Ошибка CLI до создания API worker. |

## 5. План и проверки

- [ ] Создать отдельный crate и точку запуска `--url/--token`.
- [ ] Реализовать типизированный Desert response и фоновый polling.
- [ ] Проверить header/body/path, ошибки HTTP и разбор полного fixture из `datsmagic/map.json`.
- [ ] Проверить, что window loop продолжает кадры при задержке ответа.

## История изменений

| Версия | Дата | Автор | Изменение |
|---|---|---|---|
| 1.0 | 2026-10-02 | Codex | Первоначальная спецификация сетевого клиента. |
