//! # Интеграционные тесты пространственных сущностей и коллизий (FE-003)
//!
//! Проверяет детекцию коллизий, сбор сокровищ, гравитационное затягивание аномалий
//! и наложение эффекта оглушения в соответствии со спецификацией
//! [`FE-003`](file:///Users/d.byta/Documents/Code/dats/datsmagic/docs/features/FE-003-spatial-entities-and-collisions.md)
//! и доменными правилами [`DR-003`](file:///Users/d.byta/Documents/Code/dats/datsmagic/docs/domain/DR-003-entity-interactions-and-collisions.md).

use server::config::ServerConfig;
use server::engine::command_buffer::{CommandError, PlayerCommand};
use server::engine::loop_runner::SpatialStepHandler;
use server::engine::state::{AnomalyState, TreasureState, WorldData};
use server::engine::{GameEngine, PlayerState};
use server::physics::Vec2;
use server::spatial::{
    check_circle_collision, check_point_in_circle, compute_anomaly_force,
    compute_environmental_forces_from_anomalies, Anomaly, CoinSpawner, CoinSpawnerConfig,
    EntityManager, SpatialCollisionManager, Treasure, WorldSpatialEngine,
};

/// TC-COL-01: Расчет силы притяжения аномалии (test_anomaly_pull_direction)
/// Вектор силы направлен строго к центру вихря и модуль равен force
#[test]
fn test_anomaly_pull_direction() {
    let anomaly_pos = Vec2::new(100.0, 100.0);
    let player_pos = Vec2::new(100.0, 50.0);
    let radius = 60.0;
    let force = 4.0;

    let pull = compute_anomaly_force(player_pos, anomaly_pos, radius, force);

    // Вектор направлен строго вверх (0, 4.0)
    assert!((pull.x - 0.0).abs() < 1e-9);
    assert!((pull.y - 4.0).abs() < 1e-9);
    assert!((pull.length() - force).abs() < 1e-9);
}

/// TC-COL-01: Сила равна нулю вне радиуса действия (test_anomaly_zero_force_outside_radius)
#[test]
fn test_anomaly_zero_force_outside_radius() {
    let anomaly_pos = Vec2::new(0.0, 0.0);
    let player_pos = Vec2::new(10.0, 0.0);
    let radius = 5.0;
    let force = 10.0;

    let pull = compute_anomaly_force(player_pos, anomaly_pos, radius, force);
    assert_eq!(pull, Vec2::ZERO);
}

/// TC-COL-01: Безопасность при нахождении ровно в центре аномалии (деление на ноль)
#[test]
fn test_anomaly_zero_force_at_center() {
    let anomaly_pos = Vec2::new(42.0, 42.0);
    let player_pos = Vec2::new(42.0, 42.0);

    let pull = compute_anomaly_force(player_pos, anomaly_pos, 50.0, 5.0);
    assert_eq!(pull, Vec2::ZERO);
}

/// TC-COL-01: Суперпозиция сил нескольких аномалий: W = sum w_i
#[test]
fn test_multiple_anomalies_superposition() {
    let anomalies = vec![
        Anomaly::new("a1", Vec2::new(10.0, 0.0), 50.0, 3.0), // тянет вправо (3.0, 0.0)
        Anomaly::new("a2", Vec2::new(0.0, 10.0), 50.0, 4.0), // тянет вверх (0.0, 4.0)
    ];

    let total_force = compute_environmental_forces_from_anomalies(Vec2::ZERO, &anomalies);
    assert!((total_force.x - 3.0).abs() < 1e-9);
    assert!((total_force.y - 4.0).abs() < 1e-9);
    assert!((total_force.length() - 5.0).abs() < 1e-9); // 3-4-5 треугольник
}

/// TC-COL-02: Захват сокровища в радиусе сбора (test_treasure_collected_within_radius)
#[test]
fn test_treasure_collected_within_radius() {
    let mut manager = EntityManager::new();
    let treasure = Treasure::new("t1", "gold_chest", Vec2::new(20.0, 20.0), 100);
    manager.add_treasure(treasure);

    let player_pos = Vec2::new(20.0, 25.0); // d = 5.0
    let capture_radius = 5.0; // ровно на границе

    let score = manager.resolve_treasure_captures(player_pos, player_pos, capture_radius);
    assert_eq!(score, 100);
    assert_eq!(manager.treasures.len(), 0);
}

