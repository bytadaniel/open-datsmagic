//! # Конфигурация игрового сервера симулятора DatsMagic
//!
//! Данный модуль инкапсулирует все параметры времени, физических констант
//! и сетевых настроек в соответствии со спецификацией [`docs/mechanics.md`](file:///Users/d.byta/Documents/Code/dats/datsmagic/docs/mechanics.md).

use serde::{Deserialize, Serialize};
use std::time::Duration;

pub const SERVER_TICK_RATE_MS: u64 = 200;

/// Конфигурация параметров симулятора и сетевого сервера
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ServerConfig {
    /// Длительность одного игрового тика в миллисекундах (по умолчанию 200 мс, Δt = 0.2 с)
    pub tick_rate_ms: u64,

    /// Временная энтропия генераторов этого процесса; не часть профиля/публичного API.
    #[serde(skip)]
    pub runtime_entropy: u64,

    /// Сетевой адрес для привязки HTTP-сервера (по умолчанию "127.0.0.1")
    pub host: String,

    /// Порт TCP для прослушивания HTTP REST API (по умолчанию 8080)
    pub port: u16,

    /// Предельный модуль вектора управляющего ускорения A_max (по умолчанию 40.0)
    pub max_acceleration: f64,

    /// Предельный модуль скорости ковра-самолета V_max (по умолчанию 110.0)
    pub max_velocity: f64,

    /// Параметры контракта Desert, перенесенные из map.json.
    pub attack_cooldown_ms: u64,
    pub attack_damage: u32,
    pub attack_explosion_radius: f64,
    pub attack_range: f64,
    pub revive_timeout_sec: u64,
    pub shield_cooldown_ms: u64,
    pub shield_time_ms: u64,
    pub transport_radius: f64,
    /// Число ковров, создаваемых для команды в этом мире.
    pub carpet_count: usize,
    pub anomaly_quota: usize,
    pub bounty_quota: usize,

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

    /// Флаг включения автоматического переспавна погибших ковров (FE-011)
    pub enable_respawn: bool,

    /// Доля личного золота ковра, теряемая при гибели (0..=100 процентов).
    pub carpet_death_loss_percent: f64,

    /// Параметры генерации аномалий, заданные выбранным профилем мира.
    pub anomaly_speed_min: f64,
    pub anomaly_speed_max: f64,
    pub anomaly_core_radius_min: f64,
    pub anomaly_core_radius_max: f64,
    pub anomaly_effect_radius_min: f64,
    pub anomaly_effect_radius_max: f64,
    pub anomaly_force_min: f64,
    pub anomaly_force_max: f64,
    pub anomaly_force_outlier_probability: f64,
    pub anomaly_force_outlier_min: f64,
    pub anomaly_force_outlier_max: f64,

    /// Параметры генерации bounty.
    pub bounty_base_value: u32,
    pub bounty_max_value: u32,
    pub bounty_spawn_margin: f64,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            tick_rate_ms: SERVER_TICK_RATE_MS,
            runtime_entropy: 0,
            host: "127.0.0.1".to_string(),
            port: 8080,
            max_acceleration: 40.0,
            max_velocity: 110.0,
            attack_cooldown_ms: 10_000,
            attack_damage: 30,
            attack_explosion_radius: 30.0,
            attack_range: 200.0,
            revive_timeout_sec: 2,
            shield_cooldown_ms: 40_000,
            shield_time_ms: 5_000,
            transport_radius: 5.0,
            carpet_count: 5,
            anomaly_quota: 50,
            bounty_quota: 5000,
            friction: 0.98,
            arena_width: 9000.0,
            arena_height: 9000.0,
            enable_spawner: false,
            enable_coin_spawner: false,
            enable_respawn: true,
            carpet_death_loss_percent: 30.0,
            anomaly_speed_min: 30.0,
            anomaly_speed_max: 160.0,
            anomaly_core_radius_min: 20.0,
            anomaly_core_radius_max: 30.0,
            anomaly_effect_radius_min: 300.0,
            anomaly_effect_radius_max: 2000.0,
            anomaly_force_min: 4.0,
            anomaly_force_max: 22.0,
            anomaly_force_outlier_probability: 0.1,
            anomaly_force_outlier_min: 55.0,
            anomaly_force_outlier_max: 100.0,
            bounty_base_value: 25,
            bounty_max_value: 1000,
            bounty_spawn_margin: 40.0,
        }
    }
}

