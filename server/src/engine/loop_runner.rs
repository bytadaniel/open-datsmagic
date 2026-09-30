//! # Главный игровой цикл и тактовый генератор симуляции
//!
//! Данный модуль реализует высокоточный игровой цикл симулятора DatsMagic
//! на базе асинхронного таймера [`tokio::time::interval`] в соответствии со спецификациями
//! [`FE-001`](file:///Users/d.byta/Documents/Code/dats/datsmagic/docs/features/FE-001-server-runtime-and-game-loop.md)
//! и [`DR-001`](file:///Users/d.byta/Documents/Code/dats/datsmagic/docs/domain/DR-001-game-loop-and-state.md).

use parking_lot::{Mutex, RwLock};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tokio::time::MissedTickBehavior;

use super::command_buffer::{CommandError, InputCommandBuffer, PlayerCommand, PlayerId};
use super::state::{GameState, SessionStatus, SharedGameState, WorldData, WorldSnapshot};
use crate::config::ServerConfig;

/// Трейт для обработчика шага физического движения (абстракция для модуля FE-002)
pub trait PhysicsStepHandler: Send + Sync {
    /// Вычисляет новые координаты и скорости для сущностей мира на основе команд и сил
    fn step(&mut self, world: &mut WorldData, commands: &HashMap<PlayerId, PlayerCommand>, dt: f64);
}

/// Трейт для обработчика пространственных сущностей и коллизий (абстракция для модуля FE-003)
pub trait SpatialStepHandler: Send + Sync {
    /// Выполняет проверку коллизий, сбор сокровищ и эффекты аномалий
    fn resolve(&mut self, world: &mut WorldData, dt: f64);

    /// Опционально устанавливает текущий номер тика перед resolve (FE-009)
    fn set_tick(&mut self, _tick: u64) {}
}

/// Пустой обработчик физики по умолчанию
#[derive(Default, Debug, Clone)]
pub struct NullPhysicsHandler;

impl PhysicsStepHandler for NullPhysicsHandler {
    fn step(
        &mut self,
        _world: &mut WorldData,
        _commands: &HashMap<PlayerId, PlayerCommand>,
        _dt: f64,
    ) {
        // Заглушка до подключения модуля FE-002
    }
}

/// Пустой обработчик пространственных коллизий по умолчанию
#[derive(Default, Debug, Clone)]
pub struct NullSpatialHandler;

impl SpatialStepHandler for NullSpatialHandler {
    fn resolve(&mut self, _world: &mut WorldData, _dt: f64) {
        // Заглушка до подключения модуля FE-003
    }
}

/// Фасад и координатор игрового движка DatsMagic
#[derive(Clone)]
pub struct GameEngine {
    /// Конфигурация параметров сервера и симуляции
    config: Arc<ServerConfig>,
    /// Потокобезопасный буфер входящих команд текущего тика
    command_buffer: Arc<Mutex<InputCommandBuffer>>,
    /// Каноническое изменяемое состояние игры
    state: SharedGameState,
    /// Кэшированный неизменяемый снапшот последнего завершенного тика
    latest_snapshot: Arc<RwLock<Arc<WorldSnapshot>>>,
    /// Обработчик физического шага
    physics_handler: Arc<tokio::sync::Mutex<Box<dyn PhysicsStepHandler>>>,
    /// Обработчик пространственных коллизий
    spatial_handler: Arc<tokio::sync::Mutex<Box<dyn SpatialStepHandler>>>,
    /// Канал оповещения о завершении работы игрового цикла
    shutdown_tx: Arc<watch::Sender<bool>>,
    /// Приемник сигнала завершения работы игрового цикла
    shutdown_rx: watch::Receiver<bool>,
}

impl GameEngine {
    /// Создает новый экземпляр игрового движка с указанной конфигурацией
    pub fn new(config: ServerConfig) -> Self {
        let friction = config.friction;
        let initial_state = GameState::new();
        let initial_snapshot = Arc::new(initial_state.to_snapshot());
        let (shutdown_tx, shutdown_rx) = watch::channel(false);

        let mut spatial_engine = crate::spatial::WorldSpatialEngine::default()
            .with_arena(config.arena_width, config.arena_height)
            .with_death_penalty(config.death_penalty_score)
            .with_carpet_respawn(config.enable_respawn);

        if config.enable_spawner {
            let spawner_config = crate::spatial::AnomalySpawnerConfig {
                arena_width: config.arena_width,
                arena_height: config.arena_height,
                max_anomalies: 5,
                ..Default::default()
            };
            let spawner = crate::spatial::AnomalySpawner::new(spawner_config);
            spatial_engine = spatial_engine.with_spawner(spawner);
        }

        if config.enable_coin_spawner {
            let coin_spawner_config = crate::spatial::CoinSpawnerConfig {
                arena_width: config.arena_width,
                arena_height: config.arena_height,
                max_coins: 10,
                ..Default::default()
            };
            let coin_spawner = crate::spatial::CoinSpawner::new(coin_spawner_config);
            spatial_engine = spatial_engine.with_coin_spawner(coin_spawner);
        }

        let spatial_handler: Box<dyn SpatialStepHandler> = Box::new(spatial_engine);

        Self {
            config: Arc::new(config),
            command_buffer: Arc::new(Mutex::new(InputCommandBuffer::new())),
            state: Arc::new(tokio::sync::RwLock::new(initial_state)),
            latest_snapshot: Arc::new(RwLock::new(initial_snapshot)),
            physics_handler: Arc::new(tokio::sync::Mutex::new(Box::new(
                crate::physics::WorldPhysicsEngine::new(friction),
            ))),
            spatial_handler: Arc::new(tokio::sync::Mutex::new(spatial_handler)),
            shutdown_tx: Arc::new(shutdown_tx),
            shutdown_rx,
        }
    }

