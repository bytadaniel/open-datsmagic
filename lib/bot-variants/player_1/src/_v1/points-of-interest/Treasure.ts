import { PointOfInterest } from "../PointOfInterest";

export class Treasure extends PointOfInterest {
  constructor(
    public readonly mapX: number,
    public readonly mapY: number,
    public readonly amount: number
  ) {
    super(mapX, mapY);
  }
}
