import fs from "fs";
import { WebSocketServer } from "ws";
import { Desert } from "./api/get-map-types";

function getState(): Desert {
  return JSON.parse(fs.readFileSync("./map.json").toString());
}

(async function () {
  const wss = new WebSocketServer({ port: 8081 });

  wss.on("connection", (ws) => {
    console.log("Клиент подключен");

    let map = getState();

    const syncVisualization = setInterval(() => {
      map = getState()
      ws.send(JSON.stringify(map));
    }, 500);

    ws.on("close", () => {
      console.log("Клиент отключен");

      clearInterval(syncVisualization);
    });
  });

  console.log("WebSocket сервер запущен на ws://localhost:8080");
})();
