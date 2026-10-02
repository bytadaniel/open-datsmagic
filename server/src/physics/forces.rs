//! # Расчет и комбинирование физических сил и трения
//!
//! Модуль инкапсулирует вычисление вязкого трения, эффективного ускорения игроков
//! и суммирование внешних сил окружения в соответствии со спецификацией
//! [`FE-002`](file:///Users/d.byta/Documents/Code/dats/datsmagic/docs/features/FE-002-euler-physics-engine.md).

use super::vector2d::Vec2;

/// Ограничивает управляющее ускорение до `max_accel`.
///
/// Силы окружения рассчитываются отдельно и не могут отключить команду.
#[inline]
pub fn compute_effective_accel(command_accel: Vec2, max_accel: f64) -> Vec2 {
    command_accel.clamp_length(max_accel)
}

/// Применяет коэффициент вязкого трения среды `k_f` к вектору скорости.
///
/// Вектор скорости умножается на `friction_coeff`. Невалидные значения (NaN, Infinity, < 0)
/// безопасно обрабатываются возвратом [`Vec2::ZERO`].
#[inline]
pub fn apply_friction(velocity: Vec2, friction_coeff: f64) -> Vec2 {
    let sanitized_vel = velocity.sanitize_or_zero();
    if !friction_coeff.is_finite() || friction_coeff < 0.0 {
        tracing::warn!(
            "apply_friction: invalid friction coefficient {}",
            friction_coeff
        );
        return Vec2::ZERO;
    }
    sanitized_vel * friction_coeff
}

/// Суммирует эффективную тягу игрока и результирующий вектор внешних сил окружения.
#[inline]
pub fn combine_forces(effective_accel: Vec2, environment_forces: Vec2) -> Vec2 {
    effective_accel.sanitize_or_zero() + environment_forces.sanitize_or_zero()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_effective_accel_normal() {
        let cmd = Vec2::new(3.0, 4.0); // длина 5.0
        let effective = compute_effective_accel(cmd, 10.0);
        assert_eq!(effective, cmd);
    }

    #[test]
    fn test_compute_effective_accel_clamping() {
        let cmd = Vec2::new(10.0, 0.0);
        let effective = compute_effective_accel(cmd, 5.0);
        assert_eq!(effective, Vec2::new(5.0, 0.0));
    }

    #[test]
    fn test_anomaly_force_does_not_disable_command_accel() {
        let cmd = Vec2::new(24.0, 32.0);
        let effective = compute_effective_accel(cmd, 40.0);
        assert_eq!(effective, cmd);
    }

    #[test]
    fn test_apply_friction() {
        let v = Vec2::new(10.0, -5.0);
        let decayed = apply_friction(v, 0.98);
        assert!((decayed.x - 9.8).abs() < 1e-9);
        assert!((decayed.y - (-4.9)).abs() < 1e-9);

        // Невалидный коэффициент трения
        assert_eq!(apply_friction(v, -0.5), Vec2::ZERO);
        assert_eq!(apply_friction(v, f64::NAN), Vec2::ZERO);
    }

    #[test]
    fn test_combine_forces() {
        let a = Vec2::new(2.0, 3.0);
        let w = Vec2::new(-1.0, 4.0);
        assert_eq!(combine_forces(a, w), Vec2::new(1.0, 7.0));
    }
}