/// TC-COL-02: Сокровище не захватывается вне радиуса (test_treasure_not_collected_outside_radius)
#[test]
fn test_treasure_not_collected_outside_radius() {
    let mut manager = EntityManager::new();
    let treasure = Treasure::new("t1", "gold_chest", Vec2::new(20.0, 20.0), 100);
    manager.add_treasure(treasure);

    let player_pos = Vec2::new(20.0, 25.1); // d = 5.1 > 5.0
    let capture_radius = 5.0;

    let score = manager.resolve_treasure_captures(player_pos, player_pos, capture_radius);
    assert_eq!(score, 0);
    assert_eq!(manager.treasures.len(), 1);
}

/// TC-COL-02: Одновременный сбор нескольких сокровищ в один такт
#[test]
fn test_multiple_treasures_collected_in_single_tick() {
    let mut manager = EntityManager::new();
    manager.add_treasure(Treasure::new("t1", "coin", Vec2::new(1.0, 0.0), 10));
    manager.add_treasure(Treasure::new("t2", "coin", Vec2::new(2.0, 0.0), 20));
    manager.add_treasure(Treasure::new("t3", "chest", Vec2::new(3.0, 0.0), 50));
    manager.add_treasure(Treasure::new("t4", "far", Vec2::new(100.0, 0.0), 500));

    let score = manager.resolve_treasure_captures(Vec2::ZERO, Vec2::ZERO, 5.0);
    assert_eq!(score, 80); // 10 + 20 + 50
    assert_eq!(manager.treasures.len(), 1);
    assert_eq!(manager.treasures[0].id, "t4");
}

/// TC-COL-03: Проверка попадания в эпицентр аномалии и оглушения
#[test]
fn test_anomaly_epicenter_stun_detection() {
    let mut manager = EntityManager::new();
    // Аномалия с радиусом 40.0, по умолчанию эпицентр = 25% = 10.0
    manager.add_anomaly(Anomaly::new("a1", Vec2::new(0.0, 0.0), 40.0, 5.0));

    // Точка в эпицентре (d = 8.0 <= 10.0)
    assert!(manager.check_and_update_stuns(Vec2::new(8.0, 0.0)));

    // Точка вне эпицентра (d = 12.0 > 10.0), хотя и внутри вихря
    assert!(!manager.check_and_update_stuns(Vec2::new(12.0, 0.0)));
}

/// Геометрические утилиты коллизий
#[test]
fn test_collision_detector_utilities() {
    assert!(check_circle_collision(
        Vec2::new(0.0, 0.0),
        10.0,
        Vec2::new(15.0, 0.0),
        10.0
    ));
    assert!(!check_circle_collision(
        Vec2::new(0.0, 0.0),
        5.0,
        Vec2::new(20.0, 0.0),
        5.0
    ));
    assert!(check_point_in_circle(Vec2::new(3.0, 4.0), Vec2::ZERO, 5.0));
    assert!(!check_point_in_circle(Vec2::new(3.0, 4.1), Vec2::ZERO, 5.0));
}

/// Сквозная интеграция GameEngine: гравитационное затягивание вихрем и сбор сокровища
#[tokio::test]
async fn test_game_engine_spatial_and_physics_full_cycle() {
    let config = ServerConfig {
        tick_rate_ms: 200,
        friction: 1.0, // Без трения для изоляции ускорения от аномалии
        max_acceleration: 5.0,
        max_velocity: 50.0,
        ..Default::default()
    };

    let engine = GameEngine::new(config);

    // Добавляем игрока, сокровище и аномалию
    {
        let shared_state = engine.shared_state();
        let mut state = shared_state.write().await;

        let player = PlayerState {
            id: "hero".to_string(),
            score: 0,
            status: "normal".to_string(),
            position: (0.0, 0.0),
            velocity: (0.0, 0.0),
            acceleration: (0.0, 0.0),
            max_acceleration: 5.0,
            max_velocity: 50.0,
            stun_remaining_ticks: 0,
            carpets: std::collections::HashMap::new(),
        };
        state.world.players.insert(player.id.clone(), player);

        // Вихрь в точке (20.0, 0.0) тянет игрока вправо с силой 5.0
        state
            .world
            .anomalies
            .push(AnomalyState::new("vortex", (20.0, 0.0), 50.0, 5.0));

        // Сокровище в точке (1.0, 0.0)
        state.world.treasures.push(TreasureState {
            id: "chest_1".to_string(),
            r#type: "chest".to_string(),
            position: (1.0, 0.0),
            value: 250,
            is_collected: false,
        });
    }

    // Игрок не подает команд ускорения, но вихрь должен придать ему скорость вправо
    engine.step_once().await;

    let snapshot = engine.get_snapshot();
    let player = snapshot.world.players.get("hero").unwrap();

    // Сила вихря F = (5.0, 0.0), dt = 0.2.
    // Скорость V = 0 + 5.0 * 0.2 = 1.0
    // Позиция P = 0 + 1.0 * 0.2 = 0.2
    assert!((player.velocity.0 - 1.0).abs() < 1e-7);
    assert!((player.position.0 - 0.2).abs() < 1e-7);

    // Сокровище находится в (1.0, 0.0), расстояние от игрока (0.2, 0.0) равно 0.8 <= 10.0 (capture_radius)
    // Сокровище должно быть захвачено!
    assert_eq!(player.score, 250);
    assert_eq!(snapshot.world.treasures.len(), 0);
}

