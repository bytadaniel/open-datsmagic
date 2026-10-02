//! # Пространственные сущности игрового мира DatsMagic
//!
//! Модуль определяет модели сокровищ, аномалий (вихрей) и игроков с поддержкой
//! 2D векторной математики [`Vec2`] в соответствии со спецификацией
//! [`FE-003`](file:///Users/d.byta/Documents/Code/dats/datsmagic/docs/features/FE-003-spatial-entities-and-collisions.md)
//! и требованиями [`DR-003`](file:///Users/d.byta/Documents/Code/dats/datsmagic/docs/domain/DR-003-entity-interactions-and-collisions.md).

use serde::{Deserialize, Serialize};

use crate::engine::state::{AnomalyState, CarpetState, PlayerState, TreasureState};
use crate::physics::Vec2;

#[inline]
fn is_false(val: &bool) -> bool {
    !*val
}

/// Сокровище на карте
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Treasure {
    /// Уникальный идентификатор сокровища
    pub id: String,
    /// Тип сокровища (например, "chest")
    pub r#type: String,
    /// Радиус-вектор положения в пространстве
    pub position: Vec2,
    /// Ценность сокровища в очках
    pub value: u32,
    /// Признак того, что сокровище уже захвачено
    #[serde(default, skip_serializing_if = "is_false")]
    pub is_collected: bool,
}

impl Treasure {
    /// Создает новое сокровище
    pub fn new(
        id: impl Into<String>,
        r#type: impl Into<String>,
        position: Vec2,
        value: u32,
    ) -> Self {
        Self {
            id: id.into(),
            r#type: r#type.into(),
            position,
            value,
            is_collected: false,
        }
    }

    /// Помечает сокровище собранным
    #[inline]
    pub fn collect(&mut self) {
        self.is_collected = true;
    }
}

impl From<TreasureState> for Treasure {
    fn from(state: TreasureState) -> Self {
        Self {
            id: state.id,
            r#type: state.r#type,
            position: Vec2::new(state.position.0, state.position.1),
            value: state.value,
            is_collected: state.is_collected,
        }
    }
}

impl From<&TreasureState> for Treasure {
    fn from(state: &TreasureState) -> Self {
        Self {
            id: state.id.clone(),
            r#type: state.r#type.clone(),
            position: Vec2::new(state.position.0, state.position.1),
            value: state.value,
            is_collected: state.is_collected,
        }
    }
}

impl From<Treasure> for TreasureState {
    fn from(t: Treasure) -> Self {
        Self {
            id: t.id,
            r#type: t.r#type,
            position: (t.position.x, t.position.y),
            value: t.value,
            is_collected: t.is_collected,
        }
    }
}

/// Аномалия / Песчаный вихрь
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Anomaly {
    /// Уникальный идентификатор аномалии
    pub id: String,
    /// Координаты эпицентра аномалии
    pub position: Vec2,
    /// Радиус зоны гравитационного притяжения
    pub radius: f64,
    /// Модуль силы притяжения ковров к центру
    pub force: f64,
}

impl Anomaly {
    /// Создает новую аномалию
    pub fn new(id: impl Into<String>, position: Vec2, radius: f64, force: f64) -> Self {
        Self {
            id: id.into(),
            position,
            radius,
            force,
        }
    }

    /// Проверяет, находится ли точка внутри радиуса действия аномалии
    #[inline]
    pub fn contains_point(&self, point: Vec2) -> bool {
        self.position.distance_squared(point) <= self.radius * self.radius
    }
}

impl From<AnomalyState> for Anomaly {
    fn from(state: AnomalyState) -> Self {
        Self {
            id: state.id,
            position: Vec2::new(state.position.0, state.position.1),
            radius: state.radius,
            force: state.force,
        }
    }
}

impl From<&AnomalyState> for Anomaly {
    fn from(state: &AnomalyState) -> Self {
        Self {
            id: state.id.clone(),
            position: Vec2::new(state.position.0, state.position.1),
            radius: state.radius,
            force: state.force,
        }
    }
}

impl From<Anomaly> for AnomalyState {
    fn from(a: Anomaly) -> Self {
        Self {
            id: a.id,
            anomaly_type: "attracting".to_string(),
            position: (a.position.x, a.position.y),
            velocity: (0.0, 0.0),
            core_radius: 0.0,
            radius: a.radius,
            force: a.force,
        }
    }
}

