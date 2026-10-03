//! Обработчики единственного публичного API DatsMagic: `POST /play/magcarp/player/move`.

use axum::body::Bytes;
use axum::extract::{
    ws::{Message, WebSocket, WebSocketUpgrade},
    Extension, Request, State,
};
use axum::http::HeaderMap;
use axum::middleware::Next;
use axum::response::Response;
use axum::Json;
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::SystemTime;

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

#[derive(Deserialize)]
struct TokenRegistryFile {
    #[serde(default)]
    teams: Vec<TokenRegistryTeam>,
}

#[derive(Deserialize)]
struct TokenRegistryTeam {
    token: String,
    #[serde(default)]
    name: String,
}

struct TokenRegistryCache {
    path: PathBuf,
    modified: Option<SystemTime>,
    tokens: std::collections::HashMap<String, String>,
}

static TOKEN_REGISTRY_CACHE: OnceLock<Mutex<Option<TokenRegistryCache>>> = OnceLock::new();

fn registered_team_name(token: &str) -> Option<String> {
    let path = std::env::var_os("DATS_TOKEN_REGISTRY_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .ancestors()
                .nth(2)
                .unwrap_or_else(|| std::path::Path::new("."))
                .join("apps/arena-hub/data/registry.json")
        });
    let cache = TOKEN_REGISTRY_CACHE.get_or_init(|| Mutex::new(None));
    let Ok(mut cache) = cache.lock() else {
        return None;
    };
    let modified = std::fs::metadata(&path)
        .and_then(|metadata| metadata.modified())
        .ok();
    if let Some(current) = cache.as_ref() {
        if modified.is_some() && current.path == path && current.modified == modified {
            return current.tokens.get(token).cloned();
        }
    }
    let tokens: std::collections::HashMap<String, String> = std::fs::read(&path)
        .ok()
        .and_then(|source| serde_json::from_slice::<TokenRegistryFile>(&source).ok())
        .map(|registry| {
            registry
                .teams
                .into_iter()
                .map(|team| (team.token, team.name))
                .collect()
        })
        .unwrap_or_default();
    let registered = tokens.get(token).cloned();
    *cache = Some(TokenRegistryCache {
        path,
        modified,
        tokens,
    });
    registered
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthToken {
    player_id: String,
    team_name: String,
    observer: bool,
}

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
    let observer = std::env::var("DATS_OBSERVER_TOKEN")
        .ok()
        .filter(|configured| !configured.is_empty() && configured == &token)
        .is_some();
    let (player_id, team_name) = if observer {
        ("__dats_observer__".to_string(), "Наблюдатель".to_string())
    } else {
        (
            token_fingerprint(&token),
            registered_team_name(&token).ok_or(ApiError::Unauthorized)?,
        )
    };
    req.extensions_mut().insert(AuthToken {
        player_id,
        team_name,
        observer,
    });
    Ok(next.run(req).await)
}

fn token_fingerprint(token: &str) -> String {
    let hash = token
        .as_bytes()
        .iter()
        .fold(0xcbf29ce484222325_u64, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
        });
    format!("{hash:016x}")
}

async fn ensure_player(engine: &GameEngine, player_id: &str) {
    if engine.get_snapshot().world.players.contains_key(player_id) {
        return;
    }
    let config = engine.config();
    let player = PlayerState::new_with_carpet_count(
        player_id.to_string(),
        0.0,
        0.0,
        config.max_acceleration,
        config.max_velocity,
        config.carpet_count,
    );
    let shared = engine.shared_state();
    let mut state = shared.write().await;
    if state.world.players.contains_key(player_id) {
        return;
    }
    let carpet_ids: Vec<_> = player.carpets.keys().cloned().collect();
    state.world.players.insert(player_id.to_string(), player);
    let token_seed = player_id
        .as_bytes()
        .iter()
        .fold(0xcbf29ce484222325_u64, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
        });
    let seed = config.runtime_entropy ^ token_seed ^ 0x5350_4157_4e50_4f49;
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
    player_id: Option<&str>,
    team_name: &str,
    engine: &GameEngine,
    errors: Vec<String>,
) -> LegacyDesertDto {
    let config = engine.config();
    let player = player_id.and_then(|id| snapshot.world.players.get(id));
    let mut transports: Vec<_> = player
        .into_iter()
        .flat_map(|player| player.carpets.values())
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
                .filter(|(other_player_id, _)| {
                    player_id.is_none_or(|id| other_player_id.as_str() != id)
                })
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
        name: team_name.to_string(),
        points: player.map_or(0, |player| player.score),
        revive_timeout_sec: config.revive_timeout_sec,
        shield_cooldown_ms: config.shield_cooldown_ms,
        shield_time_ms: config.shield_time_ms,
        transport_radius: config.transport_radius,
        transports,
        wanted_list: Vec::new(),
    }
}

