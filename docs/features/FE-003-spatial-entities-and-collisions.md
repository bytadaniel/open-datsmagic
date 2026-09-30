---
id: FE-003
title: "Подсистема управления пространственными сущностями и детекции коллизий"
module: "server::spatial"
author: "AI Agent & Game Logic Engineer"
created_at: "2026-09-30"
updated_at: "2026-09-30"
status: "approved"
version: 1.0
tags:
  - rust
  - collisions
  - entities
  - treasures
  - anomalies
related_domain_records:
  - DR-003
related_test_cases:
  - TC-COL-01
  - TC-COL-02
  - TC-COL-03
---

# FE-003: Подсистема управления пространственными сущностями и детекции коллизий

> Описание технической реализации менеджера пространственных объектов на карте, сбора сокровищ, гравитационного влияния песчаных вихрей и детекции столкновений на Rust.

---

## 1. Контекст и бизнес-цель

Фича реализует учет объектов игрового мира DatsMagic и их взаимных коллизий на 2D-карте. Модуль должен эффективно определять факт достижения сокровищ игроками, рассчитывать гравитационные силы вихрей-аномалий и контролировать статус оглушения ковров согласно спецификации [`docs/mechanics.md`](file:///Users/d.byta/Documents/Code/dats/datsmagic/docs/mechanics.md) и требованиям [`DR-003`](file:///Users/d.byta/Documents/Code/dats/datsmagic/docs/domain/DR-003-entity-interactions-and-collisions.md).

---

## 2. Архитектурное решение

Модуль инкапсулирует коллекции сущностей и реализует трейт `SpatialCollisionManager`. Для оптимизации вычислений все проверки расстояния до объектов производятся через сравнение квадратов расстояний ($d^2 \le R^2$).

```mermaid
classDiagram
    class SpatialCollisionManager {
        <<trait>>
        +compute_environmental_forces(player_pos) Vec2
        +resolve_treasure_captures(player_pos, capture_radius) u32
        +check_and_update_stuns(player_pos) bool
    }
    class EntityManager {
        -Vec~Treasure~ treasures
        -Vec~Anomaly~ anomalies
        -Vec~EnemyPlayer~ enemies
    }
    SpatialCollisionManager <|.. EntityManager
```

---

## 3. Модели и структуры данных (Rust)

```rust
use crate::physics::Vec2;

/// Сокровище на карте
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Treasure {
    pub id: String,
    pub r#type: String,
    pub position: Vec2,
    pub value: u32,
    pub is_collected: bool,
}

/// Аномалия / Песчаный вихрь
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Anomaly {
    pub id: String,
    pub position: Vec2,
    pub radius: f64,
    pub force: f64,
}

/// Состояние игрока
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct PlayerEntity {
    pub id: String,
    pub score: u32,
    pub status: String, // "normal" | "stunned"
    pub position: Vec2,
    pub velocity: Vec2,
    pub max_acceleration: f64,
    pub max_velocity: f64,
    pub stun_remaining_ticks: u32,
}
```

---

## 4. Алгоритмы взаимодействия

### 4.1. Расчет суммарной силы вихрей
Для каждой аномалии $i$:
1. Вектор разности: $\Delta \vec{P} = \vec{P}_{anomaly} - \vec{P}_{player}$.
2. Квадрат расстояния: $d^2 = \Delta \vec{P}.\text{length\_squared}()$.
3. Если $d^2 \le R_{anomaly}^2$:
   - Единичный вектор: $\vec{u} = \Delta \vec{P}.\text{normalize\_or\_zero}()$.
   - Сила вихря: $\vec{w}_i = \vec{u} \cdot F_{pull}$.
4. Результирующий вектор: $\vec{W} = \sum \vec{w}_i$.

### 4.2. Сбор сокровищ
1. Проверка условия: $(\vec{P}_{player} - \vec{P}_{treasure}).\text{length\_squared}() \le R_{capture}^2$.
2. При срабатывании:
   - `player.score += treasure.value`;
   - Сокровище удаляется из активного пула карты.

---

## 5. Обработка ошибок (Error Handling)

| Ситуация | Реакция системы |
| :--- | :--- |
| Позиция игрока строго совпадает с центром аномалии ($\Delta \vec{P} = \vec{0}$) | Метод нормализации возвращает `Vec2::ZERO`, исключая деление на ноль |
| Несколько сокровищ захвачены в один тик | Игроку суммируются очки за все захваченные сокровища |

---

## 6. План реализации

- [x] **Шаг 1:** Реализовать структуры `Treasure`, `Anomaly` и `PlayerEntity`.
- [x] **Шаг 2:** Создать интерфейс `SpatialCollisionManager` и реализацию `EntityManager`.
- [x] **Шаг 3:** Реализовать метод вычисления внешних сил $\vec{W} = \sum \vec{w}_i$.
- [x] **Шаг 4:** Реализовать логику сбора сокровищ и удаления их из карты.
- [x] **Шаг 5:** Покрыть unit-тестами точность расчета сил и коллизий.

---

## 7. Тестирование

### Unit-тесты
- `test_anomaly_pull_direction`: вектор силы направлен строго к центру вихря.
- `test_anomaly_zero_force_outside_radius`: вне зоны $R_{anomaly}$ сила равна $(0, 0)$.
- `test_treasure_collected_within_radius`: при $d \le R_{capture}$ сокровище собирается и начисляются очки.
- `test_treasure_not_collected_outside_radius`: при $d > R_{capture}$ счет не меняется.

---

## История изменений

| Версия | Дата | Автор | Изменение |
| :--- | :--- | :--- | :--- |
| 1.0 | 2026-09-30 | AI Agent | Создание спецификации пространственных сущностей и коллизий |
| 1.1 | 2026-09-30 | AI Agent | Реализация EntityManager, WorldSpatialEngine, коллизий и интеграции с физикой |
