//! # Генератор динамических аномалий и сквозных траекторий (FE-006 / DR-005)
//!
//! Модуль реализует генератор подвижных вихрей [`AnomalySpawner`], алгоритм выбора
//! точки рождения снаружи арены с гарантией пересечения игрового поля (Crossing Arena Guarantee),
//! а также проверку терминального условия деспавна [`is_despawned`].

use serde::{Deserialize, Serialize};

use crate::physics::Vec2;
use crate::spatial::entities::{AnomalyType, DynamicAnomaly};

/// Детерминированный генератор псевдослучайных чисел SplitMix64 (NFR-03)
#[derive(Clone, Debug)]
pub struct SimpleRng {
    state: u64,
}

impl SimpleRng {
    /// Создает генератор с указанным сидом
    pub fn new(seed: u64) -> Self {
        Self {
            state: if seed == 0 { 0x9e3779b97f4a7c15 } else { seed },
        }
    }

    /// Генерирует следующее псевдослучайное 64-битное число
    #[inline]
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9e3779b97f4a7c15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
        z ^ (z >> 31)
    }

    /// Генерирует псевдослучайное число в полуинтервале [0.0, 1.0)
    #[inline]
    pub fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / ((1u64 << 53) as f64)
    }

    /// Генерирует вещественное число в диапазоне [min, max]
    #[inline]
    pub fn gen_range_f64(&mut self, min: f64, max: f64) -> f64 {
        if min >= max {
            return min;
        }
        min + self.next_f64() * (max - min)
    }

    /// Генерирует целое число в диапазоне [min, max)
    #[inline]
    pub fn gen_range_usize(&mut self, min: usize, max: usize) -> usize {
        if min >= max {
            return min;
        }
        min + (self.next_u64() as usize % (max - min))
    }

    /// Генерирует целое беззнаковое 64-битное число в полуинтервале [min, max)
    #[inline]
    pub fn gen_range_u64(&mut self, min: u64, max: u64) -> u64 {
        if min >= max {
            return min;
        }
        min + (self.next_u64() % (max - min))
    }

    /// Генерирует целое беззнаковое 32-битное число в полуинтервале [min, max)
    #[inline]
    pub fn gen_range_u32(&mut self, min: u32, max: u32) -> u32 {
        if min >= max {
            return min;
        }
        min + ((self.next_u64() as u32) % (max - min))
    }

    /// Генерирует случайный логический флаг (true/false)
    #[inline]
    pub fn gen_bool(&mut self) -> bool {
        (self.next_u64() & 1) == 1
    }
}

/// Конфигурация генератора подвижных аномалий
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AnomalySpawnerConfig {
    /// Максимальное число одновременно существующих аномалий в мире (квота N_max)
    pub max_anomalies: usize,
    /// Ширина игровой арены
    pub arena_width: f64,
    /// Высота игровой арены
    pub arena_height: f64,
    /// Толщина буферной зоны снаружи арены delta_buffer
    pub buffer_distance: f64,
    /// Минимальная скорость движения аномалии V_min
    pub speed_min: f64,
    /// Максимальная скорость движения аномалии V_max
    pub speed_max: f64,
    /// Минимальный радиус смертоносного ядра R_core_min
    pub core_radius_min: f64,
    /// Максимальный радиус смертоносного ядра R_core_max
    pub core_radius_max: f64,
    /// Минимальный радиус зоны воздействия R_effect_min
    pub effect_radius_min: f64,
    /// Максимальный радиус зоны воздействия R_effect_max
    pub effect_radius_max: f64,
    /// Минимальная сила вихря F_min
    pub force_min: f64,
    /// Максимальная сила вихря F_max
    pub force_max: f64,
    /// Вероятность исключения из зависимости силы от размера
    pub force_outlier_probability: f64,
    /// Нижняя граница силы редких выбросов, превосходящих max_acceleration
    pub force_outlier_min: f64,
    /// Верхняя граница силы редких выбросов
    pub force_outlier_max: f64,
}

