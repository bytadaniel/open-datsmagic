//! # Буфер команд игроков для текущего игрового такта
//!
//! Реализует потокобезопасный прием, валидацию и извлечение управляющих векторов ускорения
//! от игроков в соответствии со спецификацией [`FE-001`](file:///Users/d.byta/Documents/Code/dats/datsmagic/docs/features/FE-001-server-runtime-and-game-loop.md).

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use thiserror::Error;

/// Уникальный строковый идентификатор игрока / команды
pub type PlayerId = String;

/// Команда игрока, содержащая управляющий вектор ускорения (ax, ay)
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlayerCommand {
    /// Вектор ускорения в декартовой плоскости (X вправо, Y вверх)
    pub acceleration: (f64, f64),
}

impl PlayerCommand {
    /// Создает новую команду игрока с заданным вектором ускорения
    pub fn new(ax: f64, ay: f64) -> Self {
        Self {
            acceleration: (ax, ay),
        }
    }

    /// Проверяет, что координаты ускорения являются конечными вещественными числами (не NaN и не ±Infinity)
    #[inline]
    pub fn is_valid(&self) -> bool {
        self.acceleration.0.is_finite() && self.acceleration.1.is_finite()
    }
}

/// Ошибки при регистрации команды игрока в буфере такта
#[derive(Debug, Error, PartialEq, Eq)]
pub enum CommandError {
    /// Игрок уже отправил команду в текущем тике (нарушение лимита 1 команда/тик)
    #[error("Команда от игрока уже зарегистрирована в текущем тике")]
    AlreadySubmitted,

    /// Сессия не находится в активном состоянии (на паузе или завершена)
    #[error("Игровая сессия не активна")]
    SessionNotActive,

    /// Переданы невалидные значения (NaN или Infinity)
    #[error("Некорректные параметры команды: {0}")]
    InvalidCommand(String),

    /// Игрок уничтожен при контакте с ядром аномалии и не может выполнять команды
    #[error("Игрок уничтожен")]
    PlayerDestroyed,
}

/// Входной буфер команд игроков, собираемых в течение одного игрового тика
#[derive(Default, Debug)]
pub struct InputCommandBuffer {
    /// Карта команд, ожидающих применения на следующем физическом шаге (ключ: carpet_id или player_id)
    pub pending_commands: HashMap<String, PlayerCommand>,
    /// Множество игроков, уже подавших команды в текущем тике (для rate limiting 1 запрос/тик)
    pub submitted_players: HashSet<PlayerId>,
}

impl InputCommandBuffer {
    /// Создает новый пустой буфер команд
    pub fn new() -> Self {
        Self {
            pending_commands: HashMap::new(),
            submitted_players: HashSet::new(),
        }
    }

    /// Регистрирует команду игрока на текущий такт.
    ///
    /// # Ошибки
    /// - [`CommandError::InvalidCommand`] если вектор ускорения содержит NaN или Infinity.
    /// - [`CommandError::AlreadySubmitted`] если игрок с данным `player_id` уже отправил команду в этом тике.
    pub fn register_command(
        &mut self,
        player_id: PlayerId,
        command: PlayerCommand,
    ) -> Result<(), CommandError> {
        if !command.is_valid() {
            return Err(CommandError::InvalidCommand(
                "Координаты вектора ускорения должны быть конечными числами (не NaN/Infinity)"
                    .to_string(),
            ));
        }

        if self.submitted_players.contains(&player_id) {
            return Err(CommandError::AlreadySubmitted);
        }

        self.submitted_players.insert(player_id.clone());
        self.pending_commands.insert(player_id, command);
        Ok(())
    }

    /// Регистрирует пакет команд для флота ковров игрока на текущий такт (FE-010).
    ///
    /// # Ошибки
    /// - [`CommandError::InvalidCommand`] если список пуст или вектор содержит невалидные значения.
    /// - [`CommandError::AlreadySubmitted`] если игрок с данным `player_id` уже подал команду в текущем тике.
    pub fn register_batch_commands(
        &mut self,
        player_id: PlayerId,
        commands: Vec<(String, PlayerCommand)>,
    ) -> Result<usize, CommandError> {
        if commands.is_empty() {
            return Err(CommandError::InvalidCommand(
                "Список команд не может быть пустым".to_string(),
            ));
        }

        for (carpet_id, cmd) in &commands {
            if !cmd.is_valid() {
                return Err(CommandError::InvalidCommand(format!(
                    "Координаты вектора ускорения для ковра {} должны быть конечными числами (не NaN/Infinity)",
                    carpet_id
                )));
            }
        }

        if self.submitted_players.contains(&player_id) {
            return Err(CommandError::AlreadySubmitted);
        }

        self.submitted_players.insert(player_id);
        let count = commands.len();
        for (carpet_id, cmd) in commands {
            self.pending_commands.insert(carpet_id, cmd);
        }
        Ok(count)
    }

    /// Realtime-ввод браузера: несколько обновлений за тик заменяют предыдущие.
    /// Игровой REST API продолжает использовать `register_batch_commands` и его лимит.
    pub fn register_realtime_batch_commands(
        &mut self,
        player_id: PlayerId,
        commands: Vec<(String, PlayerCommand)>,
    ) -> Result<usize, CommandError> {
        if commands.is_empty() {
            return Ok(0);
        }
        for (carpet_id, command) in &commands {
            if !command.is_valid() {
                return Err(CommandError::InvalidCommand(format!(
                    "Координаты ускорения для ковра {carpet_id} должны быть конечными числами"
                )));
            }
        }
        self.submitted_players.insert(player_id);
        let count = commands.len();
        for (carpet_id, command) in commands {
            self.pending_commands.insert(carpet_id, command);
        }
        Ok(count)
    }

