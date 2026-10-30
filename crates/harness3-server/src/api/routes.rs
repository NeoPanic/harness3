use super::auth::{guard, login, logout};
use super::handlers::*;
use super::*;
pub fn router(app: App) -> Router {
    Router::new()
        .route(
            "/board.js",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
                    include_str!(concat!(
                        env!("CARGO_MANIFEST_DIR"),
                        "/../../web/src/features/board/board.js"
                    )),
                )
            }),
        )
        .route("/api/board", get(board_list))
        .route("/api/board/tasks", post(board_create))
        .route("/api/board/tasks/{id}", get(board_show))
        .route("/api/board/tasks/{id}/cancel", post(board_cancel))
        .route("/api/v1/board", get(board_list))
        .route("/api/v1/board/tasks", post(board_create))
        .route("/api/v1/board/tasks/{id}", get(board_show))
        .route("/api/v1/board/tasks/{id}/cancel", post(board_cancel))
        .route(
            "/studio.css",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/css; charset=utf-8")],
                    include_str!(concat!(
                        env!("CARGO_MANIFEST_DIR"),
                        "/../../web/src/features/studio/studio.css"
                    )),
                )
            }),
        )
        .route(
            "/studio.js",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
                    include_str!(concat!(
                        env!("CARGO_MANIFEST_DIR"),
                        "/../../web/src/features/studio/studio.js"
                    )),
                )
            }),
        )
        .route(
            "/",
            get(|| async {
                Html(include_str!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../../web/index.html"
                )))
            }),
        )
        .route(
            "/app.js",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
                    include_str!(concat!(
                        env!("CARGO_MANIFEST_DIR"),
                        "/../../web/src/app/app.js"
                    )),
                )
            }),
        )
        .route(
            "/app.css",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/css; charset=utf-8")],
                    include_str!(concat!(
                        env!("CARGO_MANIFEST_DIR"),
                        "/../../web/src/app/app.css"
                    )),
                )
            }),
        )
        .route(
            "/presets.js",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
                    include_str!(concat!(
                        env!("CARGO_MANIFEST_DIR"),
                        "/../../web/src/features/presets/presets.js"
                    )),
                )
            }),
        )
        .route(
            "/plugins.js",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
                    include_str!(concat!(
                        env!("CARGO_MANIFEST_DIR"),
                        "/../../web/src/features/plugins/plugins.js"
                    )),
                )
            }),
        )
        .route(
            "/fleets.js",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
                    include_str!(concat!(
                        env!("CARGO_MANIFEST_DIR"),
                        "/../../web/src/features/agents/fleets.js"
                    )),
                )
            }),
        )
        .route(
            "/goals.js",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
                    include_str!(concat!(
                        env!("CARGO_MANIFEST_DIR"),
                        "/../../web/src/features/goals/goals.js"
                    )),
                )
            }),
        )
        .route(
            "/mark.svg",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "image/svg+xml")],
                    include_str!(concat!(
                        env!("CARGO_MANIFEST_DIR"),
                        "/../../docs/brand/mark.svg"
                    )),
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
        .route(
            "/research.js",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
                    include_str!(concat!(
                        env!("CARGO_MANIFEST_DIR"),
                        "/../../web/src/features/research/research.js"
                    )),
                )
            }),
        )
        .route(
            "/api/v1/research-jobs",
            get(research_list).post(research_create),
        )
        .route("/api/v1/research-jobs/{id}", get(research_show))
        .route("/api/v1/research-jobs/{id}/cancel", post(research_cancel))
        .route(
            "/messages.js",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
                    include_str!(concat!(
                        env!("CARGO_MANIFEST_DIR"),
                        "/../../web/src/features/messages/messages.js"
                    )),
                )
            }),
        )
        .route("/api/v1/agent-instances/{id}/messages", post(send_message))
        .route("/api/v1/messages", get(messages))
        .route("/api/v1/messages/{id}", get(message))
        .route("/api/v1/messages/{id}/actions", post(message_action))
        .route("/api/v1/presets", post(publish))
        .route("/api/v1/plugins", post(install))
        .route(
            "/api/v1/process-plugins",
            get(process_plugins).post(install_process),
        )
        .route(
            "/api/v1/process-plugins/{id}/{version}",
            get(process_plugin),
        )
        .route(
            "/api/v1/process-plugins/{id}/{version}/actions",
            post(process_plugin_action),
        )
        .route("/api/v1/agent-fleets", get(fleets).post(spawn))
        .route("/api/v1/agent-fleets/{id}", get(fleet))
        .route("/api/v1/agent-fleets/{id}/actions", post(fleet_action))
        .route("/api/v1/operations/{id}", get(operation))
        .route("/api/v1/agent-instances/{id}/actions", post(transition))
        .route("/api/v1/agent-instances/{id}/runs", post(enqueue))
        .route("/api/v1/agent-instances/{id}/session", get(session))
        .route("/api/v1/runs/{id}", get(run))
        .route("/api/v1/runs/{id}/cancel", post(cancel))
        .route("/api/v1/goals", get(goals).post(create_goal))
        .route("/api/v1/goals/{id}", get(goal))
        .route(
            "/verification.js",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
                    include_str!(concat!(
                        env!("CARGO_MANIFEST_DIR"),
                        "/../../web/src/features/goals/verification.js"
                    )),
                )
            }),
        )
        .route(
            "/tasks.js",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
                    include_str!(concat!(
                        env!("CARGO_MANIFEST_DIR"),
                        "/../../web/src/features/goals/tasks.js"
                    )),
                )
            }),
        )
        .route("/api/v1/goals/{id}/tasks/{task}", get(task_show))
        .route("/api/v1/goals/{id}/tasks/{task}/claim", post(task_claim))
        .route(
            "/api/v1/goals/{id}/tasks/{task}/release",
            post(task_release),
        )
        .route("/api/v1/goals/{id}/cancel", post(cancel_goal))
        .route("/api/v1/goals/{id}/accept", post(accept_goal))
        .route(
            "/api/v1/goals/{id}/verification/retry",
            post(retry_verification),
        )
        .route(
            "/api/v1/goals/{id}/context",
            get(goal_context).post(context_publish),
        )
        .route("/api/v1/goals/{id}/context/{key}", get(context_history))
        .route(
            "/collaboration.js",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
                    include_str!(concat!(
                        env!("CARGO_MANIFEST_DIR"),
                        "/../../web/src/features/collaboration/context.js"
                    )),
                )
            }),
        )
        .route("/api/v1/goals/{id}/rework", post(rework_goal))
        .route("/api/v1/artifacts", get(artifacts))
        .route("/api/v1/artifacts/{id}", get(artifact))
        .route(
            "/artifacts.js",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
                    include_str!(concat!(
                        env!("CARGO_MANIFEST_DIR"),
                        "/../../web/src/features/artifacts/artifacts.js"
                    )),
                )
            }),
        )
        .route("/api/v1/workspaces", get(workspace_catalog))
        .route(
            "/api/v1/agent-instances/{id}/workspace",
            get(workspace_files),
        )
        .route(
            "/api/v1/agent-instances/{id}/workspace/file",
            get(workspace_file),
        )
        .route("/api/v1/workspace-artifacts", get(workspace_artifacts))
        .route("/api/v1/workspace-artifacts/{id}", get(workspace_artifact))
        .route("/api/v1/channels", get(channels).post(channel_create))
        .route("/api/v1/channels/{id}", get(channel_show))
        .route("/api/v1/channels/{id}/posts", post(channel_post))
        .route("/api/v1/channels/{id}/members", post(channel_members))
        .route(
            "/api/v1/channels/{id}/subscription",
            post(channel_subscription),
        )
        .route(
            "/autonomy.js",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
                    include_str!(concat!(
                        env!("CARGO_MANIFEST_DIR"),
                        "/../../web/src/features/autonomy/autonomy.js"
                    )),
                )
            }),
        )
        .route("/api/v1/goals/{id}/autonomy", get(goal_autonomy))
        .route(
            "/workspaces.js",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
                    include_str!(concat!(
                        env!("CARGO_MANIFEST_DIR"),
                        "/../../web/src/features/workspaces/workspaces.js"
                    )),
                )
            }),
        )
        .route(
            "/channels.js",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
                    include_str!(concat!(
                        env!("CARGO_MANIFEST_DIR"),
                        "/../../web/src/features/channels/channels.js"
                    )),
                )
            }),
        )
        .route("/api/v1/events", get(events))
        .layer(DefaultBodyLimit::max(128 * 1024))
        .layer(middleware::from_fn_with_state(app.clone(), guard))
        .with_state(app)
}
