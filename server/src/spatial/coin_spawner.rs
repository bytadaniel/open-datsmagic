//! # Генератор монет с прогрессивным масштабированием номинала от тика (FE-009 / DR-008)
//!
//! Модуль реализует контроллер пула нематериальных сокровищ [`CoinSpawner`],
//! поддерживающий постоянную квоту монет на арене и вычисляющий прогрессивный
//! номинал $V(T)$ с учетом прошедшего игрового времени (тиков симулятора).
//!
//! ## Математическая модель расчета номинала (FE-009 раздел 2.1):
//! $$V(T) = \min\left(V_{max\_cap}, \; V_{base} + \lfloor \alpha \cdot T^{\gamma} \rfloor + \text{Random}\left(0, \; V_{spread\_base} + \lfloor \beta \cdot T \rfloor\right)\right)$$
//!
//! Где:
//! - $V_{base} = 25$
//! - $\alpha = 0.05, \gamma = 1.05$
//! - $V_{spread\_base} = 20, \beta = 0.1$
//! - $V_{max\_cap} = 1000$

use serde::{Deserialize, Serialize};

use crate::engine::state::AnomalyState;
use crate::physics::Vec2;
use crate::spatial::entities::Treasure;
use crate::spatial::spawner::SimpleRng;

/// Базовый коэффициент гарантированного роста от тика alpha
pub const DEFAULT_ALPHA: f64 = 0.05;

/// Степень прогрессивного роста от тика gamma
pub const DEFAULT_GAMMA: f64 = 1.05;

/// Базовый разброс случайной дисперсии номинала V_spread_base
pub const DEFAULT_SPREAD_BASE: u32 = 20;

/// Временной коэффициент расширения случайного разброса beta
pub const DEFAULT_BETA: f64 = 0.1;

/// Градация ценности сокровищ (FE-009 раздел 2.2 / DR-008)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoinTier {
    /// Common (Бронза): V < 100
    Common,
    /// Silver (Серебро): 100 <= V < 250
    Silver,
    /// Gold (Золото): 250 <= V < 500
    Gold,
    /// Legendary (Эпик / Рубин): V >= 500
    Legendary,
}

impl CoinTier {
    /// Определяет категорию монеты на основе ее целочисленного номинала
    pub fn from_value(value: u32) -> Self {
        if value < 100 {
            CoinTier::Common
        } else if value < 250 {
            CoinTier::Silver
        } else if value < 500 {
            CoinTier::Gold
        } else {
            CoinTier::Legendary
        }
    }

    /// Возвращает канонический строковый идентификатор тира
    #[inline]
    pub fn as_str(&self) -> &'static str {
        match self {
            CoinTier::Common => "common",
            CoinTier::Silver => "silver",
            CoinTier::Gold => "gold",
            CoinTier::Legendary => "legendary",
        }
    }
}

/// Вычисляет номинал монеты на заданном тике по формуле масштабирования
pub fn calculate_coin_value(
    tick: u64,
    base_value: u32,
    max_value_cap: u32,
    rng: &mut SimpleRng,
) -> u32 {
    let t_f64 = tick as f64;
    let guaranteed = (DEFAULT_ALPHA * t_f64.powf(DEFAULT_GAMMA)).floor() as u64;
    let spread_limit = (DEFAULT_SPREAD_BASE as f64 + (DEFAULT_BETA * t_f64).floor()) as u64;
    let random_spread = if spread_limit > 0 {
        rng.gen_range_u64(0, spread_limit + 1)
    } else {
        0
    };

    let raw_val = (base_value as u64)
        .saturating_add(guaranteed)
        .saturating_add(random_spread);

    raw_val.min(max_value_cap as u64) as u32
}

