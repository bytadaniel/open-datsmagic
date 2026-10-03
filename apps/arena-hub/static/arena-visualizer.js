(() => {
  const canvas = document.querySelector('#arena-canvas');
  const context = canvas.getContext('2d', { alpha: false });
  const wrap = document.querySelector('.canvas-wrap');
  const tokenInput = document.querySelector('#viz-token');
  const connectForm = document.querySelector('#connect-form');
  const select = document.querySelector('#carpet-select');
  const followButton = document.querySelector('#follow-toggle');
  const manualButton = document.querySelector('#manual-toggle');
  const message = document.querySelector('#viz-message');
  const badge = document.querySelector('#connection-badge');
  const touchStick = document.querySelector('#touch-stick');
  const stickBase = touchStick.querySelector('.stick-base');
  const shell = document.querySelector('.visualizer-shell');
  const fullscreenButton = document.querySelector('#fullscreen-toggle');
  const observerButton = document.querySelector('#observer-connect');

  const state = {
    token: '', observer: false, current: null, previous: null, receivedAt: 0, previousAt: 0,
    selectedId: '', follow: false, manual: false, leaseId: '', releaseLeaseId: '',
    camera: { x: 0, y: 0, zoom: 1, initialized: false },
    width: 0, height: 0, dpr: 1, frameAt: performance.now(), frameCount: 0,
    fps: 0, pointer: null, pointerInside: false, stickVector: null,
    pointers: new Map(), gestureDistance: 0, dragging: false, dragStart: null,
    polling: false,
  };

  const ownCarpets = snapshot => snapshot?.transports || [];
  const enemyCarpets = snapshot => (snapshot?.enemies || []).map((item, index) => ({ ...item, id: `enemy_${index}`, own: false }));
  const allCarpets = snapshot => [
    ...ownCarpets(snapshot).map(item => ({ ...item, own: true })), ...enemyCarpets(snapshot),
  ];
  const byId = (snapshot, id) => allCarpets(snapshot).find(item => item.id === id);
  const alive = item => item?.status === 'alive';
  const finite = value => Number.isFinite(Number(value)) ? Number(value) : 0;
  const fmt = value => finite(value).toFixed(1);
  const newLeaseId = () => globalThis.crypto?.randomUUID?.()
    || `web-${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}-${Math.random().toString(36).slice(2)}`;

  function setConnection(kind, text) {
    badge.className = `connection-badge ${kind || ''}`;
    badge.textContent = text;
  }

  function resize() {
    const bounds = wrap.getBoundingClientRect();
    state.dpr = Math.min(window.devicePixelRatio || 1, 2);
    state.width = Math.max(1, bounds.width);
    state.height = Math.max(1, bounds.height);
    canvas.width = Math.round(state.width * state.dpr);
    canvas.height = Math.round(state.height * state.dpr);
  }
  new ResizeObserver(resize).observe(wrap);
  window.addEventListener('resize', resize, { passive: true });
  resize();

  function mapSize(snapshot = state.current) {
    return { x: Math.max(1, finite(snapshot?.mapSize?.x) || 10000), y: Math.max(1, finite(snapshot?.mapSize?.y) || 10000) };
  }
  function fitScale(snapshot = state.current) {
    const map = mapSize(snapshot);
    return Math.max(.001, Math.min(state.width / map.x, state.height / map.y) * .94);
  }
  function scale() { return fitScale() * state.camera.zoom; }
  function worldToScreen(position) {
    const factor = scale();
    return { x: state.width / 2 + (finite(position?.x) - state.camera.x) * factor,
      y: state.height / 2 - (finite(position?.y) - state.camera.y) * factor };
  }
  function screenToWorld(position) {
    const factor = scale();
    return { x: state.camera.x + (position.x - state.width / 2) / factor,
      y: state.camera.y - (position.y - state.height / 2) / factor };
  }
  function moveCamera(dx, dy) {
    const factor = scale();
    state.camera.x += dx / factor;
    state.camera.y -= dy / factor;
  }
  function zoomAt(factor, anchor) {
    if (!state.current) return;
    const before = screenToWorld(anchor);
    state.camera.zoom = Math.max(.18, Math.min(24, state.camera.zoom * factor));
    const after = screenToWorld(anchor);
    state.camera.x += before.x - after.x;
    state.camera.y += before.y - after.y;
  }

  function interpolateVector(a, b, alpha) {
    return { x: finite(a?.x) + (finite(b?.x) - finite(a?.x)) * alpha,
      y: finite(a?.y) + (finite(b?.y) - finite(a?.y)) * alpha };
  }
  function interpolatedState(now) {
    const current = state.current;
    if (!current) return null;
    const previous = state.previous;
    const interval = Math.max(100, state.receivedAt - state.previousAt || 200);
    const renderAt = now - interval;
    const alpha = previous ? Math.max(0, Math.min(1, (renderAt - state.previousAt) / interval)) : 1;
    const oldById = new Map(allCarpets(previous).map(item => [item.id, item]));
    const blend = item => {
      const old = oldById.get(item.id);
      if (!old) return item;
      return { ...item,
        x: finite(old.x) + (finite(item.x) - finite(old.x)) * alpha,
        y: finite(old.y) + (finite(item.y) - finite(old.y)) * alpha,
        velocity: interpolateVector(old.velocity, item.velocity, alpha),
        selfAcceleration: interpolateVector(old.selfAcceleration, item.selfAcceleration, alpha),
        anomalyAcceleration: interpolateVector(old.anomalyAcceleration, item.anomalyAcceleration, alpha),
      };
    };
    const oldAnomalies = new Map((previous?.anomalies || []).map(item => [item.id, item]));
    const anomalies = (current.anomalies || []).map(item => {
      const old = oldAnomalies.get(item.id);
      return old ? { ...item,
        x: finite(old.x) + (finite(item.x) - finite(old.x)) * alpha,
        y: finite(old.y) + (finite(item.y) - finite(old.y)) * alpha } : item;
    });
    return { ...current, transports: ownCarpets(current).map(blend),
      enemies: enemyCarpets(current).map(blend), anomalies };
  }

  function drawArrow(ctx, origin, vector, color, maximum, width = 2) {
    const vx = finite(vector?.x), vy = finite(vector?.y);
    const magnitude = Math.hypot(vx, vy);
    if (magnitude < 0.01) return;
    const length = 18 + 48 * Math.min(1, magnitude / Math.max(1, maximum));
    const end = { x: origin.x + vx / magnitude * length, y: origin.y - vy / magnitude * length };
    const angle = Math.atan2(end.y - origin.y, end.x - origin.x);
    ctx.strokeStyle = color; ctx.fillStyle = color; ctx.lineWidth = width;
    ctx.beginPath(); ctx.moveTo(origin.x, origin.y); ctx.lineTo(end.x, end.y); ctx.stroke();
    ctx.beginPath(); ctx.moveTo(end.x, end.y);
    ctx.lineTo(end.x - 8 * Math.cos(angle - .42), end.y - 8 * Math.sin(angle - .42));
    ctx.lineTo(end.x - 8 * Math.cos(angle + .42), end.y - 8 * Math.sin(angle + .42));
    ctx.closePath(); ctx.fill();
  }

  function drawCoins(ctx, snapshot) {
    const coins = snapshot?.bounties || [], factor = scale(), highlights = [];
    // Render directly from world units: no whole-map bitmap to stretch on zoom.
    // Batch same-color circles into two Canvas paths to keep high coin counts cheap.
    ctx.fillStyle = '#d89a19'; ctx.beginPath();
    for (const coin of coins) {
      const p = worldToScreen(coin);
      const worldRadius = finite(coin.radius) || finite(snapshot?.transportRadius) || 4;
      const radius = Math.max(.35, worldRadius * factor);
      if (p.x < -radius || p.x > state.width + radius || p.y < -radius || p.y > state.height + radius) continue;
      ctx.moveTo(p.x + radius, p.y); ctx.arc(p.x, p.y, radius, 0, Math.PI * 2);
      if (radius >= 2.5) highlights.push([p.x - radius * .25, p.y - radius * .25, radius * .28]);
    }
    ctx.fill();
    if (highlights.length) {
      ctx.fillStyle = '#ffe09a'; ctx.beginPath();
      for (const [x, y, radius] of highlights) { ctx.moveTo(x + radius, y); ctx.arc(x, y, radius, 0, Math.PI * 2); }
      ctx.fill();
    }
  }

  function drawAnomalies(ctx, anomalies) {
    for (const anomaly of anomalies || []) {
      const center = worldToScreen(anomaly);
      const attracting = finite(anomaly.strength) > 0;
      const color = attracting ? '#e34850' : '#287bd4';
      const fieldRadius = Math.max(3, finite(anomaly.effectiveRadius) * scale());
      const coreRadius = Math.max(3.5, finite(anomaly.radius) * scale());
      ctx.fillStyle = attracting ? 'rgba(224,55,66,.105)' : 'rgba(35,112,210,.11)';
      ctx.beginPath(); ctx.arc(center.x, center.y, fieldRadius, 0, Math.PI * 2); ctx.fill();
      ctx.fillStyle = color; ctx.beginPath(); ctx.arc(center.x, center.y, coreRadius, 0, Math.PI * 2); ctx.fill();
      ctx.fillStyle = '#fff'; ctx.font = '600 11px system-ui'; ctx.textAlign = 'center'; ctx.textBaseline = 'middle';
      ctx.fillText(`${attracting ? '+' : '−'}${Math.round(Math.abs(finite(anomaly.strength)))}`, center.x, center.y);
    }
  }

  function drawCarpets(ctx, snapshot) {
    const carpets = allCarpets(snapshot);
    const radius = Math.max(4, Math.min(9, finite(snapshot.transportRadius) * scale()));
    for (const carpet of carpets) {
      const center = worldToScreen(carpet);
      const isSelected = carpet.id === state.selectedId;
      const color = carpet.own ? '#bc8a13' : '#30875d';
      ctx.fillStyle = carpet.own ? 'rgba(245,193,67,.22)' : 'rgba(70,190,125,.20)';
      ctx.beginPath(); ctx.arc(center.x, center.y, isSelected ? radius + 8 : radius + 5, 0, Math.PI * 2); ctx.fill();
      ctx.globalAlpha = alive(carpet) ? 1 : .34;
      ctx.fillStyle = carpet.own ? '#f2c64d' : '#43b978';
      ctx.beginPath(); ctx.arc(center.x, center.y, radius, 0, Math.PI * 2); ctx.fill();
      if (isSelected) { ctx.strokeStyle = '#122536'; ctx.lineWidth = 2; ctx.beginPath(); ctx.arc(center.x, center.y, radius + 2, 0, Math.PI * 2); ctx.stroke(); }
      ctx.globalAlpha = 1;
      if (isSelected) {
        drawArrow(ctx, center, carpet.velocity, '#2276d2', finite(snapshot.maxSpeed), 2.5);
        drawArrow(ctx, center, carpet.selfAcceleration, '#d49300', finite(snapshot.maxAccel), 2.5);
        drawArrow(ctx, center, carpet.anomalyAcceleration, '#9a49b6', finite(snapshot.maxAccel), 2.5);
      }
      ctx.fillStyle = '#132638'; ctx.font = '600 10px system-ui'; ctx.textAlign = 'center'; ctx.textBaseline = 'middle';
      ctx.fillText(carpet.own ? carpet.id.slice(-2) : '×', center.x, center.y);
    }
  }

  function accelerationFor(carpet, snapshot) {
    if (state.stickVector) return state.stickVector;
    if (!state.pointer || !state.pointerInside) return null;
    const center = worldToScreen(carpet);
    let x = state.pointer.x - center.x;
    let y = center.y - state.pointer.y;
    const magnitude = Math.hypot(x, y), maximum = Math.max(0, finite(snapshot.maxAccel));
    if (magnitude > maximum && magnitude > 0) { x *= maximum / magnitude; y *= maximum / magnitude; }
    return { x, y };
  }

  function drawManual(ctx, snapshot) {
    if (!state.manual || !state.selectedId) return;
    const carpet = byId(snapshot, state.selectedId);
    if (!carpet?.own || !alive(carpet)) return;
    const center = worldToScreen(carpet);
    if (state.pointer && state.pointerInside && !state.stickVector) {
      ctx.strokeStyle = '#687782'; ctx.lineWidth = 1.5; ctx.setLineDash([6, 5]);
      ctx.beginPath(); ctx.moveTo(center.x, center.y); ctx.lineTo(state.pointer.x, state.pointer.y); ctx.stroke(); ctx.setLineDash([]);
    }
    const acceleration = accelerationFor(carpet, snapshot);
    if (acceleration) drawArrow(ctx, center, acceleration, '#ec9b10', finite(snapshot.maxAccel), 3);
  }

  function drawMap(ctx, snapshot) {
    ctx.fillStyle = '#e4e9eb'; ctx.fillRect(0, 0, state.width, state.height);
    const size = mapSize(snapshot), topLeft = worldToScreen({ x: 0, y: size.y });
    const bottomRight = worldToScreen(size);
    ctx.strokeStyle = '#73828b'; ctx.lineWidth = 1.5;
    ctx.strokeRect(topLeft.x, topLeft.y, bottomRight.x - topLeft.x, bottomRight.y - topLeft.y);
    drawCoins(ctx, snapshot);
    drawAnomalies(ctx, snapshot.anomalies);
    drawCarpets(ctx, snapshot);
    drawManual(ctx, snapshot);
  }

  function updateSelectedPanel(snapshot) {
    const item = byId(snapshot, state.selectedId);
    document.querySelector('#selected-title').textContent = item ? `${item.own ? 'Твой ковер' : 'Ковер соперника'} · ${item.id}` : 'Ничего не выбрано';
    const stats = document.querySelector('#selected-stats');
    if (!item) { stats.textContent = 'Выбери ковер на карте или в списке.'; return; }
    const speed = Math.hypot(finite(item.velocity?.x), finite(item.velocity?.y));
    const acceleration = Math.hypot(finite(item.selfAcceleration?.x), finite(item.selfAcceleration?.y));
    const force = Math.hypot(finite(item.anomalyAcceleration?.x), finite(item.anomalyAcceleration?.y));
    stats.innerHTML = [
      ['Состояние', item.status || '—'], ['Позиция', `${fmt(item.x)}, ${fmt(item.y)}`],
      ['Скорость V', `${fmt(item.velocity?.x)}, ${fmt(item.velocity?.y)} · ${fmt(speed)}`],
      ['Ускорение A', item.selfAcceleration ? `${fmt(item.selfAcceleration.x)}, ${fmt(item.selfAcceleration.y)} · ${fmt(acceleration)}` : 'не передаётся для чужих ковров'],
      ['Силы аномалий W', item.anomalyAcceleration ? `${fmt(item.anomalyAcceleration.x)}, ${fmt(item.anomalyAcceleration.y)} · ${fmt(force)}` : 'не передаются для чужих ковров'],
    ].map(([label, value]) => `<div class="stat-row"><span>${label}</span><b>${value}</b></div>`).join('');
  }

  function selectCarpet(id) {
    if (state.manual && id !== state.selectedId) disableManual(true);
    state.selectedId = id;
    select.value = id;
    const item = byId(state.current, id);
    const ownAlive = item?.own && alive(item);
    manualButton.disabled = !ownAlive;
    if (!ownAlive && state.manual) disableManual(true);
    updateSelectedPanel(interpolatedState(performance.now()) || state.current);
  }

  function rebuildSelect() {
    if (!state.current) return;
    const items = allCarpets(state.current);
    const existing = [...select.options].map(option => option.value);
    const values = items.map(item => item.id);
    if (values.length === existing.length && values.every((value, i) => value === existing[i])) return;
    select.replaceChildren(...items.map((item, index) => new Option(
      `${item.own ? 'Твой' : 'Враг'} · ${item.id || `Враг ${index + 1}`} · ${item.status || 'alive'}`, item.id)));
    select.disabled = !items.length;
    followButton.disabled = !items.length;
    manualButton.disabled = true;
    if (items.some(item => item.id === state.selectedId)) select.value = state.selectedId;
    else if (items.length) selectCarpet(items[0].id);
    else { state.selectedId = ''; document.querySelector('#selected-title').textContent = 'Ковров нет'; }
  }

  function updateSnapshot(snapshot) {
    const now = performance.now();
    if (state.current) { state.previous = state.current; state.previousAt = state.receivedAt; }
    else { state.previous = null; state.previousAt = now - 200; }
    state.current = snapshot;
    state.receivedAt = now;
    if (!state.camera.initialized) {
      state.camera.x = mapSize(snapshot).x / 2; state.camera.y = mapSize(snapshot).y / 2;
      state.camera.initialized = true;
    }
    const title = snapshot.name || 'Команда';
    document.title = `${title} · Арена StadMagic`;
    document.querySelector('#world-label').textContent = `${snapshot.mapSize?.x} × ${snapshot.mapSize?.y} · ${snapshot.name || 'Команда'} · ${snapshot.points ?? 0} золота`;
    rebuildSelect();
    const selected = byId(snapshot, state.selectedId);
    const canDrive = Boolean(selected?.own && alive(selected));
    manualButton.disabled = !canDrive;
    if (state.manual && !canDrive) disableManual(true);
    updateSelectedPanel(interpolatedState(now));
  }

  async function requestSnapshot() {
    if ((!state.token && !state.observer) || state.polling) return;
    state.polling = true;
    try {
      const body = { transports: [] };
      if (state.manual && state.selectedId) {
        const selected = byId(state.current, state.selectedId);
        if (selected?.own && alive(selected)) {
          if (!state.leaseId) state.leaseId = newLeaseId();
          const acceleration = accelerationFor(selected, state.current);
          if (acceleration) body.transports = [{ id: selected.id, acceleration }];
          body.manualCarpetId = selected.id;
          body.leaseId = state.leaseId;
        } else disableManual(true);
      }
      if (state.releaseLeaseId) body.releaseLeaseId = state.releaseLeaseId;
      const headers = { 'Content-Type': 'application/json' };
      if (state.token) headers['X-Auth-Token'] = state.token;
      const response = await fetch('/api/visualizer/move', { method: 'POST', headers, body: JSON.stringify(body) });
      const snapshot = await response.json();
      if (!response.ok) throw Object.assign(new Error(snapshot.error || `Ошибка API ${response.status}`), { status: response.status });
      updateSnapshot(snapshot);
      setConnection('connected', `На связи · ${new Date().toLocaleTimeString()}`);
      message.textContent = '';
    } catch (error) {
      setConnection('error', error.status === 401 ? 'Токен не принят' : 'Нет связи');
      message.textContent = error.message;
      message.className = 'viz-message error';
      if (error.status === 409 && state.manual) disableManual(false);
      if (error.status === 401) { state.token = ''; state.observer = false; state.manual = false; state.leaseId = ''; }
    } finally {
      state.releaseLeaseId = '';
      state.polling = false;
      if (state.token || state.observer) setTimeout(requestSnapshot, 200);
    }
  }

  function disableManual(release) {
    state.manual = false;
    touchStick.classList.remove('active');
    state.stickVector = null;
    if (release && state.leaseId) state.releaseLeaseId = state.leaseId;
    state.leaseId = '';
    manualButton.textContent = 'Ручное управление: выкл.';
    manualButton.classList.remove('manual-on');
  }
  function enableManual() {
    const item = byId(state.current, state.selectedId);
    if (!item?.own || !alive(item)) return;
    state.manual = true;
    state.leaseId = newLeaseId();
    state.releaseLeaseId = '';
    manualButton.textContent = 'Ручное управление: вкл.';
    manualButton.classList.add('manual-on');
    requestSnapshot();
  }

  function connect(token, observer) {
    const previousToken = state.token, previousLease = state.leaseId || state.releaseLeaseId;
    disableManual(false);
    state.releaseLeaseId = '';
    if (previousToken && previousLease) {
      fetch('/api/visualizer/move', { method: 'POST', keepalive: true,
        headers: { 'Content-Type': 'application/json', 'X-Auth-Token': previousToken },
        body: JSON.stringify({ transports: [], releaseLeaseId: previousLease }) }).catch(() => {});
    }
    state.token = token;
    state.observer = observer;
    state.previous = state.current = null;
    state.camera.initialized = false;
    state.selectedId = '';
    state.follow = false;
    followButton.textContent = '◎ Следить'; followButton.classList.remove('follow-on');
    message.textContent = observer ? 'Подключаюсь к публичному просмотру…' : 'Подключаюсь к твоему флоту…'; message.className = 'viz-message';
    setConnection('', 'Подключение…');
    requestSnapshot();
  }
  connectForm.addEventListener('submit', event => {
    event.preventDefault();
    const token = tokenInput.value.trim();
    if (!token) { message.textContent = 'Введи токен команды или нажми «Наблюдать».'; return; }
    connect(token, false);
  });
  observerButton.addEventListener('click', () => connect('', true));
  select.addEventListener('change', () => selectCarpet(select.value));
  followButton.addEventListener('click', () => {
    state.follow = !state.follow;
    followButton.textContent = state.follow ? '◎ Слежение: вкл.' : '◎ Следить';
    followButton.classList.toggle('follow-on', state.follow);
  });
  manualButton.addEventListener('click', () => state.manual ? disableManual(true) : enableManual());
  document.querySelector('#zoom-in').addEventListener('click', () => zoomAt(1.25, { x: state.width / 2, y: state.height / 2 }));
  document.querySelector('#zoom-out').addEventListener('click', () => zoomAt(.8, { x: state.width / 2, y: state.height / 2 }));
  document.querySelector('#camera-reset').addEventListener('click', () => {
    state.camera.zoom = 1;
    if (state.follow && state.current) {
      const item = byId(state.current, state.selectedId);
      if (item) { state.camera.x = finite(item.x); state.camera.y = finite(item.y); }
    } else if (state.current) { const map = mapSize(); state.camera.x = map.x / 2; state.camera.y = map.y / 2; }
  });

  function fullscreenActive() {
    return document.fullscreenElement === shell || document.webkitFullscreenElement === shell
      || shell.classList.contains('fullscreen-fallback');
  }
  function syncFullscreenButton() {
    fullscreenButton.textContent = fullscreenActive() ? '⛶ Выйти из полного экрана' : '⛶ На весь экран';
  }
  fullscreenButton.addEventListener('click', async () => {
    try {
      if (fullscreenActive()) {
        if (document.exitFullscreen) await document.exitFullscreen();
        else if (document.webkitExitFullscreen) document.webkitExitFullscreen();
        shell.classList.remove('fullscreen-fallback');
      } else if (shell.requestFullscreen) await shell.requestFullscreen();
      else if (shell.webkitRequestFullscreen) shell.webkitRequestFullscreen();
      else shell.classList.add('fullscreen-fallback');
    } catch (_) { shell.classList.toggle('fullscreen-fallback'); }
    syncFullscreenButton();
    setTimeout(resize, 100);
  });
  document.addEventListener('fullscreenchange', () => { syncFullscreenButton(); setTimeout(resize, 100); });
  document.addEventListener('webkitfullscreenchange', () => { syncFullscreenButton(); setTimeout(resize, 100); });
  window.addEventListener('keydown', event => {
    if (event.key === 'Escape' && shell.classList.contains('fullscreen-fallback')) {
      shell.classList.remove('fullscreen-fallback'); syncFullscreenButton(); setTimeout(resize, 100);
    }
  });

  canvas.addEventListener('wheel', event => {
    event.preventDefault();
    const rect = canvas.getBoundingClientRect();
    zoomAt(event.deltaY < 0 ? 1.12 : 1 / 1.12, { x: event.clientX - rect.left, y: event.clientY - rect.top });
  }, { passive: false });
  canvas.addEventListener('pointermove', event => {
    const rect = canvas.getBoundingClientRect();
    const point = { x: event.clientX - rect.left, y: event.clientY - rect.top };
    if (event.pointerType !== 'touch') { state.pointer = point; state.pointerInside = true; }
    if (state.pointers.has(event.pointerId)) {
      const previous = state.pointers.get(event.pointerId);
      state.pointers.set(event.pointerId, point);
      if (state.pointers.size === 1 && state.dragging && !state.manual) {
        moveCamera(previous.x - point.x, previous.y - point.y);
      } else if (state.pointers.size >= 2) {
        const points = [...state.pointers.values()];
        const distance = Math.hypot(points[0].x - points[1].x, points[0].y - points[1].y);
        if (state.gestureDistance > 0 && distance > 0) zoomAt(distance / state.gestureDistance,
          { x: (points[0].x + points[1].x) / 2, y: (points[0].y + points[1].y) / 2 });
        state.gestureDistance = distance;
      }
    }
  });
  canvas.addEventListener('pointerdown', event => {
    const rect = canvas.getBoundingClientRect(), point = { x: event.clientX - rect.left, y: event.clientY - rect.top };
    canvas.setPointerCapture(event.pointerId);
    state.pointers.set(event.pointerId, point);
    state.dragStart = point; state.dragging = true;
    if (state.pointers.size === 2) {
      const points = [...state.pointers.values()];
      state.gestureDistance = Math.hypot(points[0].x - points[1].x, points[0].y - points[1].y);
    }
  });
  canvas.addEventListener('pointerup', event => {
    const rect = canvas.getBoundingClientRect(), point = { x: event.clientX - rect.left, y: event.clientY - rect.top };
    const start = state.dragStart;
    state.pointers.delete(event.pointerId);
    if (state.pointers.size < 2) state.gestureDistance = 0;
    if (start && Math.hypot(point.x - start.x, point.y - start.y) < 8) {
      const hits = allCarpets(interpolatedState(performance.now()) || state.current).map(item => ({ item, screen: worldToScreen(item) }))
        .map(hit => ({ ...hit, distance: Math.hypot(hit.screen.x - point.x, hit.screen.y - point.y) }))
        .filter(hit => hit.distance < 18).sort((a, b) => a.distance - b.distance);
      if (hits[0]) selectCarpet(hits[0].item.id);
    }
    if (!state.pointers.size) { state.dragging = false; state.dragStart = null; }
  });
  canvas.addEventListener('pointercancel', event => { state.pointers.delete(event.pointerId); state.dragging = false; state.gestureDistance = 0; });
  canvas.addEventListener('pointerleave', event => { if (event.pointerType !== 'touch') state.pointerInside = false; });

  let stickPointer = null;
  touchStick.addEventListener('pointerdown', event => {
    if (!state.manual) return;
    event.preventDefault(); stickPointer = event.pointerId; touchStick.setPointerCapture(event.pointerId); touchStick.classList.add('active');
    updateStick(event);
  });
  touchStick.addEventListener('pointermove', event => { if (event.pointerId === stickPointer) { event.preventDefault(); updateStick(event); } });
  function updateStick(event) {
    const rect = stickBase.getBoundingClientRect(), dx = event.clientX - (rect.left + rect.width / 2), dy = event.clientY - (rect.top + rect.height / 2);
    const radius = rect.width * .36, length = Math.min(radius, Math.hypot(dx, dy));
    const nx = Math.hypot(dx, dy) ? dx / Math.hypot(dx, dy) : 0, ny = Math.hypot(dx, dy) ? dy / Math.hypot(dx, dy) : 0;
    stickBase.style.setProperty('--stick-x', `${nx * length}px`); stickBase.style.setProperty('--stick-y', `${ny * length}px`);
    const maxAccel = finite(state.current?.maxAccel);
    state.stickVector = { x: nx * maxAccel * length / radius, y: -ny * maxAccel * length / radius };
  }
  for (const eventName of ['pointerup', 'pointercancel', 'lostpointercapture']) touchStick.addEventListener(eventName, event => {
    if (event.pointerId !== stickPointer) return;
    stickPointer = null; touchStick.classList.remove('active'); state.stickVector = { x: 0, y: 0 };
    stickBase.style.setProperty('--stick-x', '0px'); stickBase.style.setProperty('--stick-y', '0px');
  });

  window.addEventListener('keydown', event => {
    if (event.target instanceof HTMLInputElement || event.target instanceof HTMLSelectElement || event.target instanceof HTMLTextAreaElement) return;
    const step = 60;
    if (event.key === 'ArrowLeft') moveCamera(-step, 0);
    else if (event.key === 'ArrowRight') moveCamera(step, 0);
    else if (event.key === 'ArrowUp') moveCamera(0, -step);
    else if (event.key === 'ArrowDown') moveCamera(0, step);
    else if (event.key === '+' || event.key === '=') zoomAt(1.15, { x: state.width / 2, y: state.height / 2 });
    else if (event.key === '-') zoomAt(.87, { x: state.width / 2, y: state.height / 2 });
    else return;
    event.preventDefault();
  });

  function draw(now) {
    const snapshot = interpolatedState(now);
    if (state.follow && snapshot) {
      const item = byId(snapshot, state.selectedId);
      if (item) { state.camera.x = finite(item.x); state.camera.y = finite(item.y); }
    }
    context.setTransform(state.dpr, 0, 0, state.dpr, 0, 0);
    if (snapshot) drawMap(context, snapshot);
    else { context.fillStyle = '#e4e9eb'; context.fillRect(0, 0, state.width, state.height); }
    state.frameCount++;
    if (now - state.frameAt >= 600) {
      state.fps = Math.round(state.frameCount * 1000 / (now - state.frameAt));
      document.querySelector('#fps-label').textContent = `${state.fps} FPS`;
      state.frameAt = now; state.frameCount = 0;
    }
    requestAnimationFrame(draw);
  }
  requestAnimationFrame(draw);
  window.addEventListener('pagehide', () => {
    if (state.token && state.leaseId) {
      fetch('/api/visualizer/move', { method: 'POST', keepalive: true,
        headers: { 'Content-Type': 'application/json', 'X-Auth-Token': state.token },
        body: JSON.stringify({ transports: [], releaseLeaseId: state.leaseId }) }).catch(() => {});
    }
  });
})();
