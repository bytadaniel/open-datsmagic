---
id: FE-003
title: "Отдельный репозиторий и запуск Rust-игрока"
module: "lib/arena-bots/rust_bytadaniel"
author: "Codex"
created_at: "2026-10-03"
updated_at: "2026-10-03"
status: "approved"
version: 1.0
tags:
  - rust
  - submodule
related_domain_records:
  - DR-001
related_test_cases:
  - TC-PLAYER-2-01
---

# FE-003: Отдельный репозиторий и запуск Rust-игрока

## 1. Контекст и бизнес-цель

Код автономного Rust-игрока развивается отдельно от основного StadMagic-репозитория. Каталог называется `lib/arena-bots/rust_bytadaniel` и подключается как Git submodule. Для локального запуска предоставляется единый launcher.

## 2. Архитектурное решение

- Репозиторий игрока подключается по адресу `https://github.com/bytadaniel/open-datsmagic-bot.git`.
- Основной репозиторий фиксирует конкретный commit submodule; изменения поведения и их история принадлежат репозиторию игрока.
- `token.txt`, `target/` и временные файлы игнорируются внутри репозитория игрока. Токены не должны попадать в коммиты.
- Cargo package остается самостоятельным и не добавляется в workspace сервера.
- `scripts/run_rust_bytadaniel.sh` проверяет наличие `DATS_PLAYER_TOKEN`, задает локальный default `DATS_SERVER_URL` и запускает release-сборку из каталога бота.
- Необязательные `DATS_PLAYER_STRATEGY`, `DATS_MOVEMENT_STRATEGY`, `DATS_DOOM_POLICY` и прочие настройки стратегии передаются процессу без изменений.
- Общий `.env.example` показывает необходимые переменные для всех внешних bot launchers; секрет команды в нём оставляется пустым.

## 3. Схема базы данных

Изменений базы данных нет.

## 4. Алгоритмы и вспомогательные функции

Поведение, стратегия, планировщик и сетевой контракт описаны в [DR-001](../domain/DR-001-autonomous-trajectory-player.md), [FE-001](FE-001-rust-trajectory-player.md) и [FE-002](FE-002-switchable-planning-strategies.md); упаковка в submodule на них не влияет.

## 5. API-контракты и DTO

Новых API нет. Используется существующий игровой API, описанный в [`mechanics.md`](../../../mechanics.md).

## 6. Обработка ошибок

Если удаленный репозиторий пуст или commit submodule не опубликован, его нельзя корректно клонировать при checkout основного проекта. Перед регистрацией submodule должен существовать доступный commit.

## 7. План реализации

- [x] Подготовить самостоятельный Git-репозиторий игрока без токенов и Cargo build artifacts.
- [x] Опубликовать исходный commit в удаленном репозитории.
- [x] Зарегистрировать `lib/arena-bots/rust_bytadaniel` как submodule основного репозитория.
- [x] Проверить инициализацию submodule и успешный Cargo test.
- [x] Добавить launcher `scripts/run_rust_bytadaniel.sh`.

## 8. Тестирование

### Unit-тесты

- Выполнить `cargo test --manifest-path lib/arena-bots/rust_bytadaniel/Cargo.toml`.
- Проверить, что `token.txt` не отслеживается Git.
- Проверить shell-синтаксис launcher через `bash -n scripts/run_rust_bytadaniel.sh`.

### E2E-тесты

- Запустить player через `DATS_PLAYER_TOKEN=... DATS_SERVER_URL=http://127.0.0.1:8080 ./scripts/run_rust_bytadaniel.sh`.

## История изменений

| Версия | Дата | Автор | Изменение |
|---|---|---|---|
| 1.0 | 2026-10-03 | Codex | Спецификация упаковки player_2 как отдельного submodule. |
| 1.1 | 2026-10-03 | Codex | Зафиксирован перенос в `lib/arena-bots` и единый launcher запуска. |
