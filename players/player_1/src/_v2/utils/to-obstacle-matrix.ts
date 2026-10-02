import { CoordinateMap } from "../CoordinateMap";
import { Enemy } from "../points-of-interest/Enemy";
import { Obstacle } from "../points-of-interest/Obstacle";

export function toObstacleMatrix(map: CoordinateMap): (0 | 1)[][] {
  return map.matrix.map((row) =>
    row.map((coord) => {
      if (coord instanceof Obstacle) {
        return 1;
      }

      if (coord instanceof Enemy) {
        return 1;
      }

      return 0;
    })
  );
}
