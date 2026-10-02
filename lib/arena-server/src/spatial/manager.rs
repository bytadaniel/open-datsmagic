//! # Менеджер пространственных сущностей и коллизий
//!
//! Модуль содержит трейт [`SpatialCollisionManager`] и структуру [`EntityManager`],
//! управляющую коллекциями сущностей игрового мира, а также реализацию
//! [`SpatialStepHandler`] для интеграции с игровым циклом симулятора.

use std::collections::{HashMap, HashSet};

use crate::engine::command_buffer::PlayerId;
use crate::engine::loop_runner::SpatialStepHandler;
use crate::engine::state::{AnomalyState, PlayerState, TreasureState, WorldData};
use crate::physics::Vec2;
use crate::spatial::coin_spawner::CoinSpawner;
use crate::spatial::collision_detector::check_point_in_circle;
use crate::spatial::entities::{Anomaly, DynamicAnomaly, EnemyPlayer, Treasure};
use crate::spatial::spawner::{is_despawned, AnomalySpawner, SimpleRng};

/// Коэффициент радиуса эпицентра аномалии по умолчанию (25% от радиуса вихря)
pub const DEFAULT_STUN_RADIUS_RATIO: f64 = 0.25;

/// Длительность оглушения в тиках по умолчанию (5 тиков = 1.0 с при шаге 200 мс)
pub const DEFAULT_STUN_TICKS: u32 = 5;

/// Радиус сбора сокровищ по умолчанию
pub const DEFAULT_CAPTURE_RADIUS: f64 = 10.0;

/// Радиус ковра игрока со смертоносным ядром по умолчанию
pub const DEFAULT_PLAYER_RADIUS: f64 = 5.0;

/// Трейт управления пространственными сущностями и коллизиями (FE-003 / FE-006)
pub trait SpatialCollisionManager: Send + Sync {
    /// Вычисляет суммарный вектор гравитационных сил окружения от всех аномалий
    fn compute_environmental_forces(&self, player_pos: Vec2) -> Vec2;

    /// Проверяет swept-касание отрезка движения с эффективным радиусом `capture_radius`.
    /// Начисляет суммарные очки и удаляет собранные сокровища из пула.
    fn resolve_treasure_captures(
        &mut self,
        previous_pos: Vec2,
        current_pos: Vec2,
        capture_radius: f64,
    ) -> u32;

    /// Проверяет нахождение игрока в эпицентре аномалий для наложения оглушения
    fn check_and_update_stuns(&mut self, player_pos: Vec2) -> bool;

    /// Продвигает координаты аномалий на шаг dt, деспавнит вышедшие за границы
    /// и восполняет численность при наличии спавнера
    fn advance_anomalies(&mut self, dt: f64);

    /// Проверяет летальный контакт ковра игрока со смертоносными ядрами аномалий
    fn check_lethal_core_collisions(&self, player_pos: Vec2, player_radius: f64) -> bool;
}

/// Вычисляет вектор силы затягивания к центру аномалии с амплитудой F_pull
/// в соответствии с разделом 4.1 спецификации FE-003.
#[inline]
pub fn compute_single_anomaly_force(
    player_pos: Vec2,
    anomaly_pos: Vec2,
    radius: f64,
    force: f64,
) -> Vec2 {
    if !player_pos.is_finite()
        || !anomaly_pos.is_finite()
        || !radius.is_finite()
        || !force.is_finite()
    {
        return Vec2::ZERO;
    }
    if radius <= 0.0 || force <= 0.0 {
        return Vec2::ZERO;
    }

    // 1. Вектор разности: ΔP = P_anomaly - P_player
    let delta = anomaly_pos - player_pos;

    // 2. Квадрат расстояния: d^2 = ΔP.length_squared()
    let dist_sq = delta.length_squared();

    // 3. Если d^2 <= R_anomaly^2
    if dist_sq <= radius * radius {
        // Единичный вектор: u = ΔP.normalize_or_zero()
        // Сила вихря: w_i = u * F_pull
        let unit_dir = delta.normalize_or_zero();
        unit_dir * force
    } else {
        Vec2::ZERO
    }
}

/// Псевдоним для единичной силы аномалии
#[inline]
pub fn compute_anomaly_force(player_pos: Vec2, anomaly_pos: Vec2, radius: f64, force: f64) -> Vec2 {
    compute_single_anomaly_force(player_pos, anomaly_pos, radius, force)
}

/// Вычисляет суммарную силу внешних воздействий от списка Anomaly
pub fn compute_environmental_forces_from_anomalies(
    player_pos: Vec2,
    anomalies: &[Anomaly],
) -> Vec2 {
    if !player_pos.is_finite() {
        return Vec2::ZERO;
    }
    let mut total = Vec2::ZERO;
    for anomaly in anomalies {
        total += compute_single_anomaly_force(
            player_pos,
            anomaly.position,
            anomaly.radius,
            anomaly.force,
        );
    }
    total
}

/// Вычисляет суммарную силу внешних воздействий от списка AnomalyState (для физического движка)
pub fn compute_environmental_forces_from_states(
    player_pos: Vec2,
    anomalies: &[AnomalyState],
) -> Vec2 {
    if !player_pos.is_finite() {
        return Vec2::ZERO;
    }
    let mut total = Vec2::ZERO;
    for a in anomalies {
        let a_pos = Vec2::new(a.position.0, a.position.1);
        let pull = compute_single_anomaly_force(player_pos, a_pos, a.radius, a.force);
        if a.anomaly_type == "repelling" {
            total -= pull;
        } else {
            total += pull;
        }
    }
    total
}

/// Вычисляет суперпозицию сил от списка DynamicAnomaly (FE-006 / DR-005)
pub fn compute_environmental_forces_from_dynamic_anomalies(
    player_pos: Vec2,
    anomalies: &[DynamicAnomaly],
) -> Vec2 {
    if !player_pos.is_finite() {
        return Vec2::ZERO;
    }
    let mut total = Vec2::ZERO;
    for anomaly in anomalies {
        total += anomaly.calculate_force(player_pos);
    }
    total
}

