//! # Сетевой интерфейс HTTP REST API (FE-004)
//!
//! Модуль предоставляет маршрутизатор веб-фреймворка Axum, реализующий
//! публичные эндпоинты симулятора DatsMagic:
//! - `GET /api/game/state` — опрос актуального состояния мира управляемой команды
//! - `POST /api/carpet/command` — отправка управляющего ускорения ковра
//!
//! в соответствии со спецификацией [`FE-004`](file:///Users/d.byta/Documents/Code/dats/datsmagic/docs/features/FE-004-simulation-rest-api.md)
//! и контрактами [`docs/mechanics.md`](file:///Users/d.byta/Documents/Code/dats/datsmagic/docs/mechanics.md#4-контракты-api-спецификация-json).

pub mod dto;
pub mod errors;
pub mod handlers;

pub use dto::{
    AnomalyDto, BatchCarpetCommandRequestDto, CarpetCommandItemDto, CarpetDto, CommandRequestDto,
    CommandResponseDto, EnemyCarpetDto, EnemyDto, ErrorResponseDto, GameStateResponseDto,
    PlayerDto, TreasureDto, Vector2DDto,
};
pub use errors::ApiError;
pub use handlers::{auth_middleware, get_game_state, post_command, AuthToken, AUTH_HEADER_NAME};

use crate::engine::GameEngine;
use axum::middleware;
use axum::routing::{get, post};
use axum::Router;

/// Конструирует маршрутизатор Axum с middleware авторизации и внедренным состоянием
pub fn create_api_router(engine: GameEngine) -> Router {
    Router::new()
        .route("/api/game/state", get(get_game_state))
        .route("/api/carpet/command", post(post_command))
        .route("/api/carpet/commands", post(post_command))
        .layer(middleware::from_fn(auth_middleware))
        .with_state(engine)
}
