//! # Ошибки сетевого REST API и их маппинг в HTTP статус-коды
//!
//! Модуль определяет перечисление [`ApiError`] и реализует трейт [`axum::response::IntoResponse`]
//! для формирования структурированных JSON-ответов с ошибками в соответствии
//! с разделом 5 спецификации [`FE-004`](file:///Users/d.byta/Documents/Code/dats/datsmagic/docs/features/FE-004-simulation-rest-api.md).

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use thiserror::Error;

use super::dto::ErrorResponseDto;

/// Ошибки API игрового сервера
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ApiError {
    /// 401: Отсутствует или неверен заголовок X-Auth-Token
    #[error("unauthorized")]
    Unauthorized,

    /// 400: Поля x, y содержат NaN, Infinity или неверный тип
    #[error("invalid vector values")]
    InvalidVector,

    /// 429: Отправлено более 1 команды за один игровой такт
    #[error("rate limit exceeded: 1 command per tick")]
    RateLimitExceeded,

    /// 429: Более 5 HTTP-запросов в секунду от одного токена
    #[error("rate limit exceeded: 5 requests per second per token")]
    RequestRateLimitExceeded,

    /// 400: Игровая сессия находится на паузе или завершена
    #[error("session is not active")]
    SessionNotActive,

    /// 400: Игрок уничтожен
    #[error("player is destroyed")]
    PlayerDestroyed,

    /// 500: Внутренняя ошибка сервера
    #[error("internal server error")]
    Internal(String),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, msg) = match self {
            Self::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized"),
            Self::InvalidVector => (StatusCode::BAD_REQUEST, "invalid vector values"),
            Self::RateLimitExceeded => (
                StatusCode::TOO_MANY_REQUESTS,
                "rate limit exceeded: 1 command per tick",
            ),
            Self::RequestRateLimitExceeded => (
                StatusCode::TOO_MANY_REQUESTS,
                "rate limit exceeded: 5 requests per second per token",
            ),
            Self::SessionNotActive => (StatusCode::BAD_REQUEST, "session is not active"),
            Self::PlayerDestroyed => (StatusCode::BAD_REQUEST, "player_destroyed"),
            Self::Internal(ref err) => {
                tracing::error!("Внутренняя ошибка API: {}", err);
                (StatusCode::INTERNAL_SERVER_ERROR, "internal server error")
            }
        };

        (status, Json(ErrorResponseDto::new(msg))).into_response()
    }
}
