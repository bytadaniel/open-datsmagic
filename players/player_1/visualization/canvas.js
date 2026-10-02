// Получение canvas элемента
const canvas = document.getElementById('mapCanvas');
const ctx = canvas.getContext('2d');

let sizeRatio = 0.5; // Изначальное значение для клетки

let carpetRadius = 0;
let attackRadius = 0;


const tileColor = {
  player: 'purple',
  treasure: 'green',
  obstacle: 'gray',
  enemy: 'red',
}

// Функция для динамической подстройки размера canvas
function adjustCanvasSize(width, height) {
  // Устанавливаем размер canvas на основе размеров карты
  canvas.width = width * sizeRatio;
  canvas.height = height * sizeRatio;
}

function drawAgents(transports) {
  transports.forEach(agent => {
    const { x, y, health } = agent;

    ctx.beginPath();
    ctx.arc(
      x * sizeRatio,
      y * sizeRatio,
      attackRadius * sizeRatio,
      0,
      2 * Math.PI,
      false
    );
    ctx.fillStyle = `rgba(255, 255, 0, 0.3)`;  // Полупрозрачный круг
    ctx.fill();

    ctx.beginPath();
    ctx.arc(
      x * sizeRatio,
      y * sizeRatio,
      carpetRadius * sizeRatio,
      0,
      2 * Math.PI,
      false
    );
    ctx.fillStyle = agent.shieldLeftMs > 0 ? 'aqua' : 'green';
    ctx.fill();

    ctx.font = "10px serif";
    ctx.fillText(
      String(health),
      x * sizeRatio,
      y * sizeRatio
    );

    drawAllVectorsForTransport(agent)
  })
}

function drawVector(
  start,
  end,
  color = 'black'
) {
  const arrowLength = 10; // Длина стрелки
  const arrowWidth = 5;   // Ширина стрелки

  // Начало рисования вектора
  ctx.beginPath();
  ctx.moveTo(start.x, start.y);
  ctx.lineTo(end.x, end.y);
  ctx.strokeStyle = color;
  ctx.lineWidth = 2;
  ctx.stroke();

  // Рассчитываем направление для стрелки
  const dx = end.x - start.x;
  const dy = end.y - start.y;
  const angle = Math.atan2(dy, dx);

  // Рисуем стрелку на конце вектора
  ctx.beginPath();
  ctx.moveTo(end.x, end.y);
  ctx.lineTo(
    end.x - arrowLength * Math.cos(angle - Math.PI / 6),
    end.y - arrowLength * Math.sin(angle - Math.PI / 6)
  );
  ctx.lineTo(
    end.x - arrowLength * Math.cos(angle + Math.PI / 6),
    end.y - arrowLength * Math.sin(angle + Math.PI / 6)
  );
  ctx.lineTo(end.x, end.y);
  ctx.closePath();
  ctx.fillStyle = color;
  ctx.fill();
}


function drawAllVectorsForTransport(transport) {
  const colorVelocity = 'blue';    // Цвет для вектора скорости
  const colorSelfAccel = 'green';  // Цвет для вектора собственного ускорения
  const colorAnomalyAccel = 'red'; // Цвет для вектора аномального ускорения

  const start = { x: transport.x * sizeRatio, y: transport.y * sizeRatio };

  // Вектор скорости ковра
  const velocityEnd = {
    x: start.x + transport.velocity.x * sizeRatio * 10,
    y: start.y + transport.velocity.y * sizeRatio * 10
  };
  drawVector(start, velocityEnd, colorVelocity);

  // Вектор собственного ускорения
  const selfAccelEnd = {
    x: start.x + transport.selfAcceleration.x * sizeRatio * 10, // Умножаем на 100 для визуализации слабого ускорения
    y: start.y + transport.selfAcceleration.y * sizeRatio * 10
  };
  drawVector(start, selfAccelEnd, colorSelfAccel);

  // Вектор аномального ускорения
  const anomalyAccelEnd = {
    x: start.x + transport.anomalyAcceleration.x * sizeRatio * 10,
    y: start.y + transport.anomalyAcceleration.y * sizeRatio * 10
  };
  drawVector(start, anomalyAccelEnd, colorAnomalyAccel);
}