/// Менеджер пространственных сущностей игрового мира
#[derive(Clone, Debug)]
pub struct EntityManager {
    /// Сокровища на карте
    pub treasures: Vec<Treasure>,
    /// Статичные аномалии на карте
    pub anomalies: Vec<Anomaly>,
    /// Динамические подвижные аномалии (FE-006)
    pub dynamic_anomalies: Vec<DynamicAnomaly>,
    /// Соперники на карте
    pub enemies: Vec<EnemyPlayer>,
    /// Отношение радиуса эпицентра оглушения к радиусу вихря
    pub stun_radius_ratio: f64,
    /// Генератор динамических аномалий
    pub spawner: Option<AnomalySpawner>,
    /// Генератор прогрессивных монет (FE-009)
    pub coin_spawner: Option<CoinSpawner>,
    /// Номер текущего тика
    pub current_tick: u64,
    /// Ширина игровой арены
    pub arena_width: f64,
    /// Высота игровой арены
    pub arena_height: f64,
}

impl Default for EntityManager {
    fn default() -> Self {
        Self {
            treasures: Vec::new(),
            anomalies: Vec::new(),
            dynamic_anomalies: Vec::new(),
            enemies: Vec::new(),
            stun_radius_ratio: DEFAULT_STUN_RADIUS_RATIO,
            spawner: None,
            coin_spawner: None,
            current_tick: 0,
            arena_width: 1000.0,
            arena_height: 1000.0,
        }
    }
}

impl EntityManager {
    /// Создает новый пустой менеджер сущностей
    pub fn new() -> Self {
        Self::default()
    }

    /// Создает менеджер с указанными наборами сущностей
    pub fn with_entities(
        treasures: Vec<Treasure>,
        anomalies: Vec<Anomaly>,
        enemies: Vec<EnemyPlayer>,
    ) -> Self {
        Self {
            treasures,
            anomalies,
            dynamic_anomalies: Vec::new(),
            enemies,
            stun_radius_ratio: DEFAULT_STUN_RADIUS_RATIO,
            spawner: None,
            coin_spawner: None,
            current_tick: 0,
            arena_width: 1000.0,
            arena_height: 1000.0,
        }
    }

    /// Устанавливает генератор аномалий
    pub fn with_spawner(mut self, spawner: AnomalySpawner) -> Self {
        self.spawner = Some(spawner);
        self
    }

    /// Устанавливает генератор прогрессивных монет (FE-009)
    pub fn with_coin_spawner(mut self, coin_spawner: CoinSpawner) -> Self {
        self.coin_spawner = Some(coin_spawner);
        self
    }

    /// Устанавливает текущий номер тика
    pub fn with_current_tick(mut self, current_tick: u64) -> Self {
        self.current_tick = current_tick;
        self
    }

    /// Пополняет пул сокровищ до квоты max_coins
    pub fn replenish_coins(&mut self) {
        if let Some(ref mut coin_spawner) = self.coin_spawner {
            let live_positions = self
                .treasures
                .iter()
                .filter(|treasure| !treasure.is_collected)
                .map(|treasure| treasure.position)
                .collect::<Vec<_>>();
            let spawned = coin_spawner.replenish_balanced_avoiding_anomalies(
                &live_positions,
                self.current_tick,
                &[],
            );
            self.treasures.extend(spawned);
        }
    }

    /// Устанавливает размеры арены
    pub fn with_arena(mut self, width: f64, height: f64) -> Self {
        self.arena_width = width;
        self.arena_height = height;
        self
    }

    /// Добавляет сокровище в активный пул
    pub fn add_treasure(&mut self, treasure: Treasure) {
        self.treasures.push(treasure);
    }

    /// Добавляет аномалию
    pub fn add_anomaly(&mut self, anomaly: Anomaly) {
        self.anomalies.push(anomaly);
    }

    /// Добавляет динамическую аномалию
    pub fn add_dynamic_anomaly(&mut self, anomaly: DynamicAnomaly) {
        self.dynamic_anomalies.push(anomaly);
    }

    /// Добавляет соперника
    pub fn add_enemy(&mut self, enemy: EnemyPlayer) {
        self.enemies.push(enemy);
    }
}

impl SpatialCollisionManager for EntityManager {
    /// Реализует суперпозицию гравитационных сил W = sum w_i
    fn compute_environmental_forces(&self, player_pos: Vec2) -> Vec2 {
        let legacy_forces =
            compute_environmental_forces_from_anomalies(player_pos, &self.anomalies);
        let dynamic_forces = compute_environmental_forces_from_dynamic_anomalies(
            player_pos,
            &self.dynamic_anomalies,
        );
        legacy_forces + dynamic_forces
    }

    /// Реализует swept-сбор сокровищ на сегменте тика с эффективным радиусом касания.
    fn resolve_treasure_captures(
        &mut self,
        previous_pos: Vec2,
        current_pos: Vec2,
        capture_radius: f64,
    ) -> u32 {
        if !previous_pos.is_finite()
            || !current_pos.is_finite()
            || !capture_radius.is_finite()
            || capture_radius <= 0.0
        {
            return 0;
        }

        let mut total_score = 0;
        let radius_sq = capture_radius * capture_radius;

        for treasure in &mut self.treasures {
            if !treasure.is_collected {
                let dist_sq =
                    point_segment_distance_squared(treasure.position, previous_pos, current_pos);
                if dist_sq <= radius_sq {
                    treasure.is_collected = true;
                    total_score += treasure.value;
                }
            }
        }

        // Удаление захваченных сокровищ из активного пула карты
        self.treasures.retain(|t| !t.is_collected);

        total_score
    }

