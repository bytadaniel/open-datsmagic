(() => {
  const pageQuery = new URLSearchParams(location.search);
  const embeddedObserver = pageQuery.get('mode') === 'observer' && pageQuery.get('embed') === '1';
  const canvas = document.querySelector('#arena-canvas');
  const context = canvas.getContext('2d', { alpha: false });
  const wrap = document.querySelector('.canvas-wrap');
  const teamConnectButton = document.querySelector('#arena-team-connect');
  const select = document.querySelector('#carpet-select');
  const followButton = document.querySelector('#follow-toggle');
  const manualButton = document.querySelector('#manual-toggle');
  const message = document.querySelector('#viz-message');
  const badge = document.querySelector('#connection-badge');
  const connectionStatus = document.querySelector('#arena-connection-status');
  badge.remove();
  const touchStick = document.querySelector('#touch-stick');
  const stickBase = touchStick.querySelector('.stick-base');
  const shell = document.querySelector('.visualizer-shell');
  const fullscreenButton = document.querySelector('#fullscreen-toggle');
  const orientationButton = document.querySelector('#orientation-toggle');
  const entryTitle = document.querySelector('#arena-entry-title');
  const entryName = document.querySelector('#arena-entry-name');
  const entryDescription = document.querySelector('#arena-entry-description');
  const entryStatus = document.querySelector('#arena-entry-status');
  const entryModeBadge = document.querySelector('#arena-mode-badge');
  const entryModeCard = document.querySelector('#arena-mode-card');

  const state = {
    token: '', observer: false, current: null, previous: null, receivedAt: 0, previousAt: 0,
    selectedId: '', follow: false, autoFramePending: true, manual: false, leaseId: '',
    embeddedFollowId: '', embeddedNextFollowAt: 0, embeddedCoinFocusKey: '',
    camera: { x: 0, y: 0, zoom: 1, initialized: false },
    width: 0, height: 0, dpr: 1, frameAt: performance.now(), frameCount: 0,
    fps: 0, pointer: null, pointerInside: false, stickVector: null,
    pointers: new Map(), gestureDistance: 0, dragging: false, dragStart: null,
    realtime: null, connecting: false, reconnectDelay: 250, commandTimer: 0, leaseTimer: 0,
    arenaSeconds: null, arenaDeadline: 0, arenaActive: null, trajectoryCacheKey: '', trajectoryCache: null,
    routeSegments: [], activeRouteSegment: 0, routeStartedAt: 0,
    historyCarpetId: '', historyTrail: [], collectedMarkers: [], deathEvents: [],
    physics: { dt: 0.2, friction: 0.98 },
  };
  const worldSettingLabels = {
    arena_width: ['Ширина карты', ''], arena_height: ['Высота карты', ''], carpet_count: ['Ковров у команды', ''],
    max_velocity: ['Максимальная скорость ковра', ' ед./с'], max_acceleration: ['Максимальное ускорение ковра', ' ед./с²'],
    transport_radius: ['Радиус ковра', ' ед.'], attack_range: ['Дальность атаки', ' ед.'], attack_damage: ['Урон атаки', ''],
    attack_explosion_radius: ['Радиус взрыва', ' ед.'], attack_cooldown_ms: ['Перезарядка атаки', ' мс'],
    shield_time_ms: ['Длительность щита', ' мс'], shield_cooldown_ms: ['Перезарядка щита', ' мс'],
    friction: ['Трение', ' · доля скорости, сохраняемая за тик'], bounty_quota: ['Монет одновременно', ''],
    bounty_spawn_margin: ['Отступ монет от границы', ' ед.'],
    bounty_base_value: ['Базовая ценность монеты', ' золота'], bounty_max_value: ['Максимальная ценность монеты', ' золота'],
    anomaly_quota: ['Аномалий одновременно', ''], anomaly_speed_min: ['Скорость аномалий: минимум', ' ед./с'],
    anomaly_speed_max: ['Скорость аномалий: максимум', ' ед./с'], anomaly_core_radius_min: ['Радиус ядра: минимум', ' ед.'],
    anomaly_core_radius_max: ['Радиус ядра: максимум', ' ед.'], anomaly_effect_radius_min: ['Зона аномалий: минимум', ' ед.'],
    anomaly_effect_radius_max: ['Зона аномалий: максимум', ' ед.'], anomaly_force_min: ['Сила аномалий: минимум', ''],
    anomaly_force_max: ['Сила аномалий: максимум', ''], anomaly_force_outlier_probability: ['Вероятность аномально сильной силы', '%'],
    anomaly_force_outlier_min: ['Сила редких аномалий: минимум', ''], anomaly_force_outlier_max: ['Сила редких аномалий: максимум', ''],
    carpet_death_loss_percent: ['Потеря золота при гибели ковра', '%'], revive_timeout_sec: ['Задержка возрождения', ' с'],
    enable_respawn: ['Возрождение ковров', ''], enable_spawner: ['Появление новых аномалий', ''],
    enable_coin_spawner: ['Появление новых монет', ''],
    tick_rate_ms: ['Длительность тика', ' мс'],
  };

  const ownCarpets = snapshot => snapshot?.transports || [];
  const enemyCarpets = snapshot => (snapshot?.enemies || []).map((item, index) => {
    const owner = snapshot?.enemyTeams?.[index] || {};
    return { ...item, id: owner.carpetId || `enemy_${index}`, teamId: owner.teamId || 'unknown',
      teamName: owner.teamName || `Команда ${index + 1}`, own: false };
  });
  const allCarpets = snapshot => [
    ...ownCarpets(snapshot).map(item => ({ ...item, own: true })), ...enemyCarpets(snapshot),
  ];
  const carpetTeamKey = carpet => carpet.own ? 'own-team' : String(carpet.teamId || carpet.teamName || 'unknown');
  const byId = (snapshot, id) => allCarpets(snapshot).find(item => item.id === id);
  const alive = item => item?.status === 'alive';
  const finite = value => Number.isFinite(Number(value)) ? Number(value) : 0;
  const formatCompactGold = value => {
    let amount = Math.max(0, finite(value));
    let suffix = '';
    if (amount >= 999999.5) { amount /= 1_000_000; suffix = 'M'; }
    else if (amount >= 1000) { amount /= 1000; suffix = 'K'; }
    const formatted = suffix
      ? amount.toFixed(3).replace(/0+$/, '').replace(/\.$/, '')
      : Math.round(amount).toLocaleString('ru-RU');
    return `${formatted}${suffix} ✦`;
  };
  const carpetSlot = carpet => {
    const match = String(carpet?.id || '').match(/_(\d+)$/);
    return match ? String(Number(match[1]) + 1) : '';
  };
  const formatCompactCount = value => {
    const count = Math.max(0, Math.trunc(finite(value)));
    if (count >= 1_000_000) return `${(count / 1_000_000).toFixed(1).replace(/\.0$/, '')}M`;
    if (count >= 10_000) return `${(count / 1000).toFixed(1).replace(/\.0$/, '')}K`;
    return count.toLocaleString('ru-RU');
  };
  const deathIcon = () => {
    const icon = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
    icon.setAttribute('viewBox', '0 0 24 24');
    icon.setAttribute('aria-hidden', 'true');
    icon.classList.add('viz-death-icon');
    const path = document.createElementNS('http://www.w3.org/2000/svg', 'path');
    path.setAttribute('d', 'M12 3a8 8 0 0 0-8 8c0 3 1.5 4.7 3.4 5.8V21h9.2v-4.2C18.5 15.7 20 14 20 11a8 8 0 0 0-8-8ZM8.5 10h.01M15.5 10h.01M9 14c1.8 1.3 4.2 1.3 6 0M9 18v3m6-3v3');
    icon.append(path);
    return icon;
  };
  const ownSummary = document.querySelector('#viz-own-summary');
  const ownGold = document.querySelector('#viz-own-gold');
  const ownName = document.querySelector('#viz-own-name');
  const ownDeaths = ownSummary.querySelector('.viz-own-deaths');
  const ownDeathCount = document.querySelector('#viz-own-deaths');
  const ownRank = document.createElement('b');
  ownRank.className = 'viz-own-rank';
  ownRank.id = 'viz-own-rank';
  ownRank.textContent = '—';
  const ownMetrics = document.createElement('div');
  ownMetrics.className = 'viz-own-metrics';
  ownMetrics.append(ownGold, ownDeaths);
  ownSummary.replaceChildren(ownRank, ownName, ownMetrics);
  const MAX_CAMERA_ZOOM = 24;
  const fmt = value => finite(value).toFixed(1);
  const newLeaseId = () => globalThis.crypto?.randomUUID?.()
    || `web-${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}-${Math.random().toString(36).slice(2)}`;

  function teamColor(id) {
    let hash = 2166136261;
    for (const char of String(id || 'unknown')) hash = Math.imul(hash ^ char.charCodeAt(0), 16777619);
    const hue = (hash >>> 0) % 360;
    return { body: `hsl(${hue} 58% 48%)`, aura: `hsla(${hue} 66% 52% / .22)` };
  }

  function countdownText(seconds, status) {
    if (!Number.isFinite(seconds) || !['running', 'cooldown'].includes(status)) return status === 'starting' ? 'Скоро старт' : '—';
    const remaining = Math.max(0, Math.ceil(seconds));
    const mm = Math.floor(remaining / 60).toString().padStart(2, '0');
    const ss = (remaining % 60).toString().padStart(2, '0');
    return `${mm}:${ss}`;
  }

  function updateCountdownTone(element, seconds, status) {
    element.classList.remove('countdown-calm', 'countdown-warning', 'countdown-critical');
    const remaining = Math.max(0, Number(seconds) || 0);
    const tone = !['running', 'cooldown'].includes(status) || remaining > 300
      ? 'countdown-calm' : remaining > 60 ? 'countdown-warning' : 'countdown-critical';
    element.classList.add(tone);
  }

  function updateArenaContext(data) {
    const active = data.active || {};
    state.arenaActive = active;
    const profile = (data.worlds || []).find(world => world.id === active.world_id) || {};
    const config = profile.config || {};
    const tickRateMs = Number(config.tick_rate_ms);
    const friction = Number(config.friction);
    state.physics = {
      dt: Number.isFinite(tickRateMs) && tickRateMs > 0 ? Math.min(0.5, Math.max(0.05, tickRateMs / 1000)) : 0.2,
      friction: Number.isFinite(friction) && friction >= 0 ? friction : 0.98,
    };
    const phases = { running: 'АРЕНА ИДЁТ', starting: 'АРЕНА ЗАПУСКАЕТСЯ', stopping: 'АРЕНА ЗАВЕРШАЕТСЯ', cooldown: 'ПЕРЕРЫВ МЕЖДУ АРЕНАМИ', failed: 'ОШИБКА ЗАПУСКА', stopped: 'АРЕНА ОСТАНОВЛЕНА' };
    const phaseBadge = document.querySelector('#arena-phase');
    phaseBadge.textContent = phases[active.status] || 'ОЖИДАНИЕ АРЕНЫ';
    phaseBadge.dataset.phase = active.status || 'idle';
    document.querySelector('#arena-title').textContent = profile.name || active.world_name || 'Мир готовится';
    document.querySelector('#arena-description').textContent = profile.description || active.error || 'Описание текущего мира появится при запуске арены.';
    const worldNumber = value => Number(value).toLocaleString('ru-RU', { maximumFractionDigits: 0 });
    const displayNumber = value => Number(value).toLocaleString('ru-RU', { maximumFractionDigits: 3 });
    const worldArea = value => Number(value).toLocaleString('ru-RU', { maximumFractionDigits: 2 });
    const arenaWidth = Number(config.arena_width ?? 9000);
    const arenaHeight = Number(config.arena_height ?? 9000);
    const arenaAreaM2 = Math.max(1, arenaWidth * arenaHeight);
    const areaKm2ForDisplay = arenaAreaM2 / 1_000_000;
    const shortSide = Math.max(1, Math.min(arenaWidth, arenaHeight));
    const densityPerKm2ForDisplay = value => (Number(value || 0) / arenaAreaM2 * 1_000_000).toLocaleString('ru-RU', { maximumFractionDigits: 2 });
    const goldLabel = document.querySelector('.context-fact-gold .context-fact-label');
    const goldSymbol = document.createElement('span');
    goldSymbol.className = 'gold-symbol';
    goldSymbol.setAttribute('aria-hidden', 'true');
    goldSymbol.textContent = '✦';
    goldLabel.replaceChildren(goldSymbol, document.createTextNode('Золото на карте'));
    document.querySelector('#arena-size').textContent = config.arena_width && config.arena_height ? `${worldNumber(arenaWidth)} × ${worldNumber(arenaHeight)}` : '—';
    document.querySelector('#arena-size-note').textContent = `Площадь ${worldArea(areaKm2ForDisplay)} км²`;
    document.querySelector('#arena-bounties').textContent = config.bounty_quota == null ? '—' : worldNumber(config.bounty_quota);
    document.querySelector('#arena-bounties-note').textContent = `${densityPerKm2ForDisplay(config.bounty_quota)} монет/км²`;
    document.querySelector('#arena-anomalies').textContent = config.anomaly_quota == null ? '—' : worldNumber(config.anomaly_quota);
    document.querySelector('#arena-anomalies-note').textContent = `${densityPerKm2ForDisplay(config.anomaly_quota)} аномалий/км²`;
    const traits = document.querySelector('#arena-world-traits');
    document.querySelector('.arena-profile-label').textContent = 'ПАРАМЕТРЫ МИРА · ДИАПАЗОНЫ И ОТНОСИТЕЛЬНАЯ СИЛА';
    const speedMin = Number(config.anomaly_speed_min ?? 30);
    const speedMax = Number(config.anomaly_speed_max ?? 160);
    const effectMin = Number(config.anomaly_effect_radius_min ?? 300);
    const effectMax = Number(config.anomaly_effect_radius_max ?? 2000);
    const coreMin = Number(config.anomaly_core_radius_min ?? 20);
    const coreMax = Number(config.anomaly_core_radius_max ?? 30);
    const regularForceMin = Number(config.anomaly_force_min ?? 4);
    const regularForceMax = Number(config.anomaly_force_max ?? 22);
    const outlierForceMax = Number(config.anomaly_force_outlier_max ?? 100);
    const outliersEnabled = Number(config.anomaly_force_outlier_probability ?? .1) > 0;
    const forceMax = outliersEnabled ? Math.max(regularForceMax, outlierForceMax) : regularForceMax;
    const maxCarpetSpeed = Number(config.max_velocity ?? 110);
    const maxCarpetAcceleration = Number(config.max_acceleration ?? 40);
    const timeAcrossArena = speedMax > 0 ? shortSide / speedMax : Infinity;
    const influenceShare = effectMax / shortSide;
    const minInfluenceShare = effectMin / shortSide;
    const coreShare = effectMax > 0 ? coreMax / effectMax : 0;
    const minCoreShare = effectMin > 0 ? coreMin / effectMin : 0;
    const ordinaryForceRatio = maxCarpetAcceleration > 0 ? regularForceMax / maxCarpetAcceleration : 0;
    const minForceRatio = maxCarpetAcceleration > 0 ? regularForceMin / maxCarpetAcceleration : 0;
    const peakForceRatio = maxCarpetAcceleration > 0 ? forceMax / maxCarpetAcceleration : 0;
    const forceLevel = peakForceRatio < .5 ? 'calm' : peakForceRatio < 1 ? 'steady' : peakForceRatio < 2 ? 'high' : 'extreme';
    const hasForceOutliers = outliersEnabled && outlierForceMax > regularForceMax;
    const forceCategory = peakForceRatio < .5 ? 'Слабее разгона ковра' : peakForceRatio < 1 ? 'Обычная сила близка к разгону' : hasForceOutliers ? peakForceRatio < 2 ? 'Редкий пик сильнее разгона' : 'Редкий пик намного сильнее разгона' : peakForceRatio < 2 ? 'Сильнее разгона ковра' : 'Намного сильнее разгона ковра';
    const frictionCoefficient = Number.isFinite(friction) ? friction : .98;
    const frictionPercent = frictionCoefficient * 100;
    const outlierProbability = Number(config.anomaly_force_outlier_probability ?? .1);
    const bountyBase = Number(config.bounty_base_value ?? 25);
    const bountyMax = Number(config.bounty_max_value ?? 1000);
    const goldDensityPerKm2 = Number(config.bounty_quota || 0) / arenaAreaM2 * 1_000_000;
    const rangeText = (min, max, unit) => `${worldNumber(min)}–${worldNumber(max)} ${unit}`;
    const profileItems = [
      { label: 'Скорость аномалий', value: speedMax, unit: 'м/с', centerNote: 'макс.', range: rangeText(speedMin, speedMax, 'м/с'), minMeterValue: maxCarpetSpeed > 0 ? speedMin / maxCarpetSpeed : 0, meterValue: maxCarpetSpeed > 0 ? speedMax / maxCarpetSpeed : 0, meterMax: 6, level: timeAcrossArena <= 15 ? 'extreme' : timeAcrossArena <= 45 ? 'high' : timeAcrossArena <= 180 ? 'steady' : 'calm', category: timeAcrossArena <= 15 ? 'Очень быстрые для карты' : timeAcrossArena <= 45 ? 'Быстрые для карты' : timeAcrossArena <= 180 ? 'Умеренные для карты' : 'Медленные для карты' },
      { label: 'Радиус влияния', value: effectMax, unit: 'м', centerNote: 'макс.', range: rangeText(effectMin, effectMax, 'м'), minMeterValue: minInfluenceShare * 100, meterValue: influenceShare * 100, meterMax: 100, level: influenceShare < .1 ? 'calm' : influenceShare < .25 ? 'steady' : influenceShare < .5 ? 'high' : 'extreme', category: influenceShare < .1 ? 'Локальная зона' : influenceShare < .25 ? 'Заметная зона' : influenceShare < .5 ? 'Широкая зона' : 'Покрывает значительную часть карты' },
      { label: 'Радиус ядра', value: coreMax, unit: 'м', centerNote: 'макс.', range: rangeText(coreMin, coreMax, 'м'), minMeterValue: minCoreShare * 100, meterValue: coreShare * 100, meterMax: 100, level: coreShare < .05 ? 'calm' : coreShare < .2 ? 'steady' : coreShare < .45 ? 'high' : 'extreme', category: coreShare < .05 ? 'Мало относительно зоны' : coreShare < .2 ? 'Компактное относительно зоны' : coreShare < .45 ? 'Крупное относительно зоны' : 'Почти размером с зону' },
      { label: 'Сила воздействия', value: regularForceMax, peak: hasForceOutliers ? outlierForceMax : null, unit: 'м/с²', centerNote: 'макс.', range: rangeText(regularForceMin, regularForceMax, 'м/с²'), minMeterValue: minForceRatio, meterValue: ordinaryForceRatio, peakMeterValue: peakForceRatio, meterMax: 3, level: forceLevel, category: forceCategory },
      { label: 'Ценность золота', value: bountyMax, unit: '✦', centerNote: 'макс.', range: rangeText(bountyBase, bountyMax, '✦'), minMeterValue: bountyMax > 0 ? bountyBase / bountyMax : 0, meterValue: 1, meterMax: 1, level: bountyMax / Math.max(1, bountyBase) >= 20 ? 'high' : 'steady', category: bountyMax / Math.max(1, bountyBase) >= 20 ? `Разброс до ×${(bountyMax / bountyBase).toLocaleString('ru-RU', { maximumFractionDigits: 0 })}` : 'Ценность монет в заданном диапазоне' },
      { label: 'Плотность золота', value: goldDensityPerKm2, unit: 'монет/км²', centerNote: 'в среднем', meterValue: goldDensityPerKm2, meterMax: 100, level: goldDensityPerKm2 < 10 ? 'calm' : goldDensityPerKm2 < 30 ? 'steady' : goldDensityPerKm2 < 60 ? 'high' : 'extreme', category: goldDensityPerKm2 < 10 ? 'Золота мало на площади' : goldDensityPerKm2 < 30 ? 'Умеренная плотность' : goldDensityPerKm2 < 60 ? 'Золота много' : 'Очень плотное золото' },
      { label: 'Сохранение скорости', value: frictionPercent, unit: '%', centerNote: 'за тик', meterValue: frictionCoefficient, meterMax: 1, level: frictionCoefficient >= .95 ? 'calm' : frictionCoefficient >= .85 ? 'steady' : frictionCoefficient >= .6 ? 'high' : 'extreme', category: frictionCoefficient >= .95 ? 'Инерция почти не затухает' : frictionCoefficient >= .85 ? 'Скорость затухает медленно' : frictionCoefficient >= .6 ? 'Скорость заметно затухает' : 'Скорость быстро затухает' },
      { label: 'Шанс редкой силы', value: outlierProbability * 100, unit: '%', centerNote: 'шанс · %', meterValue: outlierProbability, meterMax: 1, level: outlierProbability >= .5 ? 'extreme' : outlierProbability >= .2 ? 'high' : outlierProbability > 0 ? 'steady' : 'calm', category: outlierProbability >= .5 ? 'Часто встречается' : outlierProbability >= .2 ? 'Встречается иногда' : outlierProbability > 0 ? 'Редкий случай' : 'Не встречается' },
    ];
    traits.replaceChildren(...(Object.keys(config).length ? profileItems : []).map(profileItem => {
      const item = document.createElement('div'); item.className = 'arena-threat-item'; item.dataset.level = profileItem.level;
      const heading = document.createElement('div'); heading.className = 'arena-threat-heading';
      const term = document.createElement('small'); term.textContent = profileItem.label;
      const category = document.createElement('b'); category.className = 'arena-threat-category'; category.textContent = profileItem.category;
      heading.append(term, category);
      const gauge = document.createElementNS('http://www.w3.org/2000/svg', 'svg'); gauge.classList.add('arena-threat-gauge'); gauge.setAttribute('viewBox', '0 0 100 100'); gauge.setAttribute('role', 'meter'); gauge.setAttribute('aria-label', profileItem.label); gauge.setAttribute('aria-valuemin', '0'); gauge.setAttribute('aria-valuemax', String(profileItem.meterMax)); gauge.setAttribute('aria-valuenow', String(Math.min(profileItem.meterValue, profileItem.meterMax))); gauge.setAttribute('aria-valuetext', profileItem.range || `${displayNumber(profileItem.value)} ${profileItem.unit}`);
      const ring = (radius, value, trackClass, progressClass) => {
        const circumference = 2 * Math.PI * radius;
        const track = document.createElementNS(gauge.namespaceURI, 'circle'); track.setAttribute('class', trackClass); track.setAttribute('cx', '50'); track.setAttribute('cy', '50'); track.setAttribute('r', String(radius));
        const progress = document.createElementNS(gauge.namespaceURI, 'circle'); progress.setAttribute('class', progressClass); progress.setAttribute('cx', '50'); progress.setAttribute('cy', '50'); progress.setAttribute('r', String(radius)); progress.setAttribute('stroke-dasharray', String(circumference)); progress.setAttribute('stroke-dashoffset', String(circumference * (1 - Math.max(0, Math.min(1, value / profileItem.meterMax))))); progress.setAttribute('transform', 'rotate(-90 50 50)');
        gauge.append(track, progress);
      };
      if (profileItem.minMeterValue != null) ring(32, profileItem.minMeterValue, 'arena-threat-range-track', 'arena-threat-range-min');
      ring(40, profileItem.meterValue, 'arena-threat-gauge-track', 'arena-threat-gauge-progress');
      if (profileItem.peak) {
        const peakAngle = Math.min(1, profileItem.peakMeterValue / profileItem.meterMax) * Math.PI * 2 - Math.PI / 2;
        const marker = document.createElementNS(gauge.namespaceURI, 'circle'); marker.setAttribute('class', 'arena-threat-peak-marker'); marker.setAttribute('cx', String(50 + 40 * Math.cos(peakAngle))); marker.setAttribute('cy', String(50 + 40 * Math.sin(peakAngle))); marker.setAttribute('r', '3.5'); marker.setAttribute('aria-label', `Редкий пик силы: ${displayNumber(profileItem.peak)} ${profileItem.unit}`); marker.setAttribute('title', `Редкий пик: ${displayNumber(profileItem.peak)} ${profileItem.unit}`); gauge.append(marker);
      }
      const value = document.createElementNS(gauge.namespaceURI, 'text'); value.setAttribute('class', 'arena-threat-gauge-value'); value.setAttribute('x', '50'); value.setAttribute('y', '49'); value.setAttribute('text-anchor', 'middle'); value.textContent = displayNumber(profileItem.value);
      const centerNote = document.createElementNS(gauge.namespaceURI, 'text'); centerNote.setAttribute('class', 'arena-threat-gauge-unit'); centerNote.setAttribute('x', '50'); centerNote.setAttribute('y', '63'); centerNote.setAttribute('text-anchor', 'middle'); centerNote.textContent = profileItem.centerNote;
      gauge.append(value, centerNote);
      item.append(heading, gauge);
      if (profileItem.range) { const range = document.createElement('small'); range.className = 'arena-threat-range'; range.textContent = profileItem.range; item.append(range); }
      return item;
    }));
    const rule = (id, text) => { document.querySelector(`#${id}`).textContent = text; };
    rule('world-carpet-count', worldNumber(config.carpet_count ?? 5));
    rule('world-carpet-speed', `${worldNumber(maxCarpetSpeed)} м/с`);
    rule('world-carpet-acceleration', `${worldNumber(maxCarpetAcceleration)} м/с²`);
    rule('world-friction', `${frictionPercent.toLocaleString('ru-RU', { maximumFractionDigits: 1 })}% за тик`);
    const booleanRule = (id, enabled, label) => {
      const element = document.querySelector(`#${id}`);
      element.textContent = enabled ? '✓' : '✕';
      element.dataset.enabled = String(enabled);
      element.setAttribute('aria-label', `${label}: ${enabled ? 'да' : 'нет'}`);
      element.title = `${label}: ${enabled ? 'да' : 'нет'}`;
    };
    booleanRule('world-coin-spawn', config.enable_coin_spawner ?? false, 'Монеты появляются');
    booleanRule('world-anomaly-spawn', config.enable_spawner ?? false, 'Аномалии появляются');
    booleanRule('world-carpet-respawn', config.enable_respawn ?? true, 'Респавн ковров');
    const settings = document.querySelector('#arena-world-settings');
    settings.replaceChildren();
    const settingEntries = Object.entries(config).filter(([key]) => key !== 'runtime_entropy');
    for (const [key, rawValue] of settingEntries) {
      const [label, suffix] = worldSettingLabels[key] || [key.replaceAll('_', ' '), ''];
      let value;
      if (typeof rawValue === 'boolean') value = rawValue ? 'включено' : 'выключено';
      else if (key === 'anomaly_force_outlier_probability') value = `${(Number(rawValue) * 100).toLocaleString('ru-RU')}%`;
      else value = `${Number.isFinite(Number(rawValue)) ? Number(rawValue).toLocaleString('ru-RU') : rawValue}${suffix}`;
      const row = document.createElement('div');
      const term = document.createElement('dt'); term.textContent = label;
      const detail = document.createElement('dd'); detail.textContent = value;
      row.append(term, detail); settings.append(row);
    }
    state.arenaSeconds = Number.isFinite(Number(active.seconds_remaining)) ? Math.max(0, Number(active.seconds_remaining)) : null;
    state.arenaDeadline = performance.now() + (state.arenaSeconds ?? 0) * 1000;
    const countdown = document.querySelector('#arena-countdown');
    countdown.dataset.phase = active.status || '';
    document.querySelector('#arena-time-label').textContent = active.status === 'cooldown' ? 'ДО СТАРТА АРЕНЫ' : 'ДО СМЕНЫ МИРА';
    countdown.textContent = countdownText(state.arenaSeconds, active.status);
    updateCountdownTone(countdown, state.arenaSeconds, active.status);
  }

  async function refreshArenaContext() {
    try {
      const response = await fetch('/api/worlds');
      if (!response.ok) throw new Error(`Hub ${response.status}`);
      const data = await response.json();
      updateArenaContext(data);
      await refreshArenaRanking(data.active || {});
    } catch (_) { setConnection('disconnected', 'Нет связи с Hub'); }
  }
  refreshArenaContext();
  setInterval(refreshArenaContext, 5000);
  document.querySelector('#arena-ranking-mode').addEventListener('change', () => refreshArenaRanking(state.arenaActive || {}));
  setInterval(() => {
    if (state.arenaSeconds === null) return;
    const countdown = document.querySelector('#arena-countdown');
    const remaining = Math.max(0, (state.arenaDeadline - performance.now()) / 1000);
    countdown.textContent = countdownText(remaining, countdown.dataset.phase);
    updateCountdownTone(countdown, remaining, countdown.dataset.phase);
  }, 1000);

  async function refreshArenaRanking(active) {
    const status = document.querySelector('#arena-ranking-status');
    const host = document.querySelector('#arena-ranking-table');
    const leaders = document.querySelector('#viz-gold-leaders');
    const renderTopThree = teams => {
      const hasExplicitRanks = teams.some(team => Number.isFinite(Number(team.rank)));
      leaders.dataset.count = String(Math.min(3, teams.length));
      leaders.replaceChildren(...[1, 2, 3].map(place => {
        const team = hasExplicitRanks ? teams.find(item => Number(item.rank) === place) : teams[place - 1];
        const item = document.createElement('div');
        item.className = `viz-gold-leader rank-${place}${team ? '' : ' viz-gold-placeholder'}`;
        item.dataset.place = String(place);
        const rank = document.createElement('b'); rank.className = 'viz-gold-rank'; rank.textContent = `#${place}`;
        const name = document.createElement('span'); name.className = 'viz-gold-team';
        name.textContent = team ? (team.name || 'Команда') : 'Место свободно';
        const gold = document.createElement('strong'); gold.className = 'viz-gold-value';
        gold.textContent = team ? formatCompactGold(team.gold) : '—';
        const metrics = document.createElement('div'); metrics.className = 'viz-gold-metrics';
        const deaths = document.createElement('small'); deaths.className = 'viz-gold-deaths';
        const lost = team ? Math.max(0, Math.trunc(finite(team.carpets_lost))) : null;
        deaths.setAttribute('aria-label', `Смерти: ${lost === null ? 'нет данных' : lost.toLocaleString('ru-RU')}`);
        deaths.append(deathIcon(), document.createTextNode(lost === null ? '—' : formatCompactCount(lost)));
        metrics.append(gold, deaths);
        item.append(rank, name, metrics);
        return item;
      }));
    };
    const updateOwnStats = teams => {
      const ownTeam = state.token && state.current?.name
        ? teams.find(team => team.name === state.current.name) : null;
      ownDeathCount.textContent = ownTeam ? String(Math.max(0, Math.trunc(finite(ownTeam.carpets_lost)))) : '0';
      ownRank.textContent = ownTeam ? `#${ownTeam.rank || teams.indexOf(ownTeam) + 1}` : '—';
    };
    try {
      let currentTeams = [];
      if (active.run_id) {
        const currentResponse = await fetch(`/api/leaderboard?scope=run&run_id=${encodeURIComponent(active.run_id)}`);
        const currentData = await currentResponse.json();
        if (!currentResponse.ok) throw new Error(currentData.error || `Hub ${currentResponse.status}`);
        currentTeams = currentData.teams || [];
      }
      renderTopThree(currentTeams);
      updateOwnStats(currentTeams);
      const mode = document.querySelector('#arena-ranking-mode').value;
      if (mode === 'current' && !active.run_id) {
        status.textContent = 'Рейтинг появится, когда начнётся первый запуск арены.';
        host.replaceChildren();
        return;
      }
      let data;
      if (mode === 'current') data = { teams: currentTeams, run: { arena_name: active.arena_name } };
      else {
        const response = await fetch(`/api/leaderboard?scope=${mode}`);
        data = await response.json();
        if (!response.ok) throw new Error(data.error || `Hub ${response.status}`);
      }
      const teams = data.teams || [];
      const countLabel = mode === 'last_10' ? `последние ${data.window_runs || 0} запусков` : mode === 'all' ? `вся история · ${teams.length} команд` : `${data.run?.arena_name || active.arena_name} · ${teams.length} команд`;
      status.textContent = teams.length ? countLabel : 'Пока нет результатов для этого зачёта.';
      host.replaceChildren();
      if (!teams.length) return;
      const table = document.createElement('table'); table.className = 'arena-ranking-table-inner';
      const headers = ['#', 'Команда', 'Золото', 'Собрано золота', 'Золото / м'];
      const thead = document.createElement('thead'), heading = document.createElement('tr');
      for (const label of headers) { const th = document.createElement('th'); th.textContent = label; heading.append(th); }
      thead.append(heading); table.append(thead);
      const tbody = document.createElement('tbody');
      for (const team of teams.slice(0, 15)) {
        const row = document.createElement('tr');
        row.className = `rank-${team.rank}`;
        const totals = mode === 'current' ? team : (team.total || {});
        const values = [team.rank, team.name, totals.gold, totals.gold_collected, team.gold_per_distance];
        values.forEach((value, index) => {
          const cell = document.createElement('td');
          cell.dataset.label = headers[index];
          if (index === 0) cell.className = 'rank-cell';
          if (index === 1) cell.className = 'team';
          if (index === 2 || index === 3) {
            cell.className = 'gold-cell';
            cell.append(document.createTextNode(Number(value || 0).toLocaleString('ru-RU')));
            const symbol = document.createElement('span'); symbol.className = 'ranking-gold-symbol';
            symbol.setAttribute('aria-label', 'золота'); symbol.textContent = '✦'; cell.append(' ', symbol);
          } else if (index === 4) {
            if (value === null || value === undefined) cell.textContent = '—';
            else {
              cell.append(document.createTextNode(Number(value).toLocaleString('ru-RU', { maximumFractionDigits: 3 })));
              const symbol = document.createElement('span'); symbol.className = 'ranking-gold-symbol';
              symbol.setAttribute('aria-label', 'золота'); symbol.textContent = '✦'; cell.append(' ', symbol, '/м');
            }
          }
          else cell.textContent = String(value ?? '—');
          row.append(cell);
        });
        tbody.append(row);
      }
      table.append(tbody); host.append(table);
      document.querySelector('#arena-ranking-updated').textContent = `Обновлено ${new Date().toLocaleTimeString()}`;
    } catch (error) { status.textContent = `Не удалось загрузить рейтинг: ${error.message}`; }
  }

  function setConnection(kind, text) {
    const stateName = kind === 'connected' ? 'connected' : kind === 'connecting' ? 'connecting' : 'disconnected';
    connectionStatus.className = `arena-connection-status ${stateName}`;
    connectionStatus.querySelector('span').textContent = stateName === 'connected' ? 'Подключено' : stateName === 'connecting' ? 'Подключаемся' : 'Не подключено';
    connectionStatus.title = text || '';
    if (typeof syncArenaEntry === 'function') syncArenaEntry();
  }

  function revealArenaCanvas() {
    wrap.scrollIntoView({ behavior: 'smooth', block: 'center' });
    wrap.classList.remove('canvas-arrival');
    requestAnimationFrame(() => wrap.classList.add('canvas-arrival'));
    window.setTimeout(() => wrap.classList.remove('canvas-arrival'), 1100);
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
  function visibleWorldBounds(padding = 0) {
    const a = screenToWorld({ x: -padding, y: -padding });
    const b = screenToWorld({ x: state.width + padding, y: state.height + padding });
    return { minX: Math.min(a.x, b.x), maxX: Math.max(a.x, b.x),
      minY: Math.min(a.y, b.y), maxY: Math.max(a.y, b.y) };
  }
  function circleVisibleOnCanvas(point, radius) {
    return point.x + radius >= 0 && point.x - radius <= state.width
      && point.y + radius >= 0 && point.y - radius <= state.height;
  }
  function moveCamera(dx, dy) {
    const factor = scale();
    state.camera.x += dx / factor;
    state.camera.y -= dy / factor;
  }
  function zoomAt(factor, anchor) {
    if (!state.current) return;
    const before = screenToWorld(anchor);
    state.camera.zoom = Math.max(.18, Math.min(MAX_CAMERA_ZOOM, state.camera.zoom * factor));
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
    const coins = snapshot?.bounties || [], factor = scale(), highlights = [], labels = [];
    // Render directly from world units: no whole-map bitmap to stretch on zoom.
    // Batch same-color circles into two Canvas paths to keep high coin counts cheap.
    ctx.fillStyle = '#d89a19'; ctx.beginPath();
    for (const coin of coins) {
      const p = worldToScreen(coin);
      const worldRadius = finite(coin.radius) || finite(snapshot?.transportRadius) || 4;
      const radius = Math.max(.35, worldRadius * factor * .75);
      if (p.x < -radius || p.x > state.width + radius || p.y < -radius || p.y > state.height + radius) continue;
      ctx.moveTo(p.x + radius, p.y); ctx.arc(p.x, p.y, radius, 0, Math.PI * 2);
      if (radius >= 2.5) highlights.push([p.x - radius * .25, p.y - radius * .25, radius * .28]);
      // Reveal values before the camera reaches its closest zoom. The compact
      // homepage preview has less room, so it gets an even earlier threshold.
      const showDenomination = embeddedObserver ? radius >= 1.7 : radius >= 3;
      if (showDenomination && Number.isFinite(Number(coin.points))) {
        labels.push({ x: p.x, y: p.y - radius - 9, value: formatCompactGold(coin.points) });
      }
    }
    ctx.fill();
    if (highlights.length) {
      ctx.fillStyle = '#ffe09a'; ctx.beginPath();
      for (const [x, y, radius] of highlights) { ctx.moveTo(x + radius, y); ctx.arc(x, y, radius, 0, Math.PI * 2); }
      ctx.fill();
    }
    if (labels.length) {
      ctx.font = '700 9px system-ui'; ctx.textAlign = 'center'; ctx.textBaseline = 'middle';
      for (const label of labels) {
        const width = ctx.measureText(label.value).width + 10, height = 14;
        const x = Math.max(width / 2 + 2, Math.min(state.width - width / 2 - 2, label.x));
        const y = Math.max(height / 2 + 2, label.y);
        ctx.fillStyle = 'rgba(13, 27, 39, .9)';
        ctx.beginPath();
        if (ctx.roundRect) ctx.roundRect(x - width / 2, y - height / 2, width, height, 4);
        else ctx.rect(x - width / 2, y - height / 2, width, height);
        ctx.fill();
        ctx.fillStyle = '#ffe09a'; ctx.fillText(label.value, x, y);
      }
    }
  }

  function drawAnomalies(ctx, anomalies) {
    for (const anomaly of anomalies || []) {
      const center = worldToScreen(anomaly);
      const attracting = finite(anomaly.strength) > 0;
      const color = attracting ? '#e34850' : '#287bd4';
      const fieldRadius = Math.max(3, finite(anomaly.effectiveRadius) * scale());
      const coreRadius = Math.max(3.5, finite(anomaly.radius) * scale());
      if (!circleVisibleOnCanvas(center, fieldRadius)) continue;
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
    const teamNumbers = new Map(), carpetSlots = new Map(), teamSlotCounts = new Map();
    for (const carpet of carpets) {
      const teamKey = carpetTeamKey(carpet);
      if (!teamNumbers.has(teamKey)) teamNumbers.set(teamKey, teamNumbers.size + 1);
      const slot = carpetSlot(carpet);
      const fallbackSlot = (teamSlotCounts.get(teamKey) || 0) + 1;
      teamSlotCounts.set(teamKey, fallbackSlot);
      carpetSlots.set(carpet.id, slot || String(fallbackSlot));
    }
    for (const carpet of carpets) {
      const center = worldToScreen(carpet);
      if (!circleVisibleOnCanvas(center, radius + (carpet.id === state.selectedId ? 70 : 10))) continue;
      const isSelected = carpet.id === state.selectedId;
      const palette = carpet.own ? { body: '#f2c64d', aura: 'rgba(245,193,67,.22)' } : teamColor(carpet.teamId);
      ctx.fillStyle = palette.aura;
      ctx.beginPath(); ctx.arc(center.x, center.y, isSelected ? radius + 8 : radius + 5, 0, Math.PI * 2); ctx.fill();
      ctx.globalAlpha = alive(carpet) ? 1 : .34;
      ctx.fillStyle = palette.body;
      const side = radius * 1.42;
      const heading = Math.atan2(-finite(carpet.velocity?.y), finite(carpet.velocity?.x));
      ctx.save(); ctx.translate(center.x, center.y); ctx.rotate(heading + Math.PI / 4);
      ctx.fillRect(-side / 2, -side / 2, side, side);
      ctx.strokeStyle = isSelected ? '#10243a' : '#07131f'; ctx.lineWidth = isSelected ? 1.8 : 1;
      ctx.strokeRect(-side / 2, -side / 2, side, side);
      ctx.restore();
      ctx.globalAlpha = 1;
      if (isSelected) {
        drawArrow(ctx, center, carpet.velocity, '#2276d2', finite(snapshot.maxSpeed), 2.5);
        drawArrow(ctx, center, carpet.selfAcceleration, '#d49300', finite(snapshot.maxAccel), 2.5);
        drawArrow(ctx, center, carpet.anomalyAcceleration, '#9a49b6', finite(snapshot.maxAccel), 2.5);
      }
      ctx.fillStyle = carpet.own ? '#132638' : '#fff'; ctx.font = `700 ${Math.max(6, Math.min(9, radius * 1.05))}px system-ui`; ctx.textAlign = 'center'; ctx.textBaseline = 'middle';
      ctx.fillText(carpetSlots.get(carpet.id), center.x, center.y);
      if (embeddedObserver || radius >= 5) {
        const label = `Команда ${teamNumbers.get(carpetTeamKey(carpet))}`;
        ctx.font = '700 9px system-ui';
        const width = ctx.measureText(label).width + 10, height = 14;
        const x = Math.max(width / 2 + 2, Math.min(state.width - width / 2 - 2, center.x));
        const y = Math.max(height / 2 + 2, center.y - radius - 10);
        ctx.fillStyle = 'rgba(13, 27, 39, .9)';
        ctx.beginPath();
        if (ctx.roundRect) ctx.roundRect(x - width / 2, y - height / 2, width, height, 4);
        else ctx.rect(x - width / 2, y - height / 2, width, height);
        ctx.fill();
        ctx.strokeStyle = palette.body; ctx.lineWidth = 1; ctx.stroke();
        ctx.fillStyle = '#f5f7f9'; ctx.textAlign = 'center'; ctx.textBaseline = 'middle'; ctx.fillText(label, x, y);
      }
    }
  }

  function estimateDeathCause(carpet, snapshot) {
    const point = { x: finite(carpet.x), y: finite(carpet.y) };
    const carpetRadius = Math.max(0, finite(snapshot.transportRadius));
    if ((snapshot.anomalies || []).some(anomaly =>
      Math.hypot(point.x - finite(anomaly.x), point.y - finite(anomaly.y))
        <= Math.max(0, finite(anomaly.radius)) + carpetRadius + 1)) return 'ядро аномалии';
    if (allCarpets(snapshot).some(other => other.id !== carpet.id
      && Math.hypot(point.x - finite(other.x), point.y - finite(other.y)) <= carpetRadius * 2 + 1)) {
      return 'столкновение ковров';
    }
    const map = mapSize(snapshot);
    if (point.x < 0 || point.x > map.x || point.y < 0 || point.y > map.y) return 'граница арены';
    return 'причина неясна';
  }

  function recordDeathEvents(previous, snapshot, now) {
    if (!previous) return;
    const carpets = allCarpets(snapshot), teamNumbers = new Map();
    for (const carpet of carpets) {
      const key = carpetTeamKey(carpet);
      if (!teamNumbers.has(key)) teamNumbers.set(key, teamNumbers.size + 1);
    }
    for (const carpet of carpets) {
      const before = byId(previous, carpet.id);
      if (!before) continue;
      const statusChanged = alive(before) && !alive(carpet);
      const deathCountChanged = carpet.own && finite(carpet.deathCount) > finite(before.deathCount);
      if (!statusChanged && !deathCountChanged) continue;
      const cause = statusChanged ? estimateDeathCause(carpet, snapshot) : 'причина неясна';
      state.deathEvents.unshift({
        text: `Команда ${teamNumbers.get(carpetTeamKey(carpet))} · ковёр ${carpetSlot(carpet) || '—'} · ${cause}`,
        time: now,
      });
    }
    state.deathEvents = state.deathEvents.slice(0, 5);
  }

  function drawDeathLog(ctx) {
    const now = performance.now();
    state.deathEvents = state.deathEvents.filter(event => now - event.time < 30_000);
    if (!state.deathEvents.length || state.width < 190) return;
    const padding = 9, rowHeight = 17, panelWidth = Math.min(258, state.width - 16);
    const rows = state.deathEvents.slice(0, 4), panelHeight = 24 + rows.length * rowHeight + 7;
    const x = embeddedObserver ? 8 : state.width - panelWidth - 8;
    const y = embeddedObserver ? 58 : 8;
    ctx.save();
    ctx.fillStyle = 'rgba(8, 18, 29, .88)'; ctx.strokeStyle = 'rgba(113, 133, 148, .48)'; ctx.lineWidth = 1;
    ctx.beginPath();
    if (ctx.roundRect) ctx.roundRect(x, y, panelWidth, panelHeight, 9);
    else ctx.rect(x, y, panelWidth, panelHeight);
    ctx.fill(); ctx.stroke();
    ctx.textAlign = 'left'; ctx.textBaseline = 'middle';
    ctx.font = '800 8px system-ui'; ctx.fillStyle = '#aebdca';
    ctx.fillText('СОБЫТИЯ АРЕНЫ', x + padding, y + 12);
    ctx.font = '600 9px system-ui';
    for (const [index, event] of rows.entries()) {
      let text = event.text;
      const maxTextWidth = panelWidth - padding * 2;
      while (text.length > 4 && ctx.measureText(text).width > maxTextWidth) text = `${text.slice(0, -2)}…`;
      ctx.globalAlpha = Math.max(.45, 1 - (now - event.time) / 30_000);
      ctx.fillStyle = '#f1c4c5';
      ctx.fillText(text, x + padding, y + 29 + index * rowHeight);
    }
    ctx.restore();
  }

  function accelerationFor(carpet, snapshot) {
    if (state.stickVector) return state.stickVector;
    if (!state.pointer || !state.pointerInside) return null;
    const center = worldToScreen(state.routeSegments.at(-1)?.end || carpet);
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
    const center = worldToScreen(state.routeSegments.at(-1)?.end || carpet);
    if (state.pointer && state.pointerInside && !state.stickVector) {
      ctx.strokeStyle = '#687782'; ctx.lineWidth = 1.5; ctx.setLineDash([6, 5]);
      ctx.beginPath(); ctx.moveTo(center.x, center.y); ctx.lineTo(state.pointer.x, state.pointer.y); ctx.stroke(); ctx.setLineDash([]);
    }
    const acceleration = accelerationFor(carpet, snapshot);
    if (acceleration) drawArrow(ctx, center, acceleration, '#ec9b10', finite(snapshot.maxAccel), 3);
  }

  function clampLength(vector, maximum) {
    const length = Math.hypot(vector.x, vector.y);
    if (!Number.isFinite(length) || length <= maximum || length === 0) return vector;
    return { x: vector.x * maximum / length, y: vector.y * maximum / length };
  }

  function anomalyForceAt(position, anomalies) {
    let x = 0, y = 0;
    for (const anomaly of anomalies) {
      const dx = finite(anomaly.x) - position.x, dy = finite(anomaly.y) - position.y;
      const distance = Math.hypot(dx, dy), radius = Math.max(0, finite(anomaly.effectiveRadius));
      const strength = finite(anomaly.strength);
      if (distance <= 1e-6 || distance > radius || strength === 0) continue;
      x += dx / distance * strength;
      y += dy / distance * strength;
    }
    return { x, y };
  }

  function segmentCircleEntry(from, to, radius) {
    const dx = to.x - from.x, dy = to.y - from.y;
    const a = dx * dx + dy * dy, c = from.x * from.x + from.y * from.y - radius * radius;
    if (c <= 0) return 0;
    if (a <= 1e-12) return null;
    const b = 2 * (from.x * dx + from.y * dy), discriminant = b * b - 4 * a * c;
    if (discriminant < 0) return null;
    const t = (-b - Math.sqrt(discriminant)) / (2 * a);
    return t >= 0 && t <= 1 ? t : null;
  }

  function anomalyDespawned(anomaly, map) {
    const dx = anomaly.x - Math.min(map.x, Math.max(0, anomaly.x));
    const dy = anomaly.y - Math.min(map.y, Math.max(0, anomaly.y));
    if (dx * dx + dy * dy <= finite(anomaly.effectiveRadius) ** 2) return false;
    return finite(anomaly.velocity?.x) * dx + finite(anomaly.velocity?.y) * dy > 0;
  }

  function predictSelectedTrajectory(snapshot) {
    const carpet = byId(snapshot, state.selectedId);
    if (!carpet || !alive(carpet)) return null;
    const manualVector = state.manual ? accelerationFor(carpet, snapshot) : null;
    const cacheKey = `${state.receivedAt}:${state.selectedId}:${state.manual}:${state.routeSegments.length}:${finite(manualVector?.x).toFixed(2)}:${finite(manualVector?.y).toFixed(2)}`;
    if (cacheKey === state.trajectoryCacheKey) return state.trajectoryCache;
    if (state.routeSegments.length) {
      const fixed = state.routeSegments.flatMap((segment, index) => index ? segment.points.slice(1) : segment.points);
      const tail = state.routeSegments.at(-1), acceleration = manualVector || tail.acceleration;
      const planElapsed = Math.max(0, (performance.now() - state.routeStartedAt) / 1000);
      const preview = simulateTrajectory(snapshot, carpet, { position: tail.end, velocity: tail.velocity,
        startTime: tail.endTime, environmentOffset: Math.max(0, tail.endTime - planElapsed), acceleration, horizon: 20 });
      const points = [...fixed, ...preview.slice(1)];
      state.trajectoryCacheKey = cacheKey; state.trajectoryCache = points;
      return points;
    }
    const command = carpet.own
      ? (state.manual && carpet.id === state.selectedId ? manualVector || carpet.selfAcceleration : carpet.selfAcceleration)
      : { x: 0, y: 0 };
    const points = simulateTrajectory(snapshot, carpet, { position: { x: finite(carpet.x), y: finite(carpet.y) },
      velocity: { x: finite(carpet.velocity?.x), y: finite(carpet.velocity?.y) }, startTime: 0,
      acceleration: command, horizon: 20 });
    state.trajectoryCacheKey = cacheKey;
    state.trajectoryCache = points;
    return points;
  }

  function simulateTrajectory(snapshot, carpet, options) {
    const tickDuration = Math.max(.05, state.physics.dt), startTime = Math.max(0, finite(options.startTime));
    const environmentOffset = Math.max(0, finite(options.environmentOffset ?? startTime));
    const horizon = Math.max(tickDuration, finite(options.horizon, 20)), count = Math.ceil(horizon / tickDuration);
    const maxAccel = Math.max(0, finite(snapshot.maxAccel));
    const maxSpeed = Math.max(0, finite(snapshot.maxSpeed));
    const acceleration = clampLength({ x: finite(options.acceleration?.x), y: finite(options.acceleration?.y) }, maxAccel);
    const radius = Math.max(0, finite(snapshot.transportRadius));
    const map = mapSize(snapshot);
    const collisionTargets = allCarpets(snapshot).filter(item => item.id !== carpet.id && alive(item));
    let position = { x: finite(options.position?.x), y: finite(options.position?.y) };
    let velocity = { x: finite(options.velocity?.x), y: finite(options.velocity?.y) };
    let anomalies = (snapshot.anomalies || []).map(item => ({ ...item,
      x: finite(item.x) + finite(item.velocity?.x) * environmentOffset,
      y: finite(item.y) + finite(item.velocity?.y) * environmentOffset,
    })).filter(item => !anomalyDespawned(item, map));
    const points = [{ x: position.x, y: position.y, time: startTime, velocity: { ...velocity } }];
    let elapsed = 0;
    for (let step = 0; step < count && elapsed < horizon; step++) {
      const dt = Math.min(tickDuration, horizon - elapsed);
      const nextAnomalies = anomalies.map(item => ({ ...item,
        x: item.x + finite(item.velocity?.x) * dt,
        y: item.y + finite(item.velocity?.y) * dt,
      }));
      const external = anomalyForceAt(position, anomalies);
      const nextVelocity = clampLength({
        x: velocity.x * Math.pow(state.physics.friction, dt / tickDuration) + (acceleration.x + external.x) * dt,
        y: velocity.y * Math.pow(state.physics.friction, dt / tickDuration) + (acceleration.y + external.y) * dt,
      }, maxSpeed);
      const nextPosition = { x: position.x + nextVelocity.x * dt, y: position.y + nextVelocity.y * dt };
      let terminal = null, terminalRatio = 1;
      for (let index = 0; index < anomalies.length; index++) {
        const anomaly = anomalies[index];
        const relativeFrom = { x: position.x - anomaly.x, y: position.y - anomaly.y };
        const relativeTo = { x: nextPosition.x - nextAnomalies[index].x, y: nextPosition.y - nextAnomalies[index].y };
        const hit = segmentCircleEntry(relativeFrom, relativeTo, Math.max(0, finite(anomaly.radius)) + radius);
        if (hit !== null && hit < terminalRatio) { terminal = 'core'; terminalRatio = hit; }
      }
      for (const other of collisionTargets) {
        const otherStart = { x: finite(other.x) + finite(other.velocity?.x) * (environmentOffset + elapsed),
          y: finite(other.y) + finite(other.velocity?.y) * (environmentOffset + elapsed) };
        const otherEnd = { x: finite(other.x) + finite(other.velocity?.x) * (environmentOffset + elapsed + dt),
          y: finite(other.y) + finite(other.velocity?.y) * (environmentOffset + elapsed + dt) };
        const relativeFrom = { x: position.x - otherStart.x, y: position.y - otherStart.y };
        const relativeTo = { x: nextPosition.x - otherEnd.x, y: nextPosition.y - otherEnd.y };
        const hit = segmentCircleEntry(relativeFrom, relativeTo, radius * 2);
        if (hit !== null && hit < terminalRatio) {
          terminal = 'carpet'; terminalRatio = hit;
        }
      }
      const dx = nextPosition.x - position.x, dy = nextPosition.y - position.y;
      const mapEdgeRatio = (coordinate, delta, limit) => delta < 0 && coordinate + delta < 0 ? (0 - coordinate) / delta
        : delta > 0 && coordinate + delta > limit ? (limit - coordinate) / delta : 1;
      const edgeRatio = Math.min(mapEdgeRatio(position.x, dx, map.x), mapEdgeRatio(position.y, dy, map.y));
      if (edgeRatio < terminalRatio) { terminal = 'edge'; terminalRatio = edgeRatio; }
      if (terminal) {
        points.push({ x: position.x + dx * terminalRatio, y: position.y + dy * terminalRatio,
          time: startTime + elapsed + dt * terminalRatio, velocity: { x: velocity.x + (nextVelocity.x - velocity.x) * terminalRatio,
            y: velocity.y + (nextVelocity.y - velocity.y) * terminalRatio }, terminal });
        break;
      }
      position = nextPosition;
      velocity = nextVelocity;
      anomalies = nextAnomalies.filter(item => !anomalyDespawned(item, map));
      elapsed += dt;
      points.push({ x: position.x, y: position.y, time: startTime + elapsed, velocity: { ...velocity } });
    }
    return points;
  }

  function drawSelectedTrajectory(ctx, snapshot) {
    const points = predictSelectedTrajectory(snapshot);
    if (!points?.length) return;
    ctx.save();
    ctx.lineCap = 'round'; ctx.lineJoin = 'round'; ctx.lineWidth = 2.2;
    let fixedPointCount = 0;
    for (const segment of state.routeSegments) {
      const fixed = segment.points.map(worldToScreen);
      fixedPointCount += segment.points.length - 1;
      ctx.strokeStyle = 'rgba(194, 140, 36, .9)'; ctx.lineWidth = 3;
      ctx.beginPath(); fixed.forEach((point, index) => index ? ctx.lineTo(point.x, point.y) : ctx.moveTo(point.x, point.y)); ctx.stroke();
      const anchor = worldToScreen(segment.end);
      ctx.fillStyle = '#f2c64d'; ctx.strokeStyle = '#75500e'; ctx.lineWidth = 1.5;
      ctx.beginPath(); ctx.arc(anchor.x, anchor.y, 5, 0, Math.PI * 2); ctx.fill(); ctx.stroke();
    }
    const previewStart = Math.min(fixedPointCount, points.length - 1);
    const screenPoints = points.slice(previewStart).map(worldToScreen);
    if (!state.routeSegments.length) screenPoints.unshift(worldToScreen(byId(snapshot, state.selectedId)));
    ctx.strokeStyle = 'rgba(0, 103, 112, .76)'; ctx.lineWidth = 2.2;
    ctx.beginPath(); screenPoints.forEach((point, index) => index ? ctx.lineTo(point.x, point.y) : ctx.moveTo(point.x, point.y)); ctx.stroke();
    for (let index = 1; index < screenPoints.length; index += 2) {
      const point = screenPoints[index], remaining = 1 - index / screenPoints.length;
      const radius = 1.5 + remaining * 0.65;
      ctx.globalAlpha = 0.24 + remaining * 0.56;
      ctx.fillStyle = '#087d82';
      ctx.beginPath(); ctx.arc(point.x, point.y, radius, 0, Math.PI * 2); ctx.fill();
    }
    const terminal = points.at(-1);
    if (terminal?.terminal) {
      const point = screenPoints.at(-1);
      ctx.globalAlpha = 0.9; ctx.strokeStyle = '#df424d'; ctx.lineWidth = 1.8;
      ctx.beginPath(); ctx.moveTo(point.x - 4, point.y - 4); ctx.lineTo(point.x + 4, point.y + 4);
      ctx.moveTo(point.x + 4, point.y - 4); ctx.lineTo(point.x - 4, point.y + 4); ctx.stroke();
    }
    ctx.restore();
  }

  function routeStart(snapshot, carpet) {
    const tail = state.routeSegments.at(-1);
    return tail
      ? { position: { ...tail.end }, velocity: { ...tail.velocity }, startTime: tail.endTime }
      : { position: { x: finite(carpet.x), y: finite(carpet.y) },
        velocity: { x: finite(carpet.velocity?.x), y: finite(carpet.velocity?.y) }, startTime: 0 };
  }

  function routeNotice(text) { message.textContent = text; message.className = 'viz-message'; }

  function pinRouteSegment() {
    const carpet = byId(state.current, state.selectedId);
    if (!state.manual || !carpet?.own || !alive(carpet)) return;
    const command = accelerationFor(carpet, state.current);
    if (!command || Math.hypot(command.x, command.y) < 1e-6) {
      routeNotice('Наведи прицел через монету, затем нажми F или среднюю кнопку мыши.'); return;
    }
    const start = routeStart(state.current, carpet);
    const elapsed = state.routeStartedAt ? Math.max(0, (performance.now() - state.routeStartedAt) / 1000) : 0;
    const points = simulateTrajectory(state.current, carpet, { ...start,
      environmentOffset: Math.max(0, start.startTime - elapsed), acceleration: command, horizon: 20 });
    const pickupRadius = Math.max(0, finite(state.current.transportRadius));
    const alreadyQueued = new Set(state.routeSegments.map(segment => segment.coinKey));
    let hit = null;
    for (let index = 1; index < points.length; index++) {
      const from = points[index - 1], to = points[index];
      for (const coin of state.current.bounties || []) {
        // The next leg starts inside the previous waypoint's pickup circle;
        // never let that same bounty become the new leg's zero-time endpoint.
        if (alreadyQueued.has(coinKey(coin))) continue;
        const fraction = segmentCircleEntry({ x: from.x - finite(coin.x), y: from.y - finite(coin.y) },
          { x: to.x - finite(coin.x), y: to.y - finite(coin.y) }, pickupRadius + Math.max(0, finite(coin.radius)));
        if (fraction === null) continue;
        const time = from.time + (to.time - from.time) * fraction;
        if (!hit || time < hit.time) hit = { coin, fraction, time, index, from, to };
      }
    }
    if (!hit) { routeNotice('Прогноз не пересекает монеты — измени прицел.'); return; }
    const endpoint = { x: hit.from.x + (hit.to.x - hit.from.x) * hit.fraction,
      y: hit.from.y + (hit.to.y - hit.from.y) * hit.fraction };
    const endVelocity = { x: finite(hit.from.velocity?.x) + (finite(hit.to.velocity?.x) - finite(hit.from.velocity?.x)) * hit.fraction,
      y: finite(hit.from.velocity?.y) + (finite(hit.to.velocity?.y) - finite(hit.from.velocity?.y)) * hit.fraction };
    const segmentPoints = points.slice(0, hit.index);
    segmentPoints.push({ ...endpoint, time: hit.time, velocity: endVelocity });
    state.routeSegments.push({ points: segmentPoints, end: endpoint, velocity: endVelocity, endTime: hit.time,
      coinKey: coinKey(hit.coin), coinValue: finite(hit.coin.points), acceleration: clampLength(command, finite(state.current.maxAccel)) });
    if (state.routeSegments.length === 1) { state.activeRouteSegment = 0; state.routeStartedAt = performance.now(); }
    state.trajectoryCacheKey = '';
    routeNotice(`Сегмент ${state.routeSegments.length} закреплён до монеты ${formatCompactGold(hit.coin.points)} · ~${Math.max(0, hit.time - start.startTime).toFixed(1)} с. Z / правая кнопка — undo.`);
  }

  function undoRouteSegment() {
    if (!state.routeSegments.length) return;
    state.routeSegments.pop();
    state.activeRouteSegment = Math.min(state.activeRouteSegment, Math.max(0, state.routeSegments.length - 1));
    if (!state.routeSegments.length) { state.activeRouteSegment = 0; state.routeStartedAt = 0; }
    state.trajectoryCacheKey = '';
    routeNotice(state.routeSegments.length ? `Осталось сегментов: ${state.routeSegments.length}.` : 'План очищен; прицел снова управляет сразу.');
  }

  function advanceRoutePlan(carpet, snapshot) {
    const index = state.activeRouteSegment, current = state.routeSegments[index];
    if (!current || !state.previous) return;
    const oldCarpet = byId(state.previous, carpet.id);
    if (!oldCarpet) return;
    const oldCoin = (state.previous.bounties || []).find(coin => coinKey(coin) === current.coinKey);
    const targetExists = (snapshot.bounties || []).some(coin => coinKey(coin) === current.coinKey);
    if (!oldCoin) return;

    // Use the actual swept path, not the forecast endpoint: it is possible to
    // pass a missed coin while the planned waypoint remains behind us.
    const selectedDistance = distanceToSegment(oldCoin, oldCarpet, carpet);
    const pickupRadius = Math.max(0, finite(snapshot.transportRadius)) + Math.max(0, finite(oldCoin.radius));
    if (targetExists) {
      if (selectedDistance <= pickupRadius) {
        clearRoutePlan('Ковер прошёл waypoint, но монета осталась — цепочка сброшена. Теперь ускорение задаёт прицел.');
      }
      return;
    }
    const nearestDistance = nearestCarpetDistanceToCoin(state.previous, snapshot, oldCoin);
    if (selectedDistance > pickupRadius || nearestDistance.carpetId !== carpet.id) {
      clearRoutePlan('Целевая монета исчезла, но её собрал не выбранный ковер — план сброшен. Веди прицелом.');
      return;
    }

    if (index >= state.routeSegments.length - 1) {
      clearRoutePlan('Монета собрана. План завершён — продолжай вести ковер прицелом.');
      return;
    }
    if (!rebuildRemainingRoute(carpet, snapshot, index + 1)) {
      clearRoutePlan('Маршрут после сбора больше не проходит через цель — план сброшен. Веди прицелом.');
      return;
    }
    routeNotice(`Монета собрана точно по фактическому движению. Следующий сегмент ${state.activeRouteSegment + 1}/${state.routeSegments.length} пересчитан от ковра.`);
  }

  function rebuildRemainingRoute(carpet, snapshot, firstIndex) {
    const pending = state.routeSegments.slice(firstIndex);
    let position = { x: finite(carpet.x), y: finite(carpet.y) };
    let velocity = { x: finite(carpet.velocity?.x), y: finite(carpet.velocity?.y) };
    let elapsed = 0;
    const rebuilt = [];
    for (const oldSegment of pending) {
      const coin = (snapshot.bounties || []).find(item => coinKey(item) === oldSegment.coinKey);
      if (!coin) return false;
      const points = simulateTrajectory(snapshot, carpet, { position, velocity,
        startTime: elapsed, environmentOffset: elapsed, acceleration: oldSegment.acceleration, horizon: 20 });
      const hit = firstCoinHit(points, coin, snapshot.transportRadius);
      if (!hit) return false;
      const endpoint = { x: hit.from.x + (hit.to.x - hit.from.x) * hit.fraction,
        y: hit.from.y + (hit.to.y - hit.from.y) * hit.fraction };
      const endVelocity = { x: finite(hit.from.velocity?.x) + (finite(hit.to.velocity?.x) - finite(hit.from.velocity?.x)) * hit.fraction,
        y: finite(hit.from.velocity?.y) + (finite(hit.to.velocity?.y) - finite(hit.from.velocity?.y)) * hit.fraction };
      const segmentPoints = points.slice(0, hit.index);
      segmentPoints.push({ ...endpoint, time: hit.time, velocity: endVelocity });
      rebuilt.push({ ...oldSegment, points: segmentPoints, end: endpoint, velocity: endVelocity, endTime: hit.time });
      position = endpoint; velocity = endVelocity; elapsed = hit.time;
    }
    state.routeSegments = rebuilt;
    state.activeRouteSegment = 0;
    state.routeStartedAt = performance.now();
    state.trajectoryCacheKey = '';
    return rebuilt.length > 0;
  }

  function firstCoinHit(points, coin, carpetRadius) {
    const pickupRadius = Math.max(0, finite(carpetRadius)) + Math.max(0, finite(coin.radius));
    for (let index = 1; index < points.length; index++) {
      const from = points[index - 1], to = points[index];
      const fraction = segmentCircleEntry({ x: from.x - finite(coin.x), y: from.y - finite(coin.y) },
        { x: to.x - finite(coin.x), y: to.y - finite(coin.y) }, pickupRadius);
      if (fraction !== null) return { coin, fraction, time: from.time + (to.time - from.time) * fraction,
        index, from, to };
    }
    return null;
  }

  function nearestCarpetDistanceToCoin(previous, snapshot, coin) {
    let nearest = { carpetId: '', distance: Infinity };
    for (const current of allCarpets(snapshot)) {
      if (!alive(current)) continue;
      const before = byId(previous, current.id);
      if (!before || !alive(before)) continue;
      const distance = distanceToSegment(coin, before, current);
      if (distance < nearest.distance) nearest = { carpetId: current.id, distance };
    }
    return nearest;
  }

  function routePositionAt(points, time) {
    for (let index = 1; index < points.length; index++) {
      const from = points[index - 1], to = points[index];
      if (time < from.time || time > to.time || to.time <= from.time) continue;
      const fraction = (time - from.time) / (to.time - from.time);
      return { x: from.x + (to.x - from.x) * fraction, y: from.y + (to.y - from.y) * fraction };
    }
    return null;
  }

  function clearRoutePlan(reason) {
    state.routeSegments = []; state.activeRouteSegment = 0; state.routeStartedAt = 0;
    state.trajectoryCacheKey = '';
    routeNotice(reason);
  }

  function routeHasDeviated(carpet, snapshot) {
    if (!state.routeSegments.length || !state.routeStartedAt) return false;
    const elapsed = (performance.now() - state.routeStartedAt) / 1000;
    const predicted = routePositionAt(predictSelectedTrajectory(snapshot) || [], elapsed);
    if (!predicted) return false;
    const error = Math.hypot(finite(carpet.x) - predicted.x, finite(carpet.y) - predicted.y);
    const tolerance = Math.max(4 * finite(snapshot.transportRadius), 2.5 * finite(snapshot.maxSpeed) * finite(state.physics.dt), 55);
    return error > tolerance;
  }

  function activeRouteAcceleration(carpet, snapshot) {
    if (!state.routeSegments.length) return accelerationFor(carpet, snapshot);
    return state.routeSegments[Math.min(state.activeRouteSegment, state.routeSegments.length - 1)].acceleration;
  }

  function coinKey(coin) { return coin.id ?? `${finite(coin.x).toFixed(2)}:${finite(coin.y).toFixed(2)}`; }
  function distanceToSegment(point, from, to) {
    const dx = to.x - from.x, dy = to.y - from.y, length = dx * dx + dy * dy;
    const t = length > 1e-9 ? Math.max(0, Math.min(1, ((point.x - from.x) * dx + (point.y - from.y) * dy) / length)) : 0;
    return Math.hypot(point.x - (from.x + dx * t), point.y - (from.y + dy * t));
  }
  function recordSelectedHistory(snapshot, now) {
    const id = state.selectedId;
    if (!id) return;
    if (state.historyCarpetId !== id) { state.historyCarpetId = id; state.historyTrail = []; state.collectedMarkers = []; }
    const current = byId(snapshot, id), previous = state.current && byId(state.current, id);
    if (!current || !previous || !alive(current) || !alive(previous)) return;
    const from = { x: finite(previous.x), y: finite(previous.y) }, to = { x: finite(current.x), y: finite(current.y) };
    state.historyTrail.push({ ...to, time: now });
    const oldCoins = new Map((state.current.bounties || []).map(coin => [coinKey(coin), coin]));
    const newCoins = new Set((snapshot.bounties || []).map(coinKey));
    const pickupRadius = Math.max(0, finite(snapshot.transportRadius));
    for (const [key, coin] of oldCoins) {
      if (!newCoins.has(key) && distanceToSegment(coin, from, to) <= pickupRadius + Math.max(0, finite(coin.radius)) + 18) {
        state.collectedMarkers.push({ x: finite(coin.x), y: finite(coin.y), time: now });
      }
    }
    const cutoff = now - 10_000;
    state.historyTrail = state.historyTrail.filter(point => point.time >= cutoff);
    state.collectedMarkers = state.collectedMarkers.filter(point => point.time >= cutoff);
  }
  function drawPreviousPath(ctx) {
    const points = state.historyTrail;
    if (points.length < 2) return;
    ctx.save(); ctx.lineCap = 'round'; ctx.lineJoin = 'round';
    for (let i = 1; i < points.length; i++) {
      const a = worldToScreen(points[i - 1]), b = worldToScreen(points[i]);
      const age = (performance.now() - points[i].time) / 10_000;
      ctx.globalAlpha = Math.max(.04, .23 * (1 - age)); ctx.strokeStyle = '#71808a'; ctx.lineWidth = 2;
      ctx.beginPath(); ctx.moveTo(a.x, a.y); ctx.lineTo(b.x, b.y); ctx.stroke();
    }
    for (const marker of state.collectedMarkers) {
      const p = worldToScreen(marker), age = (performance.now() - marker.time) / 10_000;
      ctx.globalAlpha = Math.max(.08, .68 * (1 - age)); ctx.fillStyle = '#89949b';
      ctx.beginPath(); ctx.arc(p.x, p.y, Math.max(2.2, 3 * scale()), 0, Math.PI * 2); ctx.fill();
      ctx.strokeStyle = '#e5eaed'; ctx.lineWidth = 1; ctx.stroke();
    }
    ctx.restore();
  }

  function drawMap(ctx, snapshot) {
    ctx.fillStyle = '#0b1521'; ctx.fillRect(0, 0, state.width, state.height);
    const size = mapSize(snapshot), topLeft = worldToScreen({ x: 0, y: size.y });
    const bottomRight = worldToScreen({ x: size.x, y: 0 });
    const mapRect = { x: topLeft.x, y: topLeft.y, width: bottomRight.x - topLeft.x, height: bottomRight.y - topLeft.y };
    ctx.fillStyle = '#e4e9eb'; ctx.fillRect(mapRect.x, mapRect.y, mapRect.width, mapRect.height);
    ctx.save(); ctx.beginPath(); ctx.rect(mapRect.x, mapRect.y, mapRect.width, mapRect.height); ctx.clip();
    drawCoins(ctx, snapshot);
    drawAnomalies(ctx, snapshot.anomalies);
    if (!embeddedObserver) {
      drawPreviousPath(ctx);
      drawSelectedTrajectory(ctx, snapshot);
    }
    drawCarpets(ctx, snapshot);
    if (!embeddedObserver) drawManual(ctx, snapshot);
    drawDeathLog(ctx);
    ctx.restore();
    ctx.strokeStyle = '#91a5b3'; ctx.lineWidth = 1.5;
    ctx.strokeRect(mapRect.x, mapRect.y, mapRect.width, mapRect.height);
  }

  function updateSelectedPanel(snapshot) {
    const item = byId(snapshot, state.selectedId);
    if (!item || !alive(item)) return;
  }

  function selectCarpet(id) {
    if (state.manual && id !== state.selectedId) disableManual(true);
    if (id !== state.historyCarpetId) { state.historyCarpetId = id; state.historyTrail = []; state.collectedMarkers = []; }
    state.selectedId = id;
    select.value = id;
    const item = byId(state.current, id);
    const ownAlive = item?.own && alive(item);
    manualButton.disabled = !ownAlive;
    if (!ownAlive && state.manual) disableManual(true);
    updateSelectedPanel(interpolatedState(performance.now()) || state.current);
  }

  function setFollowing(enabled) {
    state.follow = Boolean(enabled);
    followButton.textContent = state.follow ? '◎ Слежение' : '◎ Следить';
    followButton.classList.toggle('follow-on', state.follow);
  }

  function updateEmbeddedObserverFocus(snapshot, now) {
    const aliveCarpets = allCarpets(snapshot).filter(alive)
      .sort((a, b) => String(a.id).localeCompare(String(b.id)));
    if (!aliveCarpets.length) {
      const map = mapSize(snapshot), center = { x: map.x / 2, y: map.y / 2 };
      const coins = snapshot.bounties || [];
      const targetCoin = coins.reduce((best, coin) => {
        if (!best) return coin;
        const distance = item => Math.hypot(finite(item.x) - center.x, finite(item.y) - center.y);
        return distance(coin) < distance(best) ? coin : best;
      }, null);
      const focusKey = targetCoin ? String(coinKey(targetCoin)) : '';
      const needsFocus = state.autoFramePending || state.embeddedFollowId || focusKey !== state.embeddedCoinFocusKey;
      state.embeddedFollowId = '';
      state.embeddedNextFollowAt = 0;
      state.selectedId = '';
      select.value = '';
      if (state.follow) setFollowing(false);
      if (needsFocus) {
        state.camera.x = finite(targetCoin?.x ?? center.x);
        state.camera.y = finite(targetCoin?.y ?? center.y);
        state.camera.zoom = MAX_CAMERA_ZOOM;
        state.camera.initialized = true;
      }
      state.embeddedCoinFocusKey = focusKey;
      state.autoFramePending = false;
      return;
    }

    const currentIndex = aliveCarpets.findIndex(item => item.id === state.embeddedFollowId);
    const targetDisappeared = Boolean(state.embeddedFollowId) && currentIndex < 0;
    const rotate = aliveCarpets.length > 1 && now >= state.embeddedNextFollowAt;
    if (!state.autoFramePending && currentIndex >= 0 && !targetDisappeared && !rotate) return;

    const target = currentIndex >= 0 && rotate
      ? aliveCarpets[(currentIndex + 1) % aliveCarpets.length]
      : currentIndex >= 0 ? aliveCarpets[currentIndex]
        : aliveCarpets.find(item => String(item.id).localeCompare(String(state.embeddedFollowId)) > 0) || aliveCarpets[0];
    state.embeddedFollowId = target.id;
    state.embeddedCoinFocusKey = '';
    state.embeddedNextFollowAt = now + 10000;
    state.autoFramePending = false;
    selectCarpet(target.id);
    setFollowing(true);
    state.camera.x = finite(target.x);
    state.camera.y = finite(target.y);
    state.camera.zoom = MAX_CAMERA_ZOOM;
    state.camera.initialized = true;
  }

  function rebuildSelect() {
    if (!state.current) return;
    const items = allCarpets(state.current);
    const existing = [...select.options].map(option => option.value);
    const values = items.map(item => item.id);
    if (values.length === existing.length && values.every((value, i) => value === existing[i])) return;
    select.replaceChildren(...items.map((item, index) => new Option(
      `${item.own ? 'Твой флот' : item.teamName} · ${item.id || `Ковер ${index + 1}`} · ${item.status || 'alive'}`, item.id)));
    select.disabled = !items.length;
    followButton.disabled = !items.length;
    manualButton.disabled = true;
    if (embeddedObserver) {
      if (items.some(item => item.id === state.selectedId)) select.value = state.selectedId;
      else { state.selectedId = ''; select.value = ''; }
      return;
    }
    if (items.some(item => item.id === state.selectedId)) select.value = state.selectedId;
    else if (items.length) selectCarpet(items[0].id);
    else state.selectedId = '';
  }

  function updateSnapshot(snapshot) {
    const now = performance.now();
    recordSelectedHistory(snapshot, now);
    if (state.current) { state.previous = state.current; state.previousAt = state.receivedAt; }
    else { state.previous = null; state.previousAt = now - 200; }
    state.current = snapshot;
    recordDeathEvents(state.previous, snapshot, now);
    const goldSummary = document.querySelector('#viz-gold-summary');
    const ownSummary = document.querySelector('#viz-own-summary');
    ownSummary.hidden = !state.token;
    goldSummary.classList.toggle('without-own', !state.token);
    if (state.token) {
      document.querySelector('#viz-own-gold').textContent = `${Math.round(finite(snapshot.points)).toLocaleString('ru-RU')} ✦`;
      document.querySelector('#viz-own-name').textContent = snapshot.name || 'Золото сейчас';
    }
    state.receivedAt = now;
    if (!state.camera.initialized) {
      state.camera.x = mapSize(snapshot).x / 2; state.camera.y = mapSize(snapshot).y / 2;
      state.camera.initialized = true;
    }
    const title = snapshot.name || 'Команда';
    document.title = `${title} · Арена StadMagic`;
    rebuildSelect();
    if (embeddedObserver) {
      updateEmbeddedObserverFocus(snapshot, now);
    } else if (state.autoFramePending) {
      const carpets = allCarpets(snapshot);
      const target = (state.token ? carpets.find(item => item.own && alive(item)) : null)
        || carpets.find(alive) || carpets[0];
      if (target) {
        selectCarpet(target.id);
        state.camera.x = finite(target.x); state.camera.y = finite(target.y);
        state.camera.zoom = MAX_CAMERA_ZOOM; state.camera.initialized = true;
        state.autoFramePending = false;
        setFollowing(true);
      }
    }
    const selected = byId(snapshot, state.selectedId);
    const canDrive = Boolean(selected?.own && alive(selected));
    manualButton.disabled = !canDrive;
    if (state.manual && !canDrive) disableManual(true);
    updateSelectedPanel(interpolatedState(now));
  }

  async function requestSnapshot() {
    if ((!state.token && !state.observer) || state.connecting || state.realtime?.readyState <= WebSocket.OPEN) return;
    state.connecting = true;
    try {
      const headers = { 'Content-Type': 'application/json' };
      if (state.token) headers['X-Auth-Token'] = state.token;
      const response = await fetch('/api/visualizer/ticket', { method: 'POST', headers, body: '{}' });
      const data = await response.json();
      if (!response.ok) throw Object.assign(new Error(data.error || `Ошибка API ${response.status}`), { status: response.status });
      if (data.name && state.token) { try { localStorage.setItem('stadmagic-team-token', state.token); localStorage.setItem('stadmagic-team-name', data.name); window.dispatchEvent(new Event('stadmagic-profile-change')); } catch (_) {} }
      const socket = new WebSocket(data.websocketUrl, ['stadmagic.v1', `stadmagic-ticket.${data.ticket}`]);
      state.realtime = socket;
      socket.onopen = () => {
        state.connecting = false;
        state.reconnectDelay = 250;
        setConnection('connected', `На связи · ${data.name}`);
        message.textContent = '';
        if (state.commandTimer) clearInterval(state.commandTimer);
        state.commandTimer = setInterval(sendRealtimeCommand, 100);
        if (state.manual) startLeaseHeartbeat();
      };
      socket.onmessage = event => {
        try {
          const packet = JSON.parse(event.data);
          if (packet.type === 'snapshot') {
            const previousWaypoint = state.routeSegments[state.activeRouteSegment];
            updateSnapshot({ ...packet.state, enemyTeams: packet.enemyTeams || [] });
            // Apply a newly advanced waypoint immediately on its confirming snapshot.
            if (state.manual && previousWaypoint !== state.routeSegments[state.activeRouteSegment]) sendRealtimeCommand();
          }
          else if (packet.type === 'error') message.textContent = packet.error;
        } catch (_) { /* ignore malformed server frame */ }
      };
      socket.onerror = () => setConnection('connecting', 'Переподключение…');
      socket.onclose = () => {
        state.connecting = false;
        if (state.realtime === socket) state.realtime = null;
        if (state.commandTimer) clearInterval(state.commandTimer);
        if (state.leaseTimer) clearInterval(state.leaseTimer);
        state.commandTimer = 0;
        state.leaseTimer = 0;
        setConnection('connecting', 'Связь потеряна · переподключение…');
        if (state.token || state.observer) {
          const delay = state.reconnectDelay;
          state.reconnectDelay = Math.min(5000, state.reconnectDelay * 2);
          setTimeout(requestSnapshot, delay);
        }
      };
    } catch (error) {
      setConnection('disconnected', error.status === 401 ? 'Токен не принят' : 'Нет связи');
      message.textContent = error.message;
      message.className = 'viz-message error';
      if (state.leaseTimer) clearInterval(state.leaseTimer);
      state.leaseTimer = 0;
      if (error.status === 401) { state.token = ''; state.observer = false; state.manual = false; state.leaseId = ''; try { localStorage.removeItem('stadmagic-team-token'); localStorage.removeItem('stadmagic-team-name'); window.dispatchEvent(new Event('stadmagic-profile-change')); } catch (_) {} }
      state.connecting = false;
      if (state.token || state.observer) {
        const delay = state.reconnectDelay;
        state.reconnectDelay = Math.min(5000, state.reconnectDelay * 2);
        setTimeout(requestSnapshot, delay);
      }
    }
  }

  function sendRealtimeCommand() {
    if (!state.manual || state.realtime?.readyState !== WebSocket.OPEN) return;
    // If the link falls behind, drop this update; the next interval sends the newest aim vector.
    if (state.realtime.bufferedAmount > 4096) return;
    const selected = byId(state.current, state.selectedId);
    if (!selected?.own || !alive(selected)) { disableManual(true); return; }
    advanceRoutePlan(selected, state.current);
    if (state.routeSegments.length && routeHasDeviated(selected, state.current)) {
      clearRoutePlan('Маршрут сбился — план сброшен. Ручное управление продолжается прицелом.');
    }
    const acceleration = activeRouteAcceleration(selected, state.current);
    if (acceleration) state.realtime.send(JSON.stringify({ type: 'commands', transports: [{ id: selected.id, acceleration }] }));
  }

  function renewLease(releaseLeaseId = '') {
    if (!state.token) return Promise.resolve();
    const headers = { 'Content-Type': 'application/json', 'X-Auth-Token': state.token };
    const body = releaseLeaseId ? { releaseLeaseId } : { carpetId: state.selectedId, leaseId: state.leaseId };
    return fetch('/api/visualizer/lease', { method: 'POST', headers, body: JSON.stringify(body) }).then(response => {
      if (response.status === 409 && state.manual) {
        message.textContent = 'Ручное управление уже включено в другой вкладке.';
        disableManual(false);
      }
      return response;
    });
  }

  function startLeaseHeartbeat() {
    if (!state.manual || state.observer || !state.leaseId) return;
    renewLease().catch(() => {});
    if (state.leaseTimer) clearInterval(state.leaseTimer);
    state.leaseTimer = setInterval(() => renewLease().catch(() => {}), 500);
  }

  function sendNeutralManualCommand() {
    const selected = byId(state.current, state.selectedId);
    if (!selected?.own || state.realtime?.readyState !== WebSocket.OPEN) return false;
    try {
      state.realtime.send(JSON.stringify({ type: 'commands', transports: [{ id: selected.id, acceleration: { x: 0, y: 0 } }] }));
      return true;
    } catch (_) {
      return false;
    }
  }

  function disableManual(release) {
    const leaseId = state.leaseId;
    if (release && leaseId) {
      // Physics keeps the last acceleration when a tick has no command.
      // Clear it before returning this carpet to the bot.
      sendNeutralManualCommand();
      renewLease(leaseId).catch(() => {});
    }
    state.manual = false;
    state.routeSegments = []; state.activeRouteSegment = 0; state.routeStartedAt = 0;
    message.textContent = '';
    state.trajectoryCacheKey = '';
    touchStick.classList.remove('active');
    state.stickVector = null;
    if (state.leaseTimer) clearInterval(state.leaseTimer);
    state.leaseTimer = 0;
    state.leaseId = '';
    manualButton.textContent = 'Ручное';
    manualButton.classList.remove('manual-on');
  }
  function enableManual() {
    const item = byId(state.current, state.selectedId);
    if (!item?.own || !alive(item)) return;
    state.manual = true;
    state.leaseId = newLeaseId();
    manualButton.textContent = 'Ручное: вкл.';
    manualButton.classList.add('manual-on');
    routeNotice('Прицел через монету: F / средняя кнопка мыши — закрепить, Z / правая — undo.');
    startLeaseHeartbeat();
  }

  function connect(token, observer) {
    const previousToken = state.token, previousLease = state.leaseId;
    if (previousToken && previousLease) sendNeutralManualCommand();
    disableManual(false);
    if (previousToken && previousLease) {
      fetch('/api/visualizer/lease', { method: 'POST', keepalive: true,
        headers: { 'Content-Type': 'application/json', 'X-Auth-Token': previousToken },
        body: JSON.stringify({ releaseLeaseId: previousLease }) }).catch(() => {});
    }
    state.token = token;
    state.observer = observer;
    state.previous = state.current = null;
    if (state.realtime) state.realtime.close();
    state.realtime = null;
    state.camera.initialized = false;
    state.camera.zoom = 1;
    state.autoFramePending = true;
    state.selectedId = '';
    state.follow = false;
    state.embeddedFollowId = '';
    state.embeddedNextFollowAt = 0; state.embeddedCoinFocusKey = '';
    followButton.textContent = '◎ Следить'; followButton.classList.remove('follow-on');
    message.textContent = observer ? 'Подключаюсь к публичному просмотру…' : 'Подключаюсь к твоему флоту…'; message.className = 'viz-message';
    setConnection('connecting', 'Подключение…');
    requestSnapshot();
  }
  const storedProfile = () => {
    try { return { token: localStorage.getItem('stadmagic-team-token') || '', name: localStorage.getItem('stadmagic-team-name') || '' }; }
    catch (_) { return { token: '', name: '' }; }
  };
  function syncArenaEntry() {
    const profile = storedProfile();
    const playing = Boolean(profile.token && state.token === profile.token && !state.observer);
    entryModeCard.dataset.mode = playing ? 'player' : 'observer';
    entryModeBadge.textContent = playing ? 'ИГРОК' : 'НАБЛЮДАТЕЛЬ';
    entryTitle.textContent = playing ? 'Игра команды' : 'Публичное наблюдение';
    entryName.textContent = playing ? (profile.name || 'Моя команда') : 'Публичный просмотр';
    entryStatus.textContent = playing
      ? (connectionStatus.classList.contains('connected') ? 'Команда подключена · флот на карте' : 'Подключаем команду к арене…')
      : 'Карта и рейтинг доступны без регистрации.';
    entryDescription.textContent = playing
      ? 'Выбери свой живой ковер, чтобы при желании включить ручное управление.'
      : 'Смотри за командами или переключись на игру своим флотом.';
    teamConnectButton.innerHTML = playing
      ? 'Следить <span aria-hidden="true">↗</span>'
      : 'Играть <span aria-hidden="true">↗</span>';
  }
  teamConnectButton.addEventListener('click', () => {
    const profile = storedProfile();
    const playing = Boolean(profile.token && state.token === profile.token && !state.observer);
    if (playing) {
      connect('', true);
      revealArenaCanvas();
      return;
    }
    if (profile.token) {
      if (state.token !== profile.token || state.observer) connect(profile.token, false);
      revealArenaCanvas();
      return;
    }
    document.querySelector('#nav-profile')?.click();
  });
  const handleProfileChange = () => {
    if (embeddedObserver) return;
    const profile = storedProfile();
    syncArenaEntry();
    if (profile.token) {
      if (profile.token !== state.token || state.observer) {
        connect(profile.token, false);
        revealArenaCanvas();
      }
    } else if (!state.observer || state.token) {
      connect('', true);
    }
  };
  window.addEventListener('stadmagic-profile-change', handleProfileChange);
  window.addEventListener('storage', event => {
    if (event.key === 'stadmagic-team-token' || event.key === 'stadmagic-team-name') handleProfileChange();
  });
  syncArenaEntry();
  select.addEventListener('change', () => selectCarpet(select.value));
  followButton.addEventListener('click', () => {
    setFollowing(!state.follow);
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
    return nativeFullscreenActive() || shell.classList.contains('fullscreen-fallback');
  }
  function nativeFullscreenActive() {
    return document.fullscreenElement === shell || document.webkitFullscreenElement === shell;
  }
  function syncFullscreenButton() {
    const active = fullscreenActive();
    const mobileViewport = matchMedia('(max-width:760px), (pointer:coarse)').matches;
    document.body.classList.toggle('arena-immersive-mobile', active && mobileViewport);
    fullscreenButton.textContent = active ? '⛶ Выйти' : '⛶';
    orientationButton.hidden = !active || !mobileViewport;
    if (!orientationButton.hidden) syncOrientationButton();
    else if (!active) { try { screen.orientation?.unlock?.(); } catch (_) {} }
  }
  function syncOrientationButton() {
    const landscape = matchMedia('(orientation:landscape)').matches;
    orientationButton.textContent = landscape ? '↕' : '↻';
    orientationButton.setAttribute('aria-label', landscape ? 'Переключить в портретный режим' : 'Переключить в альбомный режим');
    orientationButton.title = landscape ? 'Переключить экран в портретную ориентацию' : 'Переключить экран в альбомную ориентацию';
  }
  fullscreenButton.addEventListener('click', async () => {
    if (nativeFullscreenActive()) {
      try {
        if (document.exitFullscreen) await document.exitFullscreen();
        else if (document.webkitExitFullscreen) document.webkitExitFullscreen();
      } catch (_) { return; }
    } else if (shell.classList.contains('fullscreen-fallback')) {
      shell.classList.remove('fullscreen-fallback');
    } else {
      try {
        if (shell.requestFullscreen) await shell.requestFullscreen();
        else if (shell.webkitRequestFullscreen) shell.webkitRequestFullscreen();
        else shell.classList.add('fullscreen-fallback');
      } catch (_) { shell.classList.add('fullscreen-fallback'); }
    }
    syncFullscreenButton();
    setTimeout(resize, 100);
  });
  orientationButton.addEventListener('click', async () => {
    const target = matchMedia('(orientation:landscape)').matches ? 'portrait' : 'landscape';
    try {
      if (screen.orientation?.lock) await screen.orientation.lock(target);
      else throw new Error('orientation lock unavailable');
      message.textContent = target === 'landscape' ? 'Включён альбомный режим.' : 'Включён портретный режим.';
      message.className = 'viz-message';
    } catch (_) {
      orientationButton.textContent = target === 'landscape' ? '↻' : '↕';
      orientationButton.setAttribute('aria-label', 'Автоповорот недоступен. Поверни устройство вручную или продолжай в текущем режиме.');
      orientationButton.title = 'Автоповорот недоступен в этом браузере. Поверни устройство вручную или продолжай в текущем режиме.';
    }
    setTimeout(resize, 150);
  });
  screen.orientation?.addEventListener?.('change', () => { syncOrientationButton(); resize(); });
  matchMedia('(orientation:landscape)').addEventListener?.('change', () => { syncOrientationButton(); resize(); });
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
  let touchPlanTapAt = 0, touchPlanLongPressTimer = 0, touchPlanLongPressed = false;
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
    if (state.manual && event.pointerType === 'mouse' && (event.button === 1 || event.button === 2)) event.preventDefault();
    if (event.pointerType === 'touch' && state.manual) {
      clearTimeout(touchPlanLongPressTimer);
      touchPlanLongPressed = false;
      if (state.pointers.size === 0) touchPlanLongPressTimer = setTimeout(() => {
        touchPlanLongPressed = true; undoRouteSegment();
      }, 650);
    }
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
    if (event.pointerType === 'touch' && state.manual) {
      clearTimeout(touchPlanLongPressTimer);
      if (touchPlanLongPressed) { touchPlanLongPressed = false; touchPlanTapAt = 0; }
      else if (start && Math.hypot(point.x - start.x, point.y - start.y) < 8) {
        const now = performance.now();
        if (touchPlanTapAt && now - touchPlanTapAt < 360) { pinRouteSegment(); touchPlanTapAt = 0; }
        else touchPlanTapAt = now;
      }
      if (!state.pointers.size) { state.dragging = false; state.dragStart = null; }
      return;
    }
    if (state.manual && event.pointerType === 'mouse' && (event.button === 1 || event.button === 2)) {
      if (event.button === 1) pinRouteSegment(); else undoRouteSegment();
      if (!state.pointers.size) { state.dragging = false; state.dragStart = null; }
      return;
    }
    if (start && Math.hypot(point.x - start.x, point.y - start.y) < 8) {
      const hits = allCarpets(interpolatedState(performance.now()) || state.current).map(item => ({ item, screen: worldToScreen(item) }))
        .map(hit => ({ ...hit, distance: Math.hypot(hit.screen.x - point.x, hit.screen.y - point.y) }))
        .filter(hit => hit.distance < 18).sort((a, b) => a.distance - b.distance);
      if (hits[0]) selectCarpet(hits[0].item.id);
    }
    if (!state.pointers.size) { state.dragging = false; state.dragStart = null; }
  });
  canvas.addEventListener('pointercancel', event => { clearTimeout(touchPlanLongPressTimer); touchPlanLongPressed = false; state.pointers.delete(event.pointerId); state.dragging = false; state.gestureDistance = 0; });
  canvas.addEventListener('pointerleave', event => { if (event.pointerType !== 'touch' && !state.manual) state.pointerInside = false; });
  canvas.addEventListener('contextmenu', event => { if (state.manual) event.preventDefault(); });

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
    stickPointer = null; touchStick.classList.remove('active');
    if (!state.routeSegments.length) {
      state.stickVector = { x: 0, y: 0 };
      stickBase.style.setProperty('--stick-x', '0px'); stickBase.style.setProperty('--stick-y', '0px');
    }
  });

  window.addEventListener('keydown', event => {
    if (event.target instanceof HTMLInputElement || event.target instanceof HTMLSelectElement || event.target instanceof HTMLTextAreaElement) return;
    if (state.manual && event.code === 'KeyF') { event.preventDefault(); pinRouteSegment(); return; }
    if (state.manual && event.code === 'KeyZ') { event.preventDefault(); undoRouteSegment(); return; }
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
  const rememberedProfile = embeddedObserver ? { token: '', name: '' } : storedProfile();
  connect(rememberedProfile.token, embeddedObserver || !rememberedProfile.token);
  window.addEventListener('pagehide', () => {
    if (state.token && state.leaseId) {
      sendNeutralManualCommand();
      fetch('/api/visualizer/lease', { method: 'POST', keepalive: true,
        headers: { 'Content-Type': 'application/json', 'X-Auth-Token': state.token },
        body: JSON.stringify({ releaseLeaseId: state.leaseId }) }).catch(() => {});
    }
  });
})();
