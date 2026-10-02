import { Obstacle } from "../points-of-interest/Obstacle";
import { AStarNode, Coord } from "../types";

function heuristic(a: Coord, b: Coord): number {
  return Math.abs(a.x - b.x) + Math.abs(a.y - b.y);
}

function reconstructPath(node: AStarNode): Coord[] {
  const path: Coord[] = [];
  let currentNode: AStarNode | null = node;

  while (currentNode) {
    path.push(currentNode.position);
    currentNode = currentNode.parent;
  }

  return path.reverse();
}

function getNeighbors(node: AStarNode, map: Coord[][]): Coord[] {
  const { x, y } = node.position;
  const neighbors: Coord[] = [];

  const potentialNeighbors = [
    { x: x + 1, y },
    { x: x - 1, y },
    { x, y: y + 1 },
    { x, y: y - 1 },
  ];

  for (const neighbor of potentialNeighbors) {
    if (
      neighbor.x >= 0 &&
      neighbor.x < map.length &&
      neighbor.y >= 0 &&
      neighbor.y < map[0].length
    ) {
      neighbors.push(map[neighbor.x][neighbor.y]);
    }
  }

  return neighbors;
}

export function aStar(
  start: Coord,
  end: Coord,
  map: Coord[][]
): Coord[] | null {
  const openSet: AStarNode[] = [];
  const closedSet: AStarNode[] = [];

  const startNode: AStarNode = {
    position: start,
    g: 0,
    h: heuristic(start, end),
    f: heuristic(start, end),
    parent: null,
  };

  openSet.push(startNode);

  while (openSet.length > 0) {
    const currentNode = openSet.reduce((prev, curr) =>
      prev.f < curr.f ? prev : curr
    );

    // Если нашли конечную точку
    if (currentNode.position.x === end.x && currentNode.position.y === end.y) {
      return reconstructPath(currentNode);
    }

    // Перемещаем текущий узел в закрытый список
    openSet.splice(openSet.indexOf(currentNode), 1);
    closedSet.push(currentNode);

    const neighbors = getNeighbors(currentNode, map);

    for (const neighbor of neighbors) {
      // Проверяем, является ли сосед препятствием или уже находится в закрытом списке
      if (
        neighbor instanceof Obstacle ||
        closedSet.some(
          (node) =>
            node.position.x === neighbor.x && node.position.y === neighbor.y
        )
      ) {
        continue;
      }

      const tentativeG = currentNode.g + 1;

      let neighborNode = openSet.find(
        (node) =>
          node.position.x === neighbor.x && node.position.y === neighbor.y
      );

      if (!neighborNode) {
        neighborNode = {
          position: neighbor,
          g: tentativeG,
          h: heuristic(neighbor, end),
          f: tentativeG + heuristic(neighbor, end),
          parent: currentNode,
        };
        openSet.push(neighborNode);
      } else if (tentativeG < neighborNode.g) {
        neighborNode.g = tentativeG;
        neighborNode.f = neighborNode.g + neighborNode.h;
        neighborNode.parent = currentNode;
      }
    }
  }

  return null;
}