/// Сквозная интеграция GameEngine: центр аномалии не отключает управление
#[tokio::test]
async fn test_game_engine_applies_command_in_anomaly_epicenter() {
    let config = ServerConfig {
        tick_rate_ms: 200,
        enable_respawn: false,
        ..Default::default()
    };

    let engine = GameEngine::new(config);

    // Игрок прямо в эпицентре аномалии
    {
        let shared_state = engine.shared_state();
        let mut state = shared_state.write().await;

        let mut player = PlayerState::new("stun_tester".to_string(), 10.0, 10.0, 5.0, 20.0);
        player.carpets.clear();
        state.world.players.insert(player.id.clone(), player);

        // Аномалия в той же точке (10.0, 10.0), радиус 40.0, эпицентр = 10.0
        state
            .world
            .anomalies
            .push(AnomalyState::new("vortex_center", (10.0, 10.0), 40.0, 2.0));
    }

    engine
        .register_command("stun_tester".to_string(), PlayerCommand::new(5.0, 0.0))
        .unwrap();
    engine.step_once().await;

    let snapshot = engine.get_snapshot();
    let player = snapshot.world.players.get("stun_tester").unwrap();
    assert_eq!(player.status, "normal");
    assert_eq!(player.acceleration, (5.0, 0.0));
}

/// FE-013: ковер, покинувший арену, получает обычный путь гибели и респавна.
#[tokio::test]
async fn test_out_of_bounds_carpet_respawns_by_default() {
    let config = ServerConfig {
        arena_width: 200.0,
        arena_height: 200.0,
        ..Default::default()
    };
    assert!(config.enable_respawn);
    let engine = GameEngine::new(config);

    {
        let shared = engine.shared_state();
        let mut state = shared.write().await;
        let mut player = PlayerState::new("boundary_runner".to_string(), 100.0, 100.0, 5.0, 20.0);
        let carpet = player.carpets.get_mut("boundary_runner_0").unwrap();
        carpet.position = (-1.0, 100.0);
        player.sync_from_carpets();
        state.world.players.insert(player.id.clone(), player);
    }

    engine.step_once().await;

    let snapshot = engine.get_snapshot();
    let carpet = snapshot
        .world
        .players
        .get("boundary_runner")
        .unwrap()
        .carpets
        .get("boundary_runner_0")
        .unwrap();
    assert_eq!(carpet.status, "normal");
    assert!((0.0..=200.0).contains(&carpet.position.0));
    assert!((0.0..=200.0).contains(&carpet.position.1));
    assert_eq!(carpet.velocity, (0.0, 0.0));
    assert_eq!(carpet.acceleration, (0.0, 0.0));
}

