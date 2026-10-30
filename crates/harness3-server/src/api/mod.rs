use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Path, Query, Request, State},
    http::{HeaderMap, StatusCode, header},
    middleware::{self, Next},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
};
use harness3::session::SessionStore;
use harness3_models::Models;
use harness3_platform::{
    domain::*,
    fleets::{FleetAction, FleetPlan},
    store::Store,
};
use harness3_worker::Cancellations;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashMap;

#[derive(Clone)]
pub struct App {
    pub store: Store,
    pub token: String,
    pub cancellations: Cancellations,
    pub models: Models,
    pub plugins: harness3_plugins::ProcessHost,
}
type PResult<T> = Result<T, ApiError>;
#[derive(Debug)]
struct ApiError(u16, String);
impl From<harness3_platform::domain::Fault> for ApiError {
    fn from(e: harness3_platform::domain::Fault) -> Self {
        Self(e.0, e.1)
    }
}
impl From<harness3_domain::Fault> for ApiError {
    fn from(e: harness3_domain::Fault) -> Self {
        Self(e.0, e.1)
    }
}
impl From<serde_json::Error> for ApiError {
    fn from(_: serde_json::Error) -> Self {
        Self(500, "stored_data_invalid".into())
    }
}
fn fault(code: u16, message: &str) -> ApiError {
    ApiError(code, message.into())
}
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            StatusCode::from_u16(self.0).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
            Json(json!({"error":{"code":self.1}})),
        )
            .into_response()
    }
}

mod auth;
mod handlers;
mod routes;
mod status;
pub use routes::router;
