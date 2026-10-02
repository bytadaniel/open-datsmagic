import { setTimeout } from "timers/promises";
import { Desert, Vector } from "./get-map-types";
import { host, token } from "./token";

export interface TransportPayload {
  acceleration?: Vector;
  activateShield?: boolean;
  attack?: {};
  id: string;
}

interface MovePayload {
  transports: TransportPayload[];
}

export async function move(payload: MovePayload): Promise<Desert> {
  const response = await fetch(`${host}/play/magcarp/player/move`, {
    method: "POST",
    headers: {
      ContentType: "application/json",
      ["X-Auth-Token"]: token,
    },
    body: JSON.stringify(payload),
  });

  if (response.status === 429) {
    await setTimeout(500);
    return move(payload)
  }

  return response.json();
}
