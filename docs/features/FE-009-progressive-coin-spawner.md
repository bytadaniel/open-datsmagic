---
id: FE-009
title: "Динамический генератор монет с прогрессивным масштабированием номинала от игрового тика"
module: "Пространственные сущности и экономика (Spatial & Coin Economy)"
author: "AI Agent & Tech Lead"
created_at: "2026-09-30"
updated_at: "2026-09-30"
status: "approved"
version: 1.0
tags:
  - coins
  - treasure-spawner
  - time-scaling
  - economy
  - rust
related_domain_records:
  - DR-003
  - DR-008
related_test_cases:
  - TC-COIN-01
  - TC-COIN-02
  - TC-COIN-03
---

# FE-009: Динамический генератор монет с прогрессивным масштабированием номинала от игрового тика (Progressive Coin Spawner)

## 1. Контекст и бизнес-цель

В соответствии с [`DR-008`](file:///Users/d.byta/Documents/Code/dats/datsmagic/docs/domain/DR-008-treasure.md) на карте должна поддерживаться постоянная квота нематериальных монет ($N_{max\_coins} \approx 10..15$). При этом ценность монет не статична: чем больше времени (тиков) прошло с начала матча, тем выше возможный максимальный номинал появляющихся сокровищ.

Цель данной фичи — реализовать модуль `CoinSpawner`, обеспечивающий автоматическое восполнение монет в пределах арены со случайными координатами и расчет прогрессивного номинала $V(T)$ по формуле временного масштабирования.

---

## 2. Архитектурное решение

Модуль генератора монет интегрируется в `WorldSpatialEngine` аналогично `AnomalySpawner`:

```mermaid
flowchart TD
    Tick["Игровой тик T (Simulation Step)"] --> Resolve["WorldSpatialEngine::resolve(world, dt)"]
    Resolve --> CheckQuota{"Активных монет < N_max?"}
    CheckQuota -- Да --> Spawner["CoinSpawner::replenish(current_count, current_tick)"]
    CheckQuota -- Нет --> Captures["Проверка сбора коврами (dist <= R_capture)"]
    Spawner --> Formula["Расчет номинала V(T) = V_base + alpha*T^gamma + Rand"]
    Formula --> AddWorld["Добавление новых монет в world.treasures"]
    AddWorld --> Captures
    Captures --> Score["player.score += coin.value & удаление монеты"]
```

### 2.1 Математическая модель расчета номинала

Номинал монеты $V(T)$ на тике $T$:
$$V(T) = \min\left(V_{max\_cap}, \; V_{base} + \lfloor \alpha \cdot T^{\gamma} \rfloor + \text{Random}\left(0, \; V_{spread\_base} + \lfloor \beta \cdot T \rfloor\right)\right)$$

Где:
- $V_{base} = 25$
- $\alpha = 0.05, \gamma = 1.05$
- $V_{spread\_base} = 20, \beta = 0.1$
- $V_{max\_cap} = 1000$

### 2.2 Градация монет по ценности (Coin Tier)
- **Common (Бронза):** $V < 100$
- **Silver (Серебро):** $100 \le V < 250$
- **Gold (Золото):** $250 \le V < 500$
- **Legendary (Эпик/Рубин):** $V \ge 500$

---

## 3. Структуры данных (Rust)

```rust
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CoinSpawnerConfig {
    pub max_coins: usize,
    pub base_value: u32,
    pub arena_width: f64,
    pub arena_height: f64,
    pub margin: f64,
    pub max_value_cap: u32,
}

impl Default for CoinSpawnerConfig {
    fn default() -> Self {
        Self {
            max_coins: 10,
            base_value: 25,
            arena_width: 1000.0,
            arena_height: 1000.0,
            margin: 40.0,
            max_value_cap: 1000,
        }
    }
}

pub struct CoinSpawner {
    config: CoinSpawnerConfig,
    counter: u64,
}
```

---

## 4. План реализации

1. **Создание модуля `server/src/spatial/coin_spawner.rs`**:
   - Реализация структуры `CoinSpawner` с методами `new`, `spawn_one(tick)` и `replenish_if_needed(current_len, tick)`.
   - Генерация случайных координат в диапазоне $[margin, W - margin] \times [margin, H - margin]$.
   - Формула расчета номинала с зависимостью от $T$.
2. **Интеграция в `WorldSpatialEngine`**:
   - Добавление `coin_spawner: Option<CoinSpawner>` в `WorldSpatialEngine`.
   - Автоматический вызов пополнения пула монет на каждом тике в `resolve()`.
3. **Обновление `ServerConfig` и `main.rs`**:
   - Добавление флага `enable_coin_spawner: bool` (по умолчанию `true` в `main.rs`).

---

## 5. Тестовые сценарии

- `TC-COIN-01`: Проверка поддержания постоянной квоты: при удалении монет генератор восполняет их ровно до $N_{max\_coins}$.
- `TC-COIN-02`: Проверка временной прогрессии: математическое ожидание номинала на тике $T=2000$ строго превышает номинал на тике $T=10$.
- `TC-COIN-03`: Проверка диапазона координат: все сгенерированные монеты находятся строго внутри арены с отступом $margin$.