    /// Позволяет создать движок с кастомными обработчиками физики и пространственных коллизий
    pub fn with_handlers(
        config: ServerConfig,
        physics: Box<dyn PhysicsStepHandler>,
        spatial: Box<dyn SpatialStepHandler>,
    ) -> Self {
        let mut engine = Self::new(config);
        engine.physics_handler = Arc::new(tokio::sync::Mutex::new(physics));
        engine.spatial_handler = Arc::new(tokio::sync::Mutex::new(spatial));
        engine
    }

    /// Возвращает ссылку на конфигурацию сервера
    pub fn config(&self) -> &ServerConfig {
        &self.config
    }

    /// Возвращает разделяемое каноническое состояние игры
    pub fn shared_state(&self) -> SharedGameState {
        Arc::clone(&self.state)
    }

    /// Регистрирует управляющую команду игрока на текущий такт симуляции
    ///
    /// # Ошибки
    /// - [`CommandError::SessionNotActive`], если игра на паузе или завершена
    /// - [`CommandError::AlreadySubmitted`], если игрок уже подал команду в данном тике
    /// - [`CommandError::InvalidCommand`], если в координатах присутствуют невалидные значения (NaN/Infinity)
    pub fn register_command(
        &self,
        player_id: PlayerId,
        command: PlayerCommand,
    ) -> Result<(), CommandError> {
        let snapshot = self.latest_snapshot.read();
        let current_status = snapshot.game_status;
        if current_status != SessionStatus::Active {
            return Err(CommandError::SessionNotActive);
        }

        if let Some(player) = snapshot.world.players.get(&player_id) {
            if player.is_destroyed() {
                return Err(CommandError::PlayerDestroyed);
            }
        }
        drop(snapshot);

        self.command_buffer
            .lock()
            .register_command(player_id, command)
    }

    /// Регистрирует пакет управляющих команд флота игрока на текущий такт симуляции (FE-010)
    ///
    /// # Ошибки
    /// - [`CommandError::SessionNotActive`], если игра на паузе или завершена
    /// - [`CommandError::AlreadySubmitted`], если игрок уже подал команду в данном тике
    /// - [`CommandError::InvalidCommand`], если в координатах присутствуют невалидные значения (NaN/Infinity)
    /// - [`CommandError::PlayerDestroyed`], если игрок/флот уничтожен
    pub fn register_batch_commands(
        &self,
        player_id: PlayerId,
        commands: Vec<(String, PlayerCommand)>,
    ) -> Result<usize, CommandError> {
        let snapshot = self.latest_snapshot.read();
        let current_status = snapshot.game_status;
        if current_status != SessionStatus::Active {
            return Err(CommandError::SessionNotActive);
        }

        if let Some(player) = snapshot.world.players.get(&player_id) {
            if player.is_destroyed() {
                return Err(CommandError::PlayerDestroyed);
            }
        }
        drop(snapshot);

        self.command_buffer
            .lock()
            .register_batch_commands(player_id, commands)
    }

    /// Возвращает актуальный неизменяемый снимок состояния мира без блокировки потока симуляции
    pub fn get_snapshot(&self) -> Arc<WorldSnapshot> {
        self.latest_snapshot.read().clone()
    }

    /// Изменяет текущий статус сессии (Active, Paused, Finished)
    pub async fn set_status(&self, new_status: SessionStatus) {
        let mut state = self.state.write().await;
        state.status = new_status;
        let snapshot = Arc::new(state.to_snapshot());
        *self.latest_snapshot.write() = snapshot;
    }

    /// Принудительно публикует текущее состояние в кэшированный снимок
    pub async fn publish_snapshot(&self) {
        let state = self.state.read().await;
        let snapshot = Arc::new(state.to_snapshot());
        *self.latest_snapshot.write() = snapshot;
    }

    /// Возвращает текущий номер тика симулятора
    pub async fn current_tick(&self) -> u64 {
        self.state.read().await.tick
    }

