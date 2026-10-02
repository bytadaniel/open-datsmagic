//! # Состояние игрового мира и сессии
//!
//! Определяет структуры данных текущего состояния симуляции, статусы сессии
//! и неизменяемые снапшоты для отдачи клиентам в соответствии со спецификацией
//! [`FE-001`](file:///Users/d.byta/Documents/Code/dats/datsmagic/docs/features/FE-001-server-runtime-and-game-loop.md)
//! и [`DR-001`](file:///Users/d.byta/Documents/Code/dats/datsmagic/docs/domain/DR-001-game-loop-and-state.md).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use super::command_buffer::PlayerId;

/// Статус жизненного цикла игровой сессии
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    /// Сессия активна, тики генерируются, команды применяются
    #[default]
    Active,
    /// Сессия приостановлена администратором, время остановлено
    Paused,
    /// Сессия завершена (по истечении времени или лимита тиков)
    Finished,
}

/// Статус состояния игрока (команды) в соответствии с DR-005 и FE-006
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlayerStatus {
    /// Обычный режим управления
    Normal,
    /// Оглушен в эпицентре вихря (управление заблокировано)
    Stunned,
    /// Уничтожен при контакте со смертоносным ядром аномалии
    Destroyed,
}

impl PlayerStatus {
    /// Возвращает строковое представление статуса
    #[inline]
    pub fn as_str(&self) -> &'static str {
        match self {
            PlayerStatus::Normal => "normal",
            PlayerStatus::Stunned => "stunned",
            PlayerStatus::Destroyed => "destroyed",
        }
    }
}

/// Количество ковров во флоте каждого игрока (FE-010 / DR-007)
pub const FLEET_CARPETS_COUNT: usize = 5;
pub const MAX_FLEET_CARPETS_COUNT: usize = 10;

/// Состояние отдельного ковра-самолета из флота игрока (FE-010 / DR-007)
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CarpetState {
    /// Уникальный идентификатор ковра (`{player_id}_{index}`)
    pub id: String,
    /// Статус ("normal", "stunned", "destroyed")
    pub status: String,
    /// Радиус-вектор положения (x, y)
    pub position: (f64, f64),
    /// Вектор текущей скорости (vx, vy)
    pub velocity: (f64, f64),
    /// Эффективный вектор управляющего ускорения после ограничения и stunned
    #[serde(default)]
    pub acceleration: (f64, f64),
    /// Предельное управляющее ускорение
    pub max_acceleration: f64,
    /// Предельная скорость
    pub max_velocity: f64,
    /// Оставшееся число тиков оглушения
    pub stun_remaining_ticks: u32,
    /// Монотонный счетчик гибелей ковра за время жизни его ID
    #[serde(default)]
    pub death_count: u32,
    /// Текущий личный баланс золота ковра.
    #[serde(default)]
    pub gold: u32,
    /// Суммарный номинал собранных этим ковром монет (не уменьшается при гибели).
    #[serde(default)]
    pub gold_collected_total: u64,
}