    /// Реализует проверку попадания в эпицентр аномалий (DR-003 FR-05)
    fn check_and_update_stuns(&mut self, player_pos: Vec2) -> bool {
        if !player_pos.is_finite() {
            return false;
        }

        for anomaly in &self.anomalies {
            let stun_radius = anomaly.radius * self.stun_radius_ratio;
            if check_point_in_circle(player_pos, anomaly.position, stun_radius) {
                return true;
            }
        }

        for anomaly in &self.dynamic_anomalies {
            let stun_radius = anomaly.effect_radius * self.stun_radius_ratio;
            if check_point_in_circle(player_pos, anomaly.position, stun_radius) {
                return true;
            }
        }

        false
    }

    /// Смещает динамические аномалии, деспавнит вышедшие за границы и пополняет пул
    fn advance_anomalies(&mut self, dt: f64) {
        for anomaly in &mut self.dynamic_anomalies {
            anomaly.step(dt);
        }

        let arena_w = self.arena_width;
        let arena_h = self.arena_height;
        self.dynamic_anomalies
            .retain(|a| !is_despawned(a, arena_w, arena_h));

        if let Some(ref mut spawner) = self.spawner {
            let spawned = spawner.replenish_if_needed(self.dynamic_anomalies.len());
            self.dynamic_anomalies.extend(spawned);
        }
    }

    /// Проверяет контакт игрока с ядром любой динамической аномалии
    fn check_lethal_core_collisions(&self, player_pos: Vec2, player_radius: f64) -> bool {
        self.dynamic_anomalies
            .iter()
            .any(|a| a.touches_core(player_pos, player_radius))
    }
}

/// Серверный обработчик фазы пространственных коллизий для интеграции в GameEngine
/// Серверный обработчик фазы пространственных коллизий для интеграции в GameEngine
#[derive(Clone, Debug)]
pub struct WorldSpatialEngine {
    /// Эффективная дистанция касания (R_player + R_coin), в текущей конфигурации 10.
    pub capture_radius: f64,
    /// Отношение радиуса эпицентра к радиусу вихря
    pub stun_radius_ratio: f64,
    /// Длительность оглушения в тиках
    pub stun_duration_ticks: u32,
    /// Радиус ковра игрока R_carpet для коллизий с ядром
    pub player_radius: f64,
    /// Ширина игровой арены
    pub arena_width: f64,
    /// Высота игровой арены
    pub arena_height: f64,
    /// Генератор динамических аномалий
    pub spawner: Option<AnomalySpawner>,
    /// Генератор прогрессивных монет (FE-009)
    pub coin_spawner: Option<CoinSpawner>,
    /// Текущий номер тика симуляции
    pub current_tick: u64,
    /// Процент личного кошелька погибшего ковра, теряемый при смерти.
    pub carpet_death_loss_percent: f64,
    /// Флаг включения автоматического переспавна погибших ковров флота (FE-011)
    pub enable_carpet_respawn: bool,
    /// Детерминированный генератор псевдослучайных чисел для безопасного переспавна
    pub rng: SimpleRng,
    /// Позиции сущностей после последнего пространственного шага, для swept-захвата монет.
    previous_positions: HashMap<String, Vec2>,
}

impl Default for WorldSpatialEngine {
    fn default() -> Self {
        Self {
            capture_radius: DEFAULT_CAPTURE_RADIUS,
            stun_radius_ratio: DEFAULT_STUN_RADIUS_RATIO,
            stun_duration_ticks: DEFAULT_STUN_TICKS,
            player_radius: DEFAULT_PLAYER_RADIUS,
            arena_width: 1000.0,
            arena_height: 1000.0,
            spawner: None,
            coin_spawner: None,
            current_tick: 0,
            carpet_death_loss_percent: 30.0,
            enable_carpet_respawn: false,
            rng: SimpleRng::new(42),
            previous_positions: HashMap::new(),
        }
    }
}

impl WorldSpatialEngine {
    /// Создает новый обработчик с заданными параметрами
    pub fn new(capture_radius: f64, stun_radius_ratio: f64, stun_duration_ticks: u32) -> Self {
        Self {
            capture_radius,
            stun_radius_ratio,
            stun_duration_ticks,
            player_radius: DEFAULT_PLAYER_RADIUS,
            arena_width: 1000.0,
            arena_height: 1000.0,
            spawner: None,
            coin_spawner: None,
            current_tick: 0,
            carpet_death_loss_percent: 30.0,
            enable_carpet_respawn: false,
            rng: SimpleRng::new(42),
            previous_positions: HashMap::new(),
        }
    }

    /// Sets the effective touch distance (carpet radius plus bounty radius).
    pub fn with_capture_radius(mut self, capture_radius: f64) -> Self {
        self.capture_radius = capture_radius.max(0.0);
        self
    }

    /// Assigns a reproducible world-derived stream for spawn and respawn placement.
    pub fn with_spawn_seed(mut self, seed: u64) -> Self {
        self.rng = SimpleRng::new(seed);
        self
    }

    /// Устанавливает генератор аномалий
    pub fn with_spawner(mut self, spawner: AnomalySpawner) -> Self {
        self.spawner = Some(spawner);
        self
    }

    /// Устанавливает генератор прогрессивных монет (FE-009)
    pub fn with_coin_spawner(mut self, coin_spawner: CoinSpawner) -> Self {
        self.coin_spawner = Some(coin_spawner);
        self
    }

    /// Устанавливает текущий номер тика
    pub fn with_current_tick(mut self, current_tick: u64) -> Self {
        self.current_tick = current_tick;
        self
    }

    /// Устанавливает размеры арены
    pub fn with_arena(mut self, width: f64, height: f64) -> Self {
        self.arena_width = width;
        self.arena_height = height;
        self
    }

    /// Устанавливает радиус ковра игрока
    pub fn with_player_radius(mut self, player_radius: f64) -> Self {
        self.player_radius = player_radius;
        self
    }

    /// Sets the fraction of the dead carpet's personal gold balance that is lost.
    pub fn with_carpet_death_loss_percent(mut self, percent: f64) -> Self {
        self.carpet_death_loss_percent = percent.clamp(0.0, 100.0);
        self
    }

