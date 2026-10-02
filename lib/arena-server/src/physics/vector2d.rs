//! # Двумерная векторная математика (Vec2)
//!
//! Модуль предоставляет структуру [`Vec2`] и перегруженные операции
//! для работы на декартовой 2D-плоскости в соответствии со спецификацией
//! [`FE-002`](file:///Users/d.byta/Documents/Code/dats/datsmagic/docs/features/FE-002-euler-physics-engine.md)
//! и требованиями [`DR-002`](file:///Users/d.byta/Documents/Code/dats/datsmagic/docs/domain/DR-002-vector-physics.md).

use serde::{Deserialize, Serialize};
use std::ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Neg, Sub, SubAssign};

/// Двумерный вектор с компонентами `f64` на декартовой плоскости
#[derive(Clone, Copy, Debug, PartialEq, Default, Serialize, Deserialize)]
pub struct Vec2 {
    /// Координата X (направлена вправо)
    pub x: f64,
    /// Координата Y (направлена вверх)
    pub y: f64,
}

/// Псевдоним типа для соответствия спецификации DR-002
pub type Vector2D = Vec2;

impl Vec2 {
    /// Нулевой вектор (0.0, 0.0)
    pub const ZERO: Self = Self { x: 0.0, y: 0.0 };

    /// Единичный вектор по оси X (1.0, 0.0)
    pub const UNIT_X: Self = Self { x: 1.0, y: 0.0 };

    /// Единичный вектор по оси Y (0.0, 1.0)
    pub const UNIT_Y: Self = Self { x: 0.0, y: 1.0 };

    /// Порог для проверки нулевой длины при нормализации
    pub const EPSILON: f64 = 1e-9;

    /// Создает новый 2D-вектор с компонентами `x` и `y`
    #[inline]
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    /// Вычисляет квадрат евклидовой длины вектора: `x^2 + y^2`
    #[inline]
    pub fn length_squared(self) -> f64 {
        self.x * self.x + self.y * self.y
    }

    /// Вычисляет евклидову длину вектора: `sqrt(x^2 + y^2)`
    #[inline]
    pub fn length(self) -> f64 {
        self.length_squared().sqrt()
    }

    /// Нормализует вектор до единичной длины.
    ///
    /// Если длина вектора меньше порога [`Self::EPSILON`] или содержит нечисловые значения,
    /// возвращает [`Vec2::ZERO`] без паники или генерации `NaN`.
    #[inline]
    pub fn normalize_or_zero(self) -> Self {
        let len = self.length();
        if len > Self::EPSILON && len.is_finite() {
            Self {
                x: self.x / len,
                y: self.y / len,
            }
        } else {
            Self::ZERO
        }
    }

    /// Ограничивает длину вектора заданным максимальным значением `max_len`.
    ///
    /// Если текущая длина превышает `max_len`, вектор сохраняет ориентацию в пространстве,
    /// но его длина масштабируется ровно до `max_len`.
    #[inline]
    pub fn clamp_length(self, max_len: f64) -> Self {
        if !max_len.is_finite() || max_len <= 0.0 {
            return Self::ZERO;
        }
        if !self.is_finite() {
            tracing::warn!("clamp_length called on non-finite Vec2: {:?}", self);
            return Self::ZERO;
        }
        if self.length_squared() > max_len * max_len {
            self.normalize_or_zero() * max_len
        } else {
            self
        }
    }

    /// Проверяет, что обе компоненты вектора являются конечными вещественными числами
    #[inline]
    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }

    /// Санитизирует вектор: возвращает `self`, если компоненты конечны,
    /// либо [`Vec2::ZERO`] с предупреждением в лог при обнаружении `NaN` или `Infinity`.
    #[inline]
    pub fn sanitize_or_zero(self) -> Self {
        if self.is_finite() {
            self
        } else {
            tracing::warn!("Sanitizing non-finite Vec2 {:?} to ZERO", self);
            Self::ZERO
        }
    }

    /// Скалярное произведение двух векторов
    #[inline]
    pub fn dot(self, rhs: Self) -> f64 {
        self.x * rhs.x + self.y * rhs.y
    }

    /// Евклидово расстояние между двумя точками
    #[inline]
    pub fn distance(self, other: Self) -> f64 {
        (self - other).length()
    }

    /// Квадрат евклидова расстояния между двумя точками
    #[inline]
    pub fn distance_squared(self, other: Self) -> f64 {
        (self - other).length_squared()
    }

    /// Преобразует вектор в кортеж `(f64, f64)`
    #[inline]
    pub fn to_tuple(self) -> (f64, f64) {
        (self.x, self.y)
    }

    /// Создает вектор из кортежа `(f64, f64)`
    #[inline]
    pub fn from_tuple(tuple: (f64, f64)) -> Self {
        Self::new(tuple.0, tuple.1)
    }
}

