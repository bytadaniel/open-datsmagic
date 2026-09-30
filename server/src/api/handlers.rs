//! # Обработчики HTTP-эндпоинтов REST API
//!
//! Модуль реализует логику обработки запросов:
//! - [`auth_middleware`]: проверка заголовка `X-Auth-Token`
//! - [`get_game_state`]: `GET /api/game/state`
//! - [`post_command`]: `POST /api/carpet/command`
//!
//! в соответствии со спецификацией [`FE-004`](file:///Users/d.byta/Documents/Code/dats/datsmagic/docs/features/FE-004-simulation-rest-api.md).

use axum::body::Bytes;
use axum::extract::{Extension, Request, State};
use axum::middleware::Next;
use axum::response::Response;
use axum::Json;

use crate::engine::command_buffer::CommandError;
use crate::engine::state::{PlayerState, SessionStatus};
use crate::engine::{GameEngine, PlayerCommand};
use crate::physics::Vec2;
use crate::spatial::Treasure;

use super::dto::{
    AnomalyDto, CarpetDto, CommandResponseDto, EnemyCarpetDto, EnemyDto, GameStateResponseDto,
    PlayerDto,
};
use super::errors::ApiError;

/// Имя HTTP-заголовка авторизации команды
pub const AUTH_HEADER_NAME: &str = "x-auth-token";

/// Токен авторизации игрока / команды
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthToken(pub String);

/// Middleware проверки заголовка X-Auth-Token (FE-004 Шаг 2)
pub async fn auth_middleware(mut req: Request, next: Next) -> Result<Response, ApiError> {
    let header_val = req
        .headers()
        .get(AUTH_HEADER_NAME)
        .or_else(|| req.headers().get("X-Auth-Token"));

    let token = match header_val {
        Some(val) => {
            let s = val.to_str().map_err(|_| ApiError::Unauthorized)?;
            let trimmed = s.trim();
            if trimmed.is_empty() {
                return Err(ApiError::Unauthorized);
            }
            trimmed.to_string()
        }
        None => return Err(ApiError::Unauthorized),
    };

    req.extensions_mut().insert(AuthToken(token));
    Ok(next.run(req).await)
}

/// Обработчик GET /api/game/state (FE-004 / FE-010)
pub async fn get_game_state(
    Extension(token): Extension<AuthToken>,
    State(engine): State<GameEngine>,
) -> Result<Json<GameStateResponseDto>, ApiError> {
    let snapshot = engine.get_snapshot();

    // 1. Поиск или автоматическая регистрация игрока
    let player_id = token.0;
    let player = if let Some(p) = snapshot.world.players.get(&player_id) {
        let mut carpets: Vec<CarpetDto> = p
            .carpets
            .values()
            .map(|c| CarpetDto {
                id: c.id.clone(),
                status: c.status.clone(),
                position: Vec2::new(c.position.0, c.position.1),
                velocity: Vec2::new(c.velocity.0, c.velocity.1),
                max_acceleration: c.max_acceleration,
                max_velocity: c.max_velocity,
            })
            .collect();
        carpets.sort_by(|a, b| a.id.cmp(&b.id));

        PlayerDto {
            id: p.id.clone(),
            score: p.score,
            status: p.status.clone(),
            position: Vec2::new(p.position.0, p.position.1),
            velocity: Vec2::new(p.velocity.0, p.velocity.1),
            max_acceleration: p.max_acceleration,
            max_velocity: p.max_velocity,
            carpets,
        }
    } else {
        // Если игрок обратился к серверу впервые, регистрируем его со спавном 5 ковров флота (FE-010)
        let config = engine.config();
        let new_player = PlayerState::new(
            player_id.clone(),
            config.arena_width / 2.0,
            config.arena_height / 2.0,
            config.max_acceleration,
            config.max_velocity,
        );
        {
            let shared = engine.shared_state();
            let mut state = shared.write().await;
            state
                .world
                .players
                .insert(player_id.clone(), new_player.clone());
        }
        engine.publish_snapshot().await;

        let mut carpets: Vec<CarpetDto> = new_player
            .carpets
            .values()
            .map(|c| CarpetDto {
                id: c.id.clone(),
                status: c.status.clone(),
                position: Vec2::new(c.position.0, c.position.1),
                velocity: Vec2::new(c.velocity.0, c.velocity.1),
                max_acceleration: c.max_acceleration,
                max_velocity: c.max_velocity,
            })
            .collect();
        carpets.sort_by(|a, b| a.id.cmp(&b.id));

        PlayerDto {
            id: new_player.id,
            score: new_player.score,
            status: new_player.status,
            position: Vec2::new(new_player.position.0, new_player.position.1),
            velocity: Vec2::new(new_player.velocity.0, new_player.velocity.1),
            max_acceleration: new_player.max_acceleration,
            max_velocity: new_player.max_velocity,
            carpets,
        }
    };

    // 2. Список противников и их флотов (все остальные игроки в мире)
    let enemies = snapshot
        .world
        .players
        .values()
        .filter(|p| p.id != player_id)
        .map(|p| {
            let mut carpets: Vec<EnemyCarpetDto> = p
                .carpets
                .values()
                .map(|c| EnemyCarpetDto {
                    id: c.id.clone(),
                    position: Vec2::new(c.position.0, c.position.1),
                    velocity: Vec2::new(c.velocity.0, c.velocity.1),
                })
                .collect();
            carpets.sort_by(|a, b| a.id.cmp(&b.id));

            EnemyDto {
                id: p.id.clone(),
                position: Vec2::new(p.position.0, p.position.1),
                velocity: Vec2::new(p.velocity.0, p.velocity.1),
                carpets,
            }
        })
        .collect();

    // 3. Сокровища
    let treasures = snapshot
        .world
        .treasures
        .iter()
        .map(Treasure::from)
        .collect();

    // 4. Аномалии (FE-007: передача параметров динамических аномалий)
    let anomalies = snapshot
        .world
        .anomalies
        .iter()
        .map(AnomalyDto::from)
        .collect();

    // 5. Статус сессии в виде строки ("active", "paused", "finished")
    let game_status = match snapshot.game_status {
        SessionStatus::Active => "active",
        SessionStatus::Paused => "paused",
        SessionStatus::Finished => "finished",
    }
    .to_string();

    Ok(Json(GameStateResponseDto {
        tick: snapshot.tick,
        game_status,
        player,
        treasures,
        anomalies,
        enemies,
    }))
}

