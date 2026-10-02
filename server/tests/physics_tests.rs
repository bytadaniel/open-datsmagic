//! # Интеграционные тесты физического движка (FE-002)
//!
//! Проверяет формулы векторной кинематики, схему численного интегрирования Эйлера,
//! затухание от трения, ограничение ускорения и скорости в соответствии со
//! спецификацией [`FE-002`](file:///Users/d.byta/Documents/Code/dats/datsmagic/docs/features/FE-002-euler-physics-engine.md),
//! доменными требованиями [`DR-002`](file:///Users/d.byta/Documents/Code/dats/datsmagic/docs/domain/DR-002-vector-physics.md)
//! и базовой механикой [`docs/mechanics.md`](file:///Users/d.byta/Documents/Code/dats/datsmagic/docs/mechanics.md).

use server::config::ServerConfig;
use server::engine::{GameEngine, PlayerCommand, PlayerState};
use server::physics::{
    apply_friction, combine_forces, compute_effective_accel, EulerIntegrator, PhysicsIntegrator,
    Vec2, WorldPhysicsEngine,
};

/// TC-PHY-01: Тест затухания скорости от трения (test_friction_decay)
/// Спецификация: "при отсутствии ускорения начальная скорость (10, 0) через 1 шаг становится
/// (10 * 0.98, 0) = (9.8, 0)"
#[test]
fn test_friction_decay() {
    let integrator = EulerIntegrator;
    let initial_pos = Vec2::new(100.0, 100.0);
    let initial_vel = Vec2::new(10.0, 0.0);
    let command_accel = Vec2::ZERO;
    let env_forces = Vec2::ZERO;
    let max_accel = 5.0;
    let max_velocity = 20.0;
    let friction = 0.98;
    let dt = 0.2;

    let (new_pos, new_vel) = integrator.step(
        initial_pos,
        initial_vel,
        command_accel,
        env_forces,
        max_accel,
        max_velocity,
        friction,
        dt,
    );

    // Новая скорость: V_new = V_old * 0.98 = (9.8, 0.0)
    assert!(
        (new_vel.x - 9.8).abs() < 1e-9,
        "Ожидалась скорость x = 9.8, получено {}",
        new_vel.x
    );
    assert!(
        (new_vel.y - 0.0).abs() < 1e-9,
        "Ожидалась скорость y = 0.0, получено {}",
        new_vel.y
    );

    // Новая позиция: P_new = P_old + V_new * dt = 100.0 + 9.8 * 0.2 = 101.96
    let expected_x = 100.0 + 9.8 * dt;
    assert!(
        (new_pos.x - expected_x).abs() < 1e-9,
        "Ожидалась координата x = {}, получено {}",
        expected_x,
        new_pos.x
    );
    assert_eq!(new_pos.y, 100.0);
}

/// TC-PHY-01: Экспоненциальное затухание скорости за серию шагов
#[test]
fn test_friction_decay_multi_step() {
    let integrator = EulerIntegrator;
    let mut pos = Vec2::ZERO;
    let mut vel = Vec2::new(10.0, 0.0);
    let friction = 0.98;
    let dt = 0.2;

    for step in 1..=10 {
        let (next_pos, next_vel) =
            integrator.step(pos, vel, Vec2::ZERO, Vec2::ZERO, 5.0, 20.0, friction, dt);
        let expected_speed = 10.0 * friction.powi(step);
        assert!(
            (next_vel.x - expected_speed).abs() < 1e-7,
            "Шаг {}: ожидалась скорость {}, получено {}",
            step,
            expected_speed,
            next_vel.x
        );
        pos = next_pos;
        vel = next_vel;
    }
}

/// TC-PHY-02: Ограничение ускорения (test_acceleration_clamping)
/// Спецификация: "при заявленном векторе (10, 0) с A_max = 5.0 применяется вектор (5.0, 0)"
#[test]
fn test_acceleration_clamping() {
    let integrator = EulerIntegrator;
    let initial_pos = Vec2::ZERO;
    let initial_vel = Vec2::ZERO;
    let command_accel = Vec2::new(10.0, 0.0);
    let env_forces = Vec2::ZERO;
    let max_accel = 5.0;
    let max_velocity = 20.0;
    let friction = 1.0; // Без трения для изоляции ускорения
    let dt = 0.2;

    let (_, new_vel) = integrator.step(
        initial_pos,
        initial_vel,
        command_accel,
        env_forces,
        max_accel,
        max_velocity,
        friction,
        dt,
    );

    // Применяемое ускорение равно (5.0, 0.0). Скорость V = 5.0 * 0.2 = 1.0
    assert!(
        (new_vel.x - 1.0).abs() < 1e-9,
        "Ожидалась скорость 1.0, получено {}",
        new_vel.x
    );
    assert!((new_vel.y - 0.0).abs() < 1e-9);
}