impl ServerConfig {
    /// Проверяет значения, влияющие на численную устойчивость мира и генераторы.
    pub fn validate(&self) -> Result<(), String> {
        let finite_positive = |name: &str, value: f64| {
            if value.is_finite() && value > 0.0 {
                Ok(())
            } else {
                Err(format!("{name} must be finite and > 0"))
            }
        };
        if self.tick_rate_ms != SERVER_TICK_RATE_MS {
            return Err(format!("tick_rate_ms is fixed at {SERVER_TICK_RATE_MS}"));
        }
        finite_positive("arena_width", self.arena_width)?;
        finite_positive("arena_height", self.arena_height)?;
        finite_positive("max_acceleration", self.max_acceleration)?;
        finite_positive("max_velocity", self.max_velocity)?;
        finite_positive("transport_radius", self.transport_radius)?;
        if self.carpet_count == 0 || self.carpet_count > 10 {
            return Err("carpet_count must be between 1 and 10".into());
        }
        if self.bounty_quota > 12_000 {
            return Err("bounty_quota must not exceed 12000".into());
        }
        if !self.friction.is_finite() || !(0.0..=1.0).contains(&self.friction) {
            return Err("friction must be between 0 and 1".into());
        }
        if !self.carpet_death_loss_percent.is_finite()
            || !(0.0..=100.0).contains(&self.carpet_death_loss_percent)
        {
            return Err("carpet_death_loss_percent must be between 0 and 100".into());
        }
        for (name, min, max) in [
            (
                "anomaly_speed",
                self.anomaly_speed_min,
                self.anomaly_speed_max,
            ),
            (
                "anomaly_core_radius",
                self.anomaly_core_radius_min,
                self.anomaly_core_radius_max,
            ),
            (
                "anomaly_effect_radius",
                self.anomaly_effect_radius_min,
                self.anomaly_effect_radius_max,
            ),
            (
                "anomaly_force",
                self.anomaly_force_min,
                self.anomaly_force_max,
            ),
            (
                "anomaly_force_outlier",
                self.anomaly_force_outlier_min,
                self.anomaly_force_outlier_max,
            ),
        ] {
            if !min.is_finite() || !max.is_finite() || min < 0.0 || min > max {
                return Err(format!(
                    "{name} range must be finite, nonnegative, and min <= max"
                ));
            }
        }
        if !self.anomaly_force_outlier_probability.is_finite()
            || !(0.0..=1.0).contains(&self.anomaly_force_outlier_probability)
        {
            return Err("anomaly_force_outlier_probability must be between 0 and 1".into());
        }
        if !self.bounty_spawn_margin.is_finite() || self.bounty_spawn_margin < 0.0 {
            return Err("bounty_spawn_margin must be finite and >= 0".into());
        }
        if self.bounty_spawn_margin * 2.0 > self.arena_width.min(self.arena_height) {
            return Err("bounty_spawn_margin must leave a non-empty spawn area".into());
        }
        if self.anomaly_quota == 0 && self.enable_spawner {
            return Err("anomaly_quota must be > 0 when anomaly spawner is enabled".into());
        }
        if self.bounty_quota == 0 && self.enable_coin_spawner {
            return Err("bounty_quota must be > 0 when bounty spawner is enabled".into());
        }
        Ok(())
    }

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
        assert_eq!(config.tick_rate_ms, SERVER_TICK_RATE_MS);
        assert_eq!(config.dt_seconds(), 0.2);
        assert_eq!(config.tick_duration(), Duration::from_millis(200));
        assert_eq!(config.max_acceleration, 40.0);
        assert_eq!(config.max_velocity, 110.0);
        assert_eq!(config.arena_width, 9000.0);
        assert_eq!(config.anomaly_quota, 50);
        assert_eq!(config.bounty_quota, 5000);
        assert_eq!(config.friction, 0.98);
        assert_eq!(config.carpet_death_loss_percent, 30.0);
        assert_eq!(config.anomaly_effect_radius_max, 2000.0);
        assert_eq!(config.bounty_base_value, 25);
        assert!(config.enable_respawn);
    }

    #[test]
    fn serde_defaults_and_percentage_validation() {
        let config: ServerConfig =
            serde_json::from_str(r#"{"arena_width":12000,"carpet_death_loss_percent":15}"#)
                .unwrap();
        assert_eq!(config.arena_width, 12000.0);
        assert_eq!(config.arena_height, 9000.0);
        assert_eq!(config.carpet_death_loss_percent, 15.0);
        assert!(config.validate().is_ok());

        let invalid: ServerConfig =
            serde_json::from_str(r#"{"carpet_death_loss_percent":101}"#).unwrap();
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn server_tick_is_immutable() {
        let config = ServerConfig {
            tick_rate_ms: 100,
            ..ServerConfig::default()
        };
        assert!(config.validate().unwrap_err().contains("fixed at 200"));
    }
}
