import type { RicPathfindPath } from "ric-pathfind";
import { Coord } from "../types";
import { CoordinateMap } from "../CoordinateMap";
import { MAP } from "../memory";

export function toCoordPath(path: RicPathfindPath): Coord[] {
  return path.map(([x, y]) => MAP.matrix[y][x]);
}