    /// Выполняет ровно один атомарный шаг симуляции (конвейер тика).
    ///
    /// Конвейер включает в себя:
    /// 1. Извлечение накопленных команд из входного буфера (`drain_commands`).
    /// 2. Фаза физического пересчета координат и скоростей (`PhysicsStepHandler::step`).
    /// 3. Фаза детекции коллизий и сбора сокровищ (`SpatialStepHandler::resolve`).
    /// 4. Инкремент счетчика тиков ($tick \to tick+1$) и атомарная публикация [`WorldSnapshot`].
    pub async fn step_once(&self) {
        let dt = self.config.dt_seconds();
        let tick_duration = self.config.tick_duration();
        let start_time = Instant::now();

        // Шаг 1: Извлечение команд игроков за текущий тик
        let commands = self.command_buffer.lock().drain_commands();

        // Блокировка состояния для выполнения мутаций мира
        let mut state = self.state.write().await;

        // Если сессия не активна, не продвигаем мир
        if state.status != SessionStatus::Active {
            return;
        }

        // Шаг 2: Фаза физического пересчета
        {
            let mut physics = self.physics_handler.lock().await;
            physics.step(&mut state.world, &commands, dt);
        }

        // Шаг 3: Фаза пространственных коллизий и аномалий
        {
            let mut spatial = self.spatial_handler.lock().await;
            spatial.set_tick(state.tick);
            spatial.resolve(&mut state.world, dt);
        }

        // Шаг 4: Инкремент тика и публикация обновленного снапшота
        state.tick += 1;
        let snapshot = Arc::new(state.to_snapshot());
        *self.latest_snapshot.write() = snapshot;

        let elapsed = start_time.elapsed();
        if elapsed > tick_duration {
            tracing::warn!(
                "Превышена длительность тика {}: вычисление заняло {:?} при лимите {:?}",
                state.tick,
                elapsed,
                tick_duration
            );
        }
    }

    /// Запускает фоновую асинхронную задачу игрового цикла с фиксированным шагом по времени
    pub fn spawn_loop(&self) -> JoinHandle<()> {
        let engine = self.clone();
        let tick_duration = engine.config.tick_duration();
        let mut shutdown_rx = engine.shutdown_rx.clone();

        tokio::spawn(async move {
            tracing::info!(
                "Запущен игровой цикл симуляции DatsMagic (интервал: {:?})",
                tick_duration
            );

            // Инициализируем таймер с первым срабатыванием через tick_duration
            let mut interval = tokio::time::interval_at(
                tokio::time::Instant::now() + tick_duration,
                tick_duration,
            );
            // Пропускать пропущенные тики при пиковых нагрузках, не пытаясь догонять их пачкой
            interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        engine.step_once().await;
                    }
                    changed = shutdown_rx.changed() => {
                        if changed.is_ok() && *shutdown_rx.borrow() {
                            tracing::info!("Получен сигнал завершения игрового цикла. Остановка.");
                            break;
                        }
                    }
                }
            }
        })
    }

    /// Отправляет сигнал на остановку фонового игрового цикла
    pub fn stop(&self) {
        let _ = self.shutdown_tx.send(true);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn test_step_once_advances_tick() {
        let config = ServerConfig::default();
        let engine = GameEngine::new(config);

        assert_eq!(engine.current_tick().await, 0);
        assert_eq!(engine.get_snapshot().tick, 0);

        engine.step_once().await;

        assert_eq!(engine.current_tick().await, 1);
        assert_eq!(engine.get_snapshot().tick, 1);
    }

    #[tokio::test]
    async fn test_register_command_and_drain_on_step() {
        let config = ServerConfig::default();
        let engine = GameEngine::new(config);

        let cmd = PlayerCommand::new(1.0, 2.0);
        assert_eq!(engine.register_command("player_1".to_string(), cmd), Ok(()));

        // Повторная регистрация в том же тике должна возвращать ошибку
        assert_eq!(
            engine.register_command("player_1".to_string(), cmd),
            Err(CommandError::AlreadySubmitted)
        );

        // Выполняем шаг цикла
        engine.step_once().await;

        // В следующем тике регистрация снова доступна
        assert_eq!(engine.register_command("player_1".to_string(), cmd), Ok(()));
    }

    #[tokio::test]
    async fn test_paused_session_does_not_advance_ticks() {
        let config = ServerConfig::default();
        let engine = GameEngine::new(config);

        engine.set_status(SessionStatus::Paused).await;

        // Попытка отправить команду на паузе
        let cmd = PlayerCommand::new(1.0, 1.0);
        assert_eq!(
            engine.register_command("p1".to_string(), cmd),
            Err(CommandError::SessionNotActive)
        );

        // Шаг цикла не должен продвигать тики
        engine.step_once().await;
        assert_eq!(engine.current_tick().await, 0);
    }

    #[tokio::test]
    async fn test_loop_shutdown_signal() {
        let config = ServerConfig {
            tick_rate_ms: 10,
            ..Default::default()
        };
        let engine = GameEngine::new(config);
        let handle = engine.spawn_loop();

        tokio::time::sleep(Duration::from_millis(35)).await;
        engine.stop();

        // Задача должна корректно завершиться
        let result = tokio::time::timeout(Duration::from_millis(200), handle).await;
        assert!(
            result.is_ok(),
            "Игровой цикл должен был завершиться после вызова stop()"
        );
    }
}
