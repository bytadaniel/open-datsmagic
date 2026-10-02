export class PointOfInterest {
  private readonly _segmentX: number = -1;
  private readonly _segmentY: number = -1;

  constructor(
    public readonly mapX: number,
    public readonly mapY: number,
  ) {}

  set segmentX(x: number) {
    this.segmentX = x;
  }

  set segmentY(y: number) {
    this.segmentY = y;
  }

  get segmentX(): number {
    return this._segmentX;
  }

  get segmentY(): number {
    return this._segmentY;
  }
}