/// Тип силового воздействия аномалии на ковры (FE-006 / DR-005).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnomalyType {
    /// Притягивает ковры по направлению к своему ядру.
    Attracting,
    /// Отталкивает ковры по направлению от своего ядра.
    Repelling,
}

impl AnomalyType {
    /// Возвращает канонический строковый идентификатор типа
    #[inline]
    pub fn as_str(&self) -> &'static str {
        match self {
            AnomalyType::Attracting => "attracting",
            AnomalyType::Repelling => "repelling",
        }
    }
}

/// Динамическая пространственная аномалия (FE-006 / DR-005).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DynamicAnomaly {
    /// Уникальный идентификатор аномалии
    pub id: String,
    /// Тип силового воздействия (притяжение или отталкивание)
    pub anomaly_type: AnomalyType,
    /// Текущее пространственное положение центра вихря
    pub position: Vec2,
    /// Вектор постоянной линейной скорости перемещения
    pub velocity: Vec2,
    /// Радиус смертоносного ядра (касание ведет к немедленному уничтожению)
    pub core_radius: f64,
    /// Радиус внешней области силового воздействия
    pub effect_radius: f64,
    /// Сила силового воздействия на ковры
    pub force: f64,
}

impl DynamicAnomaly {
    /// Создает новую динамическую аномалию с валидацией радиусов
    pub fn new(
        id: impl Into<String>,
        anomaly_type: AnomalyType,
        position: Vec2,
        velocity: Vec2,
        core_radius: f64,
        effect_radius: f64,
        force: f64,
    ) -> Self {
        assert!(
            core_radius < effect_radius,
            "core_radius ({}) must be less than effect_radius ({})",
            core_radius,
            effect_radius
        );
        Self {
            id: id.into(),
            anomaly_type,
            position,
            velocity,
            core_radius,
            effect_radius,
            force,
        }
    }

    /// Смещение аномалии за один такт симуляции на шаг dt.
    #[inline]
    pub fn step(&mut self, dt: f64) {
        if !dt.is_finite() || dt <= 0.0 {
            return;
        }
        self.position += self.velocity * dt;
    }

    /// Проверка контакта ковра со смертоносным ядром (d <= R_core + R_carpet).
    #[inline]
    pub fn touches_core(&self, carpet_pos: Vec2, carpet_radius: f64) -> bool {
        if !carpet_pos.is_finite() || !carpet_radius.is_finite() || carpet_radius < 0.0 {
            return false;
        }
        let total_r = self.core_radius + carpet_radius;
        if total_r <= 0.0 {
            return false;
        }
        self.position.distance_squared(carpet_pos) <= total_r * total_r
    }

    /// Расчет силы, оказываемой аномалией на ковер.
    pub fn calculate_force(&self, carpet_pos: Vec2) -> Vec2 {
        if !carpet_pos.is_finite() || !self.position.is_finite() {
            return Vec2::ZERO;
        }
        let dist_sq = self.position.distance_squared(carpet_pos);
        let effect_sq = self.effect_radius * self.effect_radius;

        // Вне зоны действия сила равна нулю; при дистанциях <= 1e-6 избегаем деления на ноль
        if dist_sq > effect_sq || dist_sq <= 1e-6 {
            return Vec2::ZERO;
        }

        let radial_dir = (self.position - carpet_pos).normalize_or_zero();

        match self.anomaly_type {
            AnomalyType::Attracting => radial_dir * self.force,
            AnomalyType::Repelling => -radial_dir * self.force,
        }
    }
}

impl From<DynamicAnomaly> for AnomalyState {
    fn from(d: DynamicAnomaly) -> Self {
        Self {
            id: d.id,
            anomaly_type: d.anomaly_type.as_str().to_string(),
            position: (d.position.x, d.position.y),
            velocity: (d.velocity.x, d.velocity.y),
            core_radius: d.core_radius,
            radius: d.effect_radius,
            force: d.force,
        }
    }
}

impl From<&DynamicAnomaly> for AnomalyState {
    fn from(d: &DynamicAnomaly) -> Self {
        Self {
            id: d.id.clone(),
            anomaly_type: d.anomaly_type.as_str().to_string(),
            position: (d.position.x, d.position.y),
            velocity: (d.velocity.x, d.velocity.y),
            core_radius: d.core_radius,
            radius: d.effect_radius,
            force: d.force,
        }
    }
}

