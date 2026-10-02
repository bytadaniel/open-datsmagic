import { bfsFilter } from "../bfs/bfs-filter";
import { Player } from "../player/Player";
import { PointOfInterest } from "../PointOfInterest";
import { Enemy } from "../points-of-interest/Enemy";
import { Obstacle } from "../points-of-interest/Obstacle";
import { Treasure } from "../points-of-interest/Treasure";
import { Coord } from "../types";
import { getCoordsDistance } from "./get-coords-distance";

type IndexedPointOfView<T> = {
  ref: T;
  distanceToPlayer: number;
};

export class MatrixIndex {
  public obstacles: Set<IndexedPointOfView<Obstacle>>;
  public treasures: Set<IndexedPointOfView<Treasure>>;
  public enemies: Set<IndexedPointOfView<Enemy>>;

  constructor(private readonly player: Player, private matrix: Coord[][]) {
    this.obstacles = new Set();
    this.treasures = new Set();
    this.enemies = new Set();

    this.index();
  }

  public index(): void {
    const pointsOfInterest = bfsFilter(
      PointOfInterest,
      this.player,
      this.matrix
    );

    for (const pointOfInterest of pointsOfInterest) {
      if (pointOfInterest instanceof Enemy) {
        this.enemies.add({
          ref: pointOfInterest,
          distanceToPlayer: getCoordsDistance(this.player, pointOfInterest),
        });
      }

      if (pointOfInterest instanceof Treasure) {
        this.treasures.add({
          ref: pointOfInterest,
          distanceToPlayer: getCoordsDistance(this.player, pointOfInterest),
        });
      }

      if (pointOfInterest instanceof Obstacle) {
        this.obstacles.add({
          ref: pointOfInterest,
          distanceToPlayer: getCoordsDistance(this.player, pointOfInterest),
        });
      }
    }
  }
}
