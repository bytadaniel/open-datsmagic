//! # Точка входа игрового сервера DatsMagic
//!
//! Инициализирует подсистему логирования, загружает конфигурацию,
//! запускает тактовый генератор симуляции и ожидает сигнал завершения процесса.

use serde::{Deserialize, Serialize};
use server::config::ServerConfig;
use server::engine::GameEngine;
use std::path::PathBuf;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

#[derive(Clone, Deserialize, Serialize)]
struct WorldDefinition {
    id: String,
    name: String,
    description: String,
    config: ServerConfig,
}

#[derive(Deserialize)]
struct WorldCatalog {
    worlds: Vec<WorldDefinition>,
}

#[derive(Serialize)]
struct CurrentWorldInfo {
    world_id: String,
    world_number: usize,
    name: String,
    arena_name: String,
    description: String,
    #[serde(flatten)]
    config: ServerConfig,
}

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap_or_else(|| std::path::Path::new("."))
        .to_path_buf()
}

fn load_worlds(path: &std::path::Path) -> Result<Vec<WorldDefinition>, String> {
    let source = std::fs::read_to_string(path)
        .map_err(|error| format!("cannot read world catalog {}: {error}", path.display()))?;
    let catalog: WorldCatalog = serde_json::from_str(&source)
        .map_err(|error| format!("invalid world catalog {}: {error}", path.display()))?;
    if catalog.worlds.is_empty() {
        return Err("worlds.json must contain at least one world".into());
    }
    let mut ids = std::collections::HashSet::new();
    for world in &catalog.worlds {
        if world.id.trim().is_empty() || !ids.insert(world.id.clone()) {
            return Err("world IDs must be non-empty and unique".into());
        }
        if world.name.trim().is_empty() || world.description.trim().is_empty() {
            return Err(format!(
                "world {} must have a name and description",
                world.id
            ));
        }
        world.config.validate()?;
        if world.config.arena_width > 20_000.0 || world.config.arena_height > 20_000.0 {
            return Err(format!(
                "world {} exceeds the 20000 x 20000 arena limit",
                world.id
            ));
        }
    }
    Ok(catalog.worlds)
}

fn select_world(worlds: &[WorldDefinition], requested_id: Option<&str>) -> Result<usize, String> {
    if let Some(id) = requested_id {
        return worlds
            .iter()
            .position(|world| world.id == id)
            .ok_or_else(|| format!("world ID {id:?} is not present in worlds.json"));
    }
    let entropy = runtime_entropy();
    Ok((entropy as usize) % worlds.len())
}