/// TC-PHY-02: Ограничение диагонального ускорения с сохранением угла
#[test]
fn test_diagonal_acceleration_clamping() {
    let integrator = EulerIntegrator;
    let initial_pos = Vec2::ZERO;
    let initial_vel = Vec2::ZERO;
    // Вектор (6.0, 8.0) с длиной sqrt(36 + 64) = 10.0
    let command_accel = Vec2::new(6.0, 8.0);
    let max_accel = 5.0;
    let dt = 1.0;

    let (_, new_vel) = integrator.step(
        initial_pos,
        initial_vel,
        command_accel,
        Vec2::ZERO,
        max_accel,
        50.0,
        1.0,
        dt,
    );

    // Должен масштабироваться вдвое: (3.0, 4.0) с длиной ровно 5.0
    assert!((new_vel.x - 3.0).abs() < 1e-9);
    assert!((new_vel.y - 4.0).abs() < 1e-9);
    assert!((new_vel.length() - 5.0).abs() < 1e-9);
}

/// TC-PHY-02: Ограничение предельной скорости (test_velocity_clamping)
/// Спецификация: "при разгоне скорость никогда не превышает V_max = 20.0"
#[test]
fn test_velocity_clamping() {
    let integrator = EulerIntegrator;
    let mut pos = Vec2::ZERO;
    let mut vel = Vec2::ZERO;
    let max_accel = 5.0;
    let max_velocity = 20.0;
    let dt = 0.2;
    let friction = 0.98;

    // Симулируем непрерывный разгон в течение 100 шагов
    for _ in 0..100 {
        let (next_pos, next_vel) = integrator.step(
            pos,
            vel,
            Vec2::new(10.0, 10.0), // Максимальная тяга
            Vec2::ZERO,
            max_accel,
            max_velocity,
            friction,
            dt,
        );
        assert!(
            next_vel.length() <= max_velocity + 1e-9,
            "Скорость {} превысила V_max {}",
            next_vel.length(),
            max_velocity
        );
        pos = next_pos;
        vel = next_vel;
    }

    // Скорость должна быть насыщена ровно до max_velocity
    assert!((vel.length() - max_velocity).abs() < 1e-7);
}

/// TC-PHY-03: Сильная внешняя сила не отключает командное ускорение.
#[test]
fn test_anomaly_force_does_not_disable_command_accel() {
    let integrator = EulerIntegrator;
    let initial_pos = Vec2::ZERO;
    let initial_vel = Vec2::new(10.0, 0.0);
    let command_accel = Vec2::new(5.0, 0.0);
    let env_forces = Vec2::new(-2.0, 0.0);
    let max_accel = 5.0;
    let max_velocity = 20.0;
    let friction = 0.98;
    let dt = 0.2;

    let (_, new_vel) = integrator.step(
        initial_pos,
        initial_vel,
        command_accel,
        env_forces,
        max_accel,
        max_velocity,
        friction,
        dt,
    );

    // V = (10 * .98) + (5 - 2) * .2 = 10.4.
    assert!((new_vel.x - 10.4).abs() < 1e-9);
    assert!((new_vel.y - 0.0).abs() < 1e-9);
}

/// TC-PHY-03: Команда и сила окружения складываются в полном объеме.
#[test]
fn test_command_and_environment_forces_are_both_applied() {
    let integrator = EulerIntegrator;
    let initial_pos = Vec2::ZERO;
    let initial_vel = Vec2::ZERO;
    let command_accel = Vec2::new(5.0, 0.0);
    let env_forces = Vec2::new(0.0, 3.0); // Внешний вихрь
    let dt = 1.0;

    let (_, new_vel) = integrator.step(
        initial_pos,
        initial_vel,
        command_accel,
        env_forces,
        5.0,
        20.0,
        1.0,
        dt,
    );

    // Тяга (5, 0) и вихрь (0, 3) приложены одновременно.
    assert!((new_vel.x - 5.0).abs() < 1e-9);
    assert!((new_vel.y - 3.0).abs() < 1e-9);
}

/// Защита от NaN, Infinity и нулевых векторов (Error Handling)
#[test]
fn test_error_handling_robustness() {
    let integrator = EulerIntegrator;

    // Вектор с NaN
    let (pos, vel) = integrator.step(
        Vec2::new(f64::NAN, 0.0),
        Vec2::new(0.0, f64::INFINITY),
        Vec2::new(f64::NAN, f64::NAN),
        Vec2::new(f64::NEG_INFINITY, 0.0),
        f64::NAN,
        f64::INFINITY,
        f64::NAN,
        f64::NAN,
    );

    assert!(pos.is_finite());
    assert!(vel.is_finite());
    assert_eq!(pos, Vec2::ZERO);
    assert_eq!(vel, Vec2::ZERO);

    // Нулевая нормализация
    assert_eq!(Vec2::ZERO.normalize_or_zero(), Vec2::ZERO);
}

