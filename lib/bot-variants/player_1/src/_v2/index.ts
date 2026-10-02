import { minBy } from "lodash";
import { setTimeout } from "timers/promises";
import { WebSocketServer } from "ws";
import { getMap } from "../api/get-map";
import { Desert } from "../api/get-map-types";
import { move, TransportPayload } from "../api/move";
import { Coord } from "./types";
import { getCoordsDistance } from "./utils/get-coords-distance";
import { calculateTrajectory } from "../vectors/calculate-trajectory";

async function main() {
  const wss = new WebSocketServer({ port: 8080 });

  wss.on("connection", (ws) => {
    console.log("Клиент подключен");

    let map: Desert;
    const transports: Map<string, TransportPayload> = new Map();

    const moveInterval = setInterval(async () => {
      const payloadTransports = [...transports.values()];
      transports.clear();
      console.log("Movement actions", payloadTransports.length);

      if (payloadTransports.length) {
        map = await move({
          transports: [...transports.values()],
        });
      }
    }, 500);

    const stateInterval = setInterval(async () => {
      map = await getMap();
      // Отправляем карту клиенту
      ws.send(JSON.stringify(map));
    }, 2000);

    new Promise(async () => {
      await setTimeout(5000);

      while (true) {
        const bounties = map.bounties.map((b) => new Coord(b.x, b.y));

        for (const transport of map.transports) {
          if (transport.status !== "alive") {
            continue;
          }

          const player = new Coord(transport.x, transport.y);

          console.time("findClosestBounty");
          const nearBounty = minBy(bounties, (b) =>
            getCoordsDistance(player, b)
          );
          console.timeEnd("findClosestBounty");

          if (nearBounty) {
            const from = { x: transport.x, y: transport.y };
            const to = { x: nearBounty!.x, y: nearBounty!.y };

            const { requiredOwnAccelerationChange } = calculateTrajectory(
              from,
              to,
              transport.velocity,
              transport.selfAcceleration,
              transport.anomalyAcceleration,
              map.maxAccel,
              map.maxSpeed
            );

            transports.set(transport.id, {
              id: transport.id,
              acceleration: requiredOwnAccelerationChange,
            });
          }

          console.time("calculateTrajectory");
          console.timeEnd("calculateTrajectory");
        }
      }
    });

    ws.on("close", () => {
      console.log("Клиент отключен");
      clearInterval(stateInterval);
      clearInterval(moveInterval);
    });
  });

  console.log("WebSocket сервер запущен на ws://localhost:8080");
}

main();

// async function main(): Promise<void> {
//   const fov = new PlayerFieldOfView(PLAYER, MAP);
//   const movement = new PlayerMovement(MAP, PLAYER, fov);

//   setInterval(() => {
//     movement.lookAround();
//   }, 1000);

//   setInterval(() => {
//     movement.move();
//   }, 300);
// }

// main();
