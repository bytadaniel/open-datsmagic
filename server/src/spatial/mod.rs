//! # Подсистема управления пространственными сущностями и детекции коллизий (FE-003)
//!
//! Модуль инкапсулирует коллекции сокровищ, аномалий и соперников,
//! производит расчет внешних сил песчаных вихрей, детекцию захвата сокровищ
//! и наложение статуса оглушения (`stunned`) в соответствии со спецификацией
//! [`FE-003`](file:///Users/d.byta/Documents/Code/dats/datsmagic/docs/features/FE-003-spatial-entities-and-collisions.md),
//! доменными правилами [`DR-003`](file:///Users/d.byta/Documents/Code/dats/datsmagic/docs/domain/DR-003-entity-interactions-and-collisions.md)
//! и архитектурным решением [`ADR-002`](file:///Users/d.byta/Documents/Code/dats/datsmagic/docs/adr/ADR-002-repository-structure.md).

pub mod coin_spawner;
pub mod collision_detector;
pub mod entities;
pub mod manager;
pub mod spawner;

pub use coin_spawner::{
    calculate_coin_value, CoinSpawner, CoinSpawnerConfig, CoinTier, DEFAULT_ALPHA, DEFAULT_BETA,
    DEFAULT_GAMMA, DEFAULT_SPREAD_BASE,
};
pub use collision_detector::{
    check_circle_collision, check_player_collision, check_point_in_circle, clamp_position_to_arena,
    is_within_arena,
};
pub use entities::{
    Anomaly, AnomalyType, CarpetEntity, DynamicAnomaly, EnemyPlayer, PlayerEntity, Treasure,
};
pub use manager::{
    compute_anomaly_force, compute_environmental_forces_from_anomalies,
    compute_environmental_forces_from_states, compute_single_anomaly_force, EntityManager,
    SpatialCollisionManager, WorldSpatialEngine, DEFAULT_CAPTURE_RADIUS, DEFAULT_STUN_RADIUS_RATIO,
    DEFAULT_STUN_TICKS,
};
pub use spawner::{
    is_despawned, trajectory_intersects_arena, AnomalySpawner, AnomalySpawnerConfig, SimpleRng,
};
