use crate::{domain::*, models::Models, runtime::Cancellations, store::Store};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Path, Query, Request, State},
    http::{HeaderMap, StatusCode, header},
    middleware::{self, Next},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
};
use harness3::session::SessionStore;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashMap;

#[derive(Clone)]
pub struct App {
    pub store: Store,
    pub token: String,
    pub cancellations: Cancellations,
    pub models: Models,
}
impl IntoResponse for Fault {
    fn into_response(self) -> Response {
        (
            StatusCode::from_u16(self.0).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
            Json(json!({"error":{"code":self.1}})),
        )
            .into_response()
    }
}
fn authorized(headers: &HeaderMap, token: &str) -> bool {
    let bearer = headers
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "));
    let cookie = headers
        .get(header::COOKIE)
        .and_then(|h| h.to_str().ok())
        .and_then(|s| {
            s.split(';')
                .find_map(|p| p.trim().strip_prefix("harness3_token="))
        });
    bearer
        .or(cookie)
        .is_some_and(|candidate| constant_time_eq(candidate.as_bytes(), token.as_bytes()))
}
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (a, b)| acc | (a ^ b)) == 0
}
async fn guard(State(app): State<App>, request: Request, next: Next) -> Response {
    let headers = request.headers();
    let host = headers
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .unwrap_or_default();
    let hostname = host.split(':').next().unwrap_or_default();
    if !["127.0.0.1", "localhost"].contains(&hostname) {
        return fault(403, "loopback_host_required").into_response();
    }
    if let Some(origin) = headers.get(header::ORIGIN).and_then(|v| v.to_str().ok())
        && origin != format!("http://{host}")
    {
        return fault(403, "cross_origin_denied").into_response();
    }
    if request.uri().path().starts_with("/api/")
        && request.uri().path() != "/api/login"
        && !authorized(headers, &app.token)
    {
        return fault(401, "authentication_required").into_response();
    }
    let mut response = next.run(request).await;
    for (name, value) in [
        (
            "content-security-policy",
            "default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self' data:; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'",
        ),
        ("x-content-type-options", "nosniff"),
        ("referrer-policy", "no-referrer"),
        ("cache-control", "no-store"),
    ] {
        response.headers_mut().insert(
            axum::http::HeaderName::from_static(name),
            axum::http::HeaderValue::from_static(value),
        );
    }
    response
}
pub fn router(app: App) -> Router {
    Router::new()
        .route(
            "/",
            get(|| async { Html(include_str!("../web/index.html")) }),
        )
        .route(
            "/app.js",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
                    include_str!("../web/app.js"),
                )
            }),
        )
        .route(
            "/app.css",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/css; charset=utf-8")],
                    include_str!("../web/app.css"),
                )
            }),
        )
        .route(
            "/goals.js",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
                    include_str!("../web/goals.js"),
                )
            }),
        )
        .route(
            "/mark.svg",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "image/svg+xml")],
                    include_str!("../../../docs/brand/mark.svg"),
                )
            }),
        )
        .route(
            "/health",
            get(|| async {
                Json(json!({"status":"ok","product":"Harness³","stage":"kernel-preview"}))
            }),
        )
        .route("/api/login", post(login))
        .route("/api/logout", post(logout))
        .route("/api/v1/snapshot", get(snapshot))
        .route("/api/v1/presets", post(publish))
        .route("/api/v1/plugins", post(install))
        .route("/api/v1/agent-fleets", post(spawn))
        .route("/api/v1/agent-instances/{id}/actions", post(transition))
        .route("/api/v1/agent-instances/{id}/runs", post(enqueue))
        .route("/api/v1/agent-instances/{id}/session", get(session))
        .route("/api/v1/runs/{id}", get(run))
        .route("/api/v1/runs/{id}/cancel", post(cancel))
        .route("/api/v1/goals", get(goals).post(create_goal))
        .route("/api/v1/goals/{id}", get(goal))
        .route("/api/v1/goals/{id}/cancel", post(cancel_goal))
        .route("/api/v1/goals/{id}/accept", post(accept_goal))
        .route("/api/v1/events", get(events))
        .layer(DefaultBodyLimit::max(128 * 1024))
        .layer(middleware::from_fn_with_state(app.clone(), guard))
        .with_state(app)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Login {
    token: String,
}
async fn login(State(app): State<App>, Json(input): Json<Login>) -> PResult<Response> {
    if !constant_time_eq(input.token.as_bytes(), app.token.as_bytes()) {
        return Err(fault(401, "invalid_token"));
    }
    Ok((
        [(
            header::SET_COOKIE,
            format!(
                "harness3_token={}; HttpOnly; SameSite=Strict; Path=/",
                app.token
            ),
        )],
        Json(json!({"authenticated":true})),
    )
        .into_response())
}
async fn logout() -> Response {
    (
        [(
            header::SET_COOKIE,
            "harness3_token=; HttpOnly; SameSite=Strict; Path=/; Max-Age=0",
        )],
        Json(json!({"authenticated":false})),
    )
        .into_response()
}
async fn snapshot(State(app): State<App>) -> PResult<Json<Value>> {
    Ok(Json(
        json!({"presets":app.store.list("presets")?,"plugins":app.store.list("plugins")?,"instances":app.store.list("instances")?,"runs":app.store.list("runs")?,"goals":app.store.list("goals")?,"capabilities":capabilities(),"models":app.models.describe(),"limits":{"instances":64,"workers":4,"runs_displayed":100,"collection_limit":200,"active_goals":32,"tasks_per_goal":16},"version":env!("CARGO_PKG_VERSION")}),
    ))
}
async fn publish(State(app): State<App>, Json(preset): Json<Preset>) -> PResult<Json<Value>> {
    if preset.protocol != Protocol::Demo {
        app.models.get(&preset.protocol)?;
    }
    Ok(Json(app.store.publish(preset)?))
}
async fn install(State(app): State<App>, Json(bundle): Json<Bundle>) -> PResult<Json<Value>> {
    Ok(Json(app.store.install(bundle)?))
}
fn key(headers: &HeaderMap) -> PResult<&str> {
    headers
        .get("idempotency-key")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| fault(400, "idempotency_key_required"))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FleetRequest {
    preset_name: String,
    preset_version: String,
    count: usize,
    #[serde(default)]
    ttl_seconds: Option<u64>,
}
async fn spawn(
    State(app): State<App>,
    headers: HeaderMap,
    Json(input): Json<FleetRequest>,
) -> PResult<(StatusCode, Json<Value>)> {
    Ok((
        StatusCode::CREATED,
        Json(app.store.spawn_with_ttl(
            &Spawn {
                preset_name: input.preset_name,
                preset_version: input.preset_version,
                count: input.count,
            },
            key(&headers)?,
            input.ttl_seconds,
        )?),
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Action {
    action: String,
}
async fn transition(
    State(app): State<App>,
    Path(agent): Path<String>,
    Json(input): Json<Action>,
) -> PResult<Json<Value>> {
    Ok(Json(app.store.transition(&agent, &input.action)?))
}
async fn enqueue(
    State(app): State<App>,
    Path(agent): Path<String>,
    headers: HeaderMap,
    Json(input): Json<RunInput>,
) -> PResult<(StatusCode, Json<Value>)> {
    Ok((
        StatusCode::ACCEPTED,
        Json(app.store.enqueue(&agent, &input, key(&headers)?)?),
    ))
}
async fn session(State(app): State<App>, Path(agent): Path<String>) -> PResult<Json<Value>> {
    if !app.store.instance_exists(&agent)? {
        return Err(fault(404, "instance_not_found"));
    }
    let session = app
        .store
        .load(&agent)
        .await
        .map_err(|_| fault(500, "session_storage_error"))?;
    Ok(Json(serde_json::to_value(session)?))
}
async fn cancel(State(app): State<App>, Path(run): Path<String>) -> PResult<Json<Value>> {
    let result = app.store.cancel(&run)?;
    if let Some(token) = app.cancellations.lock().await.get(&run) {
        token.cancel();
    }
    Ok(Json(result))
}
async fn events(
    State(app): State<App>,
    Query(query): Query<HashMap<String, String>>,
) -> PResult<Json<Value>> {
    let after = query
        .get("after")
        .map(|s| s.parse::<i64>())
        .transpose()
        .map_err(|_| fault(400, "invalid_cursor"))?
        .unwrap_or(0);
    Ok(Json(json!({"events":app.store.events(after)?})))
}

async fn run(State(app): State<App>, Path(id): Path<String>) -> PResult<Json<Value>> {
    Ok(Json(app.store.run(&id)?))
}
async fn goals(State(app): State<App>) -> PResult<Json<Value>> {
    Ok(Json(json!({"goals":app.store.list("goals")?})))
}
async fn goal(State(app): State<App>, Path(id): Path<String>) -> PResult<Json<Value>> {
    Ok(Json(app.store.goal(&id)?))
}
async fn create_goal(
    State(app): State<App>,
    headers: HeaderMap,
    Json(plan): Json<GoalPlan>,
) -> PResult<(StatusCode, Json<Value>)> {
    Ok((
        StatusCode::ACCEPTED,
        Json(app.store.create_goal(&plan, key(&headers)?)?),
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Review {
    review: String,
}
async fn accept_goal(
    State(app): State<App>,
    Path(id): Path<String>,
    Json(input): Json<Review>,
) -> PResult<Json<Value>> {
    Ok(Json(app.store.accept_goal(&id, &input.review)?))
}
async fn cancel_goal(State(app): State<App>, Path(id): Path<String>) -> PResult<Json<Value>> {
    let result = app.store.cancel_goal(&id)?;
    let cancellations = app.cancellations.lock().await;
    for run in result["run_ids"].as_array().unwrap() {
        if let Some(token) = cancellations.get(run.as_str().unwrap()) {
            token.cancel();
        }
    }
    Ok(Json(result))
}
