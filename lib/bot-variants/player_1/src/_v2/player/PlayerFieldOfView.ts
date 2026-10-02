import { minBy } from "lodash";
import { CoordinateMap } from "../CoordinateMap";
import { MAP_SIZE, PLAYER_FOV_OF_MAP } from "../memory";
import { Coord } from "../types";
import { Player } from "./Player";
import { Treasure } from "../points-of-interest/Treasure";
import { MatrixIndex } from "../utils/MatrixIndex";

export class PlayerFieldOfView {
  private readonly fov = Math.round(MAP_SIZE * PLAYER_FOV_OF_MAP);
  private index: MatrixIndex;

  constructor(
    private readonly player: Player,
    private readonly map: CoordinateMap
  ) {
    this.index = new MatrixIndex(this.player, []);
  }

  public render(): void {
    this.index = new MatrixIndex(this.player, this.getMatrix());
  }

  public getTreasures(): Treasure[] {
    return [...this.index.treasures.values()].map((i) => i.ref);
  }

  public closestTreasure(): Treasure | null {
    return (
      minBy([...this.index.treasures.values()], (i) => i.distanceToPlayer)
        ?.ref ?? null
    );
  }

  private getMatrix(): Coord[][] {
    const rowSlice = this.map.matrix.slice(
      Math.max(this.player.y - this.fov, 0),
      Math.min(this.player.y + this.fov, MAP_SIZE)
    );

    return rowSlice.map((row) =>
      row.slice(
        Math.max(this.player.x - this.fov, 0),
        Math.min(this.player.x + this.fov, MAP_SIZE)
      )
    );
  }
}
