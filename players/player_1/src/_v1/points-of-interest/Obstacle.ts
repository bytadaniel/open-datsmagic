import { PointOfInterest } from "../PointOfInterest";

export class Obstacle extends PointOfInterest {
  constructor(public readonly mapX: number, public readonly mapY: number) {
    super(mapX, mapY);
  }
}
