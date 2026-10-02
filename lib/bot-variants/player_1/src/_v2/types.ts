export class Coord {
  declare type: "player" | "treasure" | "obstacle" | "enemy" | "coord";

  constructor(public x: number, public y: number) {
    this.type = "coord";
  }
}

export type AStarNode = {
  position: Coord;
  g: number; // Стоимость от старта до текущей точки
  h: number; // Оценочная стоимость до цели
  f: number; // Общая стоимость (g + h)
  parent: AStarNode | null;
};