impl From<AnomalyState> for DynamicAnomaly {
    fn from(s: AnomalyState) -> Self {
        let anomaly_type = match s.anomaly_type.as_str() {
            "repelling" => AnomalyType::Repelling,
            _ => AnomalyType::Attracting,
        };
        Self {
            id: s.id,
            anomaly_type,
            position: Vec2::new(s.position.0, s.position.1),
            velocity: Vec2::new(s.velocity.0, s.velocity.1),
            core_radius: s.core_radius,
            effect_radius: s.radius,
            force: s.force,
        }
    }
}

impl From<&AnomalyState> for DynamicAnomaly {
    fn from(s: &AnomalyState) -> Self {
        let anomaly_type = match s.anomaly_type.as_str() {
            "repelling" => AnomalyType::Repelling,
            _ => AnomalyType::Attracting,
        };
        Self {
            id: s.id.clone(),
            anomaly_type,
            position: Vec2::new(s.position.0, s.position.1),
            velocity: Vec2::new(s.velocity.0, s.velocity.1),
            core_radius: s.core_radius,
            effect_radius: s.radius,
            force: s.force,
        }
    }
}

/// Состояние игрока
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlayerEntity {
    /// Идентификатор игрока / команды
    pub id: String,
    /// Текущее количество набранных очков
    pub score: u32,
    /// Статус ("normal" | "stunned")
    pub status: String,
    /// Координаты положения ковра
    pub position: Vec2,
    /// Текущая скорость ковра
    pub velocity: Vec2,
    /// Предельное управляющее ускорение
    pub max_acceleration: f64,
    /// Предельная скорость
    pub max_velocity: f64,
    /// Оставшееся количество тиков оглушения
    pub stun_remaining_ticks: u32,
}

impl PlayerEntity {
    /// Создает нового игрока со стандартными начальными параметрами
    pub fn new(id: impl Into<String>, position: Vec2, max_acc: f64, max_vel: f64) -> Self {
        Self {
            id: id.into(),
            score: 0,
            status: "normal".to_string(),
            position,
            velocity: Vec2::ZERO,
            max_acceleration: max_acc,
            max_velocity: max_vel,
            stun_remaining_ticks: 0,
        }
    }

    /// Проверяет, оглушен ли игрок
    #[inline]
    pub fn is_stunned(&self) -> bool {
        self.status == "stunned" || self.stun_remaining_ticks > 0
    }

    /// Проверяет, уничтожен ли ковер игрока
    #[inline]
    pub fn is_destroyed(&self) -> bool {
        self.status == "destroyed"
    }

    /// Помечает игрока перманентно уничтоженным и обнуляет скорость
    #[inline]
    pub fn mark_destroyed(&mut self) {
        self.status = "destroyed".to_string();
        self.velocity = Vec2::ZERO;
        self.stun_remaining_ticks = 0;
    }

    /// Оглушает игрока на заданное количество тиков (если игрок не уничтожен)
    pub fn apply_stun(&mut self, ticks: u32) {
        if self.is_destroyed() {
            return;
        }
        self.status = "stunned".to_string();
        self.stun_remaining_ticks = ticks;
    }

    /// Уменьшает таймер оглушения на 1 тик и восстанавливает статус при завершении
    pub fn tick_stun(&mut self) {
        if self.is_destroyed() {
            return;
        }
        if self.stun_remaining_ticks > 0 {
            self.stun_remaining_ticks -= 1;
            if self.stun_remaining_ticks == 0 {
                self.status = "normal".to_string();
            }
        } else if self.status == "stunned" {
            self.status = "normal".to_string();
        }
    }
}

impl From<PlayerState> for PlayerEntity {
    fn from(state: PlayerState) -> Self {
        Self {
            id: state.id,
            score: state.score,
            status: state.status,
            position: Vec2::new(state.position.0, state.position.1),
            velocity: Vec2::new(state.velocity.0, state.velocity.1),
            max_acceleration: state.max_acceleration,
            max_velocity: state.max_velocity,
            stun_remaining_ticks: state.stun_remaining_ticks,
        }
    }
}