/// DR-003 Бизнес-правило 1: "Правило одновременного сбора:
/// награда присуждается игроку с наименьшим евклидовым расстоянием"
#[tokio::test]
async fn test_competitive_treasure_capture_closest_player_wins() {
    let config = ServerConfig {
        tick_rate_ms: 200,
        ..Default::default()
    };
    let engine = GameEngine::new(config);

    {
        let shared_state = engine.shared_state();
        let mut state = shared_state.write().await;

        // Игрок 1 в точке (2.0, 0.0) -> расстояние до сокровища в (0, 0) = 2.0
        let p1 = PlayerState::new("player_1".to_string(), 2.0, 0.0, 5.0, 20.0);
        // Игрок 2 в точке (-9.5, 0.0) -> расстояние до сокровища в (0, 0) = 9.5 (дистанция между p1 и p2 = 11.5 > 2*R_player)
        let p2 = PlayerState::new("player_2".to_string(), -9.5, 0.0, 5.0, 20.0);

        state.world.players.insert(p1.id.clone(), p1);
        state.world.players.insert(p2.id.clone(), p2);

        // Сокровище в (0, 0) с ценностью 500
        state.world.treasures.push(TreasureState {
            id: "treasure_gem".to_string(),
            r#type: "gem".to_string(),
            position: (0.0, 0.0),
            value: 500,
            is_collected: false,
        });
    }

    engine.step_once().await;

    let snapshot = engine.get_snapshot();
    let p1 = snapshot.world.players.get("player_1").unwrap();
    let p2 = snapshot.world.players.get("player_2").unwrap();

    // Player 1 ближе (d = 2.0 < 9.5 <= 10.0), поэтому он забирает сокровище
    assert_eq!(p1.score, 500);
    assert_eq!(p2.score, 0);
    assert_eq!(snapshot.world.treasures.len(), 0);
}

/// FE-006: test_dynamic_anomaly_lifecycle_spawn_cross_despawn
/// Полный сквозной цикл жизни аномалии: рождение за пределами арены,
/// пересечение арены и деспавн после выхода за терминальную границу.
#[tokio::test]
async fn test_dynamic_anomaly_lifecycle_spawn_cross_despawn() {
    let config = ServerConfig {
        tick_rate_ms: 200, // dt = 0.2 s
        arena_width: 1000.0,
        arena_height: 1000.0,
        ..Default::default()
    };
    let engine = GameEngine::new(config);

    // Добавляем динамическую аномалию, летящую слева направо через арену
    // P_spawn = (-100.0, 500.0), V = (100.0, 0.0), R_effect = 80.0
    {
        let shared_state = engine.shared_state();
        let mut state = shared_state.write().await;
        state.world.anomalies.push(AnomalyState::new_dynamic(
            "dyn_vortex_1",
            "attracting",
            (-100.0, 500.0),
            (100.0, 0.0),
            15.0,
            80.0,
            4.0,
        ));
    }

    // Такт 1: смещение на 100.0 * 0.2 = 20.0 -> x = -80.0.
    // Расстояние до арены = 80.0 <= R_effect (80.0) -> аномалия не деспавнится
    engine.step_once().await;
    let snap1 = engine.get_snapshot();
    assert_eq!(snap1.world.anomalies.len(), 1);
    assert!((snap1.world.anomalies[0].position.0 - (-80.0)).abs() < 1e-6);

    // Продвигаем еще 30 тиков (6.0 секунд) -> смещение на 600.0 -> x = 520.0 (внутри арены)
    for _ in 0..30 {
        engine.step_once().await;
    }
    let snap2 = engine.get_snapshot();
    assert_eq!(snap2.world.anomalies.len(), 1);
    assert!((snap2.world.anomalies[0].position.0 - 520.0).abs() < 1e-6);

    // Продвигаем еще 30 тиков (6.0 секунд) -> смещение на 600.0 -> x = 1120.0.
    // Удаление от правой границы 1000.0 составляет 120.0 > R_effect (80.0),
    // и вектор скорости направлен наружу -> аномалия деспавнится и удаляется
    for _ in 0..30 {
        engine.step_once().await;
    }
    let snap3 = engine.get_snapshot();
    assert_eq!(snap3.world.anomalies.len(), 0);
}

