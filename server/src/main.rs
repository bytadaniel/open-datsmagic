//! # Точка входа игрового сервера DatsMagic
//!
//! Инициализирует подсистему логирования, загружает конфигурацию,
//! запускает тактовый генератор симуляции и ожидает сигнал завершения процесса.

use server::config::ServerConfig;
use server::engine::GameEngine;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Настройка структурированного логирования
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber)
        .expect("Не удалось инициализировать глобальный логгер");

    info!("=== Запуск игрового сервера DatsMagic ===");

    let mut config = ServerConfig::default();
    if let Ok(host) = std::env::var("HOST") {
        config.host = host;
    }
    if let Ok(port_str) = std::env::var("PORT") {
        if let Ok(p) = port_str.parse() {
            config.port = p;
        }
    }
    config.enable_spawner = true;
    config.enable_coin_spawner = true;
    info!(
        "Конфигурация сервера: шаг тика = {} мс (Δt = {} с), адрес = {}:{}, спавнер аномалий: {}, спавнер монет: {}",
        config.tick_rate_ms,
        config.dt_seconds(),
        config.host,
        config.port,
        config.enable_spawner,
        config.enable_coin_spawner
    );

    let engine = GameEngine::new(config.clone());

    // Инициализация сокровищ на карте (FE-009 / DR-008)
    {
        let shared = engine.shared_state();
        let mut state = shared.write().await;
        if config.enable_coin_spawner {
            let mut spawner =
                server::spatial::CoinSpawner::new(server::spatial::CoinSpawnerConfig {
                    arena_width: config.arena_width,
                    arena_height: config.arena_height,
                    max_coins: config.bounty_quota,
                    ..Default::default()
                });
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
    server_handle.abort();
    let _ = loop_handle.await;

    info!("Сервер DatsMagic остановлен.");
    Ok(())
}
