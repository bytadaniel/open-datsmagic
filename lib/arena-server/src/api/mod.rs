//! # Сетевой интерфейс HTTP REST API (FE-004)
//!
//! Модуль предоставляет маршрутизатор веб-фреймворка Axum, реализующий
//! единственный публичный игровой REST endpoint симулятора DatsMagic:
//! - `POST /play/magcarp/player/move` — пакет команд и снимок мира `Desert`
//!
//! в соответствии со спецификацией [`FE-004`](file:///Users/d.byta/Documents/Code/dats/datsmagic/docs/features/FE-004-simulation-rest-api.md)
//! и контрактами [`docs/mechanics.md`](file:///Users/d.byta/Documents/Code/dats/datsmagic/docs/mechanics.md#4-контракты-api-спецификация-json).

pub mod dto;
pub mod errors;
pub mod handlers;

pub use dto::{
    LegacyAnomalyDto, LegacyBountyDto, LegacyDesertDto, LegacyMoveRequestDto,
    LegacyTransportCommandDto, LegacyTransportDto, LegacyUnitDto, Vector2DDto,
};
pub use errors::ApiError;
pub use handlers::{auth_middleware, post_legacy_move, AuthToken, AUTH_HEADER_NAME};

use crate::engine::GameEngine;
use axum::middleware;
use axum::routing::{get, post};
use axum::Router;

/// Конструирует маршрутизатор Axum с middleware авторизации и внедренным состоянием
pub fn create_api_router(engine: GameEngine) -> Router {
    Router::new()
        .route(
            "/play/magcarp/player/move",
            post(post_legacy_move).route_layer(middleware::from_fn(auth_middleware)),
        )
        .route("/stream/visualizer", get(handlers::visualizer_websocket))
        .with_state(engine)
}
