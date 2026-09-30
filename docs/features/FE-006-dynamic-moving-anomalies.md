---
id: FE-006
title: "Подсистема динамических аномалий, траекторий и разрушения ковров"
module: "server::spatial"
author: "Lead Architect & AI Agent"
created_at: "2026-09-30"
updated_at: "2026-09-30"
status: "approved"
version: 1.0
tags:
  - rust
  - spatial
  - anomalies
  - physics
  - spawner
  - collisions
related_domain_records:
  - DR-005
  - DR-002
  - DR-003
related_test_cases:
  - TC-ANOM-01
  - TC-ANOM-02
  - TC-ANOM-03
  - TC-ANOM-04
  - TC-ANOM-05
---

# FE-006: Подсистема динамических аномалий, траекторий и разрушения ковров (Dynamic Anomalies & Lethal Collisions)

> Техническая спецификация подсистемы симуляции подвижных вихрей, генератора сквозных траекторий, дуальных полей притяжения/отталкивания и механики мгновенного уничтожения ковров при соприкосновении с ядром на языке Rust.

---

## 1. Контекст и бизнес-цель

Фича реализует требования предметной области [`DR-005`](file:///Users/d.byta/Documents/Code/dats/datsmagic/docs/domain/DR-005-anomalies.md). В отличие от статичных вихрей, динамические аномалии автономно перемещаются через игровое пространство, спавнятся в буферной зоне за границей мира, оказывают радиальное воздействие (притяжение либо отталкивание) на управляемые ковры и вызывают перманентное уничтожение ковра при касании ядра ($d \le R_{core} + R_{carpet}$).

---

## 2. Архитектурное решение

Модуль расширяет подсистему `server::spatial` и интегрируется с `server::physics` и `server::engine`.

### Компоненты подсистемы:
1. **`DynamicAnomaly`**: Структура, хранящая состояние аномалии (идентификатор, тип, текущие координаты $\vec{P}$, скорость $\vec{V}$, радиус смертоносного ядра $R_{core}$, радиус области действия $R_{effect}$, силу $F$).
2. **`AnomalySpawner`**: Серверный генератор, контролирующий квоту `max_anomalies`. Выбирает точку рождения за пределами арены и вектор скорости $\vec{V}$, гарантирующий пересечение полосы движения с игровой ареной.
3. **`SpatialCollisionManager`**:
   - `advance_anomalies(delta_t)`: Шаг смещения координат аномалий $\vec{P}_{new} = \vec{P} + \vec{V} \cdot \Delta t$ и удаление аномалий, вышедших за терминальные границы деспавна.
   - `compute_environmental_forces(player_pos)`: Расчет суперпозиции сил $\vec{W}$ для всех аномалий, в зону влияния которых попал ковер (притяжение к ядру для `Attracting`, отталкивание от ядра для `Repelling`).
   - `check_lethal_core_collisions(player_pos, player_radius)`: Проверка контакта с ядрами ($d \le R_{core} + R_{player}$) и фиксация уничтожения ковра.

### Схема взаимодействия компонентов

```mermaid
sequenceDiagram
    participant Loop as GameEngineLoop
    participant Spatial as SpatialCollisionManager
    participant Spawner as AnomalySpawner
    participant Physics as PhysicsIntegrator

    Loop->>Spatial: advance_anomalies(dt)
    Spatial->>Spawner: replenish_if_needed(current_count, bounds)
    Spawner-->>Spatial: spawn new DynamicAnomaly outside arena
    Spatial->>Spatial: update positions & despawn out-of-bounds
    Loop->>Spatial: check_lethal_core_collisions(player_pos, radius)
    Spatial-->>Loop: player_destroyed: bool
    Loop->>Spatial: compute_environmental_forces(player_pos)
    Spatial-->>Loop: W_total (attract/repel superposition)
    Loop->>Physics: integrate_euler(pos, vel, accel, W_total, dt)
```

---

## 3. Модели и структуры данных (Rust)

```rust
use crate::physics::vector2d::Vector2D;

/// Тип силового воздействия аномалии на ковры.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnomalyType {
    /// Притягивает ковры по направлению к своему ядру.
    Attracting,
    /// Отталкивает ковры по направлению от своего ядра.
    Repelling,
}

/// Динамическая пространственная аномалия.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DynamicAnomaly {
    pub id: String,
    pub anomaly_type: AnomalyType,
    pub position: Vector2D,
    pub velocity: Vector2D,
    pub core_radius: f32,
    pub effect_radius: f32,
    pub force: f32,
}

impl DynamicAnomaly {
    pub fn new(
        id: String,
        anomaly_type: AnomalyType,
        position: Vector2D,
        velocity: Vector2D,
        core_radius: f32,
        effect_radius: f32,
        force: f32,
    ) -> Self {
        assert!(core_radius < effect_radius, "core_radius must be less than effect_radius");
        Self {
            id,
            anomaly_type,
            position,
            velocity,
            core_radius,
            effect_radius,
            force,
        }
    }

    /// Смещение аномалии за один тик симуляции.
    pub fn step(&mut self, dt: f32) {
        self.position = self.position + self.velocity * dt;
    }

    /// Проверка контакта ковра со смертоносным ядром.
    pub fn touches_core(&self, carpet_pos: Vector2D, carpet_radius: f32) -> bool {
        let total_r = self.core_radius + carpet_radius;
        self.position.distance_squared(carpet_pos) <= total_r * total_r
    }

    /// Расчет силы, оказываемой на ковер.
    pub fn calculate_force(&self, carpet_pos: Vector2D) -> Vector2D {
        let dist_sq = self.position.distance_squared(carpet_pos);
        let effect_sq = self.effect_radius * self.effect_radius;
        
        // Вне зоны действия сила равна нулю
        if dist_sq > effect_sq || dist_sq <= 1e-6 {
            return Vector2D::ZERO;
        }

        let dist = dist_sq.sqrt();
        let radial_dir = (self.position - carpet_pos).normalize_or_zero();

        match self.anomaly_type {
            AnomalyType::Attracting => radial_dir * self.force,
            AnomalyType::Repelling => -radial_dir * self.force,
        }
    }
}
```

---

## 4. Алгоритмы и вспомогательные функции

### 4.1 Генерация сквозной траектории аномалии (Crossing Arena Guarantee)

Для гарантии того, что зона влияния аномалии обязательно пересечет прямоугольную арену $[0, W] \times [0, H]$:
1. Выбирается стартовая грань арены (Top, Bottom, Left, Right) для спавна.
2. Начальная координата $\vec{P}_{spawn}$ размещается в буферной полосе снаружи выбранной грани на удалении $\delta \in [R_{effect}, R_{effect} + \delta_{buffer}]$.
3. Целевая точка $\vec{P}_{target}$ выбирается внутри площади арены либо на противоположной грани.
4. Вектор направления задается как $\vec{u} = \frac{\vec{P}_{target} - \vec{P}_{spawn}}{\|\vec{P}_{target} - \vec{P}_{spawn}\|}$.
5. Вектор скорости равен $\vec{V} = \vec{u} \cdot \text{speed}$, где $\text{speed} \in [V_{min}, V_{max}]$.

### 4.2 Условие деспавна аномалии

Аномалия считается завершившей свой жизненный цикл, если:
1. Расстояние от центра $\vec{P}_{anomaly}$ до ближайшей точки прямоугольника арены превышает $R_{effect}$.
2. Вектор скорости направлен в сторону увеличения расстояния от арены (скалярное произведение с нормалью к грани $\vec{V} \cdot \vec{n}_{out} > 0$).
3. При выполнении условий объект удаляется из пула аномалий.

### 4.3 Механика гибели ковра

Если `touches_core(carpet_pos, carpet_radius)` возвращает `true`:
- Статус игрока переводится в `PlayerStatus::Destroyed`.
- Игрок исключается из дальнейшего расчета перемещений (скорость обнуляется, ускорение блокируется).
- В API возвращается `status: "destroyed"`.

---

## 5. Обработка ошибок и краевых случаев

| Исключительная ситуация | Реакция системы |
|---|---|
| Нулевая дистанция до центра ($d = 0$) | Мгновенно регистрируется контакт с ядром ($d \le R_{core}$); деление на ноль в расчете сил предотвращается через `normalize_or_zero` |
| Попытка спавна при полной квоте $N \ge N_{max}$ | Спавнер пропускает генерацию до освобождения слотов |
| Застревание или ошибка генерации $\vec{V} = (0,0)$ | Проверка валидности вектора скорости при спавне; нулевая скорость отбраковывается |

---

## 6. План реализации

- [x] **Шаг 1:** Объявить структуры `AnomalyType` и `DynamicAnomaly` в `server/src/spatial/entities.rs`.
- [x] **Шаг 2:** Реализовать методы расчета сил притяжения и отталкивания, а также детекции контакта с ядром `touches_core`.
- [x] **Шаг 3:** Разработать модуль спавнера `AnomalySpawner` в `server/src/spatial/spawner.rs` с алгоритмом генерации траекторий, пересекающих арену.
- [x] **Шаг 4:** Интегрировать динамические аномалии и спавнер в `SpatialCollisionManager` в `server/src/spatial/manager.rs`.
- [x] **Шаг 5:** Добавить статус игрока `Destroyed` в `PlayerState` и обработку гибели ковра в тактовом цикле `GameEngineLoop`.
- [x] **Шаг 6:** Написать unit- и интеграционные тесты для проверки траекторий, спавна, отталкивания/притяжения и уничтожения ковра при контакте с ядром.

---

## 7. Тестирование

### Unit-тесты
- `test_attracting_anomaly_force_direction`: проверка направленности силы к ядру.
- `test_repelling_anomaly_force_direction`: проверка направленности силы от ядра.
- `test_core_collision_triggers_destruction`: проверка, что при $d \le R_{core} + R_{carpet}$ фиксируется факт гибели.
- `test_anomaly_ghosting_no_interference`: проверка движения двух аномалий сквозь друг друга без коллизий.
- `test_spawner_guaranteed_arena_intersection`: проверка пересечения отрезка траектории с ареной.

### Интеграционные тесты (`tests/spatial_tests.rs`)
- `test_dynamic_anomaly_lifecycle_spawn_cross_despawn`: полный сквозной цикл от спавна за ареной до деспавна после выхода.
- `test_player_elimination_in_game_loop`: подтверждение прекращения симуляции ковра при попадании в ядро вихря.

---

## История изменений

| Версия | Дата | Автор | Изменение |
|---|---|---|---|
| 1.0 | 2026-09-30 | Lead Architect & AI Agent | Начальная спецификация фичи динамических аномалий и уничтожения ковров |