/// FE-006: test_player_elimination_in_game_loop
/// Подтверждение прекращения симуляции ковра и блокировки команд при попадании в ядро вихря
#[tokio::test]
async fn test_player_elimination_in_game_loop() {
    let config = ServerConfig {
        tick_rate_ms: 200,
        enable_respawn: false,
        ..Default::default()
    };
    let engine = GameEngine::new(config);

    let player_id = "doomed_carpet".to_string();

    {
        let shared_state = engine.shared_state();
        let mut state = shared_state.write().await;

        // Игрок в (50.0, 50.0)
        let player = PlayerState::new(player_id.clone(), 50.0, 50.0, 5.0, 20.0);
        state.world.players.insert(player.id.clone(), player);

        // Динамическая аномалия в (60.0, 50.0) со смертоносным ядром R_core = 15.0
        // Дистанция = 10.0 <= 15.0 + 5.0 (player_radius)
        state.world.anomalies.push(AnomalyState::new_dynamic(
            "death_vortex",
            "attracting",
            (60.0, 50.0),
            (0.0, 0.0),
            15.0,
            60.0,
            4.0,
        ));

        // Сокровище рядом в точке (52.0, 50.0)
        state.world.treasures.push(TreasureState {
            id: "uncollected_chest".to_string(),
            r#type: "chest".to_string(),
            position: (52.0, 50.0),
            value: 200,
            is_collected: false,
        });
    }

    // Шаг симуляции: детекция летального касания ядра
    engine.step_once().await;

    let snapshot = engine.get_snapshot();
    let p = snapshot.world.players.get(&player_id).unwrap();

    // 1. Статус игрока переводится в "destroyed"
    assert_eq!(p.status, "destroyed");
    assert!(p.is_destroyed());
    assert_eq!(p.velocity, (0.0, 0.0));

    // 2. Игрок не смог собрать сокровище (очки = 0, сокровище не собрано)
    assert_eq!(p.score, 0);
    assert_eq!(snapshot.world.treasures.len(), 1);

    // 3. Попытка подать команду уничтоженным игроком блокируется с ошибкой PlayerDestroyed
    let cmd_result = engine.register_command(player_id.clone(), PlayerCommand::new(5.0, 0.0));
    assert_eq!(cmd_result, Err(CommandError::PlayerDestroyed));

    // 4. Последующие тики: игрок остается уничтоженным, не двигается
    let pos_before = p.position;
    engine.step_once().await;
    let snap2 = engine.get_snapshot();
    let p2 = snap2.world.players.get(&player_id).unwrap();
    assert_eq!(p2.status, "destroyed");
    assert_eq!(p2.position, pos_before);
    assert_eq!(p2.velocity, (0.0, 0.0));
}

/// TC-COIN-01 / AC-01: Поддержание квоты монет в WorldSpatialEngine через resolve()
#[test]
fn test_world_spatial_engine_coin_quota_replenish() {
    let config = CoinSpawnerConfig {
        max_coins: 10,
        base_value: 25,
        arena_width: 1000.0,
        arena_height: 1000.0,
        margin: 40.0,
        max_value_cap: 1000,
    };
    let spawner = CoinSpawner::with_seed(config, 101);
    let mut engine = WorldSpatialEngine::default().with_coin_spawner(spawner);
    let mut world = WorldData::new();

    // 1. Первый такт: спавнер восполняет монеты ровно до квоты (10 шт)
    engine.resolve(&mut world, 0.2);
    assert_eq!(world.treasures.len(), 10);

    // Все монеты имеют неотрицательный номинал >= base_value
    for coin in &world.treasures {
        assert!(coin.value >= 25);
        assert!(!coin.is_collected);
    }

    // 2. Игрок подходит к одной из монет и собирает ее
    let target_coin = world.treasures[0].clone();
    let player = PlayerState::new(
        "p_collector".to_string(),
        target_coin.position.0,
        target_coin.position.1,
        5.0,
        20.0,
    );
    world.players.insert(player.id.clone(), player);

    // 3. Такт со сбором: монета захватывается, очки начисляются
    engine.resolve(&mut world, 0.2);

    let p = world.players.get("p_collector").unwrap();
    assert_eq!(p.score, target_coin.value);

    // 4. На следующем такте квота снова автоматически восполнена до 10 монет
    // (поскольку в начале resolve выполняется replenish)
    assert_eq!(world.treasures.len(), 10);
}

/// Собирает монету при пересечении суммы радиусов между endpoint-ами тика.
#[test]
fn test_bounty_is_collected_on_partial_swept_overlap() {
    let mut engine = WorldSpatialEngine::default().with_arena(1000.0, 1000.0);
    let mut world = WorldData::new();
    let mut player = PlayerState::new("sweeper".to_string(), 100.0, 100.0, 40.0, 110.0);
    player.carpets.clear();
    world.players.insert(player.id.clone(), player);

    // Capture the start of even the very first physical tick.
    engine.capture_start_positions(&world);
    world.players.get_mut("sweeper").unwrap().position = (120.0, 100.0);
    world.treasures.push(TreasureState {
        id: "partial-overlap".into(),
        r#type: "coin".into(),
        position: (110.0, 109.999), // 9.999 units from the swept centerline.
        value: 75,
        is_collected: false,
    });

    engine.resolve(&mut world, 0.2);

    assert_eq!(world.players["sweeper"].score, 75);
    assert!(world.treasures.is_empty());
}

