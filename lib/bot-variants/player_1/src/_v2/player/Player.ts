import { MAP } from "../memory";
import { Treasure } from "../points-of-interest/Treasure";
import { Coord } from "../types";

export class Player extends Coord {
  constructor(x: number, y: number) {
    super(x, y);

    this.type = "player";
  }

  private eatTreasure(coord: Coord): void {
    if (coord instanceof Treasure) {
      MAP.matrix[coord.y][coord.x] = new Coord(coord.x, coord.y);
    }
  }

  // public setPath(pointOfInterest: PointOfInterest, map: CoordinateMap): void {
  //   this.path = new PlayerMovement(
  //     toCoordPath(
  //       map,
  //       this.pathfinder.findPath(
  //         this.x,
  //         this.y,
  //         pointOfInterest.x,
  //         pointOfInterest.y,
  //         new PF.Grid(toObstacleMatrix(map))
  //       )
  //     )
  //   );
  // }

  // public async move(): Promise<void> {
  //   while (this.path && !this.path.isEmpty()) {
  //     const coord = this.path.dequeue();

  //     console.log("move to", coord);

  //     if (coord) {
  //       this.x = coord.x;
  //       this.y = coord.y;

  //       this.eatTreasure(coord);
  //     }

  //     await setTimeout(50);
  //   }
  // }
}
