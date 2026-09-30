//! # Интеграционные тесты сетевого HTTP REST API (FE-004)
//!
//! Проверяет эндпоинты `GET /api/game/state` и `POST /api/carpet/command`,
//! валидацию параметров, авторизацию по токену и ограничение Rate Limit
//! в соответствии со спецификацией [`FE-004`](file:///Users/d.byta/Documents/Code/dats/datsmagic/docs/features/FE-004-simulation-rest-api.md)
//! и контрактами [`docs/mechanics.md`](file:///Users/d.byta/Documents/Code/dats/datsmagic/docs/mechanics.md#4-контракты-api-спецификация-json).

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

use server::api::create_api_router;
use server::config::ServerConfig;
use server::engine::state::{AnomalyState, PlayerState, SessionStatus, TreasureState};
use server::engine::GameEngine;

/// Вспомогательная функция для извлечения тела ответа в виде JSON Value
async fn response_to_json(response: axum::response::Response) -> serde_json::Value {
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("Failed to read response body")
        .to_bytes();
    let text = String::from_utf8(bytes.to_vec()).expect("Body is not valid UTF-8");
    serde_json::from_str(&text).expect("Body is not valid JSON")
}

/// TC-API-01: Запрос без X-Auth-Token возвращает HTTP 401 Unauthorized
#[tokio::test]
async fn test_unauthorized_without_token() {
    let engine = GameEngine::new(ServerConfig::default());
    let app = create_api_router(engine);

    // 1. GET /api/game/state без заголовка
    let req = Request::builder()
        .method("GET")
        .uri("/api/game/state")
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    let json = response_to_json(res).await;
    assert_eq!(json["error"], "unauthorized");

    // 2. GET /api/game/state с пустым заголовком
    let req = Request::builder()
        .method("GET")
        .uri("/api/game/state")
        .header("X-Auth-Token", "   ")
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    let json = response_to_json(res).await;
    assert_eq!(json["error"], "unauthorized");

    // 3. POST /api/carpet/command без заголовка
    let req = Request::builder()
        .method("POST")
        .uri("/api/carpet/command")
        .header("Content-Type", "application/json")
        .body(Body::from(r#"{"acceleration":{"x":1.0,"y":2.0}}"#))
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    let json = response_to_json(res).await;
    assert_eq!(json["error"], "unauthorized");
}

/// TC-API-02: Команда с нечисловыми или невалидными векторами отклоняется с кодом 400
#[tokio::test]
async fn test_nan_vector_rejected() {
    let engine = GameEngine::new(ServerConfig::default());
    let app = create_api_router(engine);

    // 1. Поле x содержит строку "NaN"
    let req = Request::builder()
        .method("POST")
        .uri("/api/carpet/command")
        .header("X-Auth-Token", "team_1")
        .header("Content-Type", "application/json")
        .body(Body::from(r#"{"acceleration":{"x":"NaN","y":1.0}}"#))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let json = response_to_json(res).await;
    assert_eq!(json["error"], "invalid vector values");

    // 2. Поле y содержит строку вместо числа
    let req = Request::builder()
        .method("POST")
        .uri("/api/carpet/command")
        .header("X-Auth-Token", "team_1")
        .header("Content-Type", "application/json")
        .body(Body::from(r#"{"acceleration":{"x":1.0,"y":"invalid"}}"#))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let json = response_to_json(res).await;
    assert_eq!(json["error"], "invalid vector values");

    // 3. Некорректный JSON синтаксис
    let req = Request::builder()
        .method("POST")
        .uri("/api/carpet/command")
        .header("X-Auth-Token", "team_1")
        .header("Content-Type", "application/json")
        .body(Body::from(r#"{malformed_json}"#))
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let json = response_to_json(res).await;
    assert_eq!(json["error"], "invalid vector values");
}

/// TC-API-03: Ограничение Rate Limit (1 команда за такт симуляции, 429 Too Many Requests)
#[tokio::test]
async fn test_command_rate_limit_429() {
    let engine = GameEngine::new(ServerConfig::default());
    let app = create_api_router(engine.clone());

    let make_command_req = || {
        Request::builder()
            .method("POST")
            .uri("/api/carpet/command")
            .header("X-Auth-Token", "team_rate_test")
            .header("Content-Type", "application/json")
            .body(Body::from(r#"{"acceleration":{"x":2.0,"y":3.0}}"#))
            .unwrap()
    };

    // Первый запрос в текущем тике должен успешно пройти (200 OK)
    let res1 = app.clone().oneshot(make_command_req()).await.unwrap();
    assert_eq!(res1.status(), StatusCode::OK);
    let json1 = response_to_json(res1).await;
    assert_eq!(json1["status"], "accepted");

    // Второй запрос в том же тике должен отклониться с кодом 429
    let res2 = app.clone().oneshot(make_command_req()).await.unwrap();
    assert_eq!(res2.status(), StatusCode::TOO_MANY_REQUESTS);
    let json2 = response_to_json(res2).await;
    assert_eq!(json2["error"], "rate limit exceeded: 1 command per tick");

    // Продвигаем симуляцию на 1 такт
    engine.step_once().await;

    // В новом тике отправка команды снова успешна (200 OK)
    let res3 = app.oneshot(make_command_req()).await.unwrap();
    assert_eq!(res3.status(), StatusCode::OK);
    let json3 = response_to_json(res3).await;
    assert_eq!(json3["status"], "accepted");
}

/// Соответствие формата GET /api/game/state схеме docs/mechanics.md
#[tokio::test]
async fn test_get_game_state_format() {
    let engine = GameEngine::new(ServerConfig::default());

    // Заполняем состояние мира контрольными данными из примера mechanics.md
    {
        let shared_state = engine.shared_state();
        let mut state = shared_state.write().await;
        state.tick = 1042;
        state.status = SessionStatus::Active;

        // Наш игрок
        let player = PlayerState {
            id: "team_20".to_string(),
            score: 1420,
            status: "normal".to_string(),
            position: (412.5, 890.2),
            velocity: (8.5, -4.1),
            max_acceleration: 5.0,
            max_velocity: 20.0,
            stun_remaining_ticks: 0,
            carpets: std::collections::HashMap::new(),
        };
        state.world.players.insert(player.id.clone(), player);

        // Соперник
        let enemy = PlayerState {
            id: "team_5".to_string(),
            score: 800,
            status: "normal".to_string(),
            position: (430.0, 910.0),
            velocity: (12.0, 2.1),
            max_acceleration: 5.0,
            max_velocity: 20.0,
            stun_remaining_ticks: 0,
            carpets: std::collections::HashMap::new(),
        };
        state.world.players.insert(enemy.id.clone(), enemy);

        // Сокровище
        state.world.treasures.push(TreasureState {
            id: "t_99".to_string(),
            r#type: "chest".to_string(),
            position: (450.0, 920.0),
            value: 50,
            is_collected: false,
        });

        // Аномалия
        state
            .world
            .anomalies
            .push(AnomalyState::new("a_3", (400.0, 900.0), 40.0, 3.5));
    }

    // Публикуем инициализированное состояние в кэшированный снимок
    engine.publish_snapshot().await;
    let app = create_api_router(engine.clone());

    let req = Request::builder()
        .method("GET")
        .uri("/api/game/state")
        .header("X-Auth-Token", "team_20")
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let json = response_to_json(res).await;

    // Проверка соответствия схеме docs/mechanics.md
    assert_eq!(json["tick"], 1042);
    assert_eq!(json["game_status"], "active");

    // Игрок
    assert_eq!(json["player"]["id"], "team_20");
    assert_eq!(json["player"]["score"], 1420);
    assert_eq!(json["player"]["status"], "normal");
    assert_eq!(json["player"]["position"]["x"], 412.5);
    assert_eq!(json["player"]["position"]["y"], 890.2);
    assert_eq!(json["player"]["velocity"]["x"], 8.5);
    assert_eq!(json["player"]["velocity"]["y"], -4.1);
    assert_eq!(json["player"]["max_acceleration"], 5.0);
    assert_eq!(json["player"]["max_velocity"], 20.0);

    // Сокровища
    assert_eq!(json["treasures"][0]["id"], "t_99");
    assert_eq!(json["treasures"][0]["type"], "chest");
    assert_eq!(json["treasures"][0]["position"]["x"], 450.0);
    assert_eq!(json["treasures"][0]["position"]["y"], 920.0);
    assert_eq!(json["treasures"][0]["value"], 50);

    // Аномалии
    assert_eq!(json["anomalies"][0]["id"], "a_3");
    assert_eq!(json["anomalies"][0]["type"], "attracting");
    assert_eq!(json["anomalies"][0]["position"]["x"], 400.0);
    assert_eq!(json["anomalies"][0]["position"]["y"], 900.0);
    assert_eq!(json["anomalies"][0]["velocity"]["x"], 0.0);
    assert_eq!(json["anomalies"][0]["velocity"]["y"], 0.0);
    assert_eq!(json["anomalies"][0]["core_radius"], 0.0);
    assert_eq!(json["anomalies"][0]["radius"], 40.0);
    assert_eq!(json["anomalies"][0]["force"], 3.5);

    // Противники
    assert_eq!(json["enemies"][0]["id"], "team_5");
    assert_eq!(json["enemies"][0]["position"]["x"], 430.0);
    assert_eq!(json["enemies"][0]["position"]["y"], 910.0);
    assert_eq!(json["enemies"][0]["velocity"]["x"], 12.0);
    assert_eq!(json["enemies"][0]["velocity"]["y"], 2.1);
}

/// Попытка отправить команду при неактивной (поставленной на паузу) сессии
#[tokio::test]
async fn test_session_paused_rejects_command() {
    let engine = GameEngine::new(ServerConfig::default());
    let app = create_api_router(engine.clone());

    engine.set_status(SessionStatus::Paused).await;

    let req = Request::builder()
        .method("POST")
        .uri("/api/carpet/command")
        .header("X-Auth-Token", "team_paused")
        .header("Content-Type", "application/json")
        .body(Body::from(r#"{"acceleration":{"x":1.0,"y":1.0}}"#))
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let json = response_to_json(res).await;
    assert_eq!(json["error"], "session is not active");
}

/// TC-API-ANOM-01: GET /api/game/state с динамическими аномалиями и статусом destroyed (FE-007)
#[tokio::test]
async fn test_get_game_state_with_dynamic_anomalies() {
    let engine = GameEngine::new(ServerConfig::default());

    {
        let shared = engine.shared_state();
        let mut state = shared.write().await;
        state.tick = 420;
        state.status = SessionStatus::Active;

        // Погибший игрок
        let mut player = PlayerState::new("team_1".to_string(), 340.5, 720.0, 5.0, 20.0);
        player.score = 120;
        player.mark_destroyed();
        state.world.players.insert(player.id.clone(), player);

        // Динамические аномалии (притягивающая и отталкивающая)
        state.world.anomalies.push(AnomalyState::new_dynamic(
            "a_dyn_1",
            "attracting",
            (350.0, 725.0),
            (5.0, -1.2),
            15.0,
            80.0,
            4.5,
        ));

        state.world.anomalies.push(AnomalyState::new_dynamic(
            "a_dyn_2",
            "repelling",
            (100.0, 900.0),
            (-2.0, 3.0),
            12.0,
            60.0,
            3.0,
        ));

        // Сокровище
        state.world.treasures.push(TreasureState {
            id: "t_1".to_string(),
            r#type: "chest".to_string(),
            position: (200.0, 500.0),
            value: 50,
            is_collected: false,
        });
    }

    engine.publish_snapshot().await;
    let app = create_api_router(engine);

    let req = Request::builder()
        .method("GET")
        .uri("/api/game/state")
        .header("X-Auth-Token", "team_1")
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let json = response_to_json(res).await;

    assert_eq!(json["tick"], 420);
    assert_eq!(json["game_status"], "active");

    // Проверка телеметрии игрока со статусом destroyed
    assert_eq!(json["player"]["id"], "team_1");
    assert_eq!(json["player"]["score"], 120);
    assert_eq!(json["player"]["status"], "destroyed");
    assert_eq!(json["player"]["position"]["x"], 340.5);
    assert_eq!(json["player"]["position"]["y"], 720.0);
    assert_eq!(json["player"]["velocity"]["x"], 0.0);
    assert_eq!(json["player"]["velocity"]["y"], 0.0);

    // Проверка динамических аномалий
    assert_eq!(json["anomalies"].as_array().unwrap().len(), 2);

    let a1 = &json["anomalies"][0];
    assert_eq!(a1["id"], "a_dyn_1");
    assert_eq!(a1["type"], "attracting");
    assert_eq!(a1["position"]["x"], 350.0);
    assert_eq!(a1["position"]["y"], 725.0);
    assert_eq!(a1["velocity"]["x"], 5.0);
    assert_eq!(a1["velocity"]["y"], -1.2);
    assert_eq!(a1["core_radius"], 15.0);
    assert_eq!(a1["radius"], 80.0);
    assert_eq!(a1["force"], 4.5);

    let a2 = &json["anomalies"][1];
    assert_eq!(a2["id"], "a_dyn_2");
    assert_eq!(a2["type"], "repelling");
    assert_eq!(a2["position"]["x"], 100.0);
    assert_eq!(a2["position"]["y"], 900.0);
    assert_eq!(a2["velocity"]["x"], -2.0);
    assert_eq!(a2["velocity"]["y"], 3.0);
    assert_eq!(a2["core_radius"], 12.0);
    assert_eq!(a2["radius"], 60.0);
    assert_eq!(a2["force"], 3.0);
}

/// TC-API-ANOM-02: Попытка отправить команду от уничтоженного ковра отклоняется с кодом 400 и ошибкой player_destroyed (FE-007)
#[tokio::test]
async fn test_destroyed_player_command_rejected() {
    let engine = GameEngine::new(ServerConfig::default());

    {
        let shared = engine.shared_state();
        let mut state = shared.write().await;
        state.status = SessionStatus::Active;

        let mut player = PlayerState::new("team_wrecked".to_string(), 100.0, 100.0, 5.0, 20.0);
        player.mark_destroyed();
        state.world.players.insert(player.id.clone(), player);
    }

    engine.publish_snapshot().await;
    let app = create_api_router(engine);

    let req = Request::builder()
        .method("POST")
        .uri("/api/carpet/command")
        .header("X-Auth-Token", "team_wrecked")
        .header("Content-Type", "application/json")
        .body(Body::from(r#"{"acceleration":{"x":2.0,"y":3.0}}"#))
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);

    let json = response_to_json(res).await;
    assert_eq!(json["error"], "player_destroyed");
}

/// TC-FLEET-01: При автоматической регистрации нового игрока создаются ровно 5 ковров (FE-010).
#[tokio::test]
async fn test_tc_fleet_01_auto_registration_creates_5_carpets() {
    let engine = GameEngine::new(ServerConfig::default());
    let app = create_api_router(engine);

    let req = Request::builder()
        .method("GET")
        .uri("/api/game/state")
        .header("X-Auth-Token", "fleet_team_alpha")
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let json = response_to_json(res).await;
    let carpets = json["player"]["carpets"]
        .as_array()
        .expect("player.carpets must be an array");
    assert_eq!(carpets.len(), 5, "Player must have exactly 5 carpets in fleet");

    for (i, carpet) in carpets.iter().enumerate() {
        let expected_id = format!("fleet_team_alpha_{}", i);
        assert_eq!(carpet["id"], expected_id.as_str());
        assert_eq!(carpet["status"], "normal");
        assert_eq!(carpet["velocity"]["x"], 0.0);
        assert_eq!(carpet["velocity"]["y"], 0.0);
        assert_eq!(carpet["max_acceleration"], 5.0);
        assert_eq!(carpet["max_velocity"], 20.0);
    }
}

/// TC-FLEET-02: Пакетная отправка команд раздельно ускоряет ковер 0 и ковер 1 в противоположных направлениях (FE-010).
#[tokio::test]
async fn test_tc_fleet_02_batch_commands_independent_carpet_acceleration() {
    let config = ServerConfig {
        friction: 1.0, // без затухания трения для точного расчета v_new = a * dt
        tick_rate_ms: 200,
        ..Default::default()
    };
    let engine = GameEngine::new(config);

    // Сначала регистрируем игрока
    {
        let shared = engine.shared_state();
        let mut state = shared.write().await;
        state.status = SessionStatus::Active;
        let player = PlayerState::new("team_split".to_string(), 500.0, 500.0, 5.0, 20.0);
        state.world.players.insert(player.id.clone(), player);
    }
    engine.publish_snapshot().await;

    let app = create_api_router(engine.clone());

    // Отправляем пакет команд: ковер 0 вправо (+4.0, 0.0), ковер 1 влево (-4.0, 0.0)
    let batch_payload = serde_json::json!({
        "commands": [
            {"carpet_id": "team_split_0", "acceleration": {"x": 4.0, "y": 0.0}},
            {"carpet_id": "team_split_1", "acceleration": {"x": -4.0, "y": 0.0}}
        ]
    });

    let req = Request::builder()
        .method("POST")
        .uri("/api/carpet/commands")
        .header("X-Auth-Token", "team_split")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&batch_payload).unwrap()))
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let resp_json = response_to_json(res).await;
    assert_eq!(resp_json["status"], "accepted");
    assert_eq!(resp_json["commands_count"], 2);

    // Выполняем один такт симуляции dt = 0.2
    engine.step_once().await;

    let snapshot = engine.get_snapshot();
    let player = snapshot.world.players.get("team_split").unwrap();
    let c0 = player.carpets.get("team_split_0").unwrap();
    let c1 = player.carpets.get("team_split_1").unwrap();
    let c2 = player.carpets.get("team_split_2").unwrap();

    // c0: v_x = 4.0 * 0.2 = 0.8 > 0
    assert!(
        (c0.velocity.0 - 0.8).abs() < 1e-6,
        "c0 velocity should be 0.8, got {}",
        c0.velocity.0
    );
    assert!(c0.position.0 > 500.0, "c0 position should move right");

    // c1: v_x = -4.0 * 0.2 = -0.8 < 0
    assert!(
        (c1.velocity.0 - (-0.8)).abs() < 1e-6,
        "c1 velocity should be -0.8, got {}",
        c1.velocity.0
    );
    assert!(c1.position.0 < 470.0 + 1e-6, "c1 position should move left");

    // c2: без команды скорость осталась 0.0
    assert_eq!(c2.velocity.0, 0.0);
}

/// TC-FLEET-03: GET /api/game/state возвращает 5 ковров с корректными идентификаторами и позициями (FE-010).
#[tokio::test]
async fn test_tc_fleet_03_get_game_state_returns_all_carpets_with_details() {
    let engine = GameEngine::new(ServerConfig::default());

    // Создаем игрока и соперника
    {
        let shared = engine.shared_state();
        let mut state = shared.write().await;
        state.status = SessionStatus::Active;
        let p1 = PlayerState::new("player_me".to_string(), 300.0, 300.0, 5.0, 20.0);
        let p2 = PlayerState::new("player_rival".to_string(), 700.0, 700.0, 5.0, 20.0);
        state.world.players.insert(p1.id.clone(), p1);
        state.world.players.insert(p2.id.clone(), p2);
    }
    engine.publish_snapshot().await;

    let app = create_api_router(engine);

    let req = Request::builder()
        .method("GET")
        .uri("/api/game/state")
        .header("X-Auth-Token", "player_me")
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let json = response_to_json(res).await;

    // Проверяем свои 5 ковров
    let player_carpets = json["player"]["carpets"].as_array().unwrap();
    assert_eq!(player_carpets.len(), 5);
    for (i, carpet) in player_carpets.iter().enumerate() {
        assert_eq!(carpet["id"], format!("player_me_{}", i));
        assert!(carpet["position"]["x"].as_f64().is_some());
        assert!(carpet["position"]["y"].as_f64().is_some());
        assert!(carpet["velocity"]["x"].as_f64().is_some());
        assert!(carpet["velocity"]["y"].as_f64().is_some());
    }

    // Проверяем противника и его 5 ковров
    let enemies = json["enemies"].as_array().unwrap();
    assert_eq!(enemies.len(), 1);
    let rival = &enemies[0];
    assert_eq!(rival["id"], "player_rival");
    let rival_carpets = rival["carpets"].as_array().unwrap();
    assert_eq!(rival_carpets.len(), 5);
    for (i, carpet) in rival_carpets.iter().enumerate() {
        assert_eq!(carpet["id"], format!("player_rival_{}", i));
        assert!(carpet["position"]["x"].as_f64().is_some());
        assert!(carpet["position"]["y"].as_f64().is_some());
    }
}

