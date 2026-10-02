//! # DTO (Data Transfer Objects) для сетевого REST API
//!
//! Определяет сериализуемые структуры данных для запросов и ответов
//! HTTP REST API симулятора DatsMagic в соответствии со спецификацией
//! [`FE-004`](file:///Users/d.byta/Documents/Code/dats/datsmagic/docs/features/FE-004-simulation-rest-api.md),
//! [`FE-007`](file:///Users/d.byta/Documents/Code/dats/datsmagic/docs/features/FE-007-anomalies-rest-api-contract.md)
//! и контрактами [`docs/mechanics.md`](file:///Users/d.byta/Documents/Code/dats/datsmagic/docs/mechanics.md#4-контракты-api-спецификация-json).

use serde::{Deserialize, Serialize};

use crate::engine::state::AnomalyState;
use crate::physics::Vec2;
use crate::spatial::{Anomaly, DynamicAnomaly, Treasure};

/// DTO двухмерного вектора (FE-007).
pub type Vector2DDto = Vec2;

/// DTO сокровища на карте (FE-007).
pub type TreasureDto = Treasure;

/// Запрос команды игрока (POST /api/carpet/command)
#[derive(Clone, Copy, Debug, PartialEq, Deserialize, Serialize)]
pub struct CommandRequestDto {
    /// Вектор управляющего ускорения
    pub acceleration: Vec2,
}

/// Элемент пакетной команды для отдельного ковра флота (FE-010)
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct CarpetCommandItemDto {
    /// Идентификатор ковра (`{player_id}_{index}`)
    pub carpet_id: String,
    /// Вектор управляющего ускорения
    pub acceleration: Vec2,
}

/// Пакетный запрос команд для флота ковров (POST /api/carpet/command[s]) (FE-010)
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct BatchCarpetCommandRequestDto {
    /// Список команд для ковров флота
    pub commands: Vec<CarpetCommandItemDto>,
}

/// Канонический запрос внешнего API `POST /play/magcarp/player/move`.
///
/// Имена полей намеренно совпадают с `datsmagic/src/api/move.ts`.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct LegacyMoveRequestDto {
    pub transports: Vec<LegacyTransportCommandDto>,
}

/// Команда отдельного транспорта внешнего API.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LegacyTransportCommandDto {
    pub id: String,
    pub acceleration: Option<Vector2DDto>,
    pub activate_shield: Option<bool>,
    pub attack: Option<serde_json::Value>,
}

/// Точный ответ `Desert` референсного клиента в `./datsmagic`.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LegacyDesertDto {
    pub errors: Vec<String>,
    pub anomalies: Vec<LegacyAnomalyDto>,
    pub attack_cooldown_ms: u64,
    pub attack_damage: u32,
    pub attack_explosion_radius: f64,
    pub attack_range: f64,
    pub bounties: Vec<LegacyBountyDto>,
    pub enemies: Vec<LegacyUnitDto>,
    pub map_size: Vector2DDto,
    pub max_accel: f64,
    pub max_speed: f64,
    pub name: String,
    pub points: u32,
    pub revive_timeout_sec: u64,
    pub shield_cooldown_ms: u64,
    pub shield_time_ms: u64,
    pub transport_radius: f64,
    pub transports: Vec<LegacyTransportDto>,
    pub wanted_list: Vec<LegacyUnitDto>,
}

/// Аномалия в форме, заданной внешним контрактом.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LegacyAnomalyDto {
    pub effective_radius: f64,
    pub id: String,
    pub radius: f64,
    pub strength: f64,
    pub velocity: Vector2DDto,
    pub x: f64,
    pub y: f64,
}

/// Монета (`Bounty`) в форме внешнего контракта.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct LegacyBountyDto {
    pub points: u32,
    pub radius: f64,
    pub x: f64,
    pub y: f64,
}

/// Видимый противник или цель розыска внешнего API.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LegacyUnitDto {
    pub health: u32,
    pub kill_bounty: u32,
    pub shield_left_ms: u64,
    pub status: String,
    pub velocity: Vector2DDto,
    pub x: f64,
    pub y: f64,
}

/// Собственный транспорт внешнего API с тремя векторами телеметрии.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LegacyTransportDto {
    pub anomaly_acceleration: Vector2DDto,
    pub attack_cooldown_ms: u64,
    /// Суммарное число гибелей этого ковра за все респавны его стабильного ID.
    pub death_count: u32,
    pub health: u32,
    pub id: String,
    pub self_acceleration: Vector2DDto,
    pub shield_cooldown_ms: u64,
    pub shield_left_ms: u64,
    pub status: String,
    pub velocity: Vector2DDto,
    pub x: f64,
    pub y: f64,
}

