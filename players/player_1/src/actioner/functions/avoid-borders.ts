import { Desert, Transport, Vector } from "../../api/get-map-types";
import { vectorFromTo, vectorLength } from "./point-acceleration";

export function avoidBordersAcces(transport: Transport, map: Desert): Vector {
  const w = map.mapSize.x;
  const h = map.mapSize.y;

  // Расстояния до углов карты
  const distances = [
    vectorLength(vectorFromTo(transport, { x: 0, y: 0 })),
    vectorLength(vectorFromTo(transport, { x: w, y: 0 })),
    vectorLength(vectorFromTo(transport, { x: 0, y: h })),
    vectorLength(vectorFromTo(transport, { x: w, y: h })),
  ];

  // Находим минимальное расстояние до углов
  const minDistance = Math.min(...distances);

  // Проверяем, если транспорт слишком близко к границе (< 100)
  if (minDistance < 300) {
    return vectorFromTo(transport, {
      x: w / 2,
      y: h / 2,
    })
  }

  // Если транспорт находится на безопасном расстоянии, возвращаем нулевой вектор
  return { x: 0, y: 0 };
}
