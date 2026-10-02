import { randomInt } from "crypto";
import { PointOfInterest } from "../PointOfInterest";

export class Treasure extends PointOfInterest {
  constructor(
    public readonly x: number,
    public readonly y: number,
    public readonly amount: number = randomInt(100)
  ) {
    super(x, y);

    this.type = "treasure";
  }
}
