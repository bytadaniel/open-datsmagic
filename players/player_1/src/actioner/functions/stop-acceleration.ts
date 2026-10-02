import { Coord } from "../../_v2/types";
import { Transport, Vector } from "../../api/get-map-types";
import { addVector } from "./acceleration-to-bounty";

export function normalizeToLength(v: Vector, targetLength: number): Vector {
  // Вычисляем длину исходного вектора
  const magnitude = Math.sqrt(v.x * v.x + v.y * v.y);

  // Если длина нулевая, возвращаем вектор (0, 0)
  if (magnitude === 0) {
    return { x: 0, y: 0 };
  }

  // Нормализуем вектор, а затем умножаем его на целевую длину
  return {
    x: (v.x / magnitude) * targetLength,
    y: (v.y / magnitude) * targetLength,
  };
}

export function sumVectors(acceleration: Vector, vectors: Vector[]): Vector {
  // let result = { ...acceleration };

  const vectorsum = vectors.reduce((sum, curr) => {
    return (sum = addVector(sum, curr));
  }, new Coord(0, 0));

  // vectors.forEach((vector) => {
  //   result.x += vector.x;
  //   result.y += vector.y;
  // });

  return vectorsum;
}

export function getStopSelfVelocityAccel(transport: Transport): Vector {
  return {
    x: -transport.velocity.x,
    y: -transport.velocity.y,
  };
}

export function stopAnomalyAccel(transport: Transport): Vector {
  return {
    x: -transport.anomalyAcceleration.x,
    y: -transport.anomalyAcceleration.y,
  };
}

export function stopSelfAccel(transport: Transport): Vector {
  return {
    x: -transport.selfAcceleration.x,
    y: -transport.selfAcceleration.y,
  };
}

/** Сколько нужно приложить сил, чтобы оставаться в покоец */
export function getStopAcceleration(
  transport: Transport,
  maxAccel: number
): Vector {
  const factor = 1.1;
  return normalizeToLength(
    sumVectors(new Coord(0, 0), [
      normalizeToLength(getStopSelfVelocityAccel(transport), maxAccel), // stop velocity,
      // normalizeToLength(
      //   {
      //     x: -transport.anomalyAcceleration.x * factor,
      //     y: -transport.anomalyAcceleration.y * factor,
      //   },
      //   maxAccel
      // ), // stop anomaly accel
      // normalizeToLength(
      //   {
      //     x: -transport.selfAcceleration.x * factor,
      //     y: -transport.selfAcceleration.y * factor,
      //   },
      //   maxAccel,
      // ), // stop self accel
    ]),
    maxAccel
  );
}
