//! # Детекция геометрических коллизий и пересечений
//!
//! Модуль реализует базовые геометрические предикаты: пересечение окружностей,
//! попадание точки в круг с оптимизацией через квадраты расстояний (NFR-01),
//! а также проверку и ограничение координат в пределах арены.

use crate::physics::Vec2;

/// Проверяет пересечение двух окружностей по центрам и радиусам через d^2 <= (r1 + r2)^2.
///
/// Оптимизировано без вычисления `sqrt` согласно NFR-01.
#[inline]
pub fn check_circle_collision(pos_a: Vec2, radius_a: f64, pos_b: Vec2, radius_b: f64) -> bool {
    if !radius_a.is_finite() || !radius_b.is_finite() || radius_a < 0.0 || radius_b < 0.0 {
        return false;
    }
    let combined_radius = radius_a + radius_b;
    pos_a.distance_squared(pos_b) <= combined_radius * combined_radius
}

/// Проверяет, лежит ли точка внутри окружности заданного радиуса через d^2 <= r^2.
#[inline]
pub fn check_point_in_circle(point: Vec2, center: Vec2, radius: f64) -> bool {
    if !radius.is_finite() || radius < 0.0 {
        return false;
    }
    point.distance_squared(center) <= radius * radius
}

/// Проверяет, находится ли точка внутри прямоугольной арены [0, width] x [0, height].
#[inline]
pub fn is_within_arena(position: Vec2, width: f64, height: f64) -> bool {
    position.x >= 0.0 && position.x <= width && position.y >= 0.0 && position.y <= height
}

/// Ограничивает позицию пределами прямоугольной арены [0, width] x [0, height].
#[inline]
pub fn clamp_position_to_arena(position: Vec2, width: f64, height: f64) -> Vec2 {
    let x = position.x.clamp(0.0, width);
    let y = position.y.clamp(0.0, height);
    Vec2::new(x, y)
}

/// Проверяет столкновение двух игроков с одинаковым радиусом ковра R_player.
#[inline]
pub fn check_player_collision(pos_a: Vec2, pos_b: Vec2, player_radius: f64) -> bool {
    check_circle_collision(pos_a, player_radius, pos_b, player_radius)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_circle_collision() {
        let p1 = Vec2::new(0.0, 0.0);
        let p2 = Vec2::new(5.0, 0.0);

        // r1 = 3.0, r2 = 2.5 -> сумма 5.5 >= 5.0 (пересекаются)
        assert!(check_circle_collision(p1, 3.0, p2, 2.5));

        // r1 = 2.0, r2 = 2.0 -> сумма 4.0 < 5.0 (не пересекаются)
        assert!(!check_circle_collision(p1, 2.0, p2, 2.0));
    }

    #[test]
    fn test_point_in_circle() {
        let center = Vec2::new(10.0, 10.0);
        assert!(check_point_in_circle(Vec2::new(10.0, 14.0), center, 5.0));
        assert!(check_point_in_circle(Vec2::new(10.0, 15.0), center, 5.0)); // ровно на границе
        assert!(!check_point_in_circle(Vec2::new(10.0, 15.1), center, 5.0));
    }

    #[test]
    fn test_arena_bounds() {
        assert!(is_within_arena(Vec2::new(50.0, 50.0), 100.0, 100.0));
        assert!(!is_within_arena(Vec2::new(-1.0, 50.0), 100.0, 100.0));
        assert!(!is_within_arena(Vec2::new(50.0, 101.0), 100.0, 100.0));

        let clamped = clamp_position_to_arena(Vec2::new(-10.0, 120.0), 100.0, 100.0);
        assert_eq!(clamped, Vec2::new(0.0, 100.0));
    }

    #[test]
    fn test_invalid_parameters_safe() {
        assert!(!check_circle_collision(
            Vec2::ZERO,
            f64::NAN,
            Vec2::ZERO,
            1.0
        ));
        assert!(!check_point_in_circle(Vec2::ZERO, Vec2::ZERO, -5.0));
    }
}