fn runtime_entropy() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos() as u64)
        .unwrap_or_default()
        ^ u64::from(std::process::id()).rotate_left(23)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::args().nth(1).as_deref() == Some("--world-catalog-json") {
        let project_root = repository_root();
        let worlds_path = std::env::var_os("DATS_WORLDS_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|| project_root.join("assets/worlds.json"));
        let worlds = load_worlds(&worlds_path).map_err(std::io::Error::other)?;
        let worlds: Vec<_> = worlds
            .iter()
            .enumerate()
            .map(|(index, world)| {
                serde_json::json!({
                    "world_number": index + 1,
                    "id": world.id,
                    "name": world.name,
                    "description": world.description,
                    "config": world.config,
                })
            })
            .collect();
        println!("{}", serde_json::to_string(&worlds)?);
        return Ok(());
    }

    // Настройка структурированного логирования
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber)
        .expect("Не удалось инициализировать глобальный логгер");

    info!("=== Запуск игрового сервера DatsMagic ===");

    let project_root = repository_root();
    let worlds_path = std::env::var_os("DATS_WORLDS_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|| project_root.join("assets/worlds.json"));
    let world_info_path = std::env::var_os("DATS_WORLD_STATUS_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|| project_root.join("apps/arena-hub/data/current_world.json"));
    let worlds = load_worlds(&worlds_path).map_err(std::io::Error::other)?;
    let world_index = select_world(&worlds, std::env::var("DATS_WORLD_ID").ok().as_deref())
        .map_err(std::io::Error::other)?;
    let world = &worlds[world_index];
    let world_number = world_index + 1;
    let world_id = world.id.clone();
    let world_name = world.name.clone();
    let world_description = world.description.clone();
    let mut config = world.config.clone();
    config.runtime_entropy = runtime_entropy();
    info!(world_number, %world_id, %world_name, "Профиль мира выбран");
    if let Ok(host) = std::env::var("HOST") {
        config.host = host;
    }
    if let Ok(port_str) = std::env::var("PORT") {
        if let Ok(p) = port_str.parse() {
            config.port = p;
        }
    }
    info!(
        "Параметры мира: тик = {} мс (Δt = {} с), арена = {:.0}×{:.0}, ковры Amax={:.0} Vmax={:.0}, аномалии {}, монеты {}, адрес = {}:{}",
        config.tick_rate_ms,
        config.dt_seconds(),
        config.arena_width,
        config.arena_height,
        config.max_acceleration,
        config.max_velocity,
        config.anomaly_quota,
        config.bounty_quota,
        config.host,
        config.port
    );

    let world_info = CurrentWorldInfo {
        world_id,
        world_number,
        name: world_name,
        arena_name: std::env::var("DATS_WORLD_RUN_NAME")
            .unwrap_or_else(|_| format!("world_{world_number}")),
        description: world_description,
        config: config.clone(),
    };
    let world_info_json = serde_json::to_vec_pretty(&world_info)?;
    if let Some(parent) = world_info_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&world_info_path, world_info_json)?;
    info!(world_status = %world_info_path.display(), "Параметры мира опубликованы для визуализатора");

    let engine = GameEngine::new(config.clone());

    // Инициализация сокровищ на карте (FE-009 / DR-008)
    {
        let shared = engine.shared_state();
        let mut state = shared.write().await;
        if config.enable_coin_spawner {
            let mut spawner = server::spatial::CoinSpawner::with_seed(
                server::spatial::CoinSpawnerConfig {
                    arena_width: config.arena_width,
                    arena_height: config.arena_height,
                    max_coins: config.bounty_quota,
                    base_value: config.bounty_base_value,
                    max_value_cap: config.bounty_max_value,
                    margin: config.bounty_spawn_margin,
                },
                config.runtime_entropy ^ 0x494e_4954_434f_494e,
            );
            let initial_coins = spawner.replenish_balanced_avoiding_anomalies(&[], 0, &[]);
            state.world.treasures.extend(
                initial_coins
                    .into_iter()
                    .map(server::engine::state::TreasureState::from),
            );
        } else {
            let treasure_spots = [
                ("t_1", "chest", 250.0, 250.0, 50),
                ("t_2", "chest", 750.0, 250.0, 100),
                ("t_3", "chest", 250.0, 750.0, 100),
                ("t_4", "chest", 750.0, 750.0, 150),
                ("t_5", "chest", 500.0, 500.0, 200),
                ("t_6", "chest", 500.0, 250.0, 75),
                ("t_7", "chest", 500.0, 750.0, 75),
                ("t_8", "chest", 250.0, 500.0, 75),
                ("t_9", "chest", 750.0, 500.0, 75),
            ];
            for (id, r#type, x, y, val) in treasure_spots {
                state
                    .world
                    .treasures
                    .push(server::engine::state::TreasureState {
                        id: id.to_string(),
                        r#type: r#type.to_string(),
                        position: (x, y),
                        value: val,
                        is_collected: false,
                    });
            }
        }
    }
    engine.publish_snapshot().await;

    let leaderboard_path = std::env::var_os("DATS_LEADERBOARD_PATH")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| repository_root().join("apps/arena-hub/data/leaderboard.json"));
    let leaderboard_handle = server::leaderboard::spawn(engine.clone(), leaderboard_path)
        .await
        .map_err(|error| std::io::Error::other(error.to_string()))?;

    let loop_handle = engine.spawn_loop();

    let addr = format!("{}:{}", config.host, config.port);
    let router = server::api::create_api_router(engine.clone());
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    info!("HTTP REST API запущен на http://{}", addr);

    let server_handle = tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, router).await {
            tracing::error!("Ошибка HTTP сервера: {}", e);
        }
    });

    info!("Игровой сервер успешно инициализирован. Нажмите Ctrl+C для остановки.");

    // Ожидание сигнала завершения процесса (SIGINT / Ctrl+C)
    tokio::signal::ctrl_c().await?;
    info!("Получен сигнал завершения. Выполняется корректная остановка сервера...");

    engine.stop();
    leaderboard_handle.abort();
    server_handle.abort();
    let _ = loop_handle.await;

    info!("Сервер DatsMagic остановлен.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{load_worlds, select_world};
    use std::path::Path;

    #[test]
    fn checked_in_world_catalog_is_unique_and_resource_bounded() {
        let worlds = load_worlds(Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/worlds.json"
        )))
        .unwrap();
        assert!(!worlds.is_empty());
        assert_eq!(
            worlds
                .iter()
                .map(|world| world.id.as_str())
                .collect::<std::collections::HashSet<_>>()
                .len(),
            worlds.len()
        );
        assert!(worlds
            .iter()
            .all(|world| world.config.bounty_quota <= 12_000));
        assert!(worlds.iter().any(|world| world.config.friction < 0.9));
        assert!(worlds.iter().any(|world| world.config.friction > 0.99));
        assert!(worlds.iter().any(|world| world.config.max_velocity < 90.0));
        assert!(worlds.iter().any(|world| world.config.max_velocity > 150.0));
        let last_carpet = worlds
            .iter()
            .find(|world| world.id == "last-carpet-standing")
            .unwrap();
        assert_eq!(last_carpet.config.carpet_count, 1);
        assert!(!last_carpet.config.enable_respawn);
        let king = worlds
            .iter()
            .find(|world| world.id == "king-of-the-hill")
            .unwrap();
        assert_eq!(king.config.carpet_count, 10);
        assert!(!king.config.enable_respawn);
    }

    #[test]
    fn world_id_override_must_be_in_catalog() {
        let worlds = load_worlds(Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/worlds.json"
        )))
        .unwrap();
        assert_eq!(select_world(&worlds, Some("quiet-harbor-01")).unwrap(), 0);
        assert!(select_world(&worlds, Some("missing-world")).is_err());
    }

    #[test]
    fn catalog_loader_accepts_a_different_profile_count() {
        let path =
            std::env::temp_dir().join(format!("datsmagic-worlds-{}.json", std::process::id()));
        std::fs::write(
            &path,
            r#"{"worlds":[{"id":"only","name":"Only","description":"Single profile","config":{}}]}"#,
        )
        .unwrap();
        let worlds = load_worlds(&path).unwrap();
        let _ = std::fs::remove_file(path);
        assert_eq!(worlds.len(), 1);
        assert_eq!(select_world(&worlds, None).unwrap(), 0);
    }
}