#[derive(Clone, serde::Deserialize)]
struct RealtimeTicket {
    player_id: String,
    name: String,
    mode: String,
}

#[derive(serde::Deserialize)]
struct RealtimeClientMessage {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    transports: Vec<super::dto::LegacyTransportCommandDto>,
}

fn consume_realtime_ticket(ticket: String) -> Result<RealtimeTicket, String> {
    let hub = std::env::var("DATS_HUB_INTERNAL_URL")
        .unwrap_or_else(|_| "http://127.0.0.1:8090".to_string());
    let control_token = std::env::var("DATS_HUB_CONTROL_TOKEN")
        .or_else(|_| std::env::var("ARENA_CONTROL_TOKEN"))
        .map_err(|_| "realtime control token is not configured".to_string())?;
    let address = hub
        .strip_prefix("http://")
        .ok_or("internal Hub URL must use http")?;
    let (authority, _) = address.split_once('/').unwrap_or((address, ""));
    let mut stream = TcpStream::connect(authority).map_err(|e| e.to_string())?;
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(2)))
        .map_err(|e| e.to_string())?;
    let body =
        serde_json::to_vec(&serde_json::json!({"ticket": ticket})).map_err(|e| e.to_string())?;
    write!(stream,
        "POST /internal/visualizer/ticket/consume HTTP/1.1\r\nHost: {authority}\r\nX-Arena-Control-Token: {control_token}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()).map_err(|e| e.to_string())?;
    stream.write_all(&body).map_err(|e| e.to_string())?;
    let mut response = Vec::new();
    stream
        .read_to_end(&mut response)
        .map_err(|e| e.to_string())?;
    let separator = response
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .ok_or("invalid Hub response")?;
    let header = std::str::from_utf8(&response[..separator]).map_err(|e| e.to_string())?;
    if !header
        .lines()
        .next()
        .is_some_and(|line| line.contains(" 200 "))
    {
        return Err("realtime ticket rejected".to_string());
    }
    serde_json::from_slice(&response[separator + 4..]).map_err(|e| e.to_string())
}

pub async fn visualizer_websocket(
    State(engine): State<GameEngine>,
    headers: HeaderMap,
    websocket: WebSocketUpgrade,
) -> axum::response::Response {
    let ticket = headers
        .get("sec-websocket-protocol")
        .and_then(|value| value.to_str().ok())
        .and_then(|protocols| {
            protocols
                .split(',')
                .map(str::trim)
                .find_map(|protocol| protocol.strip_prefix("stadmagic-ticket."))
        })
        .map(str::to_owned);
    let Some(ticket) = ticket else {
        return axum::http::StatusCode::UNAUTHORIZED.into_response();
    };
    let claims = match tokio::task::spawn_blocking(move || consume_realtime_ticket(ticket)).await {
        Ok(Ok(claims)) => claims,
        _ => return axum::http::StatusCode::UNAUTHORIZED.into_response(),
    };
    if claims.mode != "player" && claims.mode != "observer" {
        return axum::http::StatusCode::UNAUTHORIZED.into_response();
    }
    websocket
        .protocols(["stadmagic.v1"])
        .max_message_size(16 * 1024)
        .on_upgrade(move |socket| visualizer_socket(socket, engine, claims))
}

use axum::response::IntoResponse;

