import { Anomaly, Vector } from "../../api/get-map-types";
import { vectorFromTo, vectorLength } from "./point-acceleration";
import { normalizeToLength } from "./stop-acceleration";

export function inCircle(
  point: Vector,
  center: Vector,
  radius: number
): boolean {
  const distance = vectorLength(vectorFromTo(point, center));

  return distance <= radius;
}

export function getRunDeepAnomalyAcceleration(
  carpet: Vector,
  anomalies: Anomaly[],
  maxAccel: number
): Vector {
  let escapeAcceleration: Vector = { x: 0, y: 0 };

  const dangerous = anomalies.filter((anomaly) => {
    return (
      anomaly.strength > 0 && inCircle(carpet, anomaly, anomaly.radius * 0.6)
    );
  });

  dangerous.forEach((danger) => {
    // Вектор от центра аномалии к ковру
    const direction = vectorFromTo(danger, carpet); // от центра

    // Нормализуем направление
    const normalizedDirection = normalizeToLength(direction, maxAccel);

    // Умножаем нормализованный вектор на радиус для нахождения безопасной точки на границе
    const escapePoint: Vector = {
      x: danger.x + normalizedDirection.x * danger.radius, // Увеличиваем радиус на 10% для безопасности
      y: danger.y + normalizedDirection.y * danger.radius,
    };

    // Вектор для выхода из опасной зоны (от ковра к безопасной точке)
    const escapeVector = vectorFromTo(carpet, escapePoint);

    // Суммируем ускорение для всех аномалий, которые затрагивают ковёр
    escapeAcceleration = {
      x: escapeAcceleration.x + escapeVector.x,
      y: escapeAcceleration.y + escapeVector.y,
    };
  });

  return normalizeToLength(escapeAcceleration, maxAccel);
}