    /// Устанавливает флаг автоматического переспавна ковров флота (FE-011)
    pub fn with_carpet_respawn(mut self, enable: bool) -> Self {
        self.enable_carpet_respawn = enable;
        self
    }

    /// Детекция взаимных столкновений ковров флотов (FE-011 Section 2.1).
    ///
    /// Проверяет условие `dist(C_i, C_j) <= 2 * R_player` для всех пар активных ковров
    /// как между разными игроками, так и внутри собственного флота команды.
    /// Возвращает детерминированный упорядоченный список пар идентификаторов столкнувшихся ковров.
    pub fn detect_carpet_collisions(
        players: &HashMap<PlayerId, PlayerState>,
        player_radius: f64,
    ) -> Vec<(String, String)> {
        let mut active_carpets: Vec<(&str, (f64, f64))> = Vec::new();

        for player in players.values() {
            if player.is_destroyed() {
                continue;
            }
            if player.carpets.is_empty() {
                active_carpets.push((player.id.as_str(), player.position));
            } else {
                for carpet in player.carpets.values() {
                    if !carpet.is_destroyed() {
                        active_carpets.push((carpet.id.as_str(), carpet.position));
                    }
                }
            }
        }

        let mut collisions = Vec::new();
        let collision_dist = 2.0 * player_radius;
        let collision_dist_sq = collision_dist * collision_dist;

        for i in 0..active_carpets.len() {
            for j in (i + 1)..active_carpets.len() {
                let (id_a, pos_a) = active_carpets[i];
                let (id_b, pos_b) = active_carpets[j];

                let dx = pos_a.0 - pos_b.0;
                let dy = pos_a.1 - pos_b.1;
                let dist_sq = dx * dx + dy * dy;

                if dist_sq <= collision_dist_sq {
                    let pair = if id_a <= id_b {
                        (id_a.to_string(), id_b.to_string())
                    } else {
                        (id_b.to_string(), id_a.to_string())
                    };
                    collisions.push(pair);
                }
            }
        }

        collisions.sort();
        collisions.dedup();
        collisions
    }

    /// Детекция выхода ковров за границы арены (FE-011 Section 2.2).
    ///
    /// Возвращает список идентификаторов активных ковров, находящихся за пределами арены.
    pub fn detect_out_of_bounds(
        players: &HashMap<PlayerId, PlayerState>,
        arena_width: f64,
        arena_height: f64,
    ) -> Vec<String> {
        let mut oob = Vec::new();

        for player in players.values() {
            if player.is_destroyed() {
                continue;
            }
            if player.carpets.is_empty() {
                let (x, y) = player.position;
                if x < 0.0 || x > arena_width || y < 0.0 || y > arena_height {
                    oob.push(player.id.clone());
                }
            } else {
                for carpet in player.carpets.values() {
                    if !carpet.is_destroyed() {
                        let (x, y) = carpet.position;
                        if x < 0.0 || x > arena_width || y < 0.0 || y > arena_height {
                            oob.push(carpet.id.clone());
                        }
                    }
                }
            }
        }

        oob.sort();
        oob
    }

    /// Списание настраиваемой доли личного кошелька погибшего ковра (FE-011 Section 2.3).
    pub fn apply_death_penalties(
        players: &mut HashMap<PlayerId, PlayerState>,
        destroyed_carpets: &[String],
        carpet_loss_percent: f64,
    ) {
        if destroyed_carpets.is_empty() {
            return;
        }

        for carpet_id in destroyed_carpets {
            for player in players.values_mut() {
                if let Some(carpet) = player.carpets.get_mut(carpet_id) {
                    let loss = ((f64::from(carpet.gold) * carpet_loss_percent.clamp(0.0, 100.0))
                        / 100.0)
                        .floor() as u32;
                    carpet.gold = carpet.gold.saturating_sub(loss);
                    player.score = player.score.saturating_sub(loss);
                    break;
                }
                if player.id == *carpet_id {
                    // Backward-compatible single-unit player (without a fleet).
                    let loss = ((f64::from(player.score) * carpet_loss_percent.clamp(0.0, 100.0))
                        / 100.0)
                        .floor() as u32;
                    player.score = player.score.saturating_sub(loss);
                    break;
                }
            }
        }
    }

    /// Поиск безопасной точки респавна для ковра в соответствии с FE-011 Section 2.4.
    pub fn find_safe_spawn_point(
        world: &WorldData,
        arena_width: f64,
        arena_height: f64,
        player_radius: f64,
        rng: &mut SimpleRng,
    ) -> (f64, f64) {
        let min_x = 50.0_f64.min(arena_width / 2.0);
        let max_x = (arena_width - 50.0_f64).max(min_x);
        let min_y = 50.0_f64.min(arena_height / 2.0);
        let max_y = (arena_height - 50.0_f64).max(min_y);

        let carpet_safe_dist_sq = (4.0 * player_radius) * (4.0 * player_radius);

        for _ in 0..100 {
            let x = rng.gen_range_f64(min_x, max_x);
            let y = rng.gen_range_f64(min_y, max_y);
            let candidate = Vec2::new(x, y);

            // 1. Проверка безопасного расстояния до ядер аномалий: dist > core_radius + 50.0
            let near_anomaly = world.anomalies.iter().any(|a| {
                let a_pos = Vec2::new(a.position.0, a.position.1);
                let safe_r = a.core_radius + 50.0;
                candidate.distance_squared(a_pos) <= safe_r * safe_r
            });
            if near_anomaly {
                continue;
            }

            // 2. Проверка безопасного расстояния до других живых ковров: dist > 4 * R_player
            let near_other_carpet = world
                .players
                .values()
                .filter(|p| !p.is_destroyed())
                .any(|p| {
                    p.carpets.values().filter(|c| !c.is_destroyed()).any(|c| {
                        let c_pos = Vec2::new(c.position.0, c.position.1);
                        candidate.distance_squared(c_pos) <= carpet_safe_dist_sq
                    })
                });
            if near_other_carpet {
                continue;
            }

            return (x, y);
        }

        (arena_width / 2.0, arena_height / 2.0)
    }

