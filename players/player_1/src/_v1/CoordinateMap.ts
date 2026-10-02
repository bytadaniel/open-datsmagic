import { CoordinateSegment } from "./CoordinateSegment";

export class CoordinateMap {
  constructor(
    private readonly width: number,
    private readonly height: number
  ) {
    new CoordinateSegment({
      x: 0,
      y: 0,
      width,
      height,
      enemies: [],
      obstacles: [],
      treasures: [],
    })
  }
}