/// Интеграция с GameEngine: проверка движения зарегистрированного игрока в тактах
#[tokio::test]
async fn test_game_engine_physics_integration() {
    let config = ServerConfig {
        tick_rate_ms: 200,
        friction: 0.98,
        max_acceleration: 5.0,
        max_velocity: 20.0,
        ..Default::default()
    };

    let engine = GameEngine::new(config);

    // Добавляем игрока в состояние
    {
        let shared_state = engine.shared_state();
        let mut state = shared_state.write().await;
        let player = PlayerState {
            id: "team_alpha".to_string(),
            score: 0,
            status: "normal".to_string(),
            position: (0.0, 0.0),
            velocity: (10.0, 0.0),
            acceleration: (0.0, 0.0),
            max_acceleration: 5.0,
            max_velocity: 20.0,
            stun_remaining_ticks: 0,
            carpets: std::collections::HashMap::new(),
        };
        state.world.players.insert(player.id.clone(), player);
    }

    // Регистрируем команду ускорения (5.0, 0.0)
    engine
        .register_command("team_alpha".to_string(), PlayerCommand::new(5.0, 0.0))
        .expect("Failed to register command");

    // Выполняем один такт симуляции
    engine.step_once().await;

    // Проверяем обновленное состояние через актуальный снимок
    let snapshot = engine.get_snapshot();
    let player = snapshot.world.players.get("team_alpha").unwrap();

    // V_new = (10.0 * 0.98) + (5.0 * 0.2) = 9.8 + 1.0 = 10.8
    assert!((player.velocity.0 - 10.8).abs() < 1e-7);
    assert!((player.velocity.1 - 0.0).abs() < 1e-7);
    assert_eq!(player.acceleration, (5.0, 0.0));

    // P_new = 0.0 + 10.8 * 0.2 = 2.16
    assert!((player.position.0 - 2.16).abs() < 1e-7);
    assert!((player.position.1 - 0.0).abs() < 1e-7);
}

/// Тестирование вспомогательных функций сил и трения (forces.rs)
#[test]
fn test_forces_helpers() {
    let accel = compute_effective_accel(Vec2::new(10.0, 0.0), 4.0);
    assert_eq!(accel, Vec2::new(4.0, 0.0));

    let env_accel = compute_effective_accel(Vec2::new(10.0, 0.0), 4.0);
    assert_eq!(env_accel, Vec2::new(4.0, 0.0));

    let decayed_vel = apply_friction(Vec2::new(10.0, 20.0), 0.5);
    assert_eq!(decayed_vel, Vec2::new(5.0, 10.0));

    let combined = combine_forces(Vec2::new(1.0, 2.0), Vec2::new(3.0, 4.0));
    assert_eq!(combined, Vec2::new(4.0, 6.0));
}

/// Прямое модульное тестирование WorldPhysicsEngine
#[test]
fn test_world_physics_engine_direct_step() {
    use server::engine::{PhysicsStepHandler, WorldData};
    use std::collections::HashMap;

    let mut engine = WorldPhysicsEngine::new(0.98);
    let mut world = WorldData::new();

    let mut player = PlayerState::new("p1".to_string(), 0.0, 0.0, 5.0, 20.0);
    player.velocity = (10.0, 0.0);
    player.sync_primary_carpet();
    world.players.insert(player.id.clone(), player);

    let mut commands = HashMap::new();
    commands.insert("p1".to_string(), PlayerCommand::new(0.0, 0.0));

    engine.step(&mut world, &commands, 0.2);

    let updated = world.players.get("p1").unwrap();
    assert!((updated.velocity.0 - 9.8).abs() < 1e-9);
    assert!((updated.position.0 - 9.8 * 0.2).abs() < 1e-9);
}

/// Missing a single network packet must not turn a previously applied carpet command into zero.
#[test]
fn test_carpet_holds_last_acceleration_when_tick_has_no_new_command() {
    use server::engine::{PhysicsStepHandler, WorldData};
    use std::collections::HashMap;

    let mut engine = WorldPhysicsEngine::new(0.98);
    let mut world = WorldData::new();
    let mut player = PlayerState::new("p2".to_string(), 100.0, 100.0, 5.0, 20.0);
    player.carpets.get_mut("p2_0").unwrap().velocity = (0.0, 0.0);
    world.players.insert(player.id.clone(), player);

    let mut commands = HashMap::new();
    commands.insert("p2_0".to_string(), PlayerCommand::new(5.0, 0.0));
    engine.step(&mut world, &commands, 0.2);

    let carpet = world.players["p2"].carpets.get("p2_0").unwrap();
    assert_eq!(carpet.acceleration, (5.0, 0.0));
    assert!((carpet.velocity.0 - 1.0).abs() < 1e-9);

    commands.clear();
    engine.step(&mut world, &commands, 0.2);

    let carpet = world.players["p2"].carpets.get("p2_0").unwrap();
    assert_eq!(carpet.acceleration, (5.0, 0.0));
    assert!((carpet.velocity.0 - 1.98).abs() < 1e-9);
}
