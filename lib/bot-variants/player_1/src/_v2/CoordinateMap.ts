import { randomInt } from "crypto";
import { bfsFindClosest } from "./bfs/bfs-find-closest";
import { Player } from "./player/Player";
import { PointOfInterest } from "./PointOfInterest";
import { Enemy } from "./points-of-interest/Enemy";
import { Obstacle } from "./points-of-interest/Obstacle";
import { Treasure } from "./points-of-interest/Treasure";
import { Coord } from "./types";
import { choose } from "./utils/choose";
import { emptyMatrix } from "./utils/emty-matrix";

export class CoordinateMap {
  public matrix: Coord[][];

  constructor(
    private readonly width: number,
    private readonly height: number,
    private readonly pointsOfInterest: number
  ) {
    this.matrix = emptyMatrix(this.width, this.height);

    for (let i = 0; i < this.pointsOfInterest; i++) {
      const pointOfInterest = this.randomPointOfInterest(
        randomInt(this.width),
        randomInt(this.height)
      );

      this.matrix[pointOfInterest.y][pointOfInterest.x] = pointOfInterest;
    }
  }

  public getCoord(x: number, y: number): Coord {
    return this.matrix[y][x];
  }

  public chooseTreasure(player: Player): Treasure {
    const treasure = bfsFindClosest(Treasure, player, this.matrix);

    if (!treasure) {
      throw new Error("Treasure not found");
    }

    return treasure as Treasure;
  }

  public setCoord(CoordClass: typeof Coord): void {
    const coord = new CoordClass(randomInt(this.width), randomInt(this.height));

    this.matrix[coord.y][coord.x] = coord;
  }

  private randomPointOfInterest(x: number, y: number): PointOfInterest {
    return choose<PointOfInterest>([
      new Enemy(x, y),
      new Obstacle(x, y),
      new Treasure(x, y, randomInt(100) + 1),
    ]);
  }
}