/// Обработчик POST /api/carpet/command и POST /api/carpet/commands (FE-004 / FE-010)
pub async fn post_command(
    Extension(token): Extension<AuthToken>,
    State(engine): State<GameEngine>,
    body: Bytes,
) -> Result<Json<CommandResponseDto>, ApiError> {
    let val: serde_json::Value =
        serde_json::from_slice(&body).map_err(|_| ApiError::InvalidVector)?;

    // 1. Проверяем пакетный формат: {"commands": [{"carpet_id": "...", "acceleration": {...}}, ...]}
    if let Some(commands_val) = val.get("commands").and_then(|v| v.as_array()) {
        if commands_val.is_empty() {
            return Err(ApiError::InvalidVector);
        }

        let mut batch_commands = Vec::with_capacity(commands_val.len());
        for item in commands_val {
            let carpet_id = item
                .get("carpet_id")
                .and_then(|v| v.as_str())
                .ok_or(ApiError::InvalidVector)?
                .to_string();

            let accel = item.get("acceleration").ok_or(ApiError::InvalidVector)?;
            let x = accel
                .get("x")
                .and_then(|v| v.as_f64())
                .ok_or(ApiError::InvalidVector)?;
            let y = accel
                .get("y")
                .and_then(|v| v.as_f64())
                .ok_or(ApiError::InvalidVector)?;

            if !x.is_finite() || !y.is_finite() {
                return Err(ApiError::InvalidVector);
            }

            batch_commands.push((carpet_id, PlayerCommand::new(x, y)));
        }

        match engine.register_batch_commands(token.0, batch_commands) {
            Ok(count) => Ok(Json(CommandResponseDto {
                status: "accepted",
                commands_count: Some(count),
            })),
            Err(CommandError::AlreadySubmitted) => Err(ApiError::RateLimitExceeded),
            Err(CommandError::InvalidCommand(_)) => Err(ApiError::InvalidVector),
            Err(CommandError::SessionNotActive) => Err(ApiError::SessionNotActive),
            Err(CommandError::PlayerDestroyed) => Err(ApiError::PlayerDestroyed),
        }
    } else if let Some(accel) = val.get("acceleration") {
        // 2. Одиночный формат (обратная совместимость): {"acceleration": {"x": ..., "y": ...}}
        let x = accel
            .get("x")
            .and_then(|v| v.as_f64())
            .ok_or(ApiError::InvalidVector)?;
        let y = accel
            .get("y")
            .and_then(|v| v.as_f64())
            .ok_or(ApiError::InvalidVector)?;

        if !x.is_finite() || !y.is_finite() {
            return Err(ApiError::InvalidVector);
        }

        let command = PlayerCommand::new(x, y);

        match engine.register_command(token.0, command) {
            Ok(()) => Ok(Json(CommandResponseDto::default())),
            Err(CommandError::AlreadySubmitted) => Err(ApiError::RateLimitExceeded),
            Err(CommandError::InvalidCommand(_)) => Err(ApiError::InvalidVector),
            Err(CommandError::SessionNotActive) => Err(ApiError::SessionNotActive),
            Err(CommandError::PlayerDestroyed) => Err(ApiError::PlayerDestroyed),
        }
    } else {
        Err(ApiError::InvalidVector)
    }
}
