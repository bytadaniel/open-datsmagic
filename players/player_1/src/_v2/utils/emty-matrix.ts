import { Coord } from "../types";

export function emptyMatrix(width: number, height: number): Coord[][] {
  const matrix: Coord[][] = [];

  for (let y = 0; y < height; y++) {
    matrix.push([]);
  }

  matrix.forEach((row, y) => {
    for (let x = 0; x < width; x++) {
      row.push(new Coord(x, y));
    }
  });

  return matrix;
}