impl Default for AnomalySpawnerConfig {
    fn default() -> Self {
        Self {
            max_anomalies: 50,
            arena_width: 2200.0,
            arena_height: 1600.0,
            buffer_distance: 50.0,
            speed_min: 30.0,
            speed_max: 160.0,
            core_radius_min: 20.0,
            core_radius_max: 30.0,
            effect_radius_min: 300.0,
            effect_radius_max: 2000.0,
            force_min: 4.0,
            force_max: 22.0,
            force_outlier_probability: 0.1,
            force_outlier_min: 55.0,
            force_outlier_max: 100.0,
        }
    }
}

/// Проверяет условие деспавна аномалии (FE-006 раздел 4.2 / DR-005 правило 5):
/// 1. Расстояние от центра P_anomaly до арены превышает R_effect (зона влияния не касается арены).
/// 2. Вектор скорости направлен наружу от арены (dot product с вектором смещения от арены > 0).
pub fn is_despawned(anomaly: &DynamicAnomaly, arena_width: f64, arena_height: f64) -> bool {
    let px = anomaly.position.x;
    let py = anomaly.position.y;

    let clamped_x = px.clamp(0.0, arena_width);
    let clamped_y = py.clamp(0.0, arena_height);

    let dx = px - clamped_x;
    let dy = py - clamped_y;
    let dist_sq = dx * dx + dy * dy;

    // Если расстояние до арены <= effect_radius, зона действия касается арены -> не деспавним
    if dist_sq <= anomaly.effect_radius * anomaly.effect_radius {
        return false;
    }

    // Вектор (dx, dy) направлен наружу от арены к центру аномалии.
    // Если скалярное произведение скорости и этого вектора положительно, объект удаляется от арены
    let dot = anomaly.velocity.x * dx + anomaly.velocity.y * dy;
    dot > 0.0
}

/// Проверяет, пересекает ли луч из spawn_pos в направлении velocity прямоугольник арены
/// с учетом радиуса действия effect_radius (Crossing Arena Guarantee).
pub fn trajectory_intersects_arena(
    spawn_pos: Vec2,
    velocity: Vec2,
    effect_radius: f64,
    arena_width: f64,
    arena_height: f64,
) -> bool {
    if velocity.length_squared() < 1e-9 {
        return false;
    }

    // Расширенный прямоугольник арены: [-R, W + R] x [-R, H + R]
    let min_x = -effect_radius;
    let max_x = arena_width + effect_radius;
    let min_y = -effect_radius;
    let max_y = arena_height + effect_radius;

    let inv_vx = if velocity.x.abs() > 1e-9 {
        1.0 / velocity.x
    } else {
        f64::INFINITY
    };
    let inv_vy = if velocity.y.abs() > 1e-9 {
        1.0 / velocity.y
    } else {
        f64::INFINITY
    };

    let (t_min_x, t_max_x) = if inv_vx >= 0.0 {
        (
            (min_x - spawn_pos.x) * inv_vx,
            (max_x - spawn_pos.x) * inv_vx,
        )
    } else {
        (
            (max_x - spawn_pos.x) * inv_vx,
            (min_x - spawn_pos.x) * inv_vx,
        )
    };

    let (t_min_y, t_max_y) = if inv_vy >= 0.0 {
        (
            (min_y - spawn_pos.y) * inv_vy,
            (max_y - spawn_pos.y) * inv_vy,
        )
    } else {
        (
            (max_y - spawn_pos.y) * inv_vy,
            (min_y - spawn_pos.y) * inv_vy,
        )
    };

    let t_enter = t_min_x.max(t_min_y);
    let t_exit = t_max_x.min(t_max_y);

    t_enter <= t_exit && t_exit >= 0.0
}

/// Серверный генератор и контроллер численности динамических аномалий
#[derive(Clone, Debug)]
pub struct AnomalySpawner {
    pub config: AnomalySpawnerConfig,
    rng: SimpleRng,
    next_id: u64,
}