impl From<&PlayerState> for PlayerEntity {
    fn from(state: &PlayerState) -> Self {
        Self {
            id: state.id.clone(),
            score: state.score,
            status: state.status.clone(),
            position: Vec2::new(state.position.0, state.position.1),
            velocity: Vec2::new(state.velocity.0, state.velocity.1),
            max_acceleration: state.max_acceleration,
            max_velocity: state.max_velocity,
            stun_remaining_ticks: state.stun_remaining_ticks,
        }
    }
}

impl From<PlayerEntity> for PlayerState {
    fn from(p: PlayerEntity) -> Self {
        Self {
            id: p.id,
            score: p.score,
            status: p.status,
            position: (p.position.x, p.position.y),
            velocity: (p.velocity.x, p.velocity.y),
            acceleration: (0.0, 0.0),
            max_acceleration: p.max_acceleration,
            max_velocity: p.max_velocity,
            stun_remaining_ticks: p.stun_remaining_ticks,
            carpets: std::collections::HashMap::new(),
        }
    }
}

/// Ковер-самолет как пространственная сущность (FE-010 / DR-006 / DR-007)
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CarpetEntity {
    /// Уникальный идентификатор ковра (`{player_id}_{index}`)
    pub id: String,
    /// Статус ("normal", "stunned", "destroyed")
    pub status: String,
    /// Координаты в пространстве
    pub position: Vec2,
    /// Вектор скорости
    pub velocity: Vec2,
    /// Максимальное ускорение
    pub max_acceleration: f64,
    /// Максимальная скорость
    pub max_velocity: f64,
    /// Оставшиеся тики оглушения
    pub stun_remaining_ticks: u32,
    /// Монотонный счетчик гибелей ковра
    #[serde(default)]
    pub death_count: u32,
}

impl CarpetEntity {
    /// Создает новую сущность ковра
    pub fn new(id: impl Into<String>, position: Vec2, max_acc: f64, max_vel: f64) -> Self {
        Self {
            id: id.into(),
            status: "normal".to_string(),
            position,
            velocity: Vec2::ZERO,
            max_acceleration: max_acc,
            max_velocity: max_vel,
            stun_remaining_ticks: 0,
            death_count: 0,
        }
    }

    /// Проверяет, оглушен ли ковер
    #[inline]
    pub fn is_stunned(&self) -> bool {
        self.status == "stunned" || self.stun_remaining_ticks > 0
    }

    /// Проверяет, уничтожен ли ковер
    #[inline]
    pub fn is_destroyed(&self) -> bool {
        self.status == "destroyed"
    }

    /// Помечает ковер перманентно уничтоженным
    #[inline]
    pub fn mark_destroyed(&mut self) {
        if !self.is_destroyed() {
            self.death_count = self.death_count.saturating_add(1);
        }
        self.status = "destroyed".to_string();
        self.velocity = Vec2::ZERO;
        self.stun_remaining_ticks = 0;
    }

    /// Накладывает оглушение на заданное количество тиков
    pub fn apply_stun(&mut self, ticks: u32) {
        if self.is_destroyed() {
            return;
        }
        self.status = "stunned".to_string();
        self.stun_remaining_ticks = ticks;
    }

    /// Уменьшает счетчик оглушения на 1 тик
    pub fn tick_stun(&mut self) {
        if self.is_destroyed() {
            return;
        }
        if self.stun_remaining_ticks > 0 {
            self.stun_remaining_ticks -= 1;
            if self.stun_remaining_ticks == 0 {
                self.status = "normal".to_string();
            }
        } else if self.status == "stunned" {
            self.status = "normal".to_string();
        }
    }
}

impl From<&CarpetState> for CarpetEntity {
    fn from(state: &CarpetState) -> Self {
        Self {
            id: state.id.clone(),
            status: state.status.clone(),
            position: Vec2::new(state.position.0, state.position.1),
            velocity: Vec2::new(state.velocity.0, state.velocity.1),
            max_acceleration: state.max_acceleration,
            max_velocity: state.max_velocity,
            stun_remaining_ticks: state.stun_remaining_ticks,
            death_count: state.death_count,
        }
    }
}