    /// Безопасный переспавн уничтоженных ковров флота живых игроков (FE-011 Section 2.4).
    pub fn respawn_destroyed_carpets(&mut self, world: &mut WorldData) {
        let arena_w = self.arena_width;
        let arena_h = self.arena_height;
        let p_radius = self.player_radius;

        let mut to_respawn_carpets = Vec::new();
        let mut to_respawn_players = Vec::new();

        for (player_id, player) in &world.players {
            if player.carpets.is_empty() {
                if player.is_destroyed() {
                    to_respawn_players.push(player_id.clone());
                }
            } else {
                for (carpet_id, carpet) in &player.carpets {
                    if carpet.is_destroyed() {
                        to_respawn_carpets.push((player_id.clone(), carpet_id.clone()));
                    }
                }
            }
        }

        for (player_id, carpet_id) in to_respawn_carpets {
            let (spawn_x, spawn_y) =
                Self::find_safe_spawn_point(world, arena_w, arena_h, p_radius, &mut self.rng);

            if let Some(player) = world.players.get_mut(&player_id) {
                if let Some(carpet) = player.carpets.get_mut(&carpet_id) {
                    carpet.position = (spawn_x, spawn_y);
                    carpet.velocity = (0.0, 0.0);
                    carpet.acceleration = (0.0, 0.0);
                    carpet.status = "normal".to_string();
                    carpet.stun_remaining_ticks = 0;
                }
                player.sync_from_carpets();
            }
        }

        for player_id in to_respawn_players {
            let (spawn_x, spawn_y) =
                Self::find_safe_spawn_point(world, arena_w, arena_h, p_radius, &mut self.rng);

            if let Some(player) = world.players.get_mut(&player_id) {
                player.position = (spawn_x, spawn_y);
                player.velocity = (0.0, 0.0);
                player.acceleration = (0.0, 0.0);
                player.status = "normal".to_string();
                player.stun_remaining_ticks = 0;
            }
        }
    }
}

impl SpatialStepHandler for WorldSpatialEngine {
    fn capture_start_positions(&mut self, world: &WorldData) {
        self.previous_positions.clear();
        for player in world.players.values() {
            if player.carpets.is_empty() {
                self.previous_positions
                    .insert(player.id.clone(), Vec2::from_tuple(player.position));
            } else {
                for carpet in player.carpets.values() {
                    self.previous_positions
                        .insert(carpet.id.clone(), Vec2::from_tuple(carpet.position));
                }
            }
        }
    }

    fn set_tick(&mut self, tick: u64) {
        self.current_tick = tick;
    }

