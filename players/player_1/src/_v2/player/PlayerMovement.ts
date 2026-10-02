import PF from "ric-pathfind";
import { bfsFindClosest } from "../bfs/bfs-find-closest";
import { CoordinateMap } from "../CoordinateMap";
import { Treasure } from "../points-of-interest/Treasure";
import { Coord } from "../types";
import { toCoordPath } from "../utils/to-coord-path";
import { toObstacleMatrix } from "../utils/to-obstacle-matrix";
import { Player } from "./Player";
import { PlayerFieldOfView } from "./PlayerFieldOfView";

export class PlayerMovement {
  public readonly traveled: Coord[] = [];
  public readonly ahead: Coord[] = [];

  private readonly pathfinder = new PF.JumpPointFinder();

  constructor(
    private readonly map: CoordinateMap,
    private readonly player: Player,
    private readonly playerFov: PlayerFieldOfView
  ) {}

  public lookAround(): void {
    this.playerFov.render();
  }

  public async move(): Promise<void> {
    const coord = this.ahead.shift();

    if (coord) {
      this.player.x = coord.x;
      this.player.y = coord.y;

      if (coord instanceof Treasure) {
        this.map.matrix[coord.y][coord.x] = new Coord(coord.x, coord.y);
      }

      this.traveled.push(coord);
    } else {
      const treasure = this.searchForTreasure();

      if (!treasure) {
        process.exit(0);
      }

      const nextPath = this.pathfinder.findPath(
        this.player.x,
        this.player.y,
        treasure.x,
        treasure.y,
        new PF.Grid(toObstacleMatrix(this.map))
      );

      this.ahead.push(...toCoordPath(nextPath));
    }
  }

  private searchForTreasure(): Treasure | null {
    const indexedTreasure = this.playerFov.closestTreasure();

    if (indexedTreasure) {
      return indexedTreasure;
    }

    const bfsTreasure = bfsFindClosest(Treasure, this.player, this.map.matrix);

    if (bfsTreasure) {
      return bfsTreasure as Treasure;
    }

    return null;
  }
}
