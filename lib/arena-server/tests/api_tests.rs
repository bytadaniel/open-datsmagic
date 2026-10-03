//! Контрактные тесты единственного внешнего API `Desert`.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use std::io::Read;
use std::sync::OnceLock;
use tower::ServiceExt;

use server::api::create_api_router;
use server::config::ServerConfig;
use server::engine::state::{AnomalyState, PlayerState, TreasureState};
use server::engine::GameEngine;

fn install_test_token_registry() {
    static SETUP: OnceLock<()> = OnceLock::new();
    SETUP.get_or_init(|| {
        let path = std::env::temp_dir().join(format!(
            "datsmagic-api-test-registry-{}.json",
            std::process::id()
        ));
        let teams = [
            ("team", "Example Team"),
            ("alpha", "Alpha"),
            ("beta", "Beta"),
            ("last-carpet-team", "Last Carpet"),
            ("king-team", "King Team"),
            ("gzip-team", "Gzip Team"),
            ("limited-team", "Limited Team"),
            ("independent-team", "Independent Team"),
            ("load-team-0", "Load Test Team"),
            ("load-team-1", "Load Test Team"),
            ("load-team-2", "Load Test Team"),
            ("load-team-3", "Load Test Team"),
            ("load-team-4", "Load Test Team"),
            ("load-team-5", "Load Test Team"),
            ("load-team-6", "Load Test Team"),
            ("load-team-7", "Load Test Team"),
            ("load-team-8", "Load Test Team"),
            ("load-team-9", "Load Test Team"),
        ];
        let registry = serde_json::json!({
            "teams": teams.into_iter().map(|(token, name)| {
                serde_json::json!({"token": token, "name": name})
            }).collect::<Vec<_>>()
        });
        std::fs::write(&path, serde_json::to_vec(&registry).unwrap()).unwrap();
        std::env::set_var("DATS_TOKEN_REGISTRY_PATH", path);
    });
}

fn token_id(token: &str) -> String {
    let hash = token
        .as_bytes()
        .iter()
        .fold(0xcbf29ce484222325_u64, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
        });
    format!("{hash:016x}")
}

async fn response_to_json(response: axum::response::Response) -> serde_json::Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn test_move_returns_exact_desert_contract() {
    install_test_token_registry();
    let engine = GameEngine::new(ServerConfig::default());
    {
        let shared = engine.shared_state();
        let mut state = shared.write().await;
        let team_id = token_id("team");
        let mut team = PlayerState::new(team_id.clone(), 400.0, 300.0, 5.0, 20.0);
        team.carpets
            .get_mut(&format!("{team_id}_0"))
            .unwrap()
            .death_count = 3;
        state.world.players.insert(team_id, team);
        state.world.players.insert(
            "opponent".to_string(),
            PlayerState::new("opponent".to_string(), 800.0, 700.0, 5.0, 20.0),
        );
        state.world.anomalies.push(AnomalyState::new_dynamic(
            "repel",
            "repelling",
            (100.0, 100.0),
            (2.0, -1.0),
            15.0,
            80.0,
            3.5,
        ));
        state.world.treasures.push(TreasureState {
            id: "coin".to_string(),
            r#type: "coin".to_string(),
            position: (200.0, 250.0),
            value: 17,
            is_collected: false,
        });
    }
    engine.publish_snapshot().await;
    let app = create_api_router(engine);
    let request = Request::builder()
        .method("POST")
        .uri("/play/magcarp/player/move")
        .header("X-Auth-Token", "team")
        .header("Content-Type", "application/json")
        .body(Body::from(r#"{"transports":[]}"#))
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let json = response_to_json(response).await;

    let keys: std::collections::BTreeSet<_> = json
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        keys,
        [
            "errors",
            "anomalies",
            "attackCooldownMs",
            "attackDamage",
            "attackExplosionRadius",
            "attackRange",
            "bounties",
            "enemies",
            "mapSize",
            "maxAccel",
            "maxSpeed",
            "name",
            "points",
            "reviveTimeoutSec",
            "shieldCooldownMs",
            "shieldTimeMs",
            "transportRadius",
            "transports",
            "wantedList",
        ]
        .into_iter()
        .collect()
    );
    assert_eq!(json["name"], "Example Team");
    assert_eq!(
        json["transports"][0]["id"],
        format!("{}_0", token_id("team"))
    );
    assert!(!serde_json::to_string(&json).unwrap().contains("\"team\""));
    assert_eq!(json["transports"][0]["deathCount"], 3);
    assert_eq!(json["enemies"].as_array().unwrap().len(), 5);
    assert_eq!(
        json["bounties"][0],
        serde_json::json!({"points": 17, "radius": 5.0, "x": 200.0, "y": 250.0})
    );
    assert_eq!(json["anomalies"][0]["radius"], 15.0);
    assert_eq!(json["anomalies"][0]["effectiveRadius"], 80.0);
    assert_eq!(json["anomalies"][0]["strength"], -3.5);
    let transport_keys: std::collections::BTreeSet<_> = json["transports"][0]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        transport_keys,
        [
            "anomalyAcceleration",
            "attackCooldownMs",
            "deathCount",
            "health",
            "id",
            "selfAcceleration",
            "shieldCooldownMs",
            "shieldLeftMs",
            "status",
            "velocity",
            "x",
            "y",
        ]
        .into_iter()
        .collect()
    );
}