function drawEnemies(enemies) {
  enemies.forEach(enemy => {
    const { x, y } = enemy;

    ctx.beginPath();
    ctx.arc(
      x * sizeRatio,
      y * sizeRatio,
      attackRadius * sizeRatio,
      0,
      2 * Math.PI,
      false
    );
    ctx.fillStyle = `rgba(0, 255, 0, 0.3)`;  // Полупрозрачный круг
    ctx.fill();

    ctx.beginPath();
    ctx.arc(
      x * sizeRatio,
      y * sizeRatio,
      carpetRadius * sizeRatio,
      0,
      2 * Math.PI,
      false
    );
    ctx.fillStyle = 'purple'
    ctx.fill();
  })
}

function drawBounties(bounties) {
  bounties.forEach(bounty => {
    const { x, y, points } = bounty;

    ctx.beginPath();
    ctx.arc(
      x * sizeRatio,
      y * sizeRatio,
      bounty.radius * sizeRatio,
      0,
      2 * Math.PI,
      false
    );
    ctx.fillStyle = 'yellow'
    ctx.fill();

    ctx.font = "10px serif";
    ctx.fillText(
      String(points),
      x * sizeRatio,
      y * sizeRatio
    );
  })
}

function drawAnomalies(anomalies) {
  anomalies.forEach(anomaly => {
    const { x, y, radius, effectiveRadius, strength } = anomaly;

    const R = strength >= 0 ? 255 : 0;  // Красный цвет для положительной силы
    const B = strength < 0 ? 255 : 0;  // Синий цвет для отрицательной силы

    // Первый полупрозрачный круг (основной радиус)
    ctx.beginPath();
    ctx.arc(
      x * sizeRatio,
      y * sizeRatio,
      radius * sizeRatio,
      0,
      2 * Math.PI,
      false
    );
    ctx.fillStyle = `rgba(${R}, 0, ${B}, 0.2)`;  // Полупрозрачный круг
    ctx.fill();

    // Второй полупрозрачный круг (эффективный радиус)
    ctx.beginPath();
    ctx.arc(
      x * sizeRatio,
      y * sizeRatio,
      effectiveRadius * sizeRatio,
      0,
      2 * Math.PI,
      false
    );
    ctx.fillStyle = `rgba(${R}, 0, ${B}, 0.2)`;  // Полупрозрачный круг
    ctx.fill();

    // Третий кружок (центр аномалии)
    ctx.beginPath();
    ctx.arc(
      x * sizeRatio,
      y * sizeRatio,
      5,  // Размер кружка для центра
      0,
      2 * Math.PI,
      false
    );
    ctx.fillStyle = `rgba(${R}, 0, ${B}, 1)`;  // Полностью насыщенный цвет для центра
    ctx.fill();
  });
}

// Подключение к WebSocket серверу
const socket = new WebSocket('ws://localhost:8081');

socket.onopen = () => {
  console.log('Подключено к серверу');
};

// При получении данных карты
socket.onmessage = (event) => {
  const {
    mapSize: { x: width, y: height },
    anomalies,
    transports,
    enemies,
    bounties,
    transportRadius: carperRadius,
    attackRange: transportAttackRange,
  } = JSON.parse(event.data);

  carpetRadius = carperRadius;
  attackRadius = transportAttackRange;

  ctx.clearRect(0, 0, canvas.width, canvas.height); // Очищаем canvas перед отрисовкой
  adjustCanvasSize(width, height);

  ctx.fillStyle = "rgba(0,0,0,0.5)";
  ctx.fillRect(0, 0, canvas.width, canvas.height);

  drawAnomalies(anomalies);
  drawEnemies(enemies);
  drawBounties(bounties);
  drawAgents(transports);
};

socket.onclose = () => {
  console.log('Соединение с сервером закрыто');
};