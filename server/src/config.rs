//! # Конфигурация игрового сервера симулятора DatsMagic
//!
//! Данный модуль инкапсулирует все параметры времени, физических констант
//! и сетевых настроек в соответствии со спецификацией [`docs/mechanics.md`](file:///Users/d.byta/Documents/Code/dats/datsmagic/docs/mechanics.md).

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Конфигурация параметров симулятора и сетевого сервера
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ServerConfig {
    /// Длительность одного игрового тика в миллисекундах (по умолчанию 200 мс, Δt = 0.2 с)
    pub tick_rate_ms: u64,

    /// Сетевой адрес для привязки HTTP-сервера (по умолчанию "127.0.0.1")
    pub host: String,

    /// Порт TCP для прослушивания HTTP REST API (по умолчанию 8080)
    pub port: u16,

    /// Предельный модуль вектора управляющего ускорения A_max (по умолчанию 5.0)
    pub max_acceleration: f64,

    /// Предельный модуль скорости ковра-самолета V_max (по умолчанию 20.0)
    pub max_velocity: f64,

    /// Коэффициент вязкого трения среды k_f (по умолчанию 0.98)
    pub friction: f64,

    /// Ширина двумерной арены симуляции (по оси X)
    pub arena_width: f64,

    /// Высота двумерной арены симуляции (по оси Y)
    pub arena_height: f64,

    /// Флаг включения автоматического генератора динамических аномалий
    pub enable_spawner: bool,

    /// Флаг включения автоматического генератора прогрессивных монет (FE-009)
    pub enable_coin_spawner: bool,

    /// Штраф за гибель ковра в очках P_death (FE-011, по умолчанию 50)
    pub death_penalty_score: u32,

    /// Флаг включения автоматического переспавна погибших ковров (FE-011)
    pub enable_respawn: bool,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            tick_rate_ms: 200,
            host: "127.0.0.1".to_string(),
            port: 8080,
            max_acceleration: 5.0,
            max_velocity: 20.0,
            friction: 0.98,
            arena_width: 1000.0,
            arena_height: 1000.0,
            enable_spawner: false,
            enable_coin_spawner: false,
            death_penalty_score: 50,
            enable_respawn: false,
        }
    }
}

impl ServerConfig {
    /// Возвращает длительность тика в виде [`Duration`]
    #[inline]
    pub fn tick_duration(&self) -> Duration {
        Duration::from_millis(self.tick_rate_ms)
    }

    /// Возвращает шаг времени симуляции Δt в секундах
    #[inline]
    pub fn dt_seconds(&self) -> f64 {
        self.tick_rate_ms as f64 / 1000.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_values() {
        let config = ServerConfig::default();
        assert_eq!(config.tick_rate_ms, 200);
        assert_eq!(config.dt_seconds(), 0.2);
        assert_eq!(config.tick_duration(), Duration::from_millis(200));
        assert_eq!(config.max_acceleration, 5.0);
        assert_eq!(config.max_velocity, 20.0);
        assert_eq!(config.friction, 0.98);
        assert_eq!(config.death_penalty_score, 50);
        assert!(!config.enable_respawn);
    }
}