impl CarpetState {
    /// Создает новый ковер во флоте
    pub fn new(id: impl Into<String>, pos_x: f64, pos_y: f64, max_acc: f64, max_vel: f64) -> Self {
        Self {
            id: id.into(),
            status: "normal".to_string(),
            position: (pos_x, pos_y),
            velocity: (0.0, 0.0),
            acceleration: (0.0, 0.0),
            max_acceleration: max_acc,
            max_velocity: max_vel,
            stun_remaining_ticks: 0,
            death_count: 0,
            gold: 0,
            gold_collected_total: 0,
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

    /// Помечает ковер перманентно уничтоженным и обнуляет скорость
    #[inline]
    pub fn mark_destroyed(&mut self) {
        if !self.is_destroyed() {
            self.death_count = self.death_count.saturating_add(1);
        }
        self.status = "destroyed".to_string();
        self.velocity = (0.0, 0.0);
        self.acceleration = (0.0, 0.0);
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

#[cfg(test)]
mod tests {
    use super::{CarpetState, PlayerState};

    #[test]
    fn player_profile_can_create_a_single_carpet() {
        let player =
            PlayerState::new_with_carpet_count("one-life".into(), 0.0, 0.0, 40.0, 110.0, 1);
        assert_eq!(player.carpets.len(), 1);
        assert!(player.carpets.contains_key("one-life_0"));
    }

    #[test]
    fn player_profile_can_create_ten_carpets() {
        let player = PlayerState::new_with_carpet_count("king".into(), 0.0, 0.0, 40.0, 110.0, 10);
        assert_eq!(player.carpets.len(), 10);
        assert!(player.carpets.contains_key("king_9"));
    }

    #[test]
    fn carpet_death_count_increments_once_per_life_and_survives_respawn_status_reset() {
        let mut carpet = CarpetState::new("p_0", 0.0, 0.0, 40.0, 110.0);
        carpet.mark_destroyed();
        carpet.mark_destroyed();
        assert_eq!(carpet.death_count, 1);

        // Same-tick respawn resets status/physics, not the lifetime counter.
        carpet.status = "normal".into();
        carpet.position = (100.0, 100.0);
        carpet.mark_destroyed();
        assert_eq!(carpet.death_count, 2);
    }
}

/// Состояние игрока и его флота ковров-самолетов (FE-010 / DR-007)
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlayerState {
    /// Уникальный идентификатор команды / игрока
    pub id: PlayerId,
    /// Набранные очки
    pub score: u32,
    /// Накопленный за сессию номинал монет, собранных всеми коврами.
    #[serde(default)]
    pub gold_collected_total: u64,
    /// Суммарная длина фактических перемещений ковров команды, без respawn teleport.
    #[serde(default)]
    pub distance_travelled: f64,
    /// Статус ("normal", "stunned", "destroyed")
    pub status: String,
    /// Радиус-вектор положения (x, y) первого ковра
    pub position: (f64, f64),
    /// Вектор текущей скорости (vx, vy) первого ковра
    pub velocity: (f64, f64),
    /// Эффективный вектор управляющего ускорения первого ковра
    #[serde(default)]
    pub acceleration: (f64, f64),
    /// Предельное ускорение ковра
    pub max_acceleration: f64,
    /// Предельная скорость ковра
    pub max_velocity: f64,
    /// Оставшееся число тиков оглушения
    pub stun_remaining_ticks: u32,
    /// Флот ковров игрока; его размер задаёт профиль мира (FE-010)
    #[serde(default)]
    pub carpets: HashMap<String, CarpetState>,
}

impl PlayerState {
    /// Создаёт игрока со стандартным флотом из пяти ковров.
    pub fn new(id: PlayerId, pos_x: f64, pos_y: f64, max_acc: f64, max_vel: f64) -> Self {
        Self::new_with_carpet_count(id, pos_x, pos_y, max_acc, max_vel, FLEET_CARPETS_COUNT)
    }

    /// Создает игрока с размером флота, заданным профилем мира.
    pub fn new_with_carpet_count(
        id: PlayerId,
        pos_x: f64,
        pos_y: f64,
        max_acc: f64,
        max_vel: f64,
        carpet_count: usize,
    ) -> Self {
        let mut carpets = HashMap::new();
        for i in 0..carpet_count.min(MAX_FLEET_CARPETS_COUNT) {
            // Спираль Ферма: компактный флот без прямоугольной решетки.
            let radius = 18.0 * (i as f64).sqrt();
            let angle = i as f64 * 2.399_963_229_728_653;
            let dx = radius * angle.cos();
            let dy = radius * angle.sin();
            let carpet_id = format!("{}_{}", id, i);
            let carpet =
                CarpetState::new(carpet_id.clone(), pos_x + dx, pos_y + dy, max_acc, max_vel);
            carpets.insert(carpet_id, carpet);
        }

        Self {
            id,
            score: 0,
            gold_collected_total: 0,
            distance_travelled: 0.0,
            status: "normal".to_string(),
            position: (pos_x, pos_y),
            velocity: (0.0, 0.0),
            acceleration: (0.0, 0.0),
            max_acceleration: max_acc,
            max_velocity: max_vel,
            stun_remaining_ticks: 0,
            carpets,
        }
    }

    /// Проверяет, оглушен ли игрок (или все его живые ковры)
    #[inline]
    pub fn is_stunned(&self) -> bool {
        self.status == "stunned" || self.stun_remaining_ticks > 0
    }

    /// Проверяет, уничтожен ли ковер / флот игрока
    #[inline]
    pub fn is_destroyed(&self) -> bool {
        self.status == "destroyed"
            || (!self.carpets.is_empty() && self.carpets.values().all(|c| c.is_destroyed()))
    }

    /// Помечает игрока и все его ковры перманентно уничтоженными
    #[inline]
    pub fn mark_destroyed(&mut self) {
        self.status = "destroyed".to_string();
        self.velocity = (0.0, 0.0);
        self.acceleration = (0.0, 0.0);
        for carpet in self.carpets.values_mut() {
            carpet.mark_destroyed();
        }
    }

    /// Синхронизирует базовые атрибуты игрока из основного ковра (индекс 0)
    pub fn sync_from_carpets(&mut self) {
        let c0_id = format!("{}_0", self.id);
        if let Some(c0) = self
            .carpets
            .get(&c0_id)
            .or_else(|| self.carpets.values().next())
        {
            self.position = c0.position;
            self.velocity = c0.velocity;
            self.acceleration = c0.acceleration;
            self.status = c0.status.clone();
            self.stun_remaining_ticks = c0.stun_remaining_ticks;
        }
        if !self.carpets.is_empty() && self.carpets.values().all(|c| c.is_destroyed()) {
            self.status = "destroyed".to_string();
            self.velocity = (0.0, 0.0);
            self.acceleration = (0.0, 0.0);
        }
    }

    /// Синхронизирует позицию и скорость первого ковра из общих полей игрока
    pub fn sync_primary_carpet(&mut self) {
        let c0_id = format!("{}_0", self.id);
        if let Some(c0) = self.carpets.get_mut(&c0_id) {
            c0.position = self.position;
            c0.velocity = self.velocity;
            c0.acceleration = self.acceleration;
            c0.status = self.status.clone();
            c0.stun_remaining_ticks = self.stun_remaining_ticks;
        }
    }
}

/// Сокровище на карте
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TreasureState {
    /// Уникальный идентификатор сокровища
    pub id: String,
    /// Тип сокровища (например, "chest")
    pub r#type: String,
    /// Координаты положения (x, y)
    pub position: (f64, f64),
    /// Ценность сокровища в очках
    pub value: u32,
    /// Флаг собранности
    pub is_collected: bool,
}

#[inline]
fn default_attracting_str() -> String {
    "attracting".to_string()
}

/// Аномалия / Песчаный вихрь (поддерживает статичные и подвижные вихри FE-006)
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AnomalyState {
    /// Уникальный идентификатор аномалии
    pub id: String,
    /// Тип воздействия ("attracting" | "repelling")
    #[serde(default = "default_attracting_str")]
    pub anomaly_type: String,
    /// Координаты центра вихря (x, y)
    pub position: (f64, f64),
    /// Вектор скорости перемещения (vx, vy)
    #[serde(default)]
    pub velocity: (f64, f64),
    /// Радиус смертоносного ядра
    #[serde(default)]
    pub core_radius: f64,
    /// Радиус зоны воздействия вихря (R_effect)
    pub radius: f64,
    /// Сила воздействия вихря на ковры
    pub force: f64,
}

impl Default for AnomalyState {
    fn default() -> Self {
        Self {
            id: String::new(),
            anomaly_type: "attracting".to_string(),
            position: (0.0, 0.0),
            velocity: (0.0, 0.0),
            core_radius: 0.0,
            radius: 0.0,
            force: 0.0,
        }
    }
}

impl AnomalyState {
    /// Создает статичную аномалию (обратная совместимость с FE-003)
    pub fn new(id: impl Into<String>, position: (f64, f64), radius: f64, force: f64) -> Self {
        Self {
            id: id.into(),
            anomaly_type: "attracting".to_string(),
            position,
            velocity: (0.0, 0.0),
            core_radius: 0.0,
            radius,
            force,
        }
    }

    /// Создает динамическую аномалию со всеми характеристиками FE-006
    pub fn new_dynamic(
        id: impl Into<String>,
        anomaly_type: impl Into<String>,
        position: (f64, f64),
        velocity: (f64, f64),
        core_radius: f64,
        radius: f64,
        force: f64,
    ) -> Self {
        Self {
            id: id.into(),
            anomaly_type: anomaly_type.into(),
            position,
            velocity,
            core_radius,
            radius,
            force,
        }
    }
}

/// Агрегированные данные игрового мира
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct WorldData {
    /// Все зарегистрированные в мире игроки
    pub players: HashMap<PlayerId, PlayerState>,
    /// Все сокровища на карте
    pub treasures: Vec<TreasureState>,
    /// Все активные аномалии на карте
    pub anomalies: Vec<AnomalyState>,
}

impl WorldData {
    /// Создает новый пустой игровой мир
    pub fn new() -> Self {
        Self::default()
    }
}

/// Неизменяемый снимок состояния мира на определенном тике симуляции
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WorldSnapshot {
    /// Номер тика, на котором был зафиксирован снапшот
    pub tick: u64,
    /// Статус сессии на момент снапшота
    pub game_status: SessionStatus,
    /// Данные сущностей мира
    pub world: WorldData,
}

/// Полный изменяемый контекст симуляции в ОЗУ
#[derive(Debug)]
pub struct GameState {
    /// Монотонно возрастающий счетчик тиков
    pub tick: u64,
    /// Текущий статус сессии
    pub status: SessionStatus,
    /// Данные игрового мира
    pub world: WorldData,
}

impl GameState {
    /// Создает новое начальное состояние с нулевым тиком
    pub fn new() -> Self {
        Self {
            tick: 0,
            status: SessionStatus::Active,
            world: WorldData::new(),
        }
    }

    /// Формирует неизменяемый снимок [`WorldSnapshot`] на основе текущего состояния
    pub fn to_snapshot(&self) -> WorldSnapshot {
        WorldSnapshot {
            tick: self.tick,
            game_status: self.status,
            world: self.world.clone(),
        }
    }
}

impl Default for GameState {
    fn default() -> Self {
        Self::new()
    }
}

/// Потокобезопасный разделяемый указатель на состояние игры
pub type SharedGameState = Arc<RwLock<GameState>>;