/// Конфигурация параметров генератора монет
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CoinSpawnerConfig {
    /// Максимальное одновременное количество монет на карте (квота N_max_coins)
    pub max_coins: usize,
    /// Базовый минимальный номинал монеты V_base
    pub base_value: u32,
    /// Ширина игровой арены
    pub arena_width: f64,
    /// Высота игровой арены
    pub arena_height: f64,
    /// Безопасный отступ от границ арены для спавна монет
    pub margin: f64,
    /// Предельный максимальный номинал отдельной монеты V_max_cap
    pub max_value_cap: u32,
}

impl Default for CoinSpawnerConfig {
    fn default() -> Self {
        Self {
            max_coins: 1000,
            base_value: 25,
            arena_width: 2200.0,
            arena_height: 1600.0,
            margin: 40.0,
            max_value_cap: 1000,
        }
    }
}

/// Автоматический генератор монет с временным масштабированием
#[derive(Clone, Debug)]
pub struct CoinSpawner {
    /// Конфигурация параметров генерации
    pub config: CoinSpawnerConfig,
    /// Счетчик уникальных идентификаторов созданных монет
    counter: u64,
    /// Генератор псевдослучайных чисел
    rng: SimpleRng,
}

impl CoinSpawner {
    /// Создает новый спавнер с указанной конфигурацией и недетерминированным сидом
    pub fn new(config: CoinSpawnerConfig) -> Self {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0xfe009fe009fe009);
        Self::with_seed(config, seed)
    }

    /// Создает детерминированный спавнер с фиксированным сидом (для тестов)
    pub fn with_seed(config: CoinSpawnerConfig, seed: u64) -> Self {
        Self {
            config,
            counter: 0,
            rng: SimpleRng::new(seed),
        }
    }

    /// Возвращает ссылку на конфигурацию спавнера
    #[inline]
    pub fn config(&self) -> &CoinSpawnerConfig {
        &self.config
    }

    /// Возвращает текущее количество сгенерированных монет
    #[inline]
    pub fn counter(&self) -> u64 {
        self.counter
    }

    /// Вычисляет номинал монеты для заданного тика
    #[inline]
    pub fn calculate_value(&mut self, tick: u64) -> u32 {
        calculate_coin_value(
            tick,
            self.config.base_value,
            self.config.max_value_cap,
            &mut self.rng,
        )
    }

    /// Генерирует одну новую монету в случайных координатах арены с отступом `margin`
    pub fn spawn_one(&mut self, tick: u64) -> Treasure {
        self.spawn_one_avoiding(tick, &[])
    }

    /// Генерирует одну новую монету, избегая попадания в смертоносные ядра аномалий (DR-008)
    pub fn spawn_one_avoiding(&mut self, tick: u64, anomalies: &[AnomalyState]) -> Treasure {
        let min_x = self.config.margin;
        let max_x = (self.config.arena_width - self.config.margin).max(min_x);
        let min_y = self.config.margin;
        let max_y = (self.config.arena_height - self.config.margin).max(min_y);

        let mut best_x = self.rng.gen_range_f64(min_x, max_x);
        let mut best_y = self.rng.gen_range_f64(min_y, max_y);

        // До 10 попыток перегенерации при попадании в ядро опасных аномалий
        for _ in 0..10 {
            let mut inside_hazard = false;
            for a in anomalies {
                if a.core_radius > 0.0 {
                    let dx = best_x - a.position.0;
                    let dy = best_y - a.position.1;
                    let safe_dist = a.core_radius + 5.0;
                    if dx * dx + dy * dy <= safe_dist * safe_dist {
                        inside_hazard = true;
                        break;
                    }
                }
            }
            if !inside_hazard {
                break;
            }
            best_x = self.rng.gen_range_f64(min_x, max_x);
            best_y = self.rng.gen_range_f64(min_y, max_y);
        }

        self.create_treasure(tick, Vec2::new(best_x, best_y))
    }

    fn create_treasure(&mut self, tick: u64, position: Vec2) -> Treasure {
        self.counter += 1;
        let value = self.calculate_value(tick);
        let tier = CoinTier::from_value(value);
        Treasure::new(
            format!("coin_{}", self.counter),
            tier.as_str(),
            position,
            value,
        )
    }

    /// Восполняет количество монет на карте до квоты `max_coins`
    pub fn replenish(&mut self, current_count: usize, tick: u64) -> Vec<Treasure> {
        self.replenish_avoiding_anomalies(current_count, tick, &[])
    }

    /// Псевдоним метода восполнения пула монет
    #[inline]
    pub fn replenish_if_needed(&mut self, current_count: usize, tick: u64) -> Vec<Treasure> {
        self.replenish(current_count, tick)
    }

    /// Восполняет монеты с учетом активных аномалий для предотвращения спавна внутри ядер
    pub fn replenish_avoiding_anomalies(
        &mut self,
        current_count: usize,
        tick: u64,
        anomalies: &[AnomalyState],
    ) -> Vec<Treasure> {
        let mut new_coins = Vec::new();
        if current_count < self.config.max_coins {
            let to_spawn = self.config.max_coins - current_count;
            for _ in 0..to_spawn {
                new_coins.push(self.spawn_one_avoiding(tick, anomalies));
            }
        }
        new_coins
    }

    /// Восполняет квоту, предпочитая наименее заполненные области арены.
    /// Размер сетки выводится из квоты и соотношения сторон доступной площади.
    pub fn replenish_balanced_avoiding_anomalies(
        &mut self,
        current_positions: &[Vec2],
        tick: u64,
        anomalies: &[AnomalyState],
    ) -> Vec<Treasure> {
        let quota = self.config.max_coins;
        if current_positions.len() >= quota || quota == 0 {
            return Vec::new();
        }

        let min_x = self.config.margin;
        let min_y = self.config.margin;
        let usable_width = (self.config.arena_width - 2.0 * self.config.margin).max(1.0);
        let usable_height = (self.config.arena_height - 2.0 * self.config.margin).max(1.0);
        let columns = ((quota as f64 * usable_width / usable_height).sqrt().ceil() as usize).max(1);
        let rows = quota.div_ceil(columns).max(1);
        let cell_width = usable_width / columns as f64;
        let cell_height = usable_height / rows as f64;
        let mut occupancy = vec![0usize; columns * rows];
        let cell_index = |position: Vec2| {
            let column = (((position.x - min_x) / cell_width).floor() as isize)
                .clamp(0, columns as isize - 1) as usize;
            let row = (((position.y - min_y) / cell_height).floor() as isize)
                .clamp(0, rows as isize - 1) as usize;
            row * columns + column
        };
        for &position in current_positions {
            occupancy[cell_index(position)] += 1;
        }

        let mut spawned = Vec::with_capacity(quota - current_positions.len());
        for _ in current_positions.len()..quota {
            let least_occupied = occupancy.iter().copied().min().unwrap_or(0);
            let candidate_count = occupancy
                .iter()
                .filter(|count| **count == least_occupied)
                .count();
            let selected_tie = self.rng.gen_range_usize(0, candidate_count);
            let selected = occupancy
                .iter()
                .enumerate()
                .filter_map(|(index, count)| (*count == least_occupied).then_some(index))
                .nth(selected_tie)
                .expect("least-occupied candidate exists");
            let column = selected % columns;
            let row = selected / columns;
            let left = min_x + column as f64 * cell_width;
            let bottom = min_y + row as f64 * cell_height;
            let right = (left + cell_width).min(self.config.arena_width - self.config.margin);
            let top = (bottom + cell_height).min(self.config.arena_height - self.config.margin);

            // Try to keep spawns outside anomaly cores without changing the chosen region.
            let mut position = Vec2::new(
                self.rng.gen_range_f64(left, right),
                self.rng.gen_range_f64(bottom, top),
            );
            for _ in 0..10 {
                let in_core = anomalies.iter().any(|anomaly| {
                    let dx = position.x - anomaly.position.0;
                    let dy = position.y - anomaly.position.1;
                    let safe_distance = anomaly.core_radius + 5.0;
                    dx * dx + dy * dy <= safe_distance * safe_distance
                });
                if !in_core {
                    break;
                }
                position = Vec2::new(
                    self.rng.gen_range_f64(left, right),
                    self.rng.gen_range_f64(bottom, top),
                );
            }

            occupancy[selected] += 1;
            spawned.push(self.create_treasure(tick, position));
        }
        spawned
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// TC-COIN-01: Проверка поддержания постоянной квоты монет
    #[test]
    fn test_tc_coin_01_quota_maintenance() {
        let config = CoinSpawnerConfig {
            max_coins: 10,
            ..Default::default()
        };
        let mut spawner = CoinSpawner::with_seed(config, 42);

        // 1. При пустом пуле спавнится ровно max_coins монет
        let initial_coins = spawner.replenish(0, 10);
        assert_eq!(initial_coins.len(), 10);

        // 2. Если квота заполнена, новые монеты не создаются
        let no_coins = spawner.replenish(10, 11);
        assert_eq!(no_coins.len(), 0);

        // 3. При сборе 3 монет спавнится ровно 3 монеты
        let replenished = spawner.replenish(7, 12);
        assert_eq!(replenished.len(), 3);

        // 4. Проверяем уникальность сгенерированных ID
        let mut ids: Vec<String> = initial_coins.into_iter().map(|c| c.id).collect();
        ids.extend(replenished.into_iter().map(|c| c.id));
        let unique_ids: std::collections::HashSet<_> = ids.iter().collect();
        assert_eq!(unique_ids.len(), 13);
    }

    #[test]
    fn replenishment_prefers_least_occupied_region() {
        let config = CoinSpawnerConfig {
            max_coins: 4,
            arena_width: 1000.0,
            arena_height: 1000.0,
            margin: 40.0,
            ..Default::default()
        };
        let mut spawner = CoinSpawner::with_seed(config, 7);
        let live = [
            Vec2::new(100.0, 100.0),
            Vec2::new(600.0, 100.0),
            Vec2::new(700.0, 200.0),
        ];
        let spawned = spawner.replenish_balanced_avoiding_anomalies(&live, 10, &[]);
        assert_eq!(spawned.len(), 1);
        let position = spawned[0].position;
        assert!(position.x >= 500.0 || position.y >= 500.0);
    }

    #[test]
    fn balanced_replenishment_keeps_quota_and_arena_margin() {
        let config = CoinSpawnerConfig {
            max_coins: 64,
            arena_width: 9000.0,
            arena_height: 9000.0,
            margin: 40.0,
            ..Default::default()
        };
        let mut spawner = CoinSpawner::with_seed(config, 99);
        let spawned = spawner.replenish_balanced_avoiding_anomalies(&[], 1, &[]);
        assert_eq!(spawned.len(), 64);
        for coin in spawned {
            assert!((40.0..=8960.0).contains(&coin.position.x));
            assert!((40.0..=8960.0).contains(&coin.position.y));
        }
    }

    /// TC-COIN-02: Проверка временной прогрессии номинала от номера тика
    #[test]
    fn test_tc_coin_02_progressive_value_scaling() {
        let config = CoinSpawnerConfig::default();
        let mut spawner = CoinSpawner::with_seed(config.clone(), 12345);

        // Генерация выборки номиналов на раннем тике T=10
        let samples = 2000;
        let mut sum_early = 0u64;
        for _ in 0..samples {
            let val = spawner.calculate_value(10);
            assert!(
                val >= config.base_value,
                "Номинал {} должен быть >= base_value {}",
                val,
                config.base_value
            );
            sum_early += val as u64;
        }
        let mean_early = sum_early as f64 / samples as f64;

        // Генерация выборки номиналов на позднем тике T=2000
        let mut sum_late = 0u64;
        for _ in 0..samples {
            let val = spawner.calculate_value(2000);
            sum_late += val as u64;
        }
        let mean_late = sum_late as f64 / samples as f64;

        // Математическое ожидание на позднем тике строго больше
        assert!(
            mean_late > mean_early * 3.0,
            "Средний номинал на T=2000 ({}) должен значительно превышать номинал на T=10 ({})",
            mean_late,
            mean_early
        );

        // Проверка ограничения верхним потолком V_max_cap
        for _ in 0..100 {
            let extreme_val = spawner.calculate_value(50000);
            assert_eq!(
                extreme_val, config.max_value_cap,
                "Номинал должен ограничиваться потолком max_value_cap"
            );
        }
    }

    /// TC-COIN-03: Проверка допустимого диапазона координат спавна монет
    #[test]
    fn test_tc_coin_03_coordinate_bounds_and_margin() {
        let config = CoinSpawnerConfig {
            arena_width: 800.0,
            arena_height: 600.0,
            margin: 50.0,
            max_coins: 20,
            ..Default::default()
        };
        let mut spawner = CoinSpawner::with_seed(config.clone(), 999);

        for _ in 0..500 {
            let coin = spawner.spawn_one(100);
            assert!(
                coin.position.x >= config.margin
                    && coin.position.x <= config.arena_width - config.margin,
                "Координата X ({}) вышла за безопасные границы [{}, {}]",
                coin.position.x,
                config.margin,
                config.arena_width - config.margin
            );
            assert!(
                coin.position.y >= config.margin
                    && coin.position.y <= config.arena_height - config.margin,
                "Координата Y ({}) вышла за безопасные границы [{}, {}]",
                coin.position.y,
                config.margin,
                config.arena_height - config.margin
            );
        }
    }

    /// Проверка градации тиров монет CoinTier
    #[test]
    fn test_coin_tier_thresholds() {
        assert_eq!(CoinTier::from_value(25), CoinTier::Common);
        assert_eq!(CoinTier::from_value(99), CoinTier::Common);
        assert_eq!(CoinTier::from_value(100), CoinTier::Silver);
        assert_eq!(CoinTier::from_value(249), CoinTier::Silver);
        assert_eq!(CoinTier::from_value(250), CoinTier::Gold);
        assert_eq!(CoinTier::from_value(499), CoinTier::Gold);
        assert_eq!(CoinTier::from_value(500), CoinTier::Legendary);
        assert_eq!(CoinTier::from_value(1000), CoinTier::Legendary);

        assert_eq!(CoinTier::Common.as_str(), "common");
        assert_eq!(CoinTier::Silver.as_str(), "silver");
        assert_eq!(CoinTier::Gold.as_str(), "gold");
        assert_eq!(CoinTier::Legendary.as_str(), "legendary");
    }

    /// Проверка безопасного избегания ядер аномалий
    #[test]
    fn test_spawn_avoids_anomaly_cores() {
        let config = CoinSpawnerConfig {
            arena_width: 500.0,
            arena_height: 500.0,
            margin: 40.0,
            ..Default::default()
        };
        let mut spawner = CoinSpawner::with_seed(config, 777);

        // Создаем крупную аномалию в центре арены с ядром R_core = 50.0
        let hazards = vec![AnomalyState::new_dynamic(
            "hazard_1",
            "attracting",
            (250.0, 250.0),
            (0.0, 0.0),
            50.0,
            120.0,
            4.0,
        )];

        for _ in 0..100 {
            let coin = spawner.spawn_one_avoiding(50, &hazards);
            let dx = coin.position.x - 250.0;
            let dy = coin.position.y - 250.0;
            let dist = (dx * dx + dy * dy).sqrt();
            assert!(
                dist > 50.0,
                "Монета не должна спавниться внутри смертоносного ядра аномалии (дистанция: {})",
                dist
            );
        }
    }
}
