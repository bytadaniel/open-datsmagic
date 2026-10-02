import { Bounty, Transport, Vector } from "../../api/get-map-types";

export function normalizeVector(v: Vector): Vector {
  const mag = Math.hypot(v.x, v.y);
  if (mag === 0) return { x: 0, y: 0 };
  return { x: v.x / mag, y: v.y / mag };
}

export function scaleVector(v: Vector, scalar: number): Vector {
  return { x: v.x * scalar, y: v.y * scalar };
}

export function addVector(v1: Vector, v2: Vector): Vector {
  return { x: v1.x + v2.x, y: v1.y + v2.y };
}

export function subtractVector(v1: Vector, v2: Vector): Vector {
  return { x: v1.x - v2.x, y: v1.y - v2.y };
}

export function accelToPoint(
  transport: Transport,
  bounty: Bounty,
  maxAccel: number
): Vector {
  // Вспомогательные функции
  function distance(a: Vector, b: Vector): number {
    return Math.hypot(a.x - b.x, a.y - b.y);
  }

  // 1. Вычисляем желаемое общее ускорение к bounty
  const transportPos: Vector = { x: transport.x, y: transport.y };
  const directionToBounty = subtractVector({ x: bounty.x, y: bounty.y }, transportPos);
  const distanceToBounty = distance(transportPos, { x: bounty.x, y: bounty.y });

  // Если мы уже рядом с bounty, нет необходимости ускоряться
  if (distanceToBounty <= bounty.radius) {
    return { x: 0, y: 0 };
  }

  // Нормализуем направление и умножаем на максимальное ускорение
  const desiredDirectionNormalized = normalizeVector(directionToBounty);
  const desiredNetAcceleration = scaleVector(desiredDirectionNormalized, maxAccel);

  // 2. Вычисляем необходимое собственное ускорение (selfAcceleration)
  // Общее ускорение = selfAcceleration + anomalyAcceleration
  let requiredSelfAcceleration = subtractVector(desiredNetAcceleration, transport.anomalyAcceleration);

  // Ограничиваем selfAcceleration по максимальному ускорению
  const selfAccelMagnitude = Math.hypot(requiredSelfAcceleration.x, requiredSelfAcceleration.y);
  if (selfAccelMagnitude > maxAccel) {
    requiredSelfAcceleration = scaleVector(normalizeVector(requiredSelfAcceleration), maxAccel);
  }

  return requiredSelfAcceleration;
}