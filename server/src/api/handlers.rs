//! Обработчики единственного публичного API DatsMagic: `POST /play/magcarp/player/move`.

use axum::body::Bytes;
use axum::extract::{Extension, Request, State};
use axum::middleware::Next;
use axum::response::Response;
use axum::Json;

use crate::engine::command_buffer::CommandError;
use crate::engine::state::{CarpetState, PlayerState, WorldSnapshot};
use crate::engine::{GameEngine, PlayerCommand};
use crate::physics::Vec2;
use crate::spatial::{SimpleRng, WorldSpatialEngine};

use super::dto::{
    LegacyAnomalyDto, LegacyBountyDto, LegacyDesertDto, LegacyMoveRequestDto, LegacyTransportDto,
    LegacyUnitDto,
};
use super::errors::ApiError;

pub const AUTH_HEADER_NAME: &str = "x-auth-token";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthToken(pub String);

pub async fn auth_middleware(mut req: Request, next: Next) -> Result<Response, ApiError> {
    let header = req
        .headers()
        .get(AUTH_HEADER_NAME)
        .or_else(|| req.headers().get("X-Auth-Token"));
    let token = match header {
        Some(value) => {
            let value = value.to_str().map_err(|_| ApiError::Unauthorized)?.trim();
            if value.is_empty() {
                return Err(ApiError::Unauthorized);
            }
            value.to_string()
        }
        None => return Err(ApiError::Unauthorized),
    };
    req.extensions_mut().insert(AuthToken(token));
    Ok(next.run(req).await)
}

async fn ensure_player(engine: &GameEngine, player_id: &str) {
    if engine.get_snapshot().world.players.contains_key(player_id) {
        return;
    }
    let config = engine.config();
    let player = PlayerState::new(
        player_id.to_string(),
        0.0,
        0.0,
        config.max_acceleration,
        config.max_velocity,
    );
    let shared = engine.shared_state();
    let mut state = shared.write().await;
    if state.world.players.contains_key(player_id) {
        return;
    }
    let carpet_ids: Vec<_> = player.carpets.keys().cloned().collect();
    state.world.players.insert(player_id.to_string(), player);
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos() as u64)
        .unwrap_or(1);
    let mut rng = SimpleRng::new(seed);
    for carpet_id in carpet_ids {
        let (x, y) = WorldSpatialEngine::find_safe_spawn_point(
            &state.world,
            config.arena_width,
            config.arena_height,
            config.transport_radius,
            &mut rng,
        );
        if let Some(carpet) = state
            .world
            .players
            .get_mut(player_id)
            .and_then(|player| player.carpets.get_mut(&carpet_id))
        {
            carpet.position = (x, y);
        }
    }
    if let Some(player) = state.world.players.get_mut(player_id) {
        player.sync_from_carpets();
    }
    drop(state);
    engine.publish_snapshot().await;
}

fn legacy_status(status: &str) -> String {
    if status == "destroyed" {
        "dead".to_string()
    } else {
        "alive".to_string()
    }
}

fn legacy_transport(carpet: &CarpetState, snapshot: &WorldSnapshot) -> LegacyTransportDto {
    let position = Vec2::new(carpet.position.0, carpet.position.1);
    LegacyTransportDto {
        anomaly_acceleration: crate::spatial::compute_environmental_forces_from_states(
            position,
            &snapshot.world.anomalies,
        ),
        attack_cooldown_ms: 0,
        death_count: carpet.death_count,
        health: if carpet.is_destroyed() { 0 } else { 100 },
        id: carpet.id.clone(),
        self_acceleration: Vec2::new(carpet.acceleration.0, carpet.acceleration.1),
        shield_cooldown_ms: 0,
        shield_left_ms: 0,
        status: legacy_status(&carpet.status),
        velocity: Vec2::new(carpet.velocity.0, carpet.velocity.1),
        x: carpet.position.0,
        y: carpet.position.1,
    }
}