    fn resolve(&mut self, world: &mut WorldData, dt: f64) {
        let cap_rad_sq = self.capture_radius * self.capture_radius;
        let mut movement_segments = HashMap::new();
        for player in world.players.values() {
            if player.carpets.is_empty() {
                let end = Vec2::from_tuple(player.position);
                let start = self
                    .previous_positions
                    .get(&player.id)
                    .copied()
                    .unwrap_or(end);
                movement_segments.insert(player.id.clone(), (start, end));
            } else {
                for carpet in player.carpets.values() {
                    let end = Vec2::from_tuple(carpet.position);
                    let start = self
                        .previous_positions
                        .get(&carpet.id)
                        .copied()
                        .unwrap_or(end);
                    movement_segments.insert(carpet.id.clone(), (start, end));
                }
            }
        }

        // Count only actual movement segments from this tick. Position jumps made by
        // respawn happen later and are intentionally not included.
        for player in world.players.values_mut() {
            let distance = if player.carpets.is_empty() {
                if player.is_destroyed() {
                    0.0
                } else {
                    movement_segments
                        .get(&player.id)
                        .map(|(start, end)| (*end - *start).length())
                        .unwrap_or(0.0)
                }
            } else {
                player
                    .carpets
                    .values()
                    .filter(|carpet| !carpet.is_destroyed())
                    .filter_map(|carpet| movement_segments.get(&carpet.id))
                    .map(|(start, end)| (*end - *start).length())
                    .sum()
            };
            player.distance_travelled += distance;
        }

        // Шаг 1: Продвижение динамических аномалий, деспавн и восполнение
        for anomaly in &mut world.anomalies {
            anomaly.position.0 += anomaly.velocity.0 * dt;
            anomaly.position.1 += anomaly.velocity.1 * dt;
        }

        let arena_w = self.arena_width;
        let arena_h = self.arena_height;
        world.anomalies.retain(|a| {
            let dyn_a: DynamicAnomaly = a.into();
            !is_despawned(&dyn_a, arena_w, arena_h)
        });

        if let Some(ref mut spawner) = self.spawner {
            let spawned = spawner.replenish_if_needed(world.anomalies.len());
            world
                .anomalies
                .extend(spawned.into_iter().map(AnomalyState::from));
        }

        // Шаг 1b: Проверка квоты и пополнение пула прогрессивных монет (FE-009 / DR-008)
        if let Some(ref mut coin_spawner) = self.coin_spawner {
            let live_positions = world
                .treasures
                .iter()
                .filter(|treasure| !treasure.is_collected)
                .map(|treasure| Vec2::new(treasure.position.0, treasure.position.1))
                .collect::<Vec<_>>();
            let spawned = coin_spawner.replenish_balanced_avoiding_anomalies(
                &live_positions,
                self.current_tick,
                &world.anomalies,
            );
            world
                .treasures
                .extend(spawned.into_iter().map(TreasureState::from));
        }

        // Шаг 2 (Фаза 1): Проверка смертоносных ядер аномалий (R_core)
        let mut destroyed_this_tick = Vec::new();
        for player in world.players.values_mut() {
            if player.is_destroyed() {
                continue;
            }
            if player.carpets.is_empty() {
                let p_pos = Vec2::new(player.position.0, player.position.1);
                let hit_core = world.anomalies.iter().any(|a| {
                    if a.core_radius <= 0.0 {
                        return false;
                    }
                    let a_pos = Vec2::new(a.position.0, a.position.1);
                    let total_r = a.core_radius + self.player_radius;
                    a_pos.distance_squared(p_pos) <= total_r * total_r
                });

                if hit_core {
                    player.mark_destroyed();
                    destroyed_this_tick.push(player.id.clone());
                }
            } else {
                for carpet in player.carpets.values_mut() {
                    if carpet.is_destroyed() {
                        continue;
                    }
                    let c_pos = Vec2::new(carpet.position.0, carpet.position.1);
                    let hit_core = world.anomalies.iter().any(|a| {
                        if a.core_radius <= 0.0 {
                            return false;
                        }
                        let a_pos = Vec2::new(a.position.0, a.position.1);
                        let total_r = a.core_radius + self.player_radius;
                        a_pos.distance_squared(c_pos) <= total_r * total_r
                    });

                    if hit_core {
                        carpet.mark_destroyed();
                        destroyed_this_tick.push(carpet.id.clone());
                    }
                }
            }
        }

        // Шаг 2b (Фаза 2): Детекция взаимных столкновений ковер-ковер (dist <= 2*R_player)
        let collision_pairs = Self::detect_carpet_collisions(&world.players, self.player_radius);
        let mut collided_ids = HashSet::new();
        for (c1, c2) in collision_pairs {
            collided_ids.insert(c1);
            collided_ids.insert(c2);
        }
        for player in world.players.values_mut() {
            if player.is_destroyed() {
                continue;
            }
            if player.carpets.is_empty() {
                if collided_ids.contains(&player.id) {
                    player.mark_destroyed();
                    destroyed_this_tick.push(player.id.clone());
                }
            } else {
                for carpet in player.carpets.values_mut() {
                    if !carpet.is_destroyed() && collided_ids.contains(&carpet.id) {
                        carpet.mark_destroyed();
                        destroyed_this_tick.push(carpet.id.clone());
                    }
                }
            }
        }

        // Шаг 2c (Фаза 3): Проверка выхода за границы арены (x < 0, x > W, y < 0, y > H)
        let oob_ids =
            Self::detect_out_of_bounds(&world.players, self.arena_width, self.arena_height);
        let oob_set: HashSet<String> = oob_ids.into_iter().collect();
        for player in world.players.values_mut() {
            if player.is_destroyed() {
                continue;
            }
            if player.carpets.is_empty() {
                if oob_set.contains(&player.id) {
                    player.mark_destroyed();
                    destroyed_this_tick.push(player.id.clone());
                }
            } else {
                for carpet in player.carpets.values_mut() {
                    if !carpet.is_destroyed() && oob_set.contains(&carpet.id) {
                        carpet.mark_destroyed();
                        destroyed_this_tick.push(carpet.id.clone());
                    }
                }
            }
        }

        // Шаг 2d (Фаза 4): Применение штрафов за гибель ковров
        Self::apply_death_penalties(
            &mut world.players,
            &destroyed_this_tick,
            self.carpet_death_loss_percent,
        );

        // Шаг 2e (Фаза 5): Безопасный переспавн погибших ковров флота
        if self.enable_carpet_respawn {
            self.respawn_destroyed_carpets(world);
        }

        // Шаг 3: Сбор сокровищ живыми игроками / коврами
        let mut collected_ids = HashSet::new();
        let teleported_ids = destroyed_this_tick.iter().cloned().collect::<HashSet<_>>();

        for treasure in &mut world.treasures {
            let t_pos = Vec2::new(treasure.position.0, treasure.position.1);
            let mut closest_collector: Option<(String, Option<String>)> = None;
            let mut min_dist_sq = f64::INFINITY;

            for (p_id, player) in &world.players {
                if player.is_destroyed() {
                    continue;
                }
                if player.carpets.is_empty() {
                    let end = Vec2::from_tuple(player.position);
                    let (start, end) = movement_segments
                        .get(&player.id)
                        .copied()
                        .unwrap_or((end, end));
                    let start = if teleported_ids.contains(&player.id) {
                        end
                    } else {
                        start
                    };
                    let dist_sq = point_segment_distance_squared(t_pos, start, end);
                    if dist_sq <= cap_rad_sq && dist_sq < min_dist_sq {
                        min_dist_sq = dist_sq;
                        closest_collector = Some((p_id.clone(), None));
                    }
                } else {
                    for (carpet_id, carpet) in &player.carpets {
                        if carpet.is_destroyed() {
                            continue;
                        }
                        let end = Vec2::from_tuple(carpet.position);
                        let (start, end) = movement_segments
                            .get(&carpet.id)
                            .copied()
                            .unwrap_or((end, end));
                        let start = if teleported_ids.contains(&carpet.id) {
                            end
                        } else {
                            start
                        };
                        let dist_sq = point_segment_distance_squared(t_pos, start, end);
                        if dist_sq <= cap_rad_sq && dist_sq < min_dist_sq {
                            min_dist_sq = dist_sq;
                            closest_collector = Some((p_id.clone(), Some(carpet_id.clone())));
                        }
                    }
                }
            }

            if let Some((player_id, carpet_id)) = closest_collector {
                if let Some(player) = world.players.get_mut(&player_id) {
                    player.score = player.score.saturating_add(treasure.value);
                    player.gold_collected_total = player
                        .gold_collected_total
                        .saturating_add(u64::from(treasure.value));
                    if let Some(carpet_id) = carpet_id {
                        if let Some(carpet) = player.carpets.get_mut(&carpet_id) {
                            carpet.gold = carpet.gold.saturating_add(treasure.value);
                            carpet.gold_collected_total = carpet
                                .gold_collected_total
                                .saturating_add(u64::from(treasure.value));
                        }
                    }
                }
                treasure.is_collected = true;
                collected_ids.insert(treasure.id.clone());
            }
        }

        // Шаг 5: Удаление собранных сокровищ из карты
        world.treasures.retain(|t| !t.is_collected);

        // Шаг 6: Немедленное восполнение пула монет взамен собранных (AC-01 / FR-02)
        if let Some(ref mut coin_spawner) = self.coin_spawner {
            let live_positions = world
                .treasures
                .iter()
                .filter(|treasure| !treasure.is_collected)
                .map(|treasure| Vec2::new(treasure.position.0, treasure.position.1))
                .collect::<Vec<_>>();
            let spawned = coin_spawner.replenish_balanced_avoiding_anomalies(
                &live_positions,
                self.current_tick,
                &world.anomalies,
            );
            world
                .treasures
                .extend(spawned.into_iter().map(TreasureState::from));
        }

        // Синхронизация игроков с обновленным состоянием ковров флота
        for player in world.players.values_mut() {
            if !player.carpets.is_empty() {
                player.sync_from_carpets();
            }
        }

        self.previous_positions.clear();
        for player in world.players.values() {
            if player.carpets.is_empty() {
                self.previous_positions
                    .insert(player.id.clone(), Vec2::from_tuple(player.position));
            } else {
                for carpet in player.carpets.values() {
                    self.previous_positions
                        .insert(carpet.id.clone(), Vec2::from_tuple(carpet.position));
                }
            }
        }

        // Инкремент локального счетчика тика симуляции
        self.current_tick += 1;
    }
}

