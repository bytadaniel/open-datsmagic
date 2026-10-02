import { Coord } from "../types";

export function getCoordsDistance(from: Coord, to: Coord): number {
  const deltaX = to.x - from.x;
  const deltaY = to.y - from.y;
  
  return Math.sqrt(deltaX * deltaX + deltaY * deltaY);
}