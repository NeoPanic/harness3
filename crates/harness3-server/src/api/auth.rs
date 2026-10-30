use super::*;
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
pub(super) async fn guard(State(app): State<App>, request: Request, next: Next) -> Response {
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
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Login {
    token: String,
}
pub(super) async fn login(State(app): State<App>, Json(input): Json<Login>) -> PResult<Response> {
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
pub(super) async fn logout() -> Response {
    (
        [(
            header::SET_COOKIE,
            "harness3_token=; HttpOnly; SameSite=Strict; Path=/; Max-Age=0",
        )],
        Json(json!({"authenticated":false})),
    )
        .into_response()
}
