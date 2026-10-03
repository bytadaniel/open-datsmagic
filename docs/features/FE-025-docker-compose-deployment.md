---
id: FE-025
title: "Compose-развёртывание Hub и управляемой Rust-арены"
module: "apps/arena-hub, apps/arena-runtime, Docker Compose"
author: "Codex"
created_at: "2026-10-03"
updated_at: "2026-10-03"
status: "approved"
version: 1.0
tags: [deployment, docker, compose, arena-lifecycle]
related_domain_records: [DR-013]
related_test_cases: [TC-DOCKER-01]
---

# FE-025: Compose-развёртывание Hub и управляемой Rust-арены

## 1. Контекст и бизнес-цель

Дать оператору воспроизводимый способ запустить StadMagic через Docker Compose и сохранять состояние между перезапусками. Публичная публикация через reverse proxy не входит в эту поставку; Compose предназначен для локального запуска и привязывает порты к loopback.

## 2. Архитектурное решение

- `hub` и `arena-runtime` — постоянно работающие Compose-сервисы во внутренней сети, каждый собирается своим Dockerfile рядом с приложением. Самостоятельный образ игрового процесса собирается из `lib/arena-server/Dockerfile`.
- `arena-runtime` слушает отдельный закрытый lifecycle API и управляет дочерним Rust игровым процессом. Hub отправляет ему `world_id`, имя/ID запуска и observer credential; runtime сверяет профиль с `assets/worlds.json`, формирует runtime-конфигурацию процесса, запускает его, передаёт stdout/stderr в Docker logs и останавливает по запросу Hub. Hub получает нормализованный Rust-конфиг миров через закрытый endpoint runtime.
- Контейнер `arena-runtime` стабилен, а игровой процесс в нём стартует на игровой запуск и завершается после окна мира. Так Hub управляет миром без Docker socket и без динамического создания привилегированных контейнеров.
- Hub и runtime используют один версионируемый `assets/worlds.json` из образа и общий именованный volume `stadmagic-data` для registry, SQLite, world snapshot, метрик и лог-файла арены.
- Порты Hub и игрового API по умолчанию публикуются только на loopback хоста. `ARENA_PUBLIC_URL` задаёт origin, который Hub показывает ботам; при локальном запуске это `http://localhost:8080`.
- Секрет внутреннего API передаётся через локальный игнорируемый `.env`; токены команд хранятся только в persistent volume.

## 3. Схема базы данных

Изменений схемы Hub SQLite нет. Персистентные файлы Hub и арены находятся в Compose volume `stadmagic-data`.

## 4. Алгоритмы и вспомогательные функции

Lifecycle API принимает `start`, `status` и `stop` только с shared secret. Запуск принимает ID из каталога миров и идентификатор запуска, а не произвольный JSON-конфиг/путь/окружение. Одновременно допускается не более одного игрового процесса.

## 5. API-контракты и DTO

Публичный игровой API и API Hub не меняются. Между Hub и внутренним controller используется закрытый служебный API, который не публикуется на порты хоста.

## 6. Обработка ошибок

| Ситуация | Реакция |
|---|---|
| Runtime недоступен или lifecycle-запрос отклонён | Текущий запуск помечается failed; Hub продолжает цикл и сообщает ошибку в состоянии арены |
| Rust-процесс арены завершился раньше планового срока | Runtime возвращает код выхода; Hub завершает текущий запуск как failed |
| Порт арены занят | Rust-процесс не запускается, ошибка видна в состоянии Hub и `docker compose logs arena-runtime` |
| Runtime-контейнер остановлен | Hub не считает арену запущенной и показывает lifecycle-ошибку |

## 7. План реализации

- [x] Добавить компонентные Dockerfile для Hub, runtime и standalone arena server, а также Compose services.
- [x] Подключить Hub к runtime lifecycle API, сохранив локальный процессный режим для запуска без Docker.
- [x] Добавить persistent data volume, локальный runtime secret и loopback-only published ports.
- [x] Документировать локальный Compose-запуск, настройки `.env`, логи и сохранение данных.

## 8. Тестирование

### Unit-тесты

- Hub формирует корректные lifecycle-команды для controller и обрабатывает статусы/ошибки без Docker.
- Runtime отвергает запрос без секрета, неизвестный мир, некорректную команду или запуск второй арены поверх первой.
- Compose config проходит `docker compose config`.

### E2E-тесты

- `docker compose up --build -d` поднимает сайт и runtime-контейнер, но не запускает игровой процесс арены.
- Супервизор Hub запускает Rust arena process по lifecycle API; остановка/окончание завершает его, а runtime-контейнер продолжает работать.
- `docker compose up -d --build` поднимает Hub и runtime на loopback с корректной подстановкой параметров из корневого `.env`.

## История изменений

| Версия | Дата | Автор | Изменение |
|---|---|---|---|
| 1.1 | 2026-10-03 | Codex | Убран необязательный Nginx-конфиг; описан локальный запуск через Compose. |