#[tokio::test]
async fn gzip_is_negotiated_without_changing_desert_json() {
    install_test_token_registry();
    let app = create_api_router(GameEngine::new(ServerConfig::default()));
    let request = Request::builder()
        .method("POST")
        .uri("/play/magcarp/player/move")
        .header("X-Auth-Token", "gzip-team")
        .header("Accept-Encoding", "gzip")
        .header("Content-Type", "application/json")
        .body(Body::from(r#"{"transports":[]}"#))
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["content-encoding"], "gzip");
    let compressed = response.into_body().collect().await.unwrap().to_bytes();
    let mut decoded = Vec::new();
    flate2::read::GzDecoder::new(compressed.as_ref())
        .read_to_end(&mut decoded)
        .unwrap();
    let desert: serde_json::Value = serde_json::from_slice(&decoded).unwrap();
    assert_eq!(desert["name"], "Gzip Team");
    assert!(desert["transports"].is_array());
}

#[tokio::test]
async fn api_limits_each_team_to_five_requests_per_second() {
    install_test_token_registry();
    let app = create_api_router(GameEngine::new(ServerConfig::default()));
    let request = |token: &str| {
        Request::builder()
            .method("POST")
            .uri("/play/magcarp/player/move")
            .header("X-Auth-Token", token)
            .header("Content-Type", "application/json")
            .body(Body::from(r#"{"transports":[]}"#))
            .unwrap()
    };
    for _ in 0..5 {
        assert_eq!(
            app.clone()
                .oneshot(request("limited-team"))
                .await
                .unwrap()
                .status(),
            StatusCode::OK
        );
    }
    assert_eq!(
        app.clone()
            .oneshot(request("limited-team"))
            .await
            .unwrap()
            .status(),
        StatusCode::TOO_MANY_REQUESTS
    );
    assert_eq!(
        app.oneshot(request("independent-team"))
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
}

#[tokio::test]
async fn api_serves_ten_distinct_teams_concurrently_with_large_snapshots() {
    install_test_token_registry();
    let engine = GameEngine::new(ServerConfig::default());
    {
        let shared = engine.shared_state();
        let mut state = shared.write().await;
        for index in 0..5_000 {
            state.world.treasures.push(TreasureState {
                id: format!("load-coin-{index}"),
                r#type: "coin".into(),
                position: ((index % 10_000) as f64, (index % 7_000) as f64),
                value: 1,
                is_collected: false,
            });
        }
    }
    engine.publish_snapshot().await;
    let app = create_api_router(engine);
    let mut tasks = tokio::task::JoinSet::new();
    for index in 0..10 {
        let app = app.clone();
        tasks.spawn(async move {
            let request = Request::builder()
                .method("POST")
                .uri("/play/magcarp/player/move")
                .header("X-Auth-Token", format!("load-team-{index}"))
                .header("Content-Type", "application/json")
                .header("Accept-Encoding", "gzip")
                .body(Body::from(r#"{"transports":[]}"#))
                .unwrap();
            app.oneshot(request).await.unwrap()
        });
    }
    let mut successful = 0;
    while let Some(result) = tasks.join_next().await {
        let response = result.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()["content-encoding"], "gzip");
        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert!(!body.is_empty() && body.len() < 100_000);
        successful += 1;
    }
    assert_eq!(successful, 10);
}

#[tokio::test]
async fn observer_token_reads_the_arena_without_creating_a_fleet_or_accepting_commands() {
    install_test_token_registry();
    std::env::set_var("DATS_OBSERVER_TOKEN", "internal-observer-test-secret");
    let engine = GameEngine::new(ServerConfig::default());
    {
        let shared = engine.shared_state();
        let mut state = shared.write().await;
        state.world.players.insert(
            "observer-visible-team".to_string(),
            PlayerState::new("observer-visible-team".to_string(), 400.0, 300.0, 5.0, 20.0),
        );
    }
    engine.publish_snapshot().await;
    let app = create_api_router(engine.clone());
    let request = Request::builder()
        .method("POST")
        .uri("/play/magcarp/player/move")
        .header("X-Auth-Token", "internal-observer-test-secret")
        .header("Content-Type", "application/json")
        .body(Body::from(r#"{"transports":[]}"#))
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let json = response_to_json(response).await;
    assert_eq!(json["transports"].as_array().unwrap().len(), 0);
    assert_eq!(json["enemies"].as_array().unwrap().len(), 5);
    assert_eq!(json["name"], "Наблюдатель");
    assert!(!engine
        .get_snapshot()
        .world
        .players
        .contains_key("__dats_observer__"));

    let request = Request::builder()
        .method("POST")
        .uri("/play/magcarp/player/move")
        .header("X-Auth-Token", "internal-observer-test-secret")
        .header("Content-Type", "application/json")
        .body(Body::from(
            r#"{"transports":[{"id":"observer-visible-team_0","acceleration":{"x":1,"y":0}}]}"#,
        ))
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert!(!engine
        .get_snapshot()
        .world
        .players
        .contains_key("__dats_observer__"));
    std::env::remove_var("DATS_OBSERVER_TOKEN");
}

#[tokio::test]
async fn test_only_move_route_is_registered() {
    install_test_token_registry();
    let app = create_api_router(GameEngine::new(ServerConfig::default()));
    for uri in [
        "/api/game/state",
        "/api/carpet/command",
        "/api/carpet/commands",
    ] {
        let request = Request::builder()
            .method("POST")
            .uri(uri)
            .header("X-Auth-Token", "team")
            .body(Body::empty())
            .unwrap();
        assert_eq!(
            app.clone().oneshot(request).await.unwrap().status(),
            StatusCode::NOT_FOUND
        );
    }
}

#[tokio::test]
async fn unknown_or_missing_token_is_rejected() {
    install_test_token_registry();
    let app = create_api_router(GameEngine::new(ServerConfig::default()));
    for token in [None, Some("not-registered")] {
        let mut builder = Request::builder()
            .method("POST")
            .uri("/play/magcarp/player/move")
            .header("Content-Type", "application/json");
        if let Some(token) = token {
            builder = builder.header("X-Auth-Token", token);
        }
        let response = app
            .clone()
            .oneshot(builder.body(Body::from(r#"{"transports":[]}"#)).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
}

#[tokio::test]
async fn test_token_registers_one_distributed_fleet() {
    install_test_token_registry();
    let app = create_api_router(GameEngine::new(ServerConfig::default()));
    let request = |token: &str| {
        Request::builder()
            .method("POST")
            .uri("/play/magcarp/player/move")
            .header("X-Auth-Token", token)
            .header("Content-Type", "application/json")
            .body(Body::from(r#"{"transports":[]}"#))
            .unwrap()
    };
    let alpha = response_to_json(app.clone().oneshot(request("alpha")).await.unwrap()).await;
    let alpha_repeat = response_to_json(app.clone().oneshot(request("alpha")).await.unwrap()).await;
    let beta = response_to_json(app.oneshot(request("beta")).await.unwrap()).await;

    assert_eq!(alpha["transports"].as_array().unwrap().len(), 5);
    assert_eq!(alpha["transports"], alpha_repeat["transports"]);
    assert_ne!(alpha["transports"][0]["x"], beta["transports"][0]["x"]);
    assert_ne!(alpha["transports"][0]["y"], beta["transports"][0]["y"]);
    assert_ne!(alpha["transports"][0]["x"], 4500.0);
}

#[tokio::test]
async fn disabled_respawn_still_creates_the_initial_carpet() {
    install_test_token_registry();
    let config = ServerConfig {
        carpet_count: 1,
        enable_respawn: false,
        ..ServerConfig::default()
    };
    let app = create_api_router(GameEngine::new(config));
    let request = Request::builder()
        .method("POST")
        .uri("/play/magcarp/player/move")
        .header("X-Auth-Token", "last-carpet-team")
        .header("Content-Type", "application/json")
        .body(Body::from(r#"{"transports":[]}"#))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let json = response_to_json(response).await;
    let transports = json["transports"].as_array().unwrap();
    assert_eq!(transports.len(), 1);
    assert_eq!(
        transports[0]["id"],
        format!("{}_0", token_id("last-carpet-team"))
    );
    assert_eq!(transports[0]["status"], "alive");
    assert!(transports[0]["x"].as_f64().unwrap() >= 0.0);
    assert!(transports[0]["y"].as_f64().unwrap() >= 0.0);
}

#[tokio::test]
async fn ten_carpet_world_returns_the_full_initial_fleet() {
    install_test_token_registry();
    let config = ServerConfig {
        carpet_count: 10,
        enable_respawn: false,
        ..ServerConfig::default()
    };
    let app = create_api_router(GameEngine::new(config));
    let request = Request::builder()
        .method("POST")
        .uri("/play/magcarp/player/move")
        .header("X-Auth-Token", "king-team")
        .header("Content-Type", "application/json")
        .body(Body::from(r#"{"transports":[]}"#))
        .unwrap();

    let json = response_to_json(app.oneshot(request).await.unwrap()).await;
    let transports = json["transports"].as_array().unwrap();
    assert_eq!(transports.len(), 10);
    assert!(transports
        .iter()
        .any(|transport| transport["id"] == format!("{}_9", token_id("king-team"))));
}
