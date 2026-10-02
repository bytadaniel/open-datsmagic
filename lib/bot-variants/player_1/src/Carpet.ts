import { Transport, Vector  } from "./api/get-map-types";

// Функция для расчета длины вектора
function getVectorLength(vector: Vector): number {
  return Math.sqrt(vector.x * vector.x + vector.y * vector.y);
}

// Функция для создания вектора между двумя точками
function createVector(from: Vector, to: Vector): Vector {
  return { x: to.x - from.x, y: to.y - from.y };
}

// Функция для нормализации вектора до указанной длины
function normalizeVector(vector: Vector, maxLength: number): Vector {
  const length = getVectorLength(vector);
  if (length === 0) return { x: 0, y: 0 }; // избегаем деления на ноль
  const scale = maxLength / length;
  return { x: vector.x * scale, y: vector.y * scale };
}

export class Carpet {
  constructor(private state: Transport) {}

  public updateState(state: Transport): void {
    this.state = {
      ...state,
    };
  }

  // // Пример использования: движение ковра к цели с ограничением ускорения
  // public moveToTarget(targetPosition: Vector, maxAccel: number) {

  //   let vectorToTarget = createVector({
  //     x: this.state.x,
  //     y: this.state.y,
  //   }, targetPosition);

  //   let acceleration = normalizeVector(vectorToTarget, maxAccel);

  //   let command: TransportCommand = {
  //     acceleration: acceleration,
  //     id: this.state.id,
  //   };

  //   moveTransports([command]);
  // }

  // // Пример использования: остановка ковра
  // function stopCarpet(currentVelocity: Vector, carpetId: string) {
  //   let stopAcceleration = { x: -currentVelocity.x, y: -currentVelocity.y };

  //   let command: TransportCommand = {
  //     acceleration: stopAcceleration,
  //     id: carpetId,
  //   };

  //   moveTransports([command]);
  // }
}
