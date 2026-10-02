import { Enemy } from "./points-of-interest/Enemy";
import { Obstacle } from "./points-of-interest/Obstacle";
import { Treasure } from "./points-of-interest/Treasure";

interface CoordinateSegmentContext {
  x: number;
  y: number;
  width: number;
  height: number;
  treasures: Treasure[];
  obstacles: Obstacle[];
  enemies: Enemy[];
}

export class CoordinateSegment {
  constructor(private readonly context: CoordinateSegmentContext) {}
}
