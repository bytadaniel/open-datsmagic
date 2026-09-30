//! # Модуль игрового движка симулятора DatsMagic (FE-001)
//!
//! Отвечает за:
//! - Генерацию и продвижение дискретных игровых тиков (шаг 200 мс).
//! - Атомарный сбор и валидацию команд игроков за текущий такт.
//! - Координацию фаз физического пересчета и коллизий.
//! - Формирование и публикацию потокобезопасных неизменяемых снапшотов мира.

pub mod command_buffer;
pub mod loop_runner;
pub mod state;

pub use command_buffer::{CommandError, InputCommandBuffer, PlayerCommand, PlayerId};
pub use loop_runner::{
    GameEngine, NullPhysicsHandler, NullSpatialHandler, PhysicsStepHandler, SpatialStepHandler,
};
pub use state::{
    AnomalyState, GameState, PlayerState, PlayerStatus, SessionStatus, SharedGameState,
    TreasureState, WorldData, WorldSnapshot,
};