/// Касание не срабатывает, если круги всё ещё разделены даже на малую величину.
#[test]
fn test_bounty_swept_near_miss_outside_combined_radius_is_not_collected() {
    let mut engine = WorldSpatialEngine::default().with_arena(1000.0, 1000.0);
    let mut world = WorldData::new();
    let mut player = PlayerState::new("near-miss".to_string(), 100.0, 100.0, 40.0, 110.0);
    player.carpets.clear();
    world.players.insert(player.id.clone(), player);

    engine.capture_start_positions(&world);
    world.players.get_mut("near-miss").unwrap().position = (120.0, 100.0);
    world.treasures.push(TreasureState {
        id: "outside".into(),
        r#type: "coin".into(),
        position: (110.0, 110.001),
        value: 75,
        is_collected: false,
    });

    engine.resolve(&mut world, 0.2);

    assert_eq!(world.players["near-miss"].score, 0);
    assert_eq!(world.treasures.len(), 1);
}

/// TC-COIN-02 / AC-03: Прогрессивный рост номинала монет от номера тика
#[test]
fn test_progressive_coin_spawner_value_across_ticks() {
    let config = CoinSpawnerConfig::default();
    let mut spawner = CoinSpawner::with_seed(config, 55555);

    // Измерение на тике T=10
    let mut sum_10 = 0u64;
    for _ in 0..1000 {
        sum_10 += spawner.calculate_value(10) as u64;
    }
    let avg_10 = sum_10 as f64 / 1000.0;

    // Измерение на тике T=2000
    let mut sum_2000 = 0u64;
    for _ in 0..1000 {
        sum_2000 += spawner.calculate_value(2000) as u64;
    }
    let avg_2000 = sum_2000 as f64 / 1000.0;

    assert!(
        avg_2000 > avg_10 * 3.0,
        "Средний номинал монет на тике T=2000 ({}) должен быть значительно больше, чем на тике T=10 ({})",
        avg_2000,
        avg_10
    );

    // Проверка предельного потолка ценности
    for _ in 0..50 {
        assert_eq!(spawner.calculate_value(100_000), 1000);
    }
}

/// FE-009: Полный интеграционный цикл в GameEngine с включенным генератором монет
#[tokio::test]
async fn test_game_engine_progressive_coin_spawner_integration() {
    let config = ServerConfig {
        arena_width: 500.0,
        arena_height: 500.0,
        bounty_quota: 30,
        enable_coin_spawner: true,
        enable_spawner: false,
        ..Default::default()
    };
    let engine = GameEngine::new(config);

    let player_id = "test_player".to_string();

    // Регистрируем игрока в точке (250.0, 250.0)
    {
        let shared = engine.shared_state();
        let mut state = shared.write().await;
        state.world.players.insert(
            player_id.clone(),
            PlayerState::new(player_id.clone(), 250.0, 250.0, 5.0, 20.0),
        );
    }

    // Выполняем 1 такт: спавнятся 30 монет согласно базовой квоте FE-017
    engine.step_once().await;

    let snap1 = engine.get_snapshot();
    assert_eq!(snap1.world.treasures.len(), 30);
    assert_eq!(snap1.tick, 1);

    // Перемещаем одну из монет прямо под ноги игрока для гарантированного сбора
    let (target_coin_value, score_before_capture) = {
        let shared = engine.shared_state();
        let mut state = shared.write().await;
        state.world.treasures[0].position = (250.0, 250.0);
        (
            state.world.treasures[0].value,
            state.world.players.get(&player_id).unwrap().score,
        )
    };

    // Выполняем такт сбора: монета должна быть захвачена
    engine.step_once().await;

    let snap2 = engine.get_snapshot();
    let player = snap2.world.players.get(&player_id).unwrap();
    assert_eq!(player.score, score_before_capture + target_coin_value);

    // Выполняем еще один такт: монета должна быть автоматически восполнена
    engine.step_once().await;
    let snap3 = engine.get_snapshot();
    assert_eq!(snap3.world.treasures.len(), 30);
}

