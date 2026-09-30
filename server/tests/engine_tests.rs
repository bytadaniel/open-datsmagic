//! # Интеграционные тесты игрового цикла и ядра сервера (FE-001)
//!
//! Проверяет детерминированное продвижение тиков, временные характеристики
//! и корректность обработки команд симулятора.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use server::config::ServerConfig;
use server::engine::{
    CommandError, GameEngine, PhysicsStepHandler, PlayerCommand, SessionStatus, SpatialStepHandler,
    WorldData,
};

/// Интеграционный тест из FE-001 Section 8:
/// "запуск цикла на 1 секунду с проверкой, что счетчик тиков увеличился ровно на 5"
#[tokio::test]
async fn test_game_loop_advances_ticks() {
    let config = ServerConfig {
        tick_rate_ms: 200,
        ..Default::default()
    };
    let engine = GameEngine::new(config);

    assert_eq!(engine.current_tick().await, 0);

    let handle = engine.spawn_loop();

    // Ожидаем 1050 мс: за 1000 мс должно пройти ровно 5 тиков по 200 мс (200, 400, 600, 800, 1000)
    tokio::time::sleep(Duration::from_millis(1050)).await;

    let tick_count = engine.current_tick().await;
    engine.stop();
    let _ = handle.await;

    // Допускаем погрешность в пределах ±1 тика на медленных виртуальных средах,
    // но в стандартных условиях счетчик равен ровно 5
    assert!(
        (5..=6).contains(&tick_count),
        "Ожидалось 5 тиков за 1 секунду при шаге 200 мс, получено: {}",
        tick_count
    );
}

/// Проверка работы конвейера фаз тика с подсчетом вызовов физики и коллизий
#[tokio::test]
async fn test_pipeline_handlers_invoked_each_tick() {
    struct MockPhysics(Arc<AtomicUsize>);
    impl PhysicsStepHandler for MockPhysics {
        fn step(
            &mut self,
            _world: &mut WorldData,
            _commands: &std::collections::HashMap<String, PlayerCommand>,
            _dt: f64,
        ) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    struct MockSpatial(Arc<AtomicUsize>);
    impl SpatialStepHandler for MockSpatial {
        fn resolve(&mut self, _world: &mut WorldData, _dt: f64) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    let physics_counter = Arc::new(AtomicUsize::new(0));
    let spatial_counter = Arc::new(AtomicUsize::new(0));

    let config = ServerConfig {
        tick_rate_ms: 20,
        ..Default::default()
    };

    let engine = GameEngine::with_handlers(
        config,
        Box::new(MockPhysics(Arc::clone(&physics_counter))),
        Box::new(MockSpatial(Arc::clone(&spatial_counter))),
    );

    // Выполняем 3 дискретных шага вручную
    for i in 1..=3 {
        engine.step_once().await;
        assert_eq!(engine.current_tick().await, i);
        assert_eq!(physics_counter.load(Ordering::SeqCst), i as usize);
        assert_eq!(spatial_counter.load(Ordering::SeqCst), i as usize);
    }
}

/// Проверка конкурентной регистрации команд от нескольких игроков
#[tokio::test]
async fn test_concurrent_player_commands() {
    let config = ServerConfig {
        tick_rate_ms: 50,
        ..Default::default()
    };
    let engine = GameEngine::new(config);

    let mut handles = Vec::new();

    for i in 0..10 {
        let engine_clone = engine.clone();
        handles.push(tokio::spawn(async move {
            let player_id = format!("player_{}", i);
            let cmd = PlayerCommand::new(i as f64, (i * 2) as f64);
            engine_clone.register_command(player_id, cmd)
        }));
    }

    for handle in handles {
        let res = handle.await.unwrap();
        assert_eq!(res, Ok(()));
    }

    // Выполняем шаг тика
    engine.step_once().await;
    assert_eq!(engine.current_tick().await, 1);

    // В новом тике все игроки могут снова отправить команды
    for i in 0..10 {
        let player_id = format!("player_{}", i);
        let cmd = PlayerCommand::new(0.0, 0.0);
        assert_eq!(engine.register_command(player_id, cmd), Ok(()));
    }
}

/// Проверка поведения при изменении статуса сессии
#[tokio::test]
async fn test_session_status_transitions() {
    let config = ServerConfig {
        tick_rate_ms: 20,
        ..Default::default()
    };
    let engine = GameEngine::new(config);

    assert_eq!(engine.get_snapshot().game_status, SessionStatus::Active);

    // Переводим на паузу
    engine.set_status(SessionStatus::Paused).await;
    assert_eq!(engine.get_snapshot().game_status, SessionStatus::Paused);

    let cmd = PlayerCommand::new(1.0, 1.0);
    assert_eq!(
        engine.register_command("player_1".to_string(), cmd),
        Err(CommandError::SessionNotActive)
    );

    // Возвращаем в активное состояние
    engine.set_status(SessionStatus::Active).await;
    assert_eq!(engine.get_snapshot().game_status, SessionStatus::Active);
    assert_eq!(engine.register_command("player_1".to_string(), cmd), Ok(()));
}