fn legacy_desert(
    snapshot: &WorldSnapshot,
    player_id: &str,
    engine: &GameEngine,
    errors: Vec<String>,
) -> LegacyDesertDto {
    let config = engine.config();
    let player = snapshot
        .world
        .players
        .get(player_id)
        .expect("registered player");
    let mut transports: Vec<_> = player
        .carpets
        .values()
        .map(|carpet| legacy_transport(carpet, snapshot))
        .collect();
    transports.sort_by(|a, b| a.id.cmp(&b.id));
    LegacyDesertDto {
        errors,
        anomalies: snapshot
            .world
            .anomalies
            .iter()
            .map(|a| LegacyAnomalyDto {
                effective_radius: a.radius,
                id: a.id.clone(),
                radius: a.core_radius,
                strength: if a.anomaly_type == "repelling" {
                    -a.force
                } else {
                    a.force
                },
                velocity: Vec2::new(a.velocity.0, a.velocity.1),
                x: a.position.0,
                y: a.position.1,
            })
            .collect(),
        attack_cooldown_ms: config.attack_cooldown_ms,
        attack_damage: config.attack_damage,
        attack_explosion_radius: config.attack_explosion_radius,
        attack_range: config.attack_range,
        bounties: snapshot
            .world
            .treasures
            .iter()
            .filter(|t| !t.is_collected)
            .map(|t| LegacyBountyDto {
                points: t.value,
                radius: config.transport_radius,
                x: t.position.0,
                y: t.position.1,
            })
            .collect(),
        enemies: {
            let mut enemy_carpets: Vec<_> = snapshot
                .world
                .players
                .iter()
                .filter(|(other_player_id, _)| other_player_id.as_str() != player_id)
                .flat_map(|(other_player_id, other_player)| {
                    other_player.carpets.iter().map(move |(carpet_id, carpet)| {
                        (
                            other_player_id.clone(),
                            carpet_id.clone(),
                            LegacyUnitDto {
                                health: if carpet.is_destroyed() { 0 } else { 100 },
                                kill_bounty: 0,
                                shield_left_ms: 0,
                                status: legacy_status(&carpet.status),
                                velocity: Vec2::new(carpet.velocity.0, carpet.velocity.1),
                                x: carpet.position.0,
                                y: carpet.position.1,
                            },
                        )
                    })
                })
                .collect();
            enemy_carpets.sort_by(|a, b| (&a.0, &a.1).cmp(&(&b.0, &b.1)));
            enemy_carpets
                .into_iter()
                .map(|(_, _, enemy)| enemy)
                .collect()
        },
        map_size: Vec2::new(config.arena_width, config.arena_height),
        max_accel: config.max_acceleration,
        max_speed: config.max_velocity,
        name: player.id.clone(),
        points: player.score,
        revive_timeout_sec: config.revive_timeout_sec,
        shield_cooldown_ms: config.shield_cooldown_ms,
        shield_time_ms: config.shield_time_ms,
        transport_radius: config.transport_radius,
        transports,
        wanted_list: Vec::new(),
    }
}

pub async fn post_legacy_move(
    Extension(token): Extension<AuthToken>,
    State(engine): State<GameEngine>,
    body: Bytes,
) -> Result<Json<LegacyDesertDto>, ApiError> {
    let request: LegacyMoveRequestDto =
        serde_json::from_slice(&body).map_err(|_| ApiError::InvalidVector)?;
    let player_id = token.0;
    ensure_player(&engine, &player_id).await;
    let snapshot = engine.get_snapshot();
    let player = snapshot
        .world
        .players
        .get(&player_id)
        .expect("registered player");
    let mut commands = Vec::new();
    let mut errors = Vec::new();
    for transport in request.transports {
        let Some(acceleration) = transport.acceleration else {
            continue;
        };
        if !acceleration.is_finite() {
            errors.push(format!(
                "transport {} acceleration is invalid",
                transport.id
            ));
        } else if !player.carpets.contains_key(&transport.id) {
            errors.push(format!(
                "transport {} does not belong to player",
                transport.id
            ));
        } else {
            commands.push((
                transport.id,
                PlayerCommand::new(acceleration.x, acceleration.y),
            ));
        }
    }
    drop(snapshot);
    if !commands.is_empty() {
        match engine.register_batch_commands(player_id.clone(), commands) {
            Ok(_) => {}
            Err(CommandError::AlreadySubmitted) => return Err(ApiError::RateLimitExceeded),
            Err(CommandError::InvalidCommand(message)) => errors.push(message),
            Err(CommandError::SessionNotActive) => return Err(ApiError::SessionNotActive),
            Err(CommandError::PlayerDestroyed) => return Err(ApiError::PlayerDestroyed),
        }
    }
    Ok(Json(legacy_desert(
        &engine.get_snapshot(),
        &player_id,
        &engine,
        errors,
    )))
}
