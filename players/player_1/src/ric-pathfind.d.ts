/**
 * https://www.npmjs.com/package/ric-pathfind
 */
declare module "ric-pathfind" {
  type RicPathfindPath = [x: number, y: number][];
  export class Grid {
    constructor(matrix: (0 | 1)[][]): void;
  }

  export class AStarFinder {
    constructor(
      options: {
        allowDiagonal?: boolean;
      } = {}
    ) {}

    public findPath(
      x1: number,
      y1: number,
      x2: number,
      y2: number,
      grid: Grid
    ): RicPathfindPath;
  }

  export class JumpPointFinder {
    public findPath(
      x1: number,
      y1: number,
      x2: number,
      y2: number,
      grid: Grid
    ): RicPathfindPath;
  }
}
