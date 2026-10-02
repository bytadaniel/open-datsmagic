import { Vector } from "../../api/get-map-types";

export function vectorFromTo(from: Vector, to: Vector): Vector {
  return {
    x: to.x - from.x,
    y: to.y - from.y,
  };
}

export function vectorLength(vector: Vector): number {
  // |а| = √(a1^2 + a2^2)

  return Math.abs(Math.sqrt(Math.pow(vector.x, 2) + Math.pow(vector.y, 2)));
}

// Функция для нахождения расстояния между двумя точками
export function distanceFromTo(v1: Vector, v2: Vector): number {
  return Math.sqrt(Math.pow(v2.x - v1.x, 2) + Math.pow(v2.y - v1.y, 2));
}

export function getPointAcceleration(
  from: Vector,
  to: Vector,
  maxAccel: number
): Vector {
  const fromTo = vectorFromTo(from, to);
  const length = vectorLength(fromTo);

  const factor = maxAccel / length;

  return {
    x: fromTo.x * factor,
    y: fromTo.y * factor,
  };
}
