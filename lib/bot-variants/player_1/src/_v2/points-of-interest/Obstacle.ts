import { PointOfInterest } from "../PointOfInterest";

export class Obstacle extends PointOfInterest {

  constructor(public readonly x: number, public readonly y: number) {
    super(x, y);

    this.type = "obstacle";
  }
}