impl AnomalySpawner {
    /// Создает генератор с конфигурацией по умолчанию и случайным сидом времени
    pub fn new(config: AnomalySpawnerConfig) -> Self {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x123456789abcdef0);
        Self::with_seed(config, seed)
    }

    /// Создает детерминированный генератор с фиксированным сидом (NFR-03)
    pub fn with_seed(config: AnomalySpawnerConfig, seed: u64) -> Self {
        Self {
            config,
            rng: SimpleRng::new(seed),
            next_id: 1,
        }
    }

    /// Генерирует одну новую аномалию за пределами арены с гарантией пересечения
    pub fn spawn_one(&mut self) -> DynamicAnomaly {
        let effect_radius = self
            .rng
            .gen_range_f64(self.config.effect_radius_min, self.config.effect_radius_max);
        let core_radius = self
            .rng
            .gen_range_f64(self.config.core_radius_min, self.config.core_radius_max)
            .min(effect_radius - 10.0)
            .max(1.0);
        let radius_span = self.config.effect_radius_max - self.config.effect_radius_min;
        let size_ratio = if radius_span > 0.0 {
            ((effect_radius - self.config.effect_radius_min) / radius_span).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let outlier_probability = self.config.force_outlier_probability.clamp(0.0, 1.0);
        let force = if self.rng.next_f64() < outlier_probability {
            self.rng
                .gen_range_f64(self.config.force_outlier_min, self.config.force_outlier_max)
        } else {
            self.config.force_max - (self.config.force_max - self.config.force_min) * size_ratio
        };
        let speed = self
            .rng
            .gen_range_f64(self.config.speed_min, self.config.speed_max);
        let anomaly_type = if self.rng.gen_bool() {
            AnomalyType::Attracting
        } else {
            AnomalyType::Repelling
        };

        // 1. Выбираем стартовую грань арены (0 = Left, 1 = Right, 2 = Bottom, 3 = Top)
        let edge = self.rng.gen_range_usize(0, 4);
        let buffer_delta = effect_radius
            + self
                .rng
                .gen_range_f64(5.0, self.config.buffer_distance + 5.0);

        let (spawn_x, spawn_y) = match edge {
            0 => (
                -buffer_delta,
                self.rng.gen_range_f64(0.0, self.config.arena_height),
            ),
            1 => (
                self.config.arena_width + buffer_delta,
                self.rng.gen_range_f64(0.0, self.config.arena_height),
            ),
            2 => (
                self.rng.gen_range_f64(0.0, self.config.arena_width),
                -buffer_delta,
            ),
            _ => (
                self.rng.gen_range_f64(0.0, self.config.arena_width),
                self.config.arena_height + buffer_delta,
            ),
        };
        let spawn_pos = Vec2::new(spawn_x, spawn_y);

        // 2. Выбираем целевую точку внутри арены [0.1 * W, 0.9 * W] x [0.1 * H, 0.9 * H]
        let target_x = self
            .rng
            .gen_range_f64(self.config.arena_width * 0.1, self.config.arena_width * 0.9);
        let target_y = self.rng.gen_range_f64(
            self.config.arena_height * 0.1,
            self.config.arena_height * 0.9,
        );
        let target_pos = Vec2::new(target_x, target_y);

        // 3. Вектор направления u и скорость V
        let dir = (target_pos - spawn_pos).normalize_or_zero();
        let velocity = dir * speed;

        let id = format!("a_dyn_{}", self.next_id);
        self.next_id += 1;

        DynamicAnomaly::new(
            id,
            anomaly_type,
            spawn_pos,
            velocity,
            core_radius,
            effect_radius,
            force,
        )
    }

    /// Восполняет численность аномалий до сконфигурированного лимита max_anomalies
    pub fn replenish_if_needed(&mut self, current_count: usize) -> Vec<DynamicAnomaly> {
        let mut new_anomalies = Vec::new();
        if current_count < self.config.max_anomalies {
            let to_spawn = self.config.max_anomalies - current_count;
            for _ in 0..to_spawn {
                new_anomalies.push(self.spawn_one());
            }
        }
        new_anomalies
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spawner_guaranteed_arena_intersection() {
        let config = AnomalySpawnerConfig::default();
        let mut spawner = AnomalySpawner::with_seed(config.clone(), 42);

        // Генерируем 100 аномалий и проверяем, что каждая гарантированно пересекает арену
        for _ in 0..100 {
            let anomaly = spawner.spawn_one();
            let intersects = trajectory_intersects_arena(
                anomaly.position,
                anomaly.velocity,
                anomaly.effect_radius,
                config.arena_width,
                config.arena_height,
            );
            assert!(intersects, "Anomaly {:?} must intersect arena", anomaly);
        }
    }

    #[test]
    fn test_anomaly_despawn_conditions() {
        let arena_w = 1000.0;
        let arena_h = 1000.0;

        // 1. Аномалия внутри арены -> не деспавнится
        let inside = DynamicAnomaly::new(
            "a1",
            AnomalyType::Attracting,
            Vec2::new(500.0, 500.0),
            Vec2::new(10.0, 0.0),
            10.0,
            60.0,
            4.0,
        );
        assert!(!is_despawned(&inside, arena_w, arena_h));

        // 2. Аномалия снаружи слева, но движется ВПРАВО (к арене) -> не деспавнится
        let approaching = DynamicAnomaly::new(
            "a2",
            AnomalyType::Attracting,
            Vec2::new(-100.0, 500.0),
            Vec2::new(20.0, 0.0),
            10.0,
            60.0,
            4.0,
        );
        assert!(!is_despawned(&approaching, arena_w, arena_h));

        // 3. Аномалия снаружи справа и движется ВПРАВО (от арены), расстояние > R_effect -> деспавнится
        let leaving = DynamicAnomaly::new(
            "a3",
            AnomalyType::Repelling,
            Vec2::new(1100.0, 500.0),
            Vec2::new(20.0, 0.0),
            10.0,
            60.0,
            4.0,
        );
        assert!(is_despawned(&leaving, arena_w, arena_h));

        // 4. Аномалия снаружи справа, но касается зоны влияния (d = 40 <= 60) -> не деспавнится
        let touching = DynamicAnomaly::new(
            "a4",
            AnomalyType::Repelling,
            Vec2::new(1040.0, 500.0),
            Vec2::new(20.0, 0.0),
            10.0,
            60.0,
            4.0,
        );
        assert!(!is_despawned(&touching, arena_w, arena_h));
    }

    #[test]
    fn test_spawner_quota_replenish() {
        let config = AnomalySpawnerConfig {
            max_anomalies: 3,
            ..Default::default()
        };
        let mut spawner = AnomalySpawner::with_seed(config, 100);

        let spawned = spawner.replenish_if_needed(0);
        assert_eq!(spawned.len(), 3);

        let none_spawned = spawner.replenish_if_needed(3);
        assert_eq!(none_spawned.len(), 0);

        let one_spawned = spawner.replenish_if_needed(2);
        assert_eq!(one_spawned.len(), 1);
    }

    #[test]
    fn test_spawner_emits_rare_uncompensatable_force_outliers() {
        let config = AnomalySpawnerConfig::default();
        assert_eq!(config.max_anomalies, 50);
        let mut spawner = AnomalySpawner::with_seed(config, 0xDA75_2026);
        let samples = (0..1000)
            .map(|_| spawner.spawn_one().force)
            .collect::<Vec<_>>();
        let outliers = samples
            .iter()
            .filter(|force| (55.0..=100.0).contains(*force))
            .count();
        assert!(
            (50..=150).contains(&outliers),
            "expected approximately 10% rare outliers, got {outliers}/1000"
        );
        assert!(samples
            .iter()
            .all(|force| { (4.0..=22.0).contains(force) || (55.0..=100.0).contains(force) }));
    }
}