async fn visualizer_socket(socket: WebSocket, engine: GameEngine, claims: RealtimeTicket) {
    if claims.mode == "player" {
        ensure_player(&engine, &claims.player_id).await;
    }
    let (mut sender, mut receiver) = socket.split();
    let (outgoing_tx, mut outgoing_rx) = tokio::sync::mpsc::channel::<Message>(8);
    let writer_engine = engine.clone();
    let writer_claims = claims.clone();
    let writer = tokio::spawn(async move {
        let mut snapshots = writer_engine.subscribe_snapshots();
        loop {
            tokio::select! {
                changed = snapshots.changed() => {
                    if changed.is_err() { break; }
                    let snapshot = snapshots.borrow().clone();
                    let player_id = (writer_claims.mode == "player").then_some(writer_claims.player_id.as_str());
                    let payload = serde_json::json!({
                        "type": "snapshot", "tick": snapshot.tick,
                        "state": legacy_desert(&snapshot, player_id, &writer_claims.name, &writer_engine, Vec::new())
                    });
                    if let Ok(text) = serde_json::to_string(&payload) {
                        if sender.send(Message::Text(text.into())).await.is_err() { break; }
                    }
                }
                message = outgoing_rx.recv() => {
                    let Some(message) = message else { break; };
                    if sender.send(message).await.is_err() { break; }
                }
            }
        }
    });
    let snapshot = engine.get_snapshot();
    let player_id = (claims.mode == "player").then_some(claims.player_id.as_str());
    let initial = serde_json::json!({
        "type": "snapshot", "tick": snapshot.tick,
        "state": legacy_desert(&snapshot, player_id, &claims.name, &engine, Vec::new())
    });
    if let Ok(text) = serde_json::to_string(&initial) {
        let _ = outgoing_tx.send(Message::Text(text.into())).await;
    }
    loop {
        let Some(Ok(message)) = receiver.next().await else {
            break;
        };
        match message {
            Message::Close(_) => break,
            Message::Text(text) => {
                let result = async {
                    let message: RealtimeClientMessage =
                        serde_json::from_str(&text).map_err(|_| "invalid message")?;
                    if message.kind != "commands" {
                        return Err("unsupported message type");
                    }
                    if claims.mode != "player" && !message.transports.is_empty() {
                        return Err("observer is read-only");
                    }
                    if message.transports.len() > 5 {
                        return Err("too many commands");
                    }
                    let snapshot = engine.get_snapshot();
                    let player = snapshot.world.players.get(&claims.player_id);
                    let mut commands = Vec::new();
                    for transport in message.transports {
                        let Some(acceleration) = transport.acceleration else {
                            continue;
                        };
                        if !acceleration.is_finite()
                            || !player.is_some_and(|p| p.carpets.contains_key(&transport.id))
                        {
                            return Err("invalid or foreign carpet command");
                        }
                        commands.push((
                            transport.id,
                            PlayerCommand::new(acceleration.x, acceleration.y),
                        ));
                    }
                    drop(snapshot);
                    if !commands.is_empty() {
                        engine
                            .register_realtime_batch_commands(claims.player_id.clone(), commands)
                            .map_err(|_| "command rejected")?;
                    }
                    Ok::<(), &str>(())
                }
                .await;
                if let Err(error) = result {
                    let payload = serde_json::json!({"type":"error", "error":error});
                    let _ = outgoing_tx.try_send(Message::Text(payload.to_string().into()));
                }
            }
            Message::Ping(payload) => {
                let _ = outgoing_tx.try_send(Message::Pong(payload));
            }
            Message::Pong(_) => {}
            Message::Binary(_) => {
                let _ = outgoing_tx.try_send(Message::Text(
                    serde_json::json!({"type":"error", "error":"binary messages are unsupported"})
                        .to_string()
                        .into(),
                ));
            }
        }
    }
    writer.abort();
}

pub async fn post_legacy_move(
    Extension(token): Extension<AuthToken>,
    State(engine): State<GameEngine>,
    body: Bytes,
) -> Result<Json<LegacyDesertDto>, ApiError> {
    let request: LegacyMoveRequestDto =
        serde_json::from_slice(&body).map_err(|_| ApiError::InvalidVector)?;
    let player_id = token.player_id;
    let team_name = token.team_name;
    if token.observer {
        if !request.transports.is_empty() {
            return Err(ApiError::Unauthorized);
        }
        return Ok(Json(legacy_desert(
            &engine.get_snapshot(),
            None,
            &team_name,
            &engine,
            Vec::new(),
        )));
    }
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
        Some(&player_id),
        &team_name,
        &engine,
        errors,
    )))
}
