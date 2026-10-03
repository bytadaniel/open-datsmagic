import json
import gzip
import tempfile
import unittest
import os
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch
import hub as hub_module

from hub import (
    HubState,
    Store,
    TeamRegistry,
    docs_html,
    arena_visualizer_html,
    home_html,
    leaderboard_html,
    worlds_html,
    load_world_catalog,
    load_world_configs,
    next_minute_boundary,
    pick_world,
    token_id,
    RequestHandler,
    start_runtime_arena,
)


class HubTests(unittest.TestCase):
    def test_runtime_api_receives_world_run_contract_with_internal_secret(self):
        class Response:
            def __enter__(self): return self
            def __exit__(self, *_args): return False
            def read(self): return b'{"status":"running","pid":4321}'

        with patch.object(hub_module, "ARENA_CONTROL_TOKEN", "runtime-secret"), \
             patch.object(hub_module, "RUN_SECONDS", 75), \
             patch("hub.urllib.request.urlopen", return_value=Response()) as upstream:
            arena_process = start_runtime_arena("quiet-harbor-01", "world_quiet-harbor-01_1_1", "a" * 32, "observer-secret")
        request = upstream.call_args.args[0]
        payload = json.loads(request.data)
        self.assertEqual(request.get_header("X-arena-control-token"), "runtime-secret")
        self.assertEqual(payload["world_id"], "quiet-harbor-01")
        self.assertEqual(payload["duration_sec"], 75)
        self.assertEqual(arena_process.pid, 4321)

    def test_hub_pages_use_live_layout_and_requested_leaderboard_labels(self):
        state = HubState.__new__(HubState)
        state.lock = __import__("threading").RLock()
        state.arena = {
            "status": "running", "world_number": None, "world_id": None,
            "world_name": None, "arena_name": "world-demo-1-1", "port": 8080,
            "run_id": None, "started_at": None, "ends_at": None,
            "next_start_at": None, "pid": None, "error": None,
        }
        home = home_html(state).decode("utf-8")
        leaderboard = leaderboard_html().decode("utf-8")
        self.assertIn("/static/hub.css", home)
        self.assertIn('href="/arena"', home)
        self.assertIn("setInterval(refreshHome,5000)", home)
        self.assertIn("setInterval(renderHomeCountdown,1000)", home)
        self.assertIn('id="home-podium-title"', home)
        self.assertIn('data-scope="current"', home)
        self.assertIn('data-scope="all"', home)
        self.assertIn("podiumScope='all'", home)
        self.assertIn('data-scope="all" aria-pressed="true">За всё время', home)
        self.assertIn("/api/leaderboard?", home)
        self.assertIn("home-top-teams", home)
        self.assertIn("podiumKilometers(value)", home)
        self.assertIn("пройдено, км", home)
        self.assertIn("/leaderboard?scope=all", home)
        self.assertIn("Пока нет команд в рейтинге", home)
        self.assertIn('data-scope=history', leaderboard)
        self.assertIn('data-run-id', leaderboard)
        self.assertIn('<div class="leaderboard-heading">', leaderboard)
        self.assertNotIn('<header class="leaderboard-heading">', leaderboard)
        self.assertIn('href="/leaderboard"', home)
        self.assertIn('href="/register"', home)
        self.assertIn('id="site-auth-dialog"', home)
        self.assertIn("stadmagic-team-token", home)
        self.assertIn("/api/teams/me", home)
        self.assertIn("Math.ceil((homeDeadline-Date.now())/1000)", home)
        self.assertIn("function normalizeWorld(world,index)", worlds_html().decode("utf-8"))
        self.assertIn("world?.display_name", worlds_html().decode("utf-8"))
        self.assertIn("Профиль мира", worlds_html().decode("utf-8"))
        self.assertIn("В каталоге пока нет доступных миров.", worlds_html().decode("utf-8"))
        self.assertIn("Ответ каталога имеет неверный формат", worlds_html().decode("utf-8"))
        self.assertIn("function normalizeWorld(world,index)", worlds_html().decode("utf-8"))
        self.assertNotIn("textContent=w.description||w.id", worlds_html().decode("utf-8"))
        self.assertLess(home.index('href="/register">Команды'), home.index('href="/leaderboard">Рейтинг'))
        self.assertIn("row.animate", arena_visualizer_html().decode("utf-8"))
        self.assertIn('class="button hero-primary" href="/register"', home)
        self.assertIn("Создать команду", home)
        self.assertLess(home.index('class="button hero-primary"'), home.index('class="hero-docs-link"'))
        self.assertLess(home.index('class="resource-link"'), home.index('class="resource-link resource-worlds"'))
        self.assertIn("StadMagic", home)
        hub_css = (Path(__file__).parent / "static" / "hub.css").read_text(encoding="utf-8")
        self.assertIn("--paper:#08111d", hub_css)
        self.assertIn("--space-6:32px", hub_css)
        self.assertIn(".site-header .nav-cta:hover", hub_css)
        self.assertIn(".home-podium", hub_css)
        self.assertIn(".podium-team:hover", hub_css)
        self.assertIn(".podium-scope:focus-visible", hub_css)
        self.assertIn("prefers-reduced-motion:reduce", hub_css)
        self.assertIn(".site-header .primary-nav .nav-arena", hub_css)
        self.assertIn(".page-ambient:after", hub_css)
        self.assertIn(".hero{position:relative", hub_css)
        self.assertIn(".leaderboard-table tbody tr.rank-1", hub_css)
        self.assertIn(".leaderboard-table tbody tr.rank-2", hub_css)
        self.assertIn(".leaderboard-table tbody tr.rank-3", hub_css)
        self.assertIn("details[open]>.disclosure-content", hub_css)
        self.assertNotIn("--paper:#191813", hub_css)
        self.assertIn('href="https://github.com/bytadaniel/dats-magic"', home)
        self.assertIn("Обо мне", home)
        self.assertLess(home.index("Источник вдохновения"), home.index("Обо мне"))
        self.assertIn("БЕССРОЧНЫЙ · СОРЕВНОВАТЕЛЬНЫЙ · ОНЛАЙН-ГЕЙМТОН", home)
        self.assertIn("бессрочный соревновательный онлайн-геймтон", home)
        self.assertIn('class="site-header"', home)
        self.assertIn('class="primary-nav"', home)
        self.assertIn('<main class=page-container>', home)
        self.assertIn('class="nav-arena" href="/arena"', home)
        self.assertIn('class="nav-cta nav-source" href="https://github.com/bytadaniel/dats-magic"', home)
        self.assertIn('aria-label="Исходный код StadMagic на GitHub"', home)
        self.assertNotIn('class="hero-gas"', home)
        self.assertIn('class="page-ambient"', home)
        self.assertIn("pointermove", home)
        self.assertIn("IntersectionObserver", home)
        self.assertIn("scroll-motion-ready", home)
        self.assertIn("Смотреть гонку", home)
        self.assertNotIn("Открыть API арены", home)
        self.assertNotIn("home-updated", home)
        self.assertIn("GitHub автора", home)
        self.assertIn("Node.js", home)
        self.assertIn('class="game-loop"', home)
        self.assertIn('id=home-players', home)
        self.assertNotIn('id=home-run', home)
        self.assertIn("Матч идёт", home)
        self.assertIn("/static/stadmagic-mark.svg", home)
        self.assertIn('rel="icon" type="image/svg+xml"', home)
        self.assertIn('href="https://gamethon.datsteam.dev/datsmagic"', home)
        self.assertIn("неофициальный проект, не связанный с Dats.Team", home)
        self.assertIn("я закрою серверы и доступ к игре", home)
        docs = docs_html().decode("utf-8")
        self.assertIn('href="/docs/api"', docs)
        self.assertIn('href="/docs/world"', docs)
        self.assertTrue((Path(__file__).parent.parent.parent / "docs/components/arena-hub/api.md").is_file())
        self.assertTrue((Path(__file__).parent.parent.parent / "docs/components/arena-hub/world-rules.md").is_file())
        for label in ("Золото", "Собрано золота", "Потеряно ковров от аварий", "Пройденное расстояние"):
            self.assertIn(label, leaderboard)
        self.assertNotIn("Золота на руках", leaderboard)
        self.assertNotIn("Всего собрано золота", leaderboard)

    def test_web_arena_visualizer_page_is_responsive_and_has_separate_watch_and_control(self):
        page = arena_visualizer_html().decode("utf-8")
        self.assertIn('id="arena-canvas"', page)
        self.assertIn('id="follow-toggle"', page)
        self.assertIn('id="manual-toggle"', page)
        self.assertIn('id="touch-stick"', page)
        self.assertIn('id="fullscreen-toggle"', page)
        self.assertIn('id="arena-team-connect"', page)
        self.assertIn('id="arena-entry-name"', page)
        self.assertIn('class="disclosure-content"', page)
        visualizer_script = (Path(__file__).parent / "static" / "arena-visualizer.js").read_text(encoding="utf-8")
        visualizer_css = (Path(__file__).parent / "static" / "arena-visualizer.css").read_text(encoding="utf-8")
        self.assertIn("revealArenaCanvas()", visualizer_script)
        self.assertIn('id="arena-description"', page)
        self.assertIn('id="arena-countdown"', page)
        self.assertIn('class="mode-countdown"', page)
        self.assertIn('class="mode-side-actions"', page)
        self.assertLess(page.index('id="arena-team-connect"'), page.index('class="mode-countdown"'))
        self.assertNotIn('class="context-time"', page)
        self.assertNotIn('id="team-legend"', page)
        self.assertIn('<div id="fps-label" class="viz-fps" aria-live="off">— FPS</div>', page)
        self.assertIn('class="canvas-wrap"><canvas id="arena-canvas"', page)
        self.assertIn('class="viz-toolbar-actions"', page)
        self.assertIn('class="camera-tools"', page)
        self.assertNotIn('Колесо / щипок — зум', page)
        self.assertIn('class="viz-help-disclosure"', page)
        self.assertNotIn('<details class="viz-help-disclosure" open>', page)
        self.assertLess(page.index('class="visualizer-layout"'), page.index('class="viz-help-disclosure"'))
        self.assertLess(page.index('class="viz-help-disclosure"'), page.index('id="arena-leaderboard"'))
        self.assertIn('id="arena-world-settings"', page)
        self.assertIn('id="trajectory-note"', page)
        self.assertNotIn('id="selected-stats"', page)
        self.assertNotIn('id="selected-title"', page)
        self.assertIn('class="v-trajectory"', page)
        self.assertIn("Будущий путь · 20 с", page)
        self.assertIn("Фактический путь · 10 с", page)
        self.assertIn('id="arena-vote-form"', page)
        self.assertIn('class="arena-voting" open', page)
        self.assertIn('id="arena-vote-next"', page)
        self.assertIn('class="vote-form-heading"', page)
        self.assertIn("MutationObserver(refreshSummary)", page)
        self.assertIn("new Intl.PluralRules('ru-RU')", page)
        self.assertIn('id="arena-world-traits"', page)
        self.assertIn('class="arena-mode-card"', page)
        self.assertIn('id="arena-mode-badge"', page)
        self.assertIn('id="arena-world-profile"', page)
        self.assertIn('id="arena-bounties-note"', page)
        self.assertIn('id="arena-anomalies-note"', page)
        for rule_id in ("world-carpet-count", "world-carpet-speed", "world-carpet-acceleration", "world-friction", "world-coin-spawn", "world-anomaly-spawn", "world-carpet-respawn"):
            self.assertIn(f'id="{rule_id}"', page)
        self.assertIn("phaseBadge.dataset.phase = active.status || 'idle'", visualizer_script)
        self.assertIn("gauge.setAttribute('role', 'meter')", visualizer_script)
        self.assertIn("meterValue: maxCarpetSpeed > 0 ? speedMax / maxCarpetSpeed : 0", visualizer_script)
        self.assertIn("meterValue: influenceShare * 100", visualizer_script)
        self.assertIn("meterValue: coreShare * 100", visualizer_script)
        self.assertIn("ordinaryForceRatio = maxCarpetAcceleration > 0 ? regularForceMax / maxCarpetAcceleration : 0", visualizer_script)
        self.assertIn("peak: hasForceOutliers ? outlierForceMax : null", visualizer_script)
        self.assertNotIn("Дуга — обычно", visualizer_script)
        self.assertNotIn("3× ускорение", visualizer_script)
        self.assertIn("element.textContent = enabled ? '✓' : '✕'", visualizer_script)
        self.assertIn('Монеты появляются</small>', page)
        self.assertIn('Аномалии появляются</small>', page)
        self.assertIn("goldSymbol.className = 'gold-symbol'", visualizer_script)
        self.assertIn("document.createTextNode('Золото на карте')", visualizer_script)
        self.assertIn("createElementNS('http://www.w3.org/2000/svg', 'svg')", visualizer_script)
        self.assertIn("arenaAreaM2 = Math.max(1, arenaWidth * arenaHeight)", visualizer_script)
        self.assertIn("Number(value || 0) / arenaAreaM2 * 1_000_000", visualizer_script)
        self.assertIn("${worldNumber(arenaWidth)} × ${worldNumber(arenaHeight)}", visualizer_script)
        self.assertIn("${densityPerKm2ForDisplay(config.bounty_quota)} монет/км²", visualizer_script)
        self.assertIn("Площадь ${worldArea(areaKm2ForDisplay)} км²", visualizer_script)
        self.assertIn("value: speedMax, unit: 'м/с'", visualizer_script)
        self.assertIn("value: effectMax, unit: 'м'", visualizer_script)
        self.assertIn("value: coreMax, unit: 'м'", visualizer_script)
        self.assertIn("rule('world-carpet-speed', `${worldNumber(maxCarpetSpeed)} м/с`)", visualizer_script)
        self.assertIn("rule('world-carpet-acceleration', `${worldNumber(maxCarpetAcceleration)} м/с²`)", visualizer_script)
        self.assertIn("rule('world-friction'", visualizer_script)
        self.assertIn("frictionPercent.toLocaleString('ru-RU', { maximumFractionDigits: 1 })", visualizer_script)
        self.assertIn("ring(32, profileItem.minMeterValue", visualizer_script)
        self.assertIn("arena-threat-range-min", visualizer_script)
        self.assertIn("Ценность золота", visualizer_script)
        self.assertIn("Сохранение скорости", visualizer_script)
        self.assertIn("frictionCoefficient >= .95 ? 'calm' : frictionCoefficient >= .85 ? 'steady' : frictionCoefficient >= .6 ? 'high' : 'extreme'", visualizer_script)
        self.assertIn("goldDensityPerKm2 = Number(config.bounty_quota || 0) / arenaAreaM2 * 1_000_000", visualizer_script)
        self.assertIn("label: 'Плотность золота'", visualizer_script)
        self.assertIn("Шанс редкой силы", visualizer_script)
        self.assertIn("stateName === 'connected' ? 'Подключено' : stateName === 'connecting' ? 'Подключаемся' : 'Не подключено'", visualizer_script)
        self.assertIn("entryTitle.textContent = playing ? 'Игра команды' : 'Публичное наблюдение'", visualizer_script)
        self.assertIn("booleanRule('world-coin-spawn'", visualizer_script)
        self.assertIn("booleanRule('world-carpet-respawn'", visualizer_script)
        self.assertNotIn('id="vote-form"', worlds_html().decode("utf-8"))
        self.assertIn('class="context-fact-label"', page)
        self.assertIn('class="countdown-calm"', page)
        self.assertIn('class="viz-help-grid"', page)
        self.assertIn("КОРОТКО ОБ УПРАВЛЕНИИ", page)
        self.assertIn('id="arena-leaderboard"', page)
        self.assertIn('id="viz-gold-summary"', page)
        self.assertIn('class="viz-gold-summary without-own"', page)
        self.assertIn('class="viz-gold-leaders" data-count="0"', page)
        for place in (1, 2, 3):
            self.assertIn(f'data-place="{place}"', page)
        self.assertIn("Место свободно", page)
        self.assertNotIn('id="viz-gold-summary" class="viz-gold-summary" hidden', page)
        self.assertIn('id="viz-own-summary" hidden', page)
        self.assertIn("viz-gold-leaders", page)
        self.assertIn("} ✦`", visualizer_script)
        self.assertNotIn("Можно взять из поля подключения выше", page)
        self.assertIn("Войди через кнопку профиля", page)
        self.assertLess(page.index('class="arena-context"'), page.index('class="visualizer-top card"'))
        self.assertIn('<h1>Арена</h1>', page)
        self.assertIn('href="/static/arena-visualizer.css?v=8.5"', page)
        self.assertIn(".viz-help-legend .vector-legend{grid-template-columns:repeat(3,minmax(0,1fr))", visualizer_css)
        self.assertIn(".viz-help-disclosure,.visualizer-shell .world-settings{margin:0 0 14px;padding:0 16px", visualizer_css)
        self.assertIn(".viz-help-disclosure .viz-help{gap:22px;padding:22px 0 18px}", visualizer_css)
        self.assertIn(".visualizer-shell{width:100%;max-width:none}", visualizer_css)
        self.assertIn(".visualizer-stage,.visualizer-stage:hover{width:100%;box-sizing:border-box;padding:0}", visualizer_css)
        self.assertIn('.viz-help-heading{display:grid;justify-items:start;', visualizer_css)
        self.assertIn('.viz-help-item b{font-size:.9rem;', visualizer_css)
        self.assertIn('.help-icon{width:34px;height:34px;', visualizer_css)
        self.assertIn('.arena-page-heading{position:static;', visualizer_css)
        self.assertIn('background:transparent;box-shadow:none;backdrop-filter:none', visualizer_css)
        self.assertIn('id="arena-connection-status"', page)
        self.assertLess(page.index('class="visualizer-top card"'), page.index('class="visualizer-layout"'))
        self.assertLess(page.index('class="visualizer-layout"'), page.index('id="arena-leaderboard"'))
        self.assertLess(page.index('class="viz-help-disclosure"'), page.index('class="world-settings"'))
        self.assertLess(page.index('class="world-settings"'), page.index('id="arena-leaderboard"'))
        self.assertNotIn('class="card selected-panel"', page)
        self.assertIn("Карта и ручной режим", page)
        self.assertIn('href="/static/arena-visualizer.css?v=8.5"', page)
        self.assertIn('<section class="visualizer-stage">', page)
        self.assertNotIn('<section class="visualizer-stage card">', page)
        self.assertIn("Публичный просмотр", page)
        self.assertIn(">Играть <span aria-hidden=\"true\">↗</span>", page)
        self.assertNotIn('id="viz-token"', page)
        self.assertIn("stadmagic-profile-change", visualizer_script)
        self.assertIn("connect(rememberedProfile.token, !rememberedProfile.token)", visualizer_script)
        self.assertIn("ownSummary.hidden = !state.token", visualizer_script)
        self.assertIn("refreshArenaRanking(data.active || {})", visualizer_script)
        self.assertIn("leaders.dataset.count = String(Math.min(3, teams.length))", visualizer_script)
        self.assertIn("[1, 2, 3].map(place =>", visualizer_script)
        self.assertIn("const formatCompactGold = value =>", visualizer_script)
        self.assertIn("const formatCompactCount = value =>", visualizer_script)
        self.assertIn("formatCompactCount(lost)", visualizer_script)
        self.assertIn("amount.toFixed(3)", visualizer_script)
        self.assertIn("gold.textContent = team ? formatCompactGold(team.gold) : '—'", visualizer_script)
        self.assertIn("deaths.append(deathIcon(), document.createTextNode(lost === null ? '—' : formatCompactCount(lost)))", visualizer_script)
        self.assertIn("icon.classList.add('viz-death-icon')", visualizer_script)
        self.assertIn("#viz-own-deaths", visualizer_script)
        self.assertIn("ownRank.textContent = ownTeam ? `#${ownTeam.rank || teams.indexOf(ownTeam) + 1}` : '—'", visualizer_script)
        self.assertIn("ownSummary.replaceChildren(ownRank, ownName, ownMetrics)", visualizer_script)
        self.assertIn("const ownDeaths = ownSummary.querySelector('.viz-own-deaths')", visualizer_script)
        self.assertIn("ownDeathCount.textContent = ownTeam ?", visualizer_script)
        self.assertIn("' viz-gold-placeholder'", visualizer_script)
        self.assertIn("const MAX_CAMERA_ZOOM = 24", visualizer_script)
        self.assertIn("state.camera.zoom = MAX_CAMERA_ZOOM", visualizer_script)
        self.assertIn("state.autoFramePending", visualizer_script)
        self.assertIn("setFollowing(true)", visualizer_script)
        self.assertIn("НАБЛЮДАТЕЛЬ", visualizer_script)
        self.assertIn("ИГРОК", visualizer_script)
        self.assertIn("? 'Следить <span aria-hidden=\"true\">↗</span>'", visualizer_script)
        self.assertIn(": 'Играть <span aria-hidden=\"true\">↗</span>'", visualizer_script)
        self.assertIn("window.addEventListener('storage'", visualizer_script)
        visualizer_css = (Path(__file__).parent / "static" / "arena-visualizer.css").read_text(encoding="utf-8")
        visualizer_script = (Path(__file__).parent / "static" / "arena-visualizer.js").read_text(encoding="utf-8")
        self.assertIn(".arena-ranking-table-inner th{background:#17283a!important", visualizer_css)
        self.assertIn(".team-diamond", visualizer_css)
        self.assertIn(".arena-vote-row{box-sizing:border-box;width:100%", visualizer_css)
        self.assertIn(".arena-voting .arena-vote-list{grid-column:1;grid-row:1", visualizer_css)
        self.assertIn(".arena-voting .arena-vote-layout form{grid-column:2;grid-row:1", visualizer_css)
        self.assertIn(".arena-mode-card{grid-template-columns:48px minmax(0,1fr) auto", visualizer_css)
        self.assertIn(".mode-side-actions{display:grid;grid-template-columns:repeat(2,minmax(136px,1fr));align-items:stretch;gap:22px", visualizer_css)
        self.assertIn(".mode-side-actions .mode-countdown::before{position:absolute;top:12px;bottom:12px;left:-12px", visualizer_css)
        self.assertIn(".mode-countdown strong{color:#f2d17e;font-size:1.28rem", visualizer_css)
        self.assertIn('.arena-mode-card[data-mode="observer"] .mode-switch.secondary{border-color:#ffd27a55', visualizer_css)
        self.assertIn('.arena-mode-card[data-mode="player"] .mode-switch.secondary{border-color:#ffffff1b', visualizer_css)
        self.assertIn(".viz-gold-value", visualizer_css)
        self.assertIn(".viz-gold-own{grid-template-columns:46px minmax(0,1fr);grid-template-rows:auto auto", visualizer_css)
        self.assertIn(".viz-own-metrics{box-sizing:border-box;grid-column:2;grid-row:2;display:flex", visualizer_css)
        self.assertIn(".viz-gold-leaders{grid-template-columns:repeat(3,minmax(0,1fr));align-items:stretch;justify-content:start", visualizer_css)
        self.assertIn(".viz-gold-leader{display:grid;grid-template-columns:46px minmax(0,1fr);grid-template-rows:auto auto", visualizer_css)
        self.assertIn(".viz-gold-rank{grid-column:1;grid-row:1/3;display:grid;place-items:center", visualizer_css)
        self.assertIn(".viz-gold-metrics{box-sizing:border-box;grid-column:2;grid-row:2;display:flex;align-items:center;justify-content:space-between", visualizer_css)
        self.assertIn("padding-right:12px", visualizer_css)
        self.assertIn(".viz-death-icon{width:14px;height:14px", visualizer_css)
        self.assertIn("font-size:.9rem;font-weight:850", visualizer_css)
        self.assertIn(".viz-gold-placeholder{border-style:dashed", visualizer_css)
        self.assertIn("opacity:.36", visualizer_css)
        self.assertIn(".arena-ranking-table-inner tbody tr.rank-1", visualizer_css)
        self.assertIn(".arena-ranking-table-inner tbody tr.rank-2", visualizer_css)
        self.assertIn(".arena-ranking-table-inner tbody tr.rank-3", visualizer_css)
        self.assertIn(".arena-ranking-table-inner th:nth-child(n+3),.arena-ranking-table-inner td:nth-child(n+3){text-align:center!important}", visualizer_css)
        self.assertIn(".ranking-gold-symbol", visualizer_css)
        self.assertIn(".arena-threat-item", visualizer_css)
        self.assertIn('.gold-symbol{display:inline-grid;width:18px;height:18px', visualizer_css)
        self.assertIn(".arena-context-facts{grid-template-columns:repeat(3,minmax(0,1fr));gap:0}", visualizer_css)
        self.assertIn(".arena-threat-gauge-progress{stroke:var(--meter-color)", visualizer_css)
        self.assertIn('.arena-threat-item[data-level="extreme"]{--meter-color:#f0787e}', visualizer_css)
        self.assertIn(".arena-threat-peak-marker{fill:#fff;stroke:#111c29", visualizer_css)
        self.assertIn(".arena-world-rules{display:grid;grid-template-columns:repeat(7,minmax(0,1fr))", visualizer_css)
        self.assertIn(".arena-phase{padding:0;border:0;border-radius:0;background:none", visualizer_css)
        self.assertIn(".mode-switch.secondary{align-self:center;justify-self:end", visualizer_css)
        self.assertIn("min-height:60px;padding:11px 18px", visualizer_css)
        self.assertIn("return `${mm}:${ss}`", visualizer_script)
        self.assertIn("'ДО СТАРТА АРЕНЫ' : 'ДО СМЕНЫ МИРА'", visualizer_script)
        self.assertIn(".entry-observer,.entry-team{background:linear-gradient(135deg,#14263b,#0b1723)", visualizer_css)
        self.assertIn("ctx.rotate(heading + Math.PI / 4)", visualizer_script)
        self.assertIn("ctx.fillRect(-side / 2, -side / 2, side, side)", visualizer_script)
        self.assertIn("function predictSelectedTrajectory(snapshot)", visualizer_script)
        self.assertIn("function anomalyForceAt(position, anomalies)", visualizer_script)
        self.assertIn("x: item.x + finite(item.velocity?.x) * dt", visualizer_script)
        self.assertIn("function drawSelectedTrajectory(ctx, snapshot)", visualizer_script)
        self.assertIn("horizon = 20", visualizer_script)
        self.assertIn("Math.max(state.physics.dt, 0.5)", visualizer_script)
        self.assertIn("function drawPreviousPath(ctx)", visualizer_script)
        self.assertIn("10_000", visualizer_script)
        self.assertIn(".manual-on{background:#e5b950!important", visualizer_css)
        self.assertIn(".world-settings dt,.world-settings dd{font-size:.82rem", visualizer_css)
        self.assertNotIn("Что на карте? Краткая легенда", page)
        self.assertNotIn('<details class="map-guide"', page)
        for legend_text in ("золотая монета — собрать золото", "синяя область — отталкивание", "красная область — притяжение", "залитое ядро аномалии"):
            self.assertIn(legend_text, page)
        self.assertNotIn("Просто посмотреть", page)
        self.assertNotIn("Играть своей командой", visualizer_script)
        self.assertIn("arena-visualizer.js", page)
        self.assertIn("Токен команды", page)
        self.assertTrue((Path(__file__).parent / "static" / "arena-visualizer.css").is_file())
        visualizer_css = (Path(__file__).parent / "static" / "arena-visualizer.css").read_text(encoding="utf-8")
        self.assertIn("#arena-leaderboard{width:100%", visualizer_css)
        self.assertIn(".arena-ranking-table-inner{display:table;width:100%", visualizer_css)
        self.assertIn("grid-template-columns:repeat(auto-fit,minmax(min(100%,260px),1fr))", visualizer_css)
        script_path = Path(__file__).parent / "static" / "arena-visualizer.js"
        self.assertTrue(script_path.is_file())
        script = script_path.read_text(encoding="utf-8")
        self.assertIn("function drawCoins(ctx, snapshot)", script)
        self.assertIn("worldRadius * factor", script)
        self.assertIn("headers['X-Auth-Token'] = state.token", script)
        self.assertIn("/api/visualizer/ticket", script)
        self.assertIn("new WebSocket(data.websocketUrl", script)
        self.assertIn("setInterval(sendRealtimeCommand, 100)", script)
        self.assertIn("function sendNeutralManualCommand()", script)
        self.assertIn("acceleration: { x: 0, y: 0 }", script)
        self.assertIn("sendNeutralManualCommand();\n      renewLease(leaseId)", script)
        self.assertIn("src=\"/static/arena-visualizer.js?v=8.5\"", page)
        self.assertNotIn("setTimeout(requestSnapshot, 200)", script)
        self.assertNotIn("body.token", script)
        self.assertNotIn("navigator.sendBeacon", script)
        self.assertNotIn("offscreen", script)
        self.assertIn('enemyTeams: packet.enemyTeams || []', script)
        self.assertIn('worldToScreen({ x: size.x, y: 0 })', script)
        self.assertIn('teamColor(carpet.teamId)', script)
        self.assertIn("async function refreshArenaRanking(active)", script)
        self.assertIn("async function refreshArenaContext()", script)

    def test_arena_countdown_tracks_end_while_running_and_next_start_during_cooldown(self):
        import threading
        import time

        state = HubState.__new__(HubState)
        state.lock = threading.RLock()
        now = time.time()
        state.arena = {
            "status": "running", "world_number": 1, "world_id": "world-a",
            "world_name": "Тест", "arena_name": "run-a", "port": 8080,
            "run_id": "run-a", "started_at": now - 30, "ends_at": now + 90,
            "next_start_at": None, "pid": 42, "error": None,
        }
        self.assertGreaterEqual(state.current_arena()["seconds_remaining"], 88)
        state.arena.update(status="cooldown", ends_at=now - 1, next_start_at=now + 45)
        remaining = state.current_arena()["seconds_remaining"]
        self.assertGreaterEqual(remaining, 43)
        self.assertLessEqual(remaining, 45)

    def test_visualizer_proxy_authenticates_and_uses_short_manual_lease(self):
        class HandlerStub:
            def __init__(self, registry, headers=None):
                self.state = SimpleNamespace(registry=registry)
                self.headers = headers or {}
                self.responses = []

            def send_bytes(self, status, payload, content_type, content_encoding=None):
                self.responses.append((status, payload, content_type, content_encoding))

            def send_json(self, status, payload):
                self.responses.append((status, payload))

        class UpstreamResponse:
            status = 200
            headers = {"Content-Type": "application/json", "Content-Encoding": "gzip"}

            def __enter__(self): return self
            def __exit__(self, *args): return False
            def read(self): return gzip.compress(b'{"transports":[]}')

        with tempfile.TemporaryDirectory() as directory:
            registry = TeamRegistry(Path(directory) / "registry.json")
            token = "registered-visualizer-token"
            team = registry.register(token, "Visual Team")
            lease_path = Path(directory) / "manual.json"
            handler = HandlerStub(registry, {"X-Auth-Token": token})
            command = {"id": f"{team['team_id']}_0", "acceleration": {"x": 12, "y": -3}}
            body = {"transports": [command], "manualCarpetId": command["id"], "leaseId": "browser-lease"}
            with patch.dict(os.environ, {"DATS_MANUAL_CONTROL_FILE": str(lease_path)}), \
                 patch("urllib.request.urlopen", return_value=UpstreamResponse()) as upstream:
                RequestHandler.handle_visualizer_move(handler, body)
                sent_request = upstream.call_args.args[0]
                self.assertEqual(sent_request.get_header("X-auth-token"), token)
                self.assertEqual(sent_request.get_header("Accept-encoding"), "gzip")
                self.assertNotIn(token, sent_request.full_url)
                self.assertEqual(handler.responses[0][0], 200)
                self.assertEqual(handler.responses[0][1], b'{"transports":[]}')
                self.assertIsNone(handler.responses[0][3])
                lease = json.loads(lease_path.read_text(encoding="utf-8"))
                self.assertEqual(lease["carpetId"], command["id"])
                self.assertEqual(lease["leaseId"], "browser-lease")
                self.assertLessEqual(lease["expiresAtUnixMs"], __import__("time").time() * 1000 + 1000)
                competing = HandlerStub(registry, {"X-Auth-Token": token})
                RequestHandler.handle_visualizer_move(competing, {**body, "leaseId": "other-browser"})
                self.assertEqual(competing.responses[0][0], 409)
                self.assertEqual(lease_path.read_text(encoding="utf-8"), json.dumps(lease))

            unauthorized = HandlerStub(registry, {"X-Auth-Token": "unknown"})
            with patch("urllib.request.urlopen") as upstream:
                RequestHandler.handle_visualizer_move(unauthorized, {"transports": []})
                self.assertEqual(unauthorized.responses[0][0], 401)
                upstream.assert_not_called()

            legacy_body_token = HandlerStub(registry)
            with patch("urllib.request.urlopen") as upstream:
                with self.assertRaisesRegex(ValueError, "X-Auth-Token header"):
                    RequestHandler.handle_visualizer_move(legacy_body_token, {"token": token, "transports": []})
                upstream.assert_not_called()

            observer = HandlerStub(registry)
            observer.state.observer_token = "internal-observer-token"
            with patch("urllib.request.urlopen", return_value=UpstreamResponse()) as upstream:
                RequestHandler.handle_visualizer_move(observer, {"transports": []})
                observer_request = upstream.call_args.args[0]
                self.assertEqual(observer_request.get_header("X-auth-token"), "internal-observer-token")
                self.assertEqual(observer.responses[0][0], 200)

            read_only = HandlerStub(registry)
            read_only.state.observer_token = "internal-observer-token"
            with patch("urllib.request.urlopen") as upstream:
                RequestHandler.handle_visualizer_move(read_only, {"transports": [{"id": "any", "acceleration": {"x": 1, "y": 0}}]})
                self.assertEqual(read_only.responses[0][0], 403)
                upstream.assert_not_called()

    def test_visualizer_realtime_ticket_is_short_lived_single_use_and_bound_to_active_run(self):
        import threading

        class HandlerStub:
            def __init__(self, state, headers=None):
                self.state = state
                self.headers = headers or {}
                self.responses = []
            def send_json(self, status, payload): self.responses.append((status, payload))

        with tempfile.TemporaryDirectory() as directory:
            registry = TeamRegistry(Path(directory) / "registry.json")
            team = registry.register("private-token", "Realtime Team")
            state = SimpleNamespace(
                registry=registry, lock=threading.RLock(), realtime_tickets={}, control_token="internal-secret",
                arena={"status":"running", "run_id":"run-7"},
                current_arena=lambda: {"status":"running", "run_id":"run-7", "url":"https://game.example"},
            )
            player = HandlerStub(state, {"X-Auth-Token":"private-token"})
            RequestHandler.handle_visualizer_ticket(player, {})
            status, issued = player.responses[0]
            self.assertEqual(status, 200)
            self.assertEqual(issued["mode"], "player")
            self.assertEqual(issued["websocketUrl"], "wss://game.example/stream/visualizer")
            self.assertNotIn("private-token", json.dumps(issued))

            consume = HandlerStub(state, {"X-Arena-Control-Token":"internal-secret"})
            RequestHandler.handle_consume_visualizer_ticket(consume, {"ticket":issued["ticket"]})
            self.assertEqual(consume.responses[0], (200, {
                "player_id":team["team_id"], "name":"Realtime Team", "mode":"player", "run_id":"run-7"
            }))
            replay = HandlerStub(state, {"X-Arena-Control-Token":"internal-secret"})
            RequestHandler.handle_consume_visualizer_ticket(replay, {"ticket":issued["ticket"]})
            self.assertEqual(replay.responses[0][0], 401)

            invalid = HandlerStub(state, {"X-Auth-Token":"unknown"})
            RequestHandler.handle_visualizer_ticket(invalid, {})
            self.assertEqual(invalid.responses[0][0], 401)

            observer = HandlerStub(state)
            RequestHandler.handle_visualizer_ticket(observer, {})
            self.assertEqual(observer.responses[0][1]["mode"], "observer")

    def test_visualizer_lease_heartbeat_is_small_owner_scoped_and_releasable(self):
        class HandlerStub:
            def __init__(self, registry, headers):
                self.state = SimpleNamespace(registry=registry)
                self.headers = headers
                self.responses = []
            def send_json(self, status, payload): self.responses.append((status, payload))

        with tempfile.TemporaryDirectory() as directory:
            registry = TeamRegistry(Path(directory) / "registry.json")
            token = "lease-team-token"
            team = registry.register(token, "Lease Team")
            path = Path(directory) / "manual.json"
            handler = HandlerStub(registry, {"X-Auth-Token": token})
            body = {"carpetId":f"{team['team_id']}_2", "leaseId":"lease-a"}
            with patch.dict(os.environ, {"DATS_MANUAL_CONTROL_FILE":str(path)}):
                RequestHandler.handle_visualizer_lease(handler, body)
                lease = json.loads(path.read_text(encoding="utf-8"))
                self.assertEqual(handler.responses[0][0], 200)
                self.assertEqual(lease["carpetId"], body["carpetId"])
                self.assertLessEqual(lease["expiresAtUnixMs"], __import__("time").time() * 1000 + 1500)
                RequestHandler.handle_visualizer_lease(handler, {"releaseLeaseId":"lease-a"})
                self.assertFalse(path.exists())

    def test_world_catalog_and_no_immediate_repeat(self):
        worlds = load_world_catalog()
        self.assertGreaterEqual(len(worlds), 1)
        selected = pick_world(worlds, worlds[0]["id"])
        self.assertNotEqual(selected["id"], worlds[0]["id"])
        self.assertEqual(pick_world([worlds[0]], worlds[0]["id"])["id"], worlds[0]["id"])

    def test_world_catalog_size_comes_from_json(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "worlds.json"
            path.write_text(json.dumps({"worlds": [
                {"id": "a", "name": "A"},
                {"id": "b", "name": "B"},
            ]}), encoding="utf-8")
            self.assertEqual(len(load_world_catalog(path)), 2)

    def test_world_configs_merge_runtime_values_with_catalog_metadata(self):
        catalog = [
            {"id": "a", "name": "Readable A", "description": "World description", "config": {"bounty_quota": 10}},
        ]
        runtime_result = SimpleNamespace(stdout=json.dumps([
            {"id": "a", "config": {"bounty_quota": 20}},
        ]))
        with patch.object(hub_module, "ARENA_LIFECYCLE_MODE", "process"), \
             patch.object(hub_module.subprocess, "run", return_value=runtime_result):
            worlds = load_world_configs(catalog)
        self.assertEqual(worlds, [{
            "id": "a", "name": "Readable A", "description": "World description",
            "world_number": 1, "config": {"bounty_quota": 20},
        }])

    def test_next_arena_starts_at_minute_boundary(self):
        self.assertEqual(next_minute_boundary(1200.0), 1200.0)
        self.assertEqual(next_minute_boundary(1200.1), 1260.0)
        self.assertEqual(next_minute_boundary(1259.9), 1260.0)

    def test_registry_upserts_name_but_only_returns_public_identity(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "registry.json"
            registry = TeamRegistry(path)
            first = registry.register("private-secret", "First Name")
            second = registry.register("private-secret", "Renamed")
            self.assertEqual(first["team_id"], token_id("private-secret"))
            self.assertEqual(second["name"], "Renamed")
            self.assertNotIn("token", second)
            self.assertEqual(registry.names()[first["team_id"]], "Renamed")
            registry.register("another-private-secret", "Another Team")
            public_teams = registry.public_teams()
            self.assertEqual([team["name"] for team in public_teams], ["Another Team", "Renamed"])
            self.assertNotIn("token", json.dumps(public_teams))
            self.assertNotIn(first["team_id"], json.dumps(public_teams))
            self.assertEqual(registry.resolve_token("private-secret"), first["team_id"])
            self.assertIsNone(registry.resolve_token("not-registered"))
            self.assertIn("private-secret", path.read_text(encoding="utf-8"))

    def test_team_creation_issues_secret_and_names_are_unique_case_insensitively(self):
        with tempfile.TemporaryDirectory() as directory:
            registry = TeamRegistry(Path(directory) / "registry.json")
            created = registry.create("New Team")
            self.assertEqual(created["name"], "New Team")
            self.assertGreaterEqual(len(created["token"]), 32)
            self.assertEqual(registry.resolve_token(created["token"]), created["team_id"])
            with self.assertRaisesRegex(ValueError, "already taken"):
                registry.create("new team")
            with self.assertRaisesRegex(ValueError, "already taken"):
                registry.register("other-token", "NEW TEAM")

    def test_registration_form_requests_name_and_explains_one_time_token(self):
        from hub import register_html

        page = register_html().decode("utf-8")
        self.assertIn('JSON.stringify({name:nameInput.value})', page)
        self.assertIn("Токен сохранён в этом браузере", page)
        self.assertIn('id="generate-team-name"', page)
        self.assertIn("function generateName()", page)
        self.assertIn("Сгенерировать другое имя", page)
        self.assertIn("className='token-value'", page)
        self.assertIn("className='token-row'", page)
        self.assertIn('id=registered-teams', page)
        self.assertIn("Ваша команда", page)
        self.assertIn("/api/teams", page)

    def test_public_team_catalog_exposes_only_names(self):
        with tempfile.TemporaryDirectory() as directory:
            registry = TeamRegistry(Path(directory) / "registry.json")
            registry.register("never-public-token", "Amber Crew")

            class HandlerStub:
                path = "/api/teams"
                state = SimpleNamespace(registry=registry)

                def send_json(self, status, payload):
                    self.response = (status, payload)

                def log_error(self, *_args):
                    self.fail("unexpected handler error")

            handler = HandlerStub()
            RequestHandler.do_GET(handler)
            status, payload = handler.response
            self.assertEqual(status, 200)
            self.assertEqual(payload, {"teams": [{"name": "Amber Crew"}]})
            self.assertNotIn("never-public-token", json.dumps(payload))

    def test_team_identity_endpoint_validates_header_without_echoing_token(self):
        with tempfile.TemporaryDirectory() as directory:
            registry = TeamRegistry(Path(directory) / "registry.json")
            token = "browser-profile-secret"
            registry.register(token, "Amber Crew")

            class HandlerStub:
                path = "/api/teams/me"
                headers = {"X-Auth-Token": token}
                state = SimpleNamespace(registry=registry)

                def send_json(self, status, payload):
                    self.response = (status, payload)

                def log_error(self, *_args):
                    self.fail("unexpected handler error")

            handler = HandlerStub()
            RequestHandler.do_GET(handler)
            self.assertEqual(handler.response, (200, {"name": "Amber Crew"}))
            self.assertNotIn(token, json.dumps(handler.response))
            handler.headers = {}
            RequestHandler.do_GET(handler)
            self.assertEqual(handler.response[0], 401)

    def test_world_catalog_assigns_stable_display_numbers_when_runtime_omits_them(self):
        class HandlerStub:
            path = "/api/worlds"
            state = SimpleNamespace(
                current_arena=lambda: {"status": "idle", "world_number": None},
                world_configs=[
                    {"id": "first", "name": "First", "description": "First description", "config": {"bounty_quota": 20}},
                    {"id": "second", "name": "Second", "description": "Second description", "config": {"bounty_quota": 30}},
                ],
            )

            def send_json(self, status, payload):
                self.response = (status, payload)

            def log_error(self, *_args):
                self.fail("unexpected handler error")

        handler = HandlerStub()
        RequestHandler.do_GET(handler)
        status, payload = handler.response
        self.assertEqual(status, 200)
        self.assertEqual([world["world_number"] for world in payload["worlds"]], [1, 2])
        self.assertEqual([world["name"] for world in payload["worlds"]], ["First", "Second"])
        self.assertEqual(payload["worlds"][0]["config"]["bounty_quota"], 20)

    def test_documentation_renderer_formats_tables_and_hides_front_matter(self):
        from hub import markdown_html

        rendered = markdown_html("---\nid: FE-1\n---\n# API title\n\n| Name | Value |\n|---|---|\n| `gold` | **active** |")
        self.assertNotIn("id: FE-1", rendered)
        self.assertIn('<table class="docs-table">', rendered)
        self.assertIn("<code>gold</code>", rendered)
        self.assertIn("<strong>active</strong>", rendered)

    def test_public_team_catalog_exposes_only_names(self):
        with tempfile.TemporaryDirectory() as directory:
            registry = TeamRegistry(Path(directory) / "registry.json")
            registry.register("never-public-token", "Amber Crew")

            class HandlerStub:
                path = "/api/teams"
                state = SimpleNamespace(registry=registry)

                def send_json(self, status, payload):
                    self.response = (status, payload)

                def log_error(self, *_args):
                    self.fail("unexpected handler error")

            handler = HandlerStub()
            RequestHandler.do_GET(handler)
            status, payload = handler.response
            self.assertEqual(status, 200)
            self.assertEqual(payload, {"teams": [{"name": "Amber Crew"}]})
            self.assertNotIn("never-public-token", json.dumps(payload))

    def test_votes_are_one_per_team_changeable_and_consumed_for_next_world(self):
        state = HubState.__new__(HubState)
        state.world_configs = [
            {"id": "a", "name": "A"}, {"id": "b", "name": "B"},
        ]
        state.lock = __import__("threading").RLock()
        state.votes = {}
        state.arena = {"run_id": "active-run"}
        first_team = token_id("first")
        second_team = token_id("second")

        self.assertEqual(state.set_vote(first_team, "a"), {"a": 1})
        self.assertEqual(state.set_vote(first_team, "b"), {"b": 1})
        self.assertEqual(state.set_vote(second_team, "b"), {"b": 2})
        self.assertEqual(state.consume_vote_winner(), "b")
        self.assertEqual(state.vote_results(), {})

    def test_world_selection_prefers_vote_and_falls_back_to_random(self):
        worlds = [{"id": "a"}, {"id": "b"}]
        self.assertEqual(pick_world(worlds, "a", "b")["id"], "b")
        self.assertIn(pick_world(worlds, "a")["id"], {"a", "b"})

    def test_run_names_world_filter_and_totals(self):
        with tempfile.TemporaryDirectory() as directory:
            store = Store(Path(directory) / "hub.sqlite3")
            team = token_id("team-token")
            world = {"world_number": 1, "id": "quiet-harbor-01", "name": "Тихая бухта"}
            run_a, name_a = store.start_run(world, 8080)
            run_b, name_b = store.start_run(world, 8080)
            self.assertEqual(name_a, "world_quiet-harbor-01_1_1")
            self.assertEqual(name_b, "world_quiet-harbor-01_2_2")
            store.ingest(run_a, [{"team_id": team, "gold": 40, "gold_collected": 100,
                                  "carpets_lost": 1, "distance_travelled": 12}], {})
            store.ingest(run_b, [{"team_id": team, "gold": 20, "gold_collected": 150,
                                  "carpets_lost": 2, "distance_travelled": 25}], {})

            result = store.leaderboard("world", {team: "Team One"}, world_number=1)["teams"][0]
            self.assertEqual(result["name"], "Team One")
            self.assertEqual(result["top"], {
                "gold": 20, "gold_collected": 150, "carpets_lost": 2,
                "distance_travelled": 25.0,
            })
            self.assertEqual(result["total"], {
                "gold": 60, "gold_collected": 250, "carpets_lost": 3,
                "distance_travelled": 37.0,
            })
            self.assertEqual(result["attempts"], 2)
            page = store.runs(world_number=1, limit=1, offset=0)
            self.assertEqual(page["total"], 2)
            self.assertEqual(page["runs"][0]["id"], run_b)
            older_page = store.runs(world_number=1, limit=1, offset=1)
            self.assertEqual(older_page["runs"][0]["id"], run_a)
            all_time = store.leaderboard("all", {team: "Team One"})["teams"][0]
            self.assertEqual(all_time["total"], result["total"])
            run_result = store.leaderboard("run", {team: "Team One"}, run_id=run_a)
            self.assertEqual(run_result["teams"][0]["gold_collected"], 100)
            run_c, name_c = store.start_run({"world_number": 2, "id": "storm-belt-01", "name": "Грозовой пояс"}, 8080)
            self.assertEqual(name_c, "world_storm-belt-01_1_3")
            self.assertEqual(store.leaderboard("run", {}, run_id=run_c)["teams"], [])
            store.db.close()

    def test_empty_aggregate_leaderboards_return_empty_team_lists(self):
        with tempfile.TemporaryDirectory() as directory:
            store = Store(Path(directory) / "hub.sqlite3")
            store.start_run({"world_number": 1, "id": "quiet-harbor-01", "name": "Тихая бухта"}, 8080)
            self.assertEqual(store.leaderboard("all", {})["teams"], [])
            self.assertEqual(store.leaderboard("world", {}, world_number=1)["teams"], [])
            store.db.close()

    def test_aggregate_rank_uses_total_collected_gold(self):
        with tempfile.TemporaryDirectory() as directory:
            store = Store(Path(directory) / "hub.sqlite3")
            stronger_single = token_id("single")
            consistent_team = token_id("consistent")
            world = {"world_number": 1, "id": "quiet-harbor-01", "name": "Тихая бухта"}
            first, _ = store.start_run(world, 8080)
            second, _ = store.start_run(world, 8080)
            store.ingest(first, [
                {"team_id": stronger_single, "gold_collected": 180},
                {"team_id": consistent_team, "gold_collected": 120},
            ], {})
            store.ingest(second, [
                {"team_id": consistent_team, "gold_collected": 120},
            ], {})
            teams = store.leaderboard("world", {}, world_number=1)["teams"]
            self.assertEqual(teams[0]["team_id"], consistent_team)
            self.assertEqual(teams[0]["top"]["gold_collected"], 120)
            self.assertEqual(teams[0]["total"]["gold_collected"], 240)
            store.db.close()

    def test_registry_file_is_not_part_of_public_reports(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "registry.json"
            registry = TeamRegistry(path)
            public = registry.register("do-not-publish", "Visible Name")
            self.assertEqual(set(public), {"team_id", "name"})
            self.assertEqual(json.loads(path.read_text(encoding="utf-8"))["teams"][0]["token"], "do-not-publish")

    def test_legacy_run_database_migrates_without_world_seed(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "hub.sqlite3"
            import sqlite3
            legacy = sqlite3.connect(path)
            legacy.executescript("""
                CREATE TABLE runs (
                    id TEXT PRIMARY KEY, world_number INTEGER NOT NULL,
                    world_seed INTEGER NOT NULL, seed_occurrence INTEGER NOT NULL DEFAULT 1,
                    run_number INTEGER NOT NULL DEFAULT 1, arena_name TEXT NOT NULL,
                    port INTEGER NOT NULL, started_at REAL NOT NULL, ended_at REAL,
                    status TEXT NOT NULL, error TEXT
                );
                CREATE TABLE run_team_stats (
                    run_id TEXT NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
                    team_id TEXT NOT NULL, fallback_name TEXT NOT NULL, gold INTEGER NOT NULL,
                    gold_collected INTEGER NOT NULL, carpets_lost INTEGER NOT NULL,
                    distance_travelled REAL NOT NULL, updated_at REAL NOT NULL,
                    PRIMARY KEY(run_id, team_id)
                );
                INSERT INTO runs VALUES ('old', 1, 1042, 1, 1, 'world_seed_1042_1_1', 8080, 1, NULL, 'complete', NULL);
                INSERT INTO run_team_stats VALUES ('old', '0123456789abcdef', 'Old', 10, 20, 1, 30, 1);
            """)
            legacy.close()
            store = Store(path)
            columns = {row[1] for row in store.db.execute("PRAGMA table_info(runs)")}
            self.assertNotIn("world_seed", columns)
            self.assertIn("world_id", columns)
            self.assertEqual(store.runs()["runs"][0]["arena_name"], "world_legacy_1_1_1")
            self.assertEqual(store.leaderboard("run", {}, run_id="old")["teams"][0]["gold_collected"], 20)
            self.assertEqual(store.db.execute("PRAGMA foreign_key_check").fetchall(), [])
            store.db.close()


if __name__ == "__main__":
    unittest.main()