    /// Атомарно извлекает накопленные за текущий тик команды и опустошает внутренний буфер
    /// с помощью [`std::mem::take`] для начала накопления команд следующего тика.
    pub fn drain_commands(&mut self) -> HashMap<String, PlayerCommand> {
        self.submitted_players.clear();
        std::mem::take(&mut self.pending_commands)
    }

    /// Возвращает текущее количество зарегистрированных команд
    #[inline]
    pub fn len(&self) -> usize {
        self.pending_commands.len()
    }

    /// Проверяет, пуст ли буфер команд
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.pending_commands.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Проверка извлечения команд и очистки буфера для нового тика (FE-001 Section 8)
    #[test]
    fn test_command_buffer_drain() {
        let mut buffer = InputCommandBuffer::new();
        assert!(buffer.is_empty());

        let cmd1 = PlayerCommand::new(1.0, 2.0);
        let cmd2 = PlayerCommand::new(-0.5, 3.2);

        assert!(buffer
            .register_command("player_1".to_string(), cmd1)
            .is_ok());
        assert!(buffer
            .register_command("player_2".to_string(), cmd2)
            .is_ok());
        assert_eq!(buffer.len(), 2);

        let drained = buffer.drain_commands();
        assert_eq!(drained.len(), 2);
        assert_eq!(drained.get("player_1"), Some(&cmd1));
        assert_eq!(drained.get("player_2"), Some(&cmd2));

        // Буфер должен быть очищен
        assert!(buffer.is_empty());
        assert_eq!(buffer.len(), 0);
    }

    /// Проверка отклонения повторной команды от того же PlayerId в рамках одного такта (FE-001 Section 8)
    #[test]
    fn test_rate_limit_per_tick() {
        let mut buffer = InputCommandBuffer::new();
        let cmd_initial = PlayerCommand::new(2.0, 1.0);
        let cmd_duplicate = PlayerCommand::new(3.0, 0.0);

        // Первая регистрация успешна
        assert_eq!(
            buffer.register_command("team_alpha".to_string(), cmd_initial),
            Ok(())
        );

        // Повторная регистрация в том же тике отклоняется
        assert_eq!(
            buffer.register_command("team_alpha".to_string(), cmd_duplicate),
            Err(CommandError::AlreadySubmitted)
        );

        // После извлечения команд (наступления нового тика) регистрация снова доступна
        let _ = buffer.drain_commands();
        assert_eq!(
            buffer.register_command("team_alpha".to_string(), cmd_duplicate),
            Ok(())
        );
    }

    /// Проверка отклонения некорректных чисел (NaN, Infinity)
    #[test]
    fn test_invalid_command_values() {
        let mut buffer = InputCommandBuffer::new();

        let nan_cmd = PlayerCommand::new(f64::NAN, 1.0);
        assert!(matches!(
            buffer.register_command("player_nan".to_string(), nan_cmd),
            Err(CommandError::InvalidCommand(_))
        ));

        let inf_cmd = PlayerCommand::new(1.0, f64::INFINITY);
        assert!(matches!(
            buffer.register_command("player_inf".to_string(), inf_cmd),
            Err(CommandError::InvalidCommand(_))
        ));
    }

    /// Проверка регистрации пакета команд флота и per-player rate limiting (FE-010)
    #[test]
    fn test_batch_commands_registration() {
        let mut buffer = InputCommandBuffer::new();
        let commands = vec![
            ("player_1_0".to_string(), PlayerCommand::new(1.0, 2.0)),
            ("player_1_1".to_string(), PlayerCommand::new(-1.0, 3.0)),
            ("player_1_2".to_string(), PlayerCommand::new(0.0, -2.0)),
        ];

        let res = buffer.register_batch_commands("player_1".to_string(), commands);
        assert_eq!(res, Ok(3));
        assert_eq!(buffer.len(), 3);

        // Повторный вызов в том же тике отклоняется
        let duplicate = vec![("player_1_0".to_string(), PlayerCommand::new(0.0, 0.0))];
        assert_eq!(
            buffer.register_batch_commands("player_1".to_string(), duplicate),
            Err(CommandError::AlreadySubmitted)
        );

        let drained = buffer.drain_commands();
        assert_eq!(drained.len(), 3);
        assert_eq!(
            drained.get("player_1_0"),
            Some(&PlayerCommand::new(1.0, 2.0))
        );
        assert!(buffer.is_empty());
    }

    #[test]
    fn realtime_updates_coalesce_without_changing_rest_rate_limit() {
        let mut buffer = InputCommandBuffer::new();
        buffer
            .register_realtime_batch_commands(
                "team".into(),
                vec![("team_0".into(), PlayerCommand::new(1.0, 0.0))],
            )
            .unwrap();
        buffer
            .register_realtime_batch_commands(
                "team".into(),
                vec![("team_0".into(), PlayerCommand::new(0.0, 1.0))],
            )
            .unwrap();
        assert_eq!(
            buffer.pending_commands.get("team_0"),
            Some(&PlayerCommand::new(0.0, 1.0))
        );
        assert_eq!(
            buffer.register_batch_commands(
                "team".into(),
                vec![("team_1".into(), PlayerCommand::new(1.0, 1.0))]
            ),
            Err(CommandError::AlreadySubmitted)
        );
    }
}