impl Add for Vec2 {
    type Output = Self;
    #[inline]
    fn add(self, rhs: Self) -> Self {
        Self::new(self.x + rhs.x, self.y + rhs.y)
    }
}

impl AddAssign for Vec2 {
    #[inline]
    fn add_assign(&mut self, rhs: Self) {
        self.x += rhs.x;
        self.y += rhs.y;
    }
}

impl Sub for Vec2 {
    type Output = Self;
    #[inline]
    fn sub(self, rhs: Self) -> Self {
        Self::new(self.x - rhs.x, self.y - rhs.y)
    }
}

impl SubAssign for Vec2 {
    #[inline]
    fn sub_assign(&mut self, rhs: Self) {
        self.x -= rhs.x;
        self.y -= rhs.y;
    }
}

impl Mul<f64> for Vec2 {
    type Output = Self;
    #[inline]
    fn mul(self, rhs: f64) -> Self {
        Self::new(self.x * rhs, self.y * rhs)
    }
}

impl Mul<Vec2> for f64 {
    type Output = Vec2;
    #[inline]
    fn mul(self, rhs: Vec2) -> Vec2 {
        Vec2::new(self * rhs.x, self * rhs.y)
    }
}

impl MulAssign<f64> for Vec2 {
    #[inline]
    fn mul_assign(&mut self, rhs: f64) {
        self.x *= rhs;
        self.y *= rhs;
    }
}

impl Div<f64> for Vec2 {
    type Output = Self;
    #[inline]
    fn div(self, rhs: f64) -> Self {
        if rhs.abs() > Self::EPSILON && rhs.is_finite() {
            Self::new(self.x / rhs, self.y / rhs)
        } else {
            tracing::warn!("Division by zero or invalid scalar: {}", rhs);
            Self::ZERO
        }
    }
}

impl DivAssign<f64> for Vec2 {
    #[inline]
    fn div_assign(&mut self, rhs: f64) {
        *self = *self / rhs;
    }
}

impl Neg for Vec2 {
    type Output = Self;
    #[inline]
    fn neg(self) -> Self {
        Self::new(-self.x, -self.y)
    }
}

impl From<(f64, f64)> for Vec2 {
    #[inline]
    fn from((x, y): (f64, f64)) -> Self {
        Self::new(x, y)
    }
}

impl From<Vec2> for (f64, f64) {
    #[inline]
    fn from(v: Vec2) -> Self {
        (v.x, v.y)
    }
}

impl From<[f64; 2]> for Vec2 {
    #[inline]
    fn from([x, y]: [f64; 2]) -> Self {
        Self::new(x, y)
    }
}

