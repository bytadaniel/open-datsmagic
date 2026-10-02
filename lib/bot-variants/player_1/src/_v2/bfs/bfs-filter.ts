import { PointOfInterest } from "../PointOfInterest";
import { Coord } from "../types";

export function bfsFilter<T>(
  Searchable: typeof PointOfInterest,
  start: Coord,
  grid: Coord[][]
): PointOfInterest[] {
  if (!grid.length) {
    return [];
  }

  const searchables: PointOfInterest[] = [];

  const directions = [
    { x: 0, y: 1 },
    { x: 1, y: 0 },
    { x: 0, y: -1 },
    { x: -1, y: 0 },
  ];

  const rows = grid.length;
  const cols = grid[0].length;
  const visited: boolean[][] = Array.from(Array(rows), () =>
    Array(cols).fill(false)
  );
  const queue: Coord[] = [start];
  visited[start.y][start.x] = true;

  while (queue.length > 0) {
    const current = queue.shift();
    if (!current) continue;

    const currentCoord = grid[current.y][current.x];

    // Если нашли сокровище, возвращаем его
    if (currentCoord instanceof Searchable) {
      searchables.push(currentCoord);
    }

    // Обходим всех соседей
    for (const direction of directions) {
      const nextX = current.x + direction.x;
      const nextY = current.y + direction.y;

      // Проверяем, что новая точка находится в пределах карты и не посещена
      if (
        nextX >= 0 &&
        nextX < cols &&
        nextY >= 0 &&
        nextY < rows &&
        !visited[nextY][nextX]
      ) {
        const nextCoord = grid[nextY][nextX];

        // if (!(nextCoord instanceof Obstacle) && !(nextCoord instanceof Enemy)) {
        visited[nextY][nextX] = true;
        queue.push(nextCoord);
        // }
      }
    }
  }

  return searchables;
}
