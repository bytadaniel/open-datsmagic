//! Контрактные тесты единственного внешнего API `Desert`.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

use server::api::create_api_router;
use server::config::ServerConfig;
use server::engine::state::{AnomalyState, PlayerState, TreasureState};
use server::engine::GameEngine;

async fn response_to_json(response: axum::response::Response) -> serde_json::Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn test_move_returns_exact_desert_contract() {
    let engine = GameEngine::new(ServerConfig::default());
    {
        let shared = engine.shared_state();
        let mut state = shared.write().await;
        let mut team = PlayerState::new("team".to_string(), 400.0, 300.0, 5.0, 20.0);
        team.carpets.get_mut("team_0").unwrap().death_count = 3;
        state.world.players.insert("team".to_string(), team);
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
    assert_eq!(json["name"], "team");
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
async fn test_only_move_route_is_registered() {
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
async fn test_token_registers_one_distributed_fleet() {
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
