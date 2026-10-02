import fs from "fs";
import { RateLimiter } from "limiter";
import { minBy } from "lodash";
import { Coord } from "../_v2/types";
import { between } from "../_v2/utils/between";
import { getMap } from "../api/get-map";
import { Enemy, Vector } from "../api/get-map-types";
import { move, TransportPayload } from "../api/move";
import {
  normalizeVector,
  subtractVector,
} from "./functions/acceleration-to-bounty";
import { inCircle } from "./functions/deep-anomaly-acceleration";
import {
  distanceFromTo,
  vectorFromTo,
  vectorLength,
} from "./functions/point-acceleration";
import { normalizeToLength, sumVectors } from "./functions/stop-acceleration";

function dotProduct(v1: Vector, v2: Vector): number {
  return v1.x * v2.x + v1.y * v2.y;
}

function magnitude(v: Vector): number {
  return Math.sqrt(v.x * v.x + v.y * v.y);
}

function degreesToRadians(degrees: number): number {
  return degrees * (Math.PI / 180);
}

function areVectorsAligned(
  _v1: Vector,
  _v2: Vector,
  tolerance: number = 0.1
): boolean {
  const v1 = normalizeVector(_v1);
  const v2 = normalizeVector(_v2);

  const dot = dotProduct(v1, v2);
  const mag1 = magnitude(v1);
  const mag2 = magnitude(v2);
  const cosTheta = dot / (mag1 * mag2);

  // Если косинус близок к 1, вектора смотрят в одном направлении
  return cosTheta >= 1 - tolerance;
}

// Predict enemy position after time t
function predictEnemyPosition({ x, y, velocity }: Enemy, t: number): Vector {
  return {
    x: x + velocity.x * t,
    y: y + velocity.y * t,
  };
}

// Calculate grenade throw point
function calculateGrenadeThrowPoint(
  agent: Vector,
  enemy: Enemy,
  grenadeRange: number,
  explosionRadius: number
): Vector | null {
  const maxTime = 500;
  const timeStep = 500;

  for (let t = 0; t <= maxTime; t += timeStep) {
    const predictedPosition = predictEnemyPosition(enemy, t);
    const distToEnemy = distanceFromTo(agent, predictedPosition);

    if (distToEnemy <= grenadeRange && distToEnemy > explosionRadius) {
      return predictedPosition;
    }
  }
  return null;
}

function getBountyValue(x: number, y: number, minute: number): number {
  return (Math.floor(Math.sqrt(x * x + y * y) / 1000) * minute) / 5;
}

function getCurrentMinuteOfDay(): number {
  const now = new Date();
  return now.getHours() * 60 + now.getMinutes();
}

(async function () {
  const limiter = new RateLimiter({ tokensPerInterval: 3, interval: 1000 });
  let map = await getMap();

  let targetBounty: Vector | null;

  setInterval(() => fs.writeFileSync("./map.json", JSON.stringify(map)), 500);

  new Promise(async () => {
    while (true) {
      await limiter.removeTokens(1);
      if (!map?.transports?.length) {
        console.log(map);
        process.exit(0);
      }

      const transportPayloads: TransportPayload[] = [];

      for (const transport of map.transports) {
        if (transport.status !== "alive") continue;
        const agent = new Coord(transport.x, transport.y);

        if (!map.enemies) console.log(map);

        const enemies = map.enemies.map((e) => ({
          enemy: e,
          distance: vectorLength(vectorFromTo(agent, e)),
        }));

        const enemyToAttack = minBy(
          enemies.filter((e) =>
            between(
              e.distance,
              map.attackExplosionRadius,
              map.attackRange + map.attackExplosionRadius
            )
          ),
          ({ enemy, distance }) => enemy.health / distance
        );

        if (targetBounty) {
          // sync path
          targetBounty =
            map.bounties.find(
              (b) => b.x === targetBounty!.x && b.y === targetBounty!.y
            ) ?? null;
        }

        const bounty = minBy(
          map.bounties.filter((b) => inCircle(b, agent, 200)),
          (b) => {
            const distance = distanceFromTo(agent, new Coord(b.x, b.y));
            const value = getBountyValue(b.x, b.x, b.points);
            // const windDirection = vectorLength(
            //   subtractVector(vectorFromTo(transport, b), transport.velocity)
            // );

            return distance / value;
          }
        );

        const anomaly = minBy(map.anomalies, (a) =>
          distanceFromTo(agent, new Coord(a.x, a.y))
        );

        const canAttack = enemyToAttack && transport.attackCooldownMs === 0;
        let attackCoord: Vector | null = null;
        if (canAttack) {
          attackCoord = calculateGrenadeThrowPoint(
            agent,
            enemyToAttack.enemy,
            map.attackRange,
            map.attackExplosionRadius
          );

          if (attackCoord) {
            console.log(
              `Attack with grenade {${attackCoord.x};${attackCoord.y}} from {${agent.x};${agent.y}}. Distance=${enemyToAttack.distance}`
            );
          }
        }

        const velocity = vectorLength(transport.velocity);
        const anomalyAcceleration = vectorLength(transport.anomalyAcceleration)

        const acceles: Vector[] = [];

        const antyAnomalyAcceleration = {
          x: -transport.anomalyAcceleration.x,
          y: -transport.anomalyAcceleration.y,
        }

        const antyVelocity = {
          x: -transport.velocity.x,
          y: -transport.velocity.y,
        };

        const target: Vector = targetBounty ??
          anomaly ?? { x: map.mapSize.x * 0.7, y: map.mapSize.y * 0.7 };

        const deltaVector = subtractVector(
          normalizeVector(transport.velocity),
          normalizeVector(target)
        );

        const velocityToTargetDelta = vectorLength(
          subtractVector(
            normalizeVector(transport.velocity),
            normalizeVector(target)
          )
        );

        if (anomalyAcceleration > 200) {
          acceles.push(
            antyAnomalyAcceleration,
            antyAnomalyAcceleration,
            antyAnomalyAcceleration,
            antyAnomalyAcceleration,
            antyAnomalyAcceleration
          );
        }
        if (velocity > 30 && velocityToTargetDelta > 0.15) {
          acceles.push(antyVelocity);
        } else {
          if (bounty) {
            const toTarget = vectorFromTo(agent, bounty);
            acceles.push(toTarget);

            if (vectorLength(toTarget) < 100) {
              acceles.push(antyVelocity, antyVelocity);
            }
          } else {
            const toTarget = vectorFromTo(agent, anomaly!);
            acceles.push(toTarget);
          }
        }

        const acceleration = normalizeToLength(
          sumVectors(new Coord(0, 0), acceles),
          map.maxAccel
        );

        transportPayloads.push({
          id: transport.id,
          acceleration,
          ...(enemyToAttack && {
            ...(transport.shieldCooldownMs === 0 && { activateShield: true }),
            ...(transport.attackCooldownMs === 0 &&
              canAttack &&
              attackCoord && {
                attack: attackCoord,
              }),
          }),
        });
      }

      map = await move({ transports: transportPayloads });

      if (map.errors?.length) console.log(map.errors);
    }
  });
})();