fn point_segment_distance_squared(point: Vec2, start: Vec2, end: Vec2) -> f64 {
    let segment = end - start;
    let length_squared = segment.length_squared();
    let fraction = if length_squared <= f64::EPSILON {
        0.0
    } else {
        ((point - start).dot(segment) / length_squared).clamp(0.0, 1.0)
    };
    point.distance_squared(start + segment * fraction)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::PlayerState;
    use crate::spatial::entities::AnomalyType;

    /// FE-003: test_anomaly_pull_direction
    /// Вектор силы направлен строго к центру вихря
    #[test]
    fn test_anomaly_pull_direction() {
        let anomaly_pos = Vec2::new(100.0, 100.0);
        let player_pos = Vec2::new(70.0, 60.0);
        let radius = 60.0;
        let force = 5.0;

        let pull = compute_anomaly_force(player_pos, anomaly_pos, radius, force);

        // Вектор разности (30, 40) с длиной 50 <= 60
        // Нормализованный вектор (0.6, 0.8)
        // Сила = (3.0, 4.0)
        assert!((pull.x - 3.0).abs() < 1e-9);
        assert!((pull.y - 4.0).abs() < 1e-9);
        assert!((pull.length() - force).abs() < 1e-9);
    }

    /// FE-003: test_anomaly_zero_force_outside_radius
    /// Вне зоны R_anomaly сила равна (0, 0)
    #[test]
    fn test_anomaly_zero_force_outside_radius() {
        let anomaly_pos = Vec2::new(100.0, 100.0);
        let player_pos = Vec2::new(200.0, 200.0); // расстояние ~141.4 > 50
        let pull = compute_anomaly_force(player_pos, anomaly_pos, 50.0, 10.0);
        assert_eq!(pull, Vec2::ZERO);
    }

    /// FE-003: test_treasure_collected_within_radius
    /// При d <= R_capture сокровище собирается и начисляются очки
    #[test]
    fn test_treasure_collected_within_radius() {
        let mut manager = EntityManager::new();
        let treasure = Treasure::new("t1", "chest", Vec2::new(10.0, 10.0), 50);
        manager.add_treasure(treasure);

        let player_pos = Vec2::new(10.0, 14.0); // расстояние 4.0 <= 5.0
        let score = manager.resolve_treasure_captures(player_pos, player_pos, 5.0);

        assert_eq!(score, 50);
        assert_eq!(manager.treasures.len(), 0); // удалено из пула
    }

    /// FE-003: test_treasure_not_collected_outside_radius
    /// При d > R_capture счет не меняется
    #[test]
    fn test_treasure_not_collected_outside_radius() {
        let mut manager = EntityManager::new();
        let treasure = Treasure::new("t1", "chest", Vec2::new(10.0, 10.0), 50);
        manager.add_treasure(treasure);

        let player_pos = Vec2::new(10.0, 16.0); // расстояние 6.0 > 5.0
        let score = manager.resolve_treasure_captures(player_pos, player_pos, 5.0);

        assert_eq!(score, 0);
        assert_eq!(manager.treasures.len(), 1); // осталось в пуле
    }

    /// Проверка обработки случая: игрок строго в центре аномалии
    #[test]
    fn test_center_of_anomaly_zero_force() {
        let anomaly_pos = Vec2::new(50.0, 50.0);
        let player_pos = Vec2::new(50.0, 50.0);
        let pull = compute_anomaly_force(player_pos, anomaly_pos, 100.0, 10.0);
        assert_eq!(pull, Vec2::ZERO);
    }

    /// Проверка сбора нескольких сокровищ в один тик
    #[test]
    fn test_multiple_treasures_sum_scores() {
        let mut manager = EntityManager::new();
        manager.add_treasure(Treasure::new("t1", "chest", Vec2::new(10.0, 10.0), 30));
        manager.add_treasure(Treasure::new("t2", "chest", Vec2::new(12.0, 10.0), 20));
        manager.add_treasure(Treasure::new("t3", "chest", Vec2::new(50.0, 50.0), 100));

        let player_pos = Vec2::new(11.0, 10.0);
        let score = manager.resolve_treasure_captures(player_pos, player_pos, 5.0);
        assert_eq!(score, 50); // 30 + 20
        assert_eq!(manager.treasures.len(), 1);
        assert_eq!(manager.treasures[0].id, "t3");
    }

    /// Проверка суперпозиции нескольких аномалий
    #[test]
    fn test_multiple_anomalies_superposition() {
        let mut manager = EntityManager::new();
        // Аномалия 1 тянет вправо: (10, 0) относительно игрока в (0, 0)
        manager.add_anomaly(Anomaly::new("a1", Vec2::new(10.0, 0.0), 20.0, 3.0));
        // Аномалия 2 тянет вверх: (0, 10) относительно игрока в (0, 0)
        manager.add_anomaly(Anomaly::new("a2", Vec2::new(0.0, 10.0), 20.0, 4.0));

        let forces = manager.compute_environmental_forces(Vec2::ZERO);
        assert!((forces.x - 3.0).abs() < 1e-9);
        assert!((forces.y - 4.0).abs() < 1e-9);
    }

    /// Проверка детекции попадания в эпицентр аномалии
    #[test]
    fn test_check_and_update_stuns() {
        let mut manager = EntityManager::new();
        // Аномалия с R = 100, stun_radius = 25% = 25
        manager.add_anomaly(Anomaly::new("a1", Vec2::new(50.0, 50.0), 100.0, 5.0));

        // В эпицентре (d = 10 <= 25)
        assert!(manager.check_and_update_stuns(Vec2::new(50.0, 60.0)));

        // Вне эпицентра, но внутри аномалии (d = 40 > 25, но <= 100)
        assert!(!manager.check_and_update_stuns(Vec2::new(50.0, 90.0)));
    }

    /// Проверка суперпозиции притягивающей и отталкивающей динамических аномалий
    #[test]
    fn test_manager_with_dynamic_anomalies_superposition() {
        let mut manager = EntityManager::new();
        // Аномалия 1 в (10.0, 0.0), Attracting, force = 4.0 -> тянет вправо (4, 0)
        manager.add_dynamic_anomaly(DynamicAnomaly::new(
            "dyn1",
            AnomalyType::Attracting,
            Vec2::new(10.0, 0.0),
            Vec2::ZERO,
            2.0,
            20.0,
            4.0,
        ));
        // Аномалия 2 в (-10.0, 0.0), Repelling, force = 3.0 -> толкает от себя вправо (3, 0)
        manager.add_dynamic_anomaly(DynamicAnomaly::new(
            "dyn2",
            AnomalyType::Repelling,
            Vec2::new(-10.0, 0.0),
            Vec2::ZERO,
            2.0,
            20.0,
            3.0,
        ));

        // Игрок в (0, 0) получает сумму сил: 4.0 + 3.0 = 7.0 по X
        let forces = manager.compute_environmental_forces(Vec2::ZERO);
        assert!((forces.x - 7.0).abs() < 1e-6);
        assert!(forces.y.abs() < 1e-6);
    }

    /// Проверка детекции контакта со смертоносным ядром через EntityManager
    #[test]
    fn test_manager_check_lethal_core_collisions() {
        let mut manager = EntityManager::new();
        manager.add_dynamic_anomaly(DynamicAnomaly::new(
            "hazard",
            AnomalyType::Attracting,
            Vec2::new(100.0, 100.0),
            Vec2::ZERO,
            15.0,
            60.0,
            5.0,
        ));

        let carpet_radius = 5.0; // r_total = 20.0

        // Касание ядра (d = 18.0 <= 20.0)
        assert!(manager.check_lethal_core_collisions(Vec2::new(100.0, 118.0), carpet_radius));

        // Вне ядра (d = 25.0 > 20.0)
        assert!(!manager.check_lethal_core_collisions(Vec2::new(100.0, 125.0), carpet_radius));
    }

    /// Проверка смещения аномалий и деспавна в EntityManager::advance_anomalies
    #[test]
    fn test_manager_advance_anomalies_lifecycle() {
        let mut manager = EntityManager::new().with_arena(500.0, 500.0);
        // Аномалия на выходе из арены (x = 550, moving right at vx = 20), R_effect = 40
        manager.add_dynamic_anomaly(DynamicAnomaly::new(
            "leaving",
            AnomalyType::Repelling,
            Vec2::new(530.0, 250.0),
            Vec2::new(20.0, 0.0),
            10.0,
            40.0,
            3.0,
        ));

        assert_eq!(manager.dynamic_anomalies.len(), 1);

        // Шаг dt = 1.0 -> позиция x = 550.0 > 500 + 40 -> деспавн
        manager.advance_anomalies(1.0);
        assert_eq!(manager.dynamic_anomalies.len(), 0);
    }

    /// Проверка, что в WorldSpatialEngine::resolve при касании ядра игрок помечается destroyed
    #[test]
    fn test_world_spatial_engine_resolve_eliminates_player() {
        let mut engine = WorldSpatialEngine::default();
        let mut world = WorldData::new();

        let mut player = PlayerState::new("player_victim".to_string(), 100.0, 100.0, 5.0, 20.0);
        player.velocity = (10.0, 0.0);
        world.players.insert(player.id.clone(), player);

        // Создаем динамическую аномалию с ядром R_core = 15.0 в точке (100.0, 110.0)
        // Дистанция = 10.0 <= 15.0 + 5.0 (player_radius)
        world.anomalies.push(AnomalyState::new_dynamic(
            "lethal_vortex",
            "attracting",
            (100.0, 110.0),
            (0.0, 0.0),
            15.0,
            60.0,
            4.0,
        ));

        // Выполняем шаг пространственного резолвера
        engine.resolve(&mut world, 0.2);

        let p = world.players.get("player_victim").unwrap();
        assert!(p.is_destroyed());
        assert_eq!(p.status, "destroyed");
        assert_eq!(p.velocity, (0.0, 0.0));
    }

    #[test]
    fn respawned_carpet_keeps_incremented_death_count() {
        let mut engine = WorldSpatialEngine::default().with_carpet_respawn(true);
        let mut world = WorldData::new();
        let mut player = PlayerState::new("wall_test".to_string(), 500.0, 500.0, 5.0, 20.0);
        player.carpets.get_mut("wall_test_0").unwrap().position = (-1.0, 500.0);
        world.players.insert(player.id.clone(), player);

        engine.resolve(&mut world, 0.2);

        let carpet = &world.players["wall_test"].carpets["wall_test_0"];
        assert_eq!(carpet.death_count, 1);
        assert_eq!(carpet.status, "normal");
        assert!((0.0..=engine.arena_width).contains(&carpet.position.0));
    }
}