/// FE-011 / TC-COLL-01: Лобовое сближение двух ковров на дистанцию <= 2 * R_player взаимно уничтожает оба ковра.
#[tokio::test]
async fn test_tc_coll_01_carpet_carpet_mutual_destruction() {
    let config = ServerConfig {
        tick_rate_ms: 200,
        enable_respawn: false,
        ..Default::default()
    };
    let engine = GameEngine::new(config);

    {
        let shared = engine.shared_state();
        let mut state = shared.write().await;

        let mut p1 = PlayerState::new("player_1".to_string(), 100.0, 100.0, 5.0, 20.0);
        let mut p2 = PlayerState::new("player_2".to_string(), 800.0, 800.0, 5.0, 20.0);
        p1.score = 100;
        p2.score = 100;

        // Ковер player_2_0 находится на расстоянии 8.0 <= 2 * R_player (10.0) от player_1_0
        if let Some(c2_0) = p2.carpets.get_mut("player_2_0") {
            c2_0.position = (108.0, 100.0);
        }

        state.world.players.insert(p1.id.clone(), p1);
        state.world.players.insert(p2.id.clone(), p2);
    }

    engine.step_once().await;

    let snap = engine.get_snapshot();
    let p1 = snap.world.players.get("player_1").unwrap();
    let p2 = snap.world.players.get("player_2").unwrap();

    let c1_0 = p1.carpets.get("player_1_0").unwrap();
    let c2_0 = p2.carpets.get("player_2_0").unwrap();

    assert_eq!(c1_0.status, "destroyed");
    assert_eq!(c2_0.status, "destroyed");
    // Списание штрафа за уничтожение ковра (по 50 очков с каждого игрока)
    assert_eq!(p1.score, 50);
    assert_eq!(p2.score, 50);
}

/// FE-011 / TC-COLL-02: Столкновение двух ковров одной команды наказывает игрока двойным штрафом (2 * P_death).
#[tokio::test]
async fn test_tc_coll_02_same_team_carpet_collision_double_penalty() {
    let config = ServerConfig {
        tick_rate_ms: 200,
        enable_respawn: false,
        ..Default::default()
    };
    let engine = GameEngine::new(config);

    {
        let shared = engine.shared_state();
        let mut state = shared.write().await;

        let mut p1 = PlayerState::new("team_alpha".to_string(), 300.0, 300.0, 5.0, 20.0);
        p1.score = 200;

        // Размещаем два ковра одной команды вплотную друг к другу (d = 5.0 <= 10.0)
        if let Some(c1) = p1.carpets.get_mut("team_alpha_1") {
            c1.position = (300.0, 300.0);
        }
        if let Some(c2) = p1.carpets.get_mut("team_alpha_2") {
            c2.position = (305.0, 300.0);
        }

        // Остальные ковры отодвигаем далеко
        if let Some(c0) = p1.carpets.get_mut("team_alpha_0") {
            c0.position = (100.0, 100.0);
        }
        if let Some(c3) = p1.carpets.get_mut("team_alpha_3") {
            c3.position = (700.0, 700.0);
        }
        if let Some(c4) = p1.carpets.get_mut("team_alpha_4") {
            c4.position = (800.0, 800.0);
        }

        state.world.players.insert(p1.id.clone(), p1);
    }

    engine.step_once().await;

    let snap = engine.get_snapshot();
    let player = snap.world.players.get("team_alpha").unwrap();

    let c1 = player.carpets.get("team_alpha_1").unwrap();
    let c2 = player.carpets.get("team_alpha_2").unwrap();

    assert_eq!(c1.status, "destroyed");
    assert_eq!(c2.status, "destroyed");
    // Двойной штраф: 200 - 2 * 50 = 100
    assert_eq!(player.score, 100);
}