impl From<CarpetEntity> for CarpetState {
    fn from(c: CarpetEntity) -> Self {
        Self {
            id: c.id,
            status: c.status,
            position: (c.position.x, c.position.y),
            velocity: (c.velocity.x, c.velocity.y),
            acceleration: (0.0, 0.0),
            max_acceleration: c.max_acceleration,
            max_velocity: c.max_velocity,
            stun_remaining_ticks: c.stun_remaining_ticks,
            death_count: c.death_count,
        }
    }
}

/// Соперник / Противник в зоне видимости
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EnemyPlayer {
    /// Идентификатор противника
    pub id: String,
    /// Координаты положения
    pub position: Vec2,
    /// Вектор текущей скорости
    pub velocity: Vec2,
}

impl EnemyPlayer {
    /// Создает нового соперника
    pub fn new(id: impl Into<String>, position: Vec2, velocity: Vec2) -> Self {
        Self {
            id: id.into(),
            position,
            velocity,
        }
    }
}

impl From<&PlayerEntity> for EnemyPlayer {
    fn from(p: &PlayerEntity) -> Self {
        Self {
            id: p.id.clone(),
            position: p.position,
            velocity: p.velocity,
        }
    }
}

impl From<&PlayerState> for EnemyPlayer {
    fn from(p: &PlayerState) -> Self {
        Self {
            id: p.id.clone(),
            position: Vec2::new(p.position.0, p.position.1),
            velocity: Vec2::new(p.velocity.0, p.velocity.1),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_treasure_creation_and_conversion() {
        let t = Treasure::new("t1", "chest", Vec2::new(10.0, 20.0), 100);
        assert_eq!(t.id, "t1");
        assert_eq!(t.value, 100);
        assert!(!t.is_collected);

        let state: TreasureState = t.clone().into();
        assert_eq!(state.id, "t1");
        assert_eq!(state.position, (10.0, 20.0));

        let back: Treasure = state.into();
        assert_eq!(back, t);
    }

    #[test]
    fn test_anomaly_contains_point() {
        let anomaly = Anomaly::new("a1", Vec2::new(50.0, 50.0), 30.0, 4.0);
        assert!(anomaly.contains_point(Vec2::new(50.0, 60.0)));
        assert!(anomaly.contains_point(Vec2::new(50.0, 80.0))); // ровно граница d = 30
        assert!(!anomaly.contains_point(Vec2::new(50.0, 81.0))); // вне зоны
    }

    #[test]
    fn test_player_entity_stun_lifecycle() {
        let mut player = PlayerEntity::new("p1", Vec2::new(0.0, 0.0), 5.0, 20.0);
        assert!(!player.is_stunned());

        player.apply_stun(2);
        assert!(player.is_stunned());
        assert_eq!(player.stun_remaining_ticks, 2);

        player.tick_stun();
        assert!(player.is_stunned());
        assert_eq!(player.stun_remaining_ticks, 1);

        player.tick_stun();
        assert!(!player.is_stunned());
        assert_eq!(player.status, "normal");
        assert_eq!(player.stun_remaining_ticks, 0);
    }

    #[test]
    fn test_attracting_anomaly_force_direction() {
        let anomaly = DynamicAnomaly::new(
            "a_attract",
            AnomalyType::Attracting,
            Vec2::new(100.0, 100.0),
            Vec2::new(5.0, 0.0),
            15.0,
            60.0,
            4.0,
        );

        // Игрок находится в (70.0, 60.0) -> вектор смещения к ядру (30, 40), длина 50 <= 60
        let player_pos = Vec2::new(70.0, 60.0);
        let force = anomaly.calculate_force(player_pos);

        // Единичный вектор к центру: (0.6, 0.8), умноженный на силу 4.0: (2.4, 3.2)
        assert!((force.x - 2.4).abs() < 1e-6);
        assert!((force.y - 3.2).abs() < 1e-6);
        assert!((force.length() - 4.0).abs() < 1e-6);

        // Вне радиуса действия (d = 100 > 60) сила равна нулю
        let far_player = Vec2::new(0.0, 100.0);
        assert_eq!(anomaly.calculate_force(far_player), Vec2::ZERO);
    }

    #[test]
    fn test_repelling_anomaly_force_direction() {
        let anomaly = DynamicAnomaly::new(
            "a_repel",
            AnomalyType::Repelling,
            Vec2::new(100.0, 100.0),
            Vec2::new(-5.0, 2.0),
            15.0,
            60.0,
            4.0,
        );

        // Игрок находится в (70.0, 60.0) -> сила должна быть направлена ОТ ядра: (-2.4, -3.2)
        let player_pos = Vec2::new(70.0, 60.0);
        let force = anomaly.calculate_force(player_pos);

        assert!((force.x - (-2.4)).abs() < 1e-6);
        assert!((force.y - (-3.2)).abs() < 1e-6);
        assert!((force.length() - 4.0).abs() < 1e-6);
    }

    #[test]
    fn test_core_collision_triggers_destruction() {
        let anomaly = DynamicAnomaly::new(
            "a_lethal",
            AnomalyType::Attracting,
            Vec2::new(100.0, 100.0),
            Vec2::new(0.0, 0.0),
            15.0,
            60.0,
            5.0,
        );

        let carpet_radius = 5.0; // суммарный порог = 15 + 5 = 20.0

        // 1. Игрок внутри ядра (d = 10.0 <= 20.0) -> коллизия
        assert!(anomaly.touches_core(Vec2::new(100.0, 110.0), carpet_radius));

        // 2. Игрок ровно на границе ядра (d = 20.0 <= 20.0) -> коллизия
        assert!(anomaly.touches_core(Vec2::new(100.0, 120.0), carpet_radius));

        // 3. Игрок в зоне влияния, но снаружи ядра (d = 20.1 > 20.0) -> нет летальной коллизии
        assert!(!anomaly.touches_core(Vec2::new(100.0, 120.1), carpet_radius));
    }

    #[test]
    fn test_anomaly_ghosting_no_interference() {
        // Две аномалии, летящие навстречу друг другу
        let mut a1 = DynamicAnomaly::new(
            "a1",
            AnomalyType::Attracting,
            Vec2::new(0.0, 50.0),
            Vec2::new(10.0, 0.0),
            10.0,
            40.0,
            3.0,
        );
        let mut a2 = DynamicAnomaly::new(
            "a2",
            AnomalyType::Repelling,
            Vec2::new(100.0, 50.0),
            Vec2::new(-10.0, 0.0),
            10.0,
            40.0,
            3.0,
        );

        let dt = 1.0;
        // Шаг 1..10: смещаются сквозь друг друга без искажения траекторий
        for _ in 0..10 {
            a1.step(dt);
            a2.step(dt);
        }

        assert_eq!(a1.position, Vec2::new(100.0, 50.0));
        assert_eq!(a2.position, Vec2::new(0.0, 50.0));
        assert_eq!(a1.velocity, Vec2::new(10.0, 0.0));
        assert_eq!(a2.velocity, Vec2::new(-10.0, 0.0));
    }

    #[test]
    fn test_player_destruction_lifecycle() {
        let mut player = PlayerEntity::new("p_victim", Vec2::new(50.0, 50.0), 5.0, 20.0);
        player.velocity = Vec2::new(10.0, 5.0);
        assert!(!player.is_destroyed());

        player.mark_destroyed();
        assert!(player.is_destroyed());
        assert_eq!(player.velocity, Vec2::ZERO);

        // Оглушение или тики оглушения не могут сбросить статус destroyed
        player.apply_stun(5);
        assert_eq!(player.status, "destroyed");
        assert!(!player.is_stunned());

        player.tick_stun();
        assert_eq!(player.status, "destroyed");
    }

    #[test]
    fn test_dynamic_anomaly_conversions() {
        let dyn_anom = DynamicAnomaly::new(
            "a_conv",
            AnomalyType::Repelling,
            Vec2::new(12.0, 34.0),
            Vec2::new(-1.0, 2.0),
            10.0,
            50.0,
            4.5,
        );

        let state: AnomalyState = dyn_anom.clone().into();
        assert_eq!(state.id, "a_conv");
        assert_eq!(state.anomaly_type, "repelling");
        assert_eq!(state.position, (12.0, 34.0));
        assert_eq!(state.velocity, (-1.0, 2.0));
        assert_eq!(state.core_radius, 10.0);
        assert_eq!(state.radius, 50.0);
        assert_eq!(state.force, 4.5);

        let restored: DynamicAnomaly = state.into();
        assert_eq!(restored, dyn_anom);
    }
}
