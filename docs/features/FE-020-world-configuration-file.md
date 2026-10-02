---
id: FE-020
title: "Конфигурация мира из world.json"
module: "server::config, server::spatial"
author: "Codex"
created_at: "2026-10-02"
updated_at: "2026-10-02"
status: "approved"
version: 1.0
tags: [configuration, world, spawn]
related_domain_records: [DR-011]
related_test_cases: [TC-WORLD-CONFIG-01]
---

# FE-020: Конфигурация мира из `world.json`

## 1. Контекст и цель

Заменить разбросанные в `main.rs`, `ServerConfig` и настройках спавнеров параметры мира единым файлом в корне проекта.

## 2. Архитектурное решение

`ServerConfig::load(path)` десериализует JSON поверх defaults, валидирует значения и передаёт конфигурацию физике, API, anomaly spawner и coin spawner. Путь по умолчанию — `world.json`; `DATS_WORLD_CONFIG` позволяет выбрать другой файл. `HOST`/`PORT` остаются точечными override.

Конфиг включает tick rate, размер арены, ускорение/скорость/трение/радиус ковра, respawn и штраф смерти; квоту и генеративные параметры bounty; квоту и минимумы/максимумы скорости, ядра, радиуса воздействия и силы аномалий, включая вероятность и диапазон выбросов.

## 3. Пример файла

```json
{
  "tick_rate_ms": 200,
  "arena_width": 9000,
  "arena_height": 9000,
  "max_acceleration": 40,
  "max_velocity": 110,
  "friction": 0.98,
  "transport_radius": 5,
  "enable_respawn": true,
  "death_penalty_score": 50,
  "carpet_death_loss_percent": 30,
  "bounty_quota": 1000,
  "bounty_base_value": 25,
  "bounty_max_value": 1000,
  "bounty_spawn_margin": 40,
  "anomaly_quota": 50,
  "anomaly_speed_min": 30,
  "anomaly_speed_max": 160,
  "anomaly_core_radius_min": 20,
  "anomaly_core_radius_max": 30,
  "anomaly_effect_radius_min": 300,
  "anomaly_effect_radius_max": 2000,
  "anomaly_force_min": 4,
  "anomaly_force_max": 22,
  "anomaly_force_outlier_probability": 0.1,
  "anomaly_force_outlier_min": 55,
  "anomaly_force_outlier_max": 100
}
```

## 4. Проверки

- Unit: defaults/частичный JSON/полный JSON загружаются предсказуемо.
- Unit: невалидные диапазоны и физические лимиты возвращают ошибку.
- Integration: заданные квоты и параметры попадают в оба генератора.

## История изменений

| Версия | Дата | Автор | Изменение |
|---|---|---|---|
| 1.0 | 2026-10-02 | Codex | Начальная спецификация централизованной настройки мира. |