/// FE-011 / TC-COLL-03: Ковер, оказавшийся за пределами арены, немедленно уничтожается со штрафом.
#[tokio::test]
async fn test_tc_coll_03_out_of_bounds_carpet_destruction() {
    let config = ServerConfig {
        tick_rate_ms: 200,
        arena_width: 1000.0,
        arena_height: 1000.0,
        enable_respawn: false,
        ..Default::default()
    };
    let engine = GameEngine::new(config);

    {
        let shared = engine.shared_state();
        let mut state = shared.write().await;

        let mut p1 = PlayerState::new("boundary_tester".to_string(), 500.0, 500.0, 5.0, 20.0);
        p1.score = 150;

        // Помещаем один ковер за левую границу арены (x = -10.0)
        if let Some(c) = p1.carpets.get_mut("boundary_tester_0") {
            c.position = (-10.0, 500.0);
        }
        // Помещаем второй ковер за верхнюю границу арены (y = 1010.0)
        if let Some(c) = p1.carpets.get_mut("boundary_tester_1") {
            c.position = (500.0, 1010.0);
        }

        state.world.players.insert(p1.id.clone(), p1);
    }

    engine.step_once().await;

    let snap = engine.get_snapshot();
    let player = snap.world.players.get("boundary_tester").unwrap();

    let c0 = player.carpets.get("boundary_tester_0").unwrap();
    let c1 = player.carpets.get("boundary_tester_1").unwrap();

    assert_eq!(c0.status, "destroyed");
    assert_eq!(c1.status, "destroyed");
    // Штраф за 2 погибших за границами ковра: 150 - 2 * 50 = 50
    assert_eq!(player.score, 50);
}

/// FE-011 / TC-COLL-04: После гибели ковер безопасно переспавнивается в пределах арены,
/// на безопасном расстоянии от смертоносных зон аномалий и других живых ковров.
#[tokio::test]
async fn test_tc_coll_04_safe_carpet_respawn_avoiding_hazards() {
    let config = ServerConfig {
        tick_rate_ms: 200,
        arena_width: 1000.0,
        arena_height: 1000.0,
        enable_respawn: true,
        ..Default::default()
    };
    let engine = GameEngine::new(config);

    let anomaly_pos = (500.0, 500.0);
    let core_radius = 40.0;

    {
        let shared = engine.shared_state();
        let mut state = shared.write().await;

        state.world.anomalies.push(AnomalyState {
            id: "hazard_vortex".to_string(),
            position: anomaly_pos,
            velocity: (0.0, 0.0),
            radius: 100.0,
            force: 5.0,
            core_radius,
            anomaly_type: "attracting".to_string(),
        });

        let mut p1 = PlayerState::new("respawn_tester".to_string(), 200.0, 200.0, 5.0, 20.0);
        p1.score = 100;

        // Отправляем ковер 0 за границу для немедленной гибели
        if let Some(c0) = p1.carpets.get_mut("respawn_tester_0") {
            c0.position = (-25.0, 200.0);
        }

        state.world.players.insert(p1.id.clone(), p1);
    }

    engine.step_once().await;

    let snap = engine.get_snapshot();
    let player = snap.world.players.get("respawn_tester").unwrap();

    // Штраф был списан
    assert_eq!(player.score, 50);

    // Ковер переспавнился и стал normal с нулевой скоростью
    let c0 = player.carpets.get("respawn_tester_0").unwrap();
    assert_eq!(c0.status, "normal");
    assert_eq!(c0.velocity, (0.0, 0.0));

    // Проверка границ арены с отступом 50.0: [50.0, 950.0]
    assert!(c0.position.0 >= 50.0 && c0.position.0 <= 950.0);
    assert!(c0.position.1 >= 50.0 && c0.position.1 <= 950.0);

    // Проверка безопасного расстояния до ядра аномалии: dist > core_radius + 50.0
    let dx = c0.position.0 - anomaly_pos.0;
    let dy = c0.position.1 - anomaly_pos.1;
    let dist_to_core = (dx * dx + dy * dy).sqrt();
    assert!(
        dist_to_core > core_radius + 50.0,
        "Respawned carpet too close to anomaly core: dist = {dist_to_core}"
    );

    // Проверка безопасного расстояния до других живых ковров: dist > 4 * R_player (20.0)
    for (id, other_carpet) in &player.carpets {
        if id != "respawn_tester_0" && !other_carpet.is_destroyed() {
            let cdx = c0.position.0 - other_carpet.position.0;
            let cdy = c0.position.1 - other_carpet.position.1;
            let dist_to_carpet = (cdx * cdx + cdy * cdy).sqrt();
            assert!(
                dist_to_carpet > 20.0,
                "Respawned carpet too close to carpet {id}: dist = {dist_to_carpet}"
            );
        }
    }
}