impl From<Vec2> for [f64; 2] {
    #[inline]
    fn from(v: Vec2) -> Self {
        [v.x, v.y]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vec2_constructors_and_constants() {
        let v = Vec2::new(3.0, 4.0);
        assert_eq!(v.x, 3.0);
        assert_eq!(v.y, 4.0);
        assert_eq!(Vec2::ZERO, Vec2::new(0.0, 0.0));
        assert_eq!(Vec2::UNIT_X, Vec2::new(1.0, 0.0));
        assert_eq!(Vec2::UNIT_Y, Vec2::new(0.0, 1.0));
        assert_eq!(Vec2::default(), Vec2::ZERO);
    }

    #[test]
    fn test_vec2_arithmetic() {
        let a = Vec2::new(1.0, 2.0);
        let b = Vec2::new(3.0, 4.0);

        assert_eq!(a + b, Vec2::new(4.0, 6.0));
        assert_eq!(b - a, Vec2::new(2.0, 2.0));
        assert_eq!(a * 2.0, Vec2::new(2.0, 4.0));
        assert_eq!(2.0 * a, Vec2::new(2.0, 4.0));
        assert_eq!(b / 2.0, Vec2::new(1.5, 2.0));
        assert_eq!(-a, Vec2::new(-1.0, -2.0));

        let mut c = a;
        c += b;
        assert_eq!(c, Vec2::new(4.0, 6.0));
        c -= a;
        assert_eq!(c, b);
        c *= 2.0;
        assert_eq!(c, Vec2::new(6.0, 8.0));
        c /= 2.0;
        assert_eq!(c, b);
    }

    #[test]
    fn test_vec2_length_and_distance() {
        let v = Vec2::new(3.0, 4.0);
        assert_eq!(v.length_squared(), 25.0);
        assert_eq!(v.length(), 5.0);

        let p1 = Vec2::new(1.0, 1.0);
        let p2 = Vec2::new(4.0, 5.0);
        assert_eq!(p1.distance_squared(p2), 25.0);
        assert_eq!(p1.distance(p2), 5.0);
    }

    #[test]
    fn test_vec2_normalize_or_zero() {
        let v = Vec2::new(3.0, 4.0);
        let norm = v.normalize_or_zero();
        assert!((norm.x - 0.6).abs() < 1e-9);
        assert!((norm.y - 0.8).abs() < 1e-9);
        assert!((norm.length() - 1.0).abs() < 1e-9);

        // Нулевой вектор
        assert_eq!(Vec2::ZERO.normalize_or_zero(), Vec2::ZERO);

        // Вектор с длиной меньше EPSILON
        let tiny = Vec2::new(1e-12, 1e-12);
        assert_eq!(tiny.normalize_or_zero(), Vec2::ZERO);

        // Нечисловые значения
        let nan_vec = Vec2::new(f64::NAN, 1.0);
        assert_eq!(nan_vec.normalize_or_zero(), Vec2::ZERO);
    }

    #[test]
    fn test_vec2_clamp_length() {
        let v = Vec2::new(6.0, 8.0); // длина 10.0
        let clamped = v.clamp_length(5.0);
        assert!((clamped.length() - 5.0).abs() < 1e-9);
        assert!((clamped.x - 3.0).abs() < 1e-9);
        assert!((clamped.y - 4.0).abs() < 1e-9);

        // Длина меньше лимита не должна меняться
        let unchanged = v.clamp_length(15.0);
        assert_eq!(unchanged, v);

        // Невалидный max_len
        assert_eq!(v.clamp_length(-1.0), Vec2::ZERO);
        assert_eq!(v.clamp_length(f64::NAN), Vec2::ZERO);
    }

    #[test]
    fn test_vec2_dot() {
        let a = Vec2::new(1.0, 2.0);
        let b = Vec2::new(3.0, 4.0);
        assert_eq!(a.dot(b), 1.0 * 3.0 + 2.0 * 4.0);
    }

    #[test]
    fn test_vec2_conversions() {
        let tuple = (5.5, 6.5);
        let v: Vec2 = tuple.into();
        assert_eq!(v, Vec2::new(5.5, 6.5));
        let back_tuple: (f64, f64) = v.into();
        assert_eq!(back_tuple, tuple);

        let array = [1.0, 2.0];
        let v2: Vec2 = array.into();
        assert_eq!(v2, Vec2::new(1.0, 2.0));
        let back_array: [f64; 2] = v2.into();
        assert_eq!(back_array, array);
    }

    #[test]
    fn test_vec2_serde_roundtrip() {
        let v = Vec2::new(12.34, -56.78);
        let json = serde_json::to_string(&v).expect("Failed to serialize Vec2");
        let decoded: Vec2 = serde_json::from_str(&json).expect("Failed to deserialize Vec2");
        assert_eq!(v, decoded);
    }

    #[test]
    fn test_vec2_div_by_zero_safe() {
        let v = Vec2::new(10.0, 20.0);
        assert_eq!(v / 0.0, Vec2::ZERO);
        assert_eq!(v / 1e-12, Vec2::ZERO);
    }
}
