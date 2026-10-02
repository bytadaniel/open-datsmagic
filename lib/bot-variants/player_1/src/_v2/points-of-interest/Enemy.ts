import { PointOfInterest } from "../PointOfInterest";

export class Enemy extends PointOfInterest {
  constructor(public readonly x: number, public readonly y: number) {
    super(x, y);

    this.type = "enemy";
  }
}
