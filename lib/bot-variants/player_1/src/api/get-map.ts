import { Desert } from "./get-map-types";
import { host, token } from "./token";

export async function getMap(): Promise<Desert> {
  const response = await fetch(`${host}/play/magcarp/player/move`, {
    method: "POST",
    headers: {
      ContentType: "application/json",
      ["X-Auth-Token"]: token,
    },
    body: JSON.stringify({
      transports: [],
    }),
  });

  return response.json();
}
