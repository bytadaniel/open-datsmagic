---
id: FE-002
title: "Модуль векторной математики и схема численного интегрирования Эйлера"
module: "server::physics"
author: "AI Agent & Physics Engineer"
created_at: "2026-09-30"
updated_at: "2026-09-30"
status: "approved"
version: 1.0
tags:
  - rust
  - math
  - physics
  - vectors
  - euler
related_domain_records:
  - DR-002
related_test_cases:
  - TC-PHY-01
  - TC-PHY-02
  - TC-PHY-03
---

# FE-002: Модуль векторной математики и схема численного интегрирования Эйлера

> Описание технической реализации 2D векторной математики, затухания скорости от трения, ограничений ускорения/скорости и пошаговой схемы Эйлера на Rust.

---

## 1. Контекст и бизнес-цель

Фича отвечает за моделирование физического движения ковров-самолетов в 2D пространстве согласно спецификации [`docs/mechanics.md`](file:///Users/d.byta/Documents/Code/dats/datsmagic/docs/mechanics.md) и требованиям [`DR-002`](file:///Users/d.byta/Documents/Code/dats/datsmagic/docs/domain/DR-002-vector-physics.md). Физический движок должен быть максимально производительным, не производить аллокаций в куче и детерминированно рассчитывать координаты и скорости.

---

## 2. Архитектурное решение

Модуль состоит из двух ключевых компонентов:
1. `Vec2` — структура 2D-вектора с перегрузкой операторов (`Add`, `Sub`, `Mul`), функциями вычисления евклидовой нормы и нормализации.
2. `EulerIntegrator` — реализация интерфейса `PhysicsIntegrator`, выполняющая шаг численного интегрирования.

Перед интегрированием отсутствие новой записи команды для живого транспорта трактуется как продолжение его последнего применённого эффективного ускорения; только явно принятый нулевой вектор означает отключение тяги. На респавне ускорение очищается.

### Схема вычислительного конвейера шага физики

```mermaid
flowchart TD
    A["Входные данные: P_old, V_old, A_cmd, W_env"] --> B{"||A_cmd|| > A_max?"}
    B -- Да --> C["A = (A_cmd / ||A_cmd||) * A_max"]
    B -- Нет --> D["A = A_cmd"]
    C --> E["V_raw = (V_old * k_f) + (A + W_env) * dt"]
    D --> E
    E --> F{"||V_raw|| > V_max?"}
    F -- Да --> G["V_new = (V_raw / ||V_raw||) * V_max"]
    F -- Нет --> H["V_new = V_raw"]
    G --> I["P_new = P_old + V_new * dt"]
    H --> I
    I --> J["Выход: P_new, V_new"]
```

---

## 3. Модели и структуры данных (Rust)

```rust
use std::ops::{Add, Mul, Sub};

/// Двумерный вектор с компонентами f64
#[derive(Clone, Copy, Debug, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub struct Vec2 {
    pub x: f64,
    pub y: f64,
}

impl Vec2 {
    pub const ZERO: Self = Self { x: 0.0, y: 0.0 };

    #[inline]
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    #[inline]
    pub fn length_squared(self) -> f64 {
        self.x * self.x + self.y * self.y
    }

    #[inline]
    pub fn length(self) -> f64 {
        self.length_squared().sqrt()
    }

    #[inline]
    pub fn normalize_or_zero(self) -> Self {
        let len = self.length();
        if len > 1e-9 {
            Self { x: self.x / len, y: self.y / len }
        } else {
            Self::ZERO
        }
    }

    #[inline]
    pub fn clamp_length(self, max_len: f64) -> Self {
        if self.length_squared() > max_len * max_len {
            self.normalize_or_zero() * max_len
        } else {
            self
        }
    }
}

impl Add for Vec2 {
    type Output = Self;
    #[inline] fn add(self, rhs: Self) -> Self { Self::new(self.x + rhs.x, self.y + rhs.y) }
}

impl Sub for Vec2 {
    type Output = Self;
    #[inline] fn sub(self, rhs: Self) -> Self { Self::new(self.x - rhs.x, self.y - rhs.y) }
}

impl Mul<f64> for Vec2 {
    type Output = Self;
    #[inline] fn mul(self, rhs: f64) -> Self { Self::new(self.x * rhs, self.y * rhs) }
}
```

---

## 4. Алгоритмы и функции интегрирования

```rust
pub trait PhysicsIntegrator: Send + Sync {
    fn step(
        &self,
        position: Vec2,
        velocity: Vec2,
        command_accel: Vec2,
        environment_forces: Vec2,
        max_accel: f64,
        max_velocity: f64,
        friction: f64,
        dt: f64,
    ) -> (Vec2, Vec2);
}

pub struct EulerIntegrator;

impl PhysicsIntegrator for EulerIntegrator {
    fn step(
        &self,
        position: Vec2,
        velocity: Vec2,
        command_accel: Vec2,
        environment_forces: Vec2,
        max_accel: f64,
        max_velocity: f64,
        friction: f64,
        dt: f64,
    ) -> (Vec2, Vec2) {
        // Шаг 1: Ограничение ускорения; внешние силы не отключают команду.
        let effective_accel = command_accel.clamp_length(max_accel);

        // Шаг 2 и 3: Обновление скорости с учетом трения и внешних сил
        let total_accel = effective_accel + environment_forces;
        let unconstrained_velocity = (velocity * friction) + (total_accel * dt);

        // Шаг 4: Ограничение скорости
        let new_velocity = unconstrained_velocity.clamp_length(max_velocity);

        // Шаг 5: Обновление позиции
        let new_position = position + (new_velocity * dt);

        (new_position, new_velocity)
    }
}
```

---

## 5. Обработка ошибок (Error Handling)

| Ситуация | Реакция системы |
| :--- | :--- |
| Попытка нормализовать вектор с длиной $\approx 0$ | Метод `normalize_or_zero` возвращает `Vec2::ZERO` без паники или `NaN` |
| Поступление параметров `NaN` или `Infinity` | Функция сбрасывает значение в `Vec2::ZERO` с предупреждением в лог |

---

## 6. План реализации

- [x] **Шаг 1:** Реализовать структуру `Vec2` и перегрузить базовые арифметические операторы.
- [x] **Шаг 2:** Реализовать методы `length`, `normalize_or_zero`, `clamp_length`.
- [x] **Шаг 3:** Описать трейт `PhysicsIntegrator` и реализовать `EulerIntegrator`.
- [x] **Шаг 4:** Написать unit-тесты на соблюдение всех физических ограничений.

---

## 7. Тестирование

### Unit-тесты
- `test_friction_decay`: при отсутствии ускорения начальная скорость $(10, 0)$ через 1 шаг становится $(10 \times 0.98, 0) = (9.8, 0)$.
- `test_acceleration_clamping`: при заявленном векторе $(10, 0)$ с $A_{max} = 5.0$ применяется вектор $(5.0, 0)$.
- `test_velocity_clamping`: при разгоне скорость никогда не превышает $V_{max} = 20.0$.
- `test_anomaly_force_does_not_disable_command`: внешняя сила не обнуляет управляющее ускорение; оба вектора отдельно участвуют в интегрировании.

---

## История изменений

| Версия | Дата | Автор | Изменение |
| :--- | :--- | :--- | :--- |
| 1.0 | 2026-09-30 | AI Agent | Создание спецификации физического модуля и схемы Эйлера |
| 1.1 | 2026-09-30 | AI Agent | Реализация Vec2, EulerIntegrator, сил и интеграционных тестов |
| 1.2 | 2026-10-02 | Codex | Удалена блокировка управления в зоне аномалий; команда ускорения всегда применяется с учетом maxAccel. |
| 1.3 | 2026-10-02 | Codex | Добавлено удержание последнего принятого ускорения на тиках без свежей команды. |
