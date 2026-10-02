import { minBy } from "lodash";
import { getMap } from "../api/get-map";
import { Coord } from "../_v2/types";
import { getCoordsDistance } from "../_v2/utils/get-coords-distance";

export function calculateTrajectory(
  from: { x: number; y: number },
  to: { x: number; y: number },
  velocity: { x: number; y: number },
  selfAcceleration: { x: number; y: number },
  anomalyAcceleration: { x: number; y: number },
  maxAccel: number,
  maxSpeed: number
): {
  requiredOwnAccelerationChange: { x: number; y: number };
  timeToApplyForce: number;
  totalTime: number;
} {
  // Вычисляем разницу позиций
  const deltaPosition = {
    x: to.x - from.x,
    y: to.y - from.y,
  };

  // Функция для расчёта по одной оси
  function computeAxis(
    deltaS: number,
    v0: number,
    aCurrent: number,
    aAnomaly: number
  ): { deltaAOwn: number; t: number } {
    // Максимальное собственное ускорение, которое можем добавить или вычесть
    const deltaAOwnMax = maxAccel - Math.abs(aCurrent);

    // Предполагаем, что будем использовать максимальное доступное изменение собственного ускорения в направлении к цели
    const deltaAOwn = deltaAOwnMax * Math.sign(deltaS);

    // Суммарное ускорение после изменения собственного ускорения
    const aTotal = aCurrent + deltaAOwn + aAnomaly;

    // Решаем квадратное уравнение относительно времени t:
    // 0.5 * aTotal * t^2 + v0 * t - deltaS = 0
    const a = 0.5 * aTotal;
    const b = v0;
    const c = -deltaS;

    const discriminant = b * b - 4 * a * c;

    if (discriminant < 0) {
      throw new Error("Невозможно достичь цели с заданными ограничениями.");
    }

    const sqrtDiscriminant = Math.sqrt(discriminant);

    // Два возможных решения для времени
    const t1 = (-b + sqrtDiscriminant) / (2 * a);
    const t2 = (-b - sqrtDiscriminant) / (2 * a);

    // Выбираем положительное и реальное время
    let t = Math.max(t1, t2);
    if (t < 0 || isNaN(t)) {
      throw new Error("Невозможно достичь цели с заданными ограничениями.");
    }

    // Проверяем ограничение по скорости
    const vFinal = v0 + aTotal * t;
    if (Math.abs(vFinal) > maxSpeed) {
      // Ограничиваем конечную скорость
      const vLimited = maxSpeed * Math.sign(vFinal);

      // Пересчитываем время с учётом ограниченной скорости
      const tAcc = (vLimited - v0) / aTotal;
      const sAcc = v0 * tAcc + 0.5 * aTotal * tAcc * tAcc;

      // Оставшееся расстояние
      const sConst = deltaS - sAcc;

      // Время движения с постоянной скоростью
      const tConst = sConst / vLimited;

      t = tAcc + tConst;
    }

    return { deltaAOwn, t };
  }

  // Расчёты по оси X
  const resultX = computeAxis(
    deltaPosition.x,
    velocity.x,
    selfAcceleration.x,
    anomalyAcceleration.x
  );

  // Расчёты по оси Y
  const resultY = computeAxis(
    deltaPosition.y,
    velocity.y,
    selfAcceleration.y,
    anomalyAcceleration.y
  );

  // Общее время движения — максимальное из двух
  const totalTime = Math.max(resultX.t, resultY.t);

  // Необходимое изменение собственного ускорения
  const requiredOwnAccelerationChange = {
    x: resultX.deltaAOwn,
    y: resultY.deltaAOwn,
  };

  // Время приложения силы — минимальное из времён разгона по осям
  const timeToApplyForce = Math.min(resultX.t, resultY.t);

  return {
    requiredOwnAccelerationChange,
    timeToApplyForce,
    totalTime,
  };
}

async function main() {
  const map = await getMap();

  const {
    transports: [transport],
    bounties,
  } = map;

  const player = new Coord(transport.x, transport.y);
  const treasures = bounties.map((b) => new Coord(b.x, b.y));

  console.time("findClosestBounty");
  const closestTreasure = minBy(treasures, (t) => getCoordsDistance(player, t));
  console.timeEnd("findClosestBounty");

  const from = { x: transport.x, y: transport.y };
  const to = { x: closestTreasure!.x, y: closestTreasure!.y };

  console.log(from, to);

  console.time("calculateTrajectory");
  const result = calculateTrajectory(
    from,
    to,
    transport.velocity,
    transport.selfAcceleration,
    transport.anomalyAcceleration,
    map.maxAccel,
    map.maxSpeed
  );
  console.timeEnd("calculateTrajectory");

  console.log(
    "Необходимое собственное ускорение:",
    result.requiredOwnAccelerationChange
  );
  console.log(
    "Время приложения силы:",
    result.timeToApplyForce.toFixed(2),
    "секунд"
  );
  console.log("Общее время движения:", result.totalTime.toFixed(2), "секунд");
}
