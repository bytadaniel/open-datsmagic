//! # Схема численного интегрирования Эйлера
//!
//! Реализует пошаговое вычисление новых скоростей и координат точек
//! по схеме Эйлера 1-го порядка с учетом трения и предельных ограничений
//! в соответствии со спецификацией [`FE-002`](file:///Users/d.byta/Documents/Code/dats/datsmagic/docs/features/FE-002-euler-physics-engine.md)
//! и правилами симулятора [`docs/mechanics.md`](file:///Users/d.byta/Documents/Code/dats/datsmagic/docs/mechanics.md).

use super::forces::{apply_friction, combine_forces, compute_effective_accel};
use super::vector2d::Vec2;

/// Трейт абстрактного физического интегратора
pub trait PhysicsIntegrator: Send + Sync {
    /// Выполняет один такт численного интегрирования физического состояния объекта.
    ///
    /// # Параметры:
    /// - `position`: Текущие координаты P_old
    /// - `velocity`: Текущая скорость V_old
    /// - `command_accel`: Заявленный вектор ускорения команды A_cmd
    /// - `environment_forces`: Суммарный вектор внешних сил W_env
    /// - `max_accel`: Максимальный модуль ускорения A_max
    /// - `max_velocity`: Максимальный модуль скорости V_max
    /// - `friction`: Коэффициент трения k_f
    /// - `dt`: Шаг времени тика в секундах Δt
    ///
    /// # Возвращает:
    /// Кортеж `(P_new, V_new)` с обновленными координатами и скоростью.
    #[allow(clippy::too_many_arguments)]
    fn step(
        &self,
        position: Vec2,
        velocity: Vec2,
        command_accel: Vec2,
        environment_forces: Vec2,
        max_accel: f64,
        max_velocity: f64,
        friction: f64,
        dt: f64,
    ) -> (Vec2, Vec2);
}

/// Реализация численного интегрирования по схеме Эйлера 1-го порядка
#[derive(Clone, Copy, Debug, Default)]
pub struct EulerIntegrator;

impl PhysicsIntegrator for EulerIntegrator {
    fn step(
        &self,
        position: Vec2,
        velocity: Vec2,
        command_accel: Vec2,
        environment_forces: Vec2,
        max_accel: f64,
        max_velocity: f64,
        friction: f64,
        dt: f64,
    ) -> (Vec2, Vec2) {
        // Защитная обработка параметров
        let position = position.sanitize_or_zero();
        let velocity = velocity.sanitize_or_zero();
        let command_accel = command_accel.sanitize_or_zero();
        let environment_forces = environment_forces.sanitize_or_zero();
        let max_accel = if max_accel.is_finite() && max_accel >= 0.0 {
            max_accel
        } else {
            tracing::warn!("Invalid max_accel: {}, using 0.0", max_accel);
            0.0
        };
        let max_velocity = if max_velocity.is_finite() && max_velocity >= 0.0 {
            max_velocity
        } else {
            tracing::warn!("Invalid max_velocity: {}, using 0.0", max_velocity);
            0.0
        };
        let friction = if friction.is_finite() && friction >= 0.0 {
            friction
        } else {
            tracing::warn!("Invalid friction: {}, using 0.98", friction);
            0.98
        };
        let dt = if dt.is_finite() && dt >= 0.0 {
            dt
        } else {
            tracing::warn!("Invalid dt: {}, using 0.0", dt);
            0.0
        };

        // Шаг 1: Валидация и ограничение управляющего ускорения
        let effective_accel = compute_effective_accel(command_accel, max_accel);

        // Шаг 2 и 3: Обновление скорости с учетом трения и внешних сил
        let total_accel = combine_forces(effective_accel, environment_forces);
        let velocity_decayed = apply_friction(velocity, friction);
        let unconstrained_velocity = velocity_decayed + (total_accel * dt);

        // Шаг 4: Ограничение скорости порогом max_velocity
        let new_velocity = unconstrained_velocity.clamp_length(max_velocity);

        // Шаг 5: Обновление позиции
        let new_position = position + (new_velocity * dt);

        (new_position, new_velocity)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_euler_friction_decay() {
        let integrator = EulerIntegrator;
        let pos = Vec2::ZERO;
        let vel = Vec2::new(10.0, 0.0);
        let cmd = Vec2::ZERO;
        let env = Vec2::ZERO;
        let dt = 0.2;
        let friction = 0.98;

        let (new_pos, new_vel) = integrator.step(pos, vel, cmd, env, 5.0, 20.0, friction, dt);

        assert!((new_vel.x - 9.8).abs() < 1e-9);
        assert!((new_vel.y - 0.0).abs() < 1e-9);
        assert!((new_pos.x - 9.8 * dt).abs() < 1e-9);
        assert!((new_pos.y - 0.0).abs() < 1e-9);
    }

    #[test]
    fn test_euler_acceleration_clamping() {
        let integrator = EulerIntegrator;
        let pos = Vec2::ZERO;
        let vel = Vec2::ZERO;
        let cmd = Vec2::new(10.0, 0.0); // Превышает max_accel = 5.0
        let env = Vec2::ZERO;
        let dt = 0.2;

        let (_, new_vel) = integrator.step(pos, vel, cmd, env, 5.0, 20.0, 1.0, dt);

        // Применяется ускорение (5.0, 0.0), скорость = (5.0 * 0.2, 0.0) = (1.0, 0.0)
        assert!((new_vel.x - 1.0).abs() < 1e-9);
        assert!((new_vel.y - 0.0).abs() < 1e-9);
    }

    #[test]
    fn test_euler_velocity_clamping() {
        let integrator = EulerIntegrator;
        let pos = Vec2::ZERO;
        let vel = Vec2::new(19.0, 0.0);
        let cmd = Vec2::new(10.0, 0.0); // Clamped to 5.0
        let env = Vec2::ZERO;
        let dt = 0.5; // (19.0 * 1.0) + (5.0 * 0.5) = 21.5 > 20.0

        let (_, new_vel) = integrator.step(pos, vel, cmd, env, 5.0, 20.0, 1.0, dt);

        assert!((new_vel.x - 20.0).abs() < 1e-9);
        assert_eq!(new_vel.length(), 20.0);
    }

    #[test]
    fn test_euler_anomaly_force_does_not_disable_command() {
        let integrator = EulerIntegrator;
        let pos = Vec2::ZERO;
        let vel = Vec2::new(10.0, 0.0);
        let cmd = Vec2::new(10.0, 0.0);
        let dt = 0.2;

        let (_, new_vel) =
            integrator.step(pos, vel, cmd, Vec2::new(-2.0, 0.0), 5.0, 20.0, 0.98, dt);

        // Full command and opposing environment force are both applied.
        assert!((new_vel.x - 10.4).abs() < 1e-9);
        assert!((new_vel.y - 0.0).abs() < 1e-9);
    }
}