/// Подтверждение приема команды (200 OK)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct CommandResponseDto {
    /// Статус принятия команды ("accepted")
    pub status: &'static str,
    /// Количество принятых команд (при пакетной отправке)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commands_count: Option<usize>,
}

impl Default for CommandResponseDto {
    fn default() -> Self {
        Self {
            status: "accepted",
            commands_count: None,
        }
    }
}

/// DTO динамической аномалии для сетевого протокола (FE-007).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AnomalyDto {
    /// Уникальный идентификатор аномалии
    pub id: String,
    /// Тип силового воздействия ("attracting" | "repelling")
    #[serde(rename = "type")]
    pub anomaly_type: String,
    /// Координаты эпицентра аномалии
    pub position: Vector2DDto,
    /// Вектор постоянной линейной скорости перемещения
    pub velocity: Vector2DDto,
    /// Радиус смертоносного ядра (касание ведет к немедленному уничтожению)
    pub core_radius: f64,
    /// Радиус внешней зоны силового воздействия
    pub radius: f64,
    /// Модуль силы воздействия на ковры
    pub force: f64,
}

impl AnomalyDto {
    /// Создает новый объект DTO аномалии со всеми характеристиками
    pub fn new(
        id: impl Into<String>,
        anomaly_type: impl Into<String>,
        position: Vector2DDto,
        velocity: Vector2DDto,
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

impl From<AnomalyState> for AnomalyDto {
    fn from(state: AnomalyState) -> Self {
        Self::from(&state)
    }
}

impl From<&AnomalyState> for AnomalyDto {
    fn from(state: &AnomalyState) -> Self {
        Self {
            id: state.id.clone(),
            anomaly_type: if state.anomaly_type.is_empty() {
                "attracting".to_string()
            } else {
                state.anomaly_type.clone()
            },
            position: Vec2::new(state.position.0, state.position.1),
            velocity: Vec2::new(state.velocity.0, state.velocity.1),
            core_radius: state.core_radius,
            radius: state.radius,
            force: state.force,
        }
    }
}

impl From<DynamicAnomaly> for AnomalyDto {
    fn from(a: DynamicAnomaly) -> Self {
        Self::from(&a)
    }
}

impl From<&DynamicAnomaly> for AnomalyDto {
    fn from(a: &DynamicAnomaly) -> Self {
        Self {
            id: a.id.clone(),
            anomaly_type: a.anomaly_type.as_str().to_string(),
            position: a.position,
            velocity: a.velocity,
            core_radius: a.core_radius,
            radius: a.effect_radius,
            force: a.force,
        }
    }
}

impl From<Anomaly> for AnomalyDto {
    fn from(a: Anomaly) -> Self {
        Self::from(&a)
    }
}

impl From<&Anomaly> for AnomalyDto {
    fn from(a: &Anomaly) -> Self {
        Self {
            id: a.id.clone(),
            anomaly_type: "attracting".to_string(),
            position: a.position,
            velocity: Vec2::ZERO,
            core_radius: 0.0,
            radius: a.radius,
            force: a.force,
        }
    }
}

impl From<AnomalyDto> for AnomalyState {
    fn from(dto: AnomalyDto) -> Self {
        Self {
            id: dto.id,
            anomaly_type: dto.anomaly_type,
            position: (dto.position.x, dto.position.y),
            velocity: (dto.velocity.x, dto.velocity.y),
            core_radius: dto.core_radius,
            radius: dto.radius,
            force: dto.force,
        }
    }
}

/// Снимок состояния для ответа GET /api/game/state
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GameStateResponseDto {
    /// Номер текущего игрового тика
    pub tick: u64,
    /// Статус игровой сессии ("active", "paused", "finished")
    pub game_status: String,
    /// Состояние управляемого ковра игрока
    pub player: PlayerDto,
    /// Доступные сокровища на карте
    pub treasures: Vec<Treasure>,
    /// Активные динамические аномалии / вихри (FE-007)
    pub anomalies: Vec<AnomalyDto>,
    /// Обнаруженные противники
    pub enemies: Vec<EnemyDto>,
}

/// DTO отдельного ковра-самолета во флоте игрока (FE-010)
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CarpetDto {
    /// Идентификатор ковра (`{player_id}_{index}`)
    pub id: String,
    /// Статус ("normal", "stunned", "destroyed")
    pub status: String,
    /// Текущие координаты
    pub position: Vector2DDto,
    /// Текущий вектор скорости
    pub velocity: Vector2DDto,
    /// Эффективный вектор управляющего ускорения
    pub acceleration: Vector2DDto,
    /// Максимальный модуль вектора ускорения
    pub max_acceleration: f64,
    /// Максимальный модуль вектора скорости
    pub max_velocity: f64,
}

/// DTO ковра противника (FE-010)
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EnemyCarpetDto {
    /// Идентификатор ковра
    pub id: String,
    /// Текущие координаты
    pub position: Vector2DDto,
    /// Текущий вектор скорости
    pub velocity: Vector2DDto,
    /// Эффективный вектор управляющего ускорения
    pub acceleration: Vector2DDto,
}

/// Состояние игрока и его флота (FE-007 / FE-010)
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlayerDto {
    /// Идентификатор игрока / команды
    pub id: String,
    /// Набранные очки
    pub score: u32,
    /// Статус ("normal", "stunned" или "destroyed")
    pub status: String,
    /// Текущие координаты (первичного ковра флота)
    pub position: Vector2DDto,
    /// Текущий вектор скорости (первичного ковра флота)
    pub velocity: Vector2DDto,
    /// Эффективный вектор управляющего ускорения первичного ковра
    pub acceleration: Vector2DDto,
    /// Максимальный модуль вектора ускорения
    pub max_acceleration: f64,
    /// Максимальный модуль вектора скорости
    pub max_velocity: f64,
    /// Весь флот игрока; размер задаёт профиль мира (FE-010)
    #[serde(default)]
    pub carpets: Vec<CarpetDto>,
}

impl PlayerDto {
    /// Проверяет, оглушен ли игрок
    #[inline]
    pub fn is_stunned(&self) -> bool {
        self.status == "stunned"
    }

    /// Проверяет, уничтожен ли ковер игрока
    #[inline]
    pub fn is_destroyed(&self) -> bool {
        self.status == "destroyed"
    }
}

/// Противник и его флот ковров в зоне видимости (FE-010)
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EnemyDto {
    /// Идентификатор противника
    pub id: String,
    /// Текущие координаты
    pub position: Vector2DDto,
    /// Текущий вектор скорости
    pub velocity: Vector2DDto,
    /// Эффективный вектор управляющего ускорения
    pub acceleration: Vector2DDto,
    /// Флот ковров противника (FE-010)
    #[serde(default)]
    pub carpets: Vec<EnemyCarpetDto>,
}

/// Формат возвращаемых ошибок API
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorResponseDto {
    /// Текстовое описание ошибки
    pub error: String,
}

impl ErrorResponseDto {
    /// Создает новый объект ошибки
    pub fn new(error: impl Into<String>) -> Self {
        Self {
            error: error.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dto_serialization_format() {
        let response = GameStateResponseDto {
            tick: 1042,
            game_status: "active".to_string(),
            player: PlayerDto {
                id: "team_20".to_string(),
                score: 1420,
                status: "normal".to_string(),
                position: Vec2::new(412.5, 890.2),
                velocity: Vec2::new(8.5, -4.1),
                acceleration: Vec2::new(1.0, 0.0),
                max_acceleration: 5.0,
                max_velocity: 20.0,
                carpets: vec![CarpetDto {
                    id: "team_20_0".to_string(),
                    status: "normal".to_string(),
                    position: Vec2::new(412.5, 890.2),
                    velocity: Vec2::new(8.5, -4.1),
                    acceleration: Vec2::new(1.0, 0.0),
                    max_acceleration: 5.0,
                    max_velocity: 20.0,
                }],
            },
            treasures: vec![Treasure::new("t_99", "chest", Vec2::new(450.0, 920.0), 50)],
            anomalies: vec![AnomalyDto::new(
                "a_3",
                "attracting",
                Vec2::new(400.0, 900.0),
                Vec2::new(5.0, -1.2),
                15.0,
                40.0,
                3.5,
            )],
            enemies: vec![EnemyDto {
                id: "team_5".to_string(),
                position: Vec2::new(430.0, 910.0),
                velocity: Vec2::new(12.0, 2.1),
                acceleration: Vec2::ZERO,
                carpets: vec![EnemyCarpetDto {
                    id: "team_5_0".to_string(),
                    position: Vec2::new(430.0, 910.0),
                    velocity: Vec2::new(12.0, 2.1),
                    acceleration: Vec2::ZERO,
                }],
            }],
        };

        let json_str = serde_json::to_string(&response).expect("Failed to serialize DTO");
        assert!(json_str.contains("\"tick\":1042"));
        assert!(json_str.contains("\"game_status\":\"active\""));
        assert!(json_str.contains("\"id\":\"team_20\""));
        assert!(json_str.contains("\"carpets\":["));
        assert!(json_str.contains("\"team_20_0\""));
        assert!(json_str.contains("\"team_5_0\""));
        assert!(json_str.contains("\"type\":\"attracting\""));
        assert!(json_str.contains("\"core_radius\":15.0"));
        assert!(json_str.contains("\"velocity\":{\"x\":5.0,\"y\":-1.2}"));
        assert!(!json_str.contains("is_collected")); // не должно сериализоваться для uncollected

        let deserialized: GameStateResponseDto =
            serde_json::from_str(&json_str).expect("Failed to deserialize DTO");
        assert_eq!(deserialized, response);
    }

    #[test]
    fn test_batch_carpet_commands_dto_serialization() {
        let batch = BatchCarpetCommandRequestDto {
            commands: vec![
                CarpetCommandItemDto {
                    carpet_id: "team_alpha_0".to_string(),
                    acceleration: Vec2::new(2.0, 1.0),
                },
                CarpetCommandItemDto {
                    carpet_id: "team_alpha_1".to_string(),
                    acceleration: Vec2::new(-1.5, 3.0),
                },
            ],
        };

        let json = serde_json::to_string(&batch).unwrap();
        assert!(json.contains("team_alpha_0"));
        assert!(json.contains("team_alpha_1"));

        let deserialized: BatchCarpetCommandRequestDto = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, batch);

        let response = CommandResponseDto {
            status: "accepted",
            commands_count: Some(5),
        };
        let resp_json = serde_json::to_string(&response).unwrap();
        assert!(resp_json.contains("\"commands_count\":5"));
    }

    #[test]
    fn test_anomaly_dto_serialization_roundtrip() {
        let anomaly_attract = AnomalyDto {
            id: "a_dyn_1".to_string(),
            anomaly_type: "attracting".to_string(),
            position: Vector2DDto::new(350.0, 725.0),
            velocity: Vector2DDto::new(5.0, -1.2),
            core_radius: 15.0,
            radius: 80.0,
            force: 4.5,
        };

        let json = serde_json::to_string(&anomaly_attract).unwrap();
        assert!(json.contains(r#""type":"attracting""#));
        assert!(json.contains(r#""core_radius":15.0"#));
        assert!(json.contains(r#""velocity":{"x":5.0,"y":-1.2}"#));

        let deserialized: AnomalyDto = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, anomaly_attract);

        let anomaly_repel = AnomalyDto {
            id: "a_dyn_2".to_string(),
            anomaly_type: "repelling".to_string(),
            position: Vector2DDto::new(100.0, 900.0),
            velocity: Vector2DDto::new(-2.0, 3.0),
            core_radius: 12.0,
            radius: 60.0,
            force: 3.0,
        };

        let json_repel = serde_json::to_string(&anomaly_repel).unwrap();
        assert!(json_repel.contains(r#""type":"repelling""#));
        let deserialized_repel: AnomalyDto = serde_json::from_str(&json_repel).unwrap();
        assert_eq!(deserialized_repel, anomaly_repel);
    }

    #[test]
    fn test_player_dto_destroyed_status() {
        let player = PlayerDto {
            id: "team_dead".to_string(),
            score: 120,
            status: "destroyed".to_string(),
            position: Vector2DDto::new(340.5, 720.0),
            velocity: Vector2DDto::ZERO,
            acceleration: Vector2DDto::ZERO,
            max_acceleration: 5.0,
            max_velocity: 20.0,
            carpets: vec![],
        };

        assert!(player.is_destroyed());
        assert!(!player.is_stunned());

        let json = serde_json::to_string(&player).unwrap();
        assert!(json.contains(r#""status":"destroyed""#));

        let deserialized: PlayerDto = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.status, "destroyed");
        assert!(deserialized.is_destroyed());
    }

    #[test]
    fn test_anomaly_dto_conversions() {
        // Из AnomalyState
        let state = AnomalyState::new_dynamic(
            "dyn_1",
            "repelling",
            (10.0, 20.0),
            (1.0, -1.0),
            5.0,
            50.0,
            2.5,
        );
        let dto = AnomalyDto::from(&state);
        assert_eq!(dto.id, "dyn_1");
        assert_eq!(dto.anomaly_type, "repelling");
        assert_eq!(dto.position, Vec2::new(10.0, 20.0));
        assert_eq!(dto.velocity, Vec2::new(1.0, -1.0));
        assert_eq!(dto.core_radius, 5.0);
        assert_eq!(dto.radius, 50.0);
        assert_eq!(dto.force, 2.5);

        // Обратно в AnomalyState
        let back_state = AnomalyState::from(dto);
        assert_eq!(back_state, state);

        // Из старой Anomaly
        let old_anomaly = Anomaly::new("old_1", Vec2::new(30.0, 40.0), 25.0, 1.8);
        let old_dto = AnomalyDto::from(&old_anomaly);
        assert_eq!(old_dto.id, "old_1");
        assert_eq!(old_dto.anomaly_type, "attracting");
        assert_eq!(old_dto.core_radius, 0.0);
        assert_eq!(old_dto.velocity, Vec2::ZERO);
        assert_eq!(old_dto.radius, 25.0);
        assert_eq!(old_dto.force, 1.8);
    }
}
