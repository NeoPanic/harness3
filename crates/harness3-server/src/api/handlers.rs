use super::status::capabilities;
use super::*;
pub(super) async fn snapshot(State(app): State<App>) -> PResult<Json<Value>> {
    let mut value = json!({"channels":app.store.channels(None)?["channels"],"artifacts":app.store.recent_artifacts()?,"research_jobs":app.store.researches()?,"messages":app.store.recent_messages()?,"builtin_plugins":harness3_platform::organization::packages(),"builtin_plugin_status":harness3_builtin_plugins::status(),"process_plugins":app.store.process_plugins()?,"process_plugin_execution":app.plugins.is_enabled(),"capacity":app.store.capacity()?,"fleets":app.store.fleets()?,"presets":app.store.list("presets")?,"plugins":app.store.list("plugins")?,"instances":app.store.list("instances")?,"runs":app.store.list("runs")?,"goals":app.store.list("goals")?,"capabilities":capabilities(),"models":app.models.describe(),"limits":{"instances":harness3_platform::limits::INSTANCES,"workers":harness3_platform::limits::WORKERS,"runs_displayed":100,"collection_limit":200,"active_goals":32,"tasks_per_goal":harness3_platform::limits::TASKS_PER_GOAL},"version":env!("CARGO_PKG_VERSION")});
    value["board"] = board_value(&app)?;
    Ok(Json(value))
}
pub(super) fn board_value(app: &App) -> PResult<Value> {
    let mut board = app.store.board_snapshot()?;
    board["router"] = match harness3_platform::routing::Router::from_env(&app.models) {
        Ok(router) => router.describe(),
        Err(error) => json!({"mode":"unavailable","error":error.1}),
    };
    board["routing"] = json!(app.store.board_routing_status()?);
    Ok(board)
}
pub(super) async fn board_list(State(app): State<App>) -> PResult<Json<Value>> {
    Ok(Json(board_value(&app)?))
}
pub(super) async fn board_create(
    State(app): State<App>,
    headers: HeaderMap,
    Json(input): Json<harness3_platform::board::PublishInput>,
) -> PResult<(StatusCode, Json<Value>)> {
    Ok((
        StatusCode::CREATED,
        Json(app.store.publish_board_task(&input, key(&headers)?)?),
    ))
}
pub(super) async fn board_show(
    State(app): State<App>,
    Path(id): Path<String>,
) -> PResult<Json<Value>> {
    Ok(Json(app.store.board_task(&id)?))
}
pub(super) async fn board_cancel(
    State(app): State<App>,
    Path(id): Path<String>,
) -> PResult<Json<Value>> {
    Ok(Json(app.store.cancel_board_task(&id)?))
}
pub(super) async fn publish(
    State(app): State<App>,
    Json(preset): Json<Value>,
) -> PResult<Json<Value>> {
    Ok(Json(app.store.publish_checked(preset, |protocol| {
        if *protocol != Protocol::Demo {
            app.models.get(protocol)?;
        }
        Ok(())
    })?))
}
pub(super) async fn install(
    State(app): State<App>,
    Json(bundle): Json<Bundle>,
) -> PResult<Json<Value>> {
    Ok(Json(app.store.install(bundle)?))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProcessInstall {
    source_dir: std::path::PathBuf,
}
pub(super) async fn install_process(
    State(app): State<App>,
    Json(input): Json<ProcessInstall>,
) -> PResult<Json<Value>> {
    Ok(Json(app.plugins.install(input.source_dir).await?))
}
pub(super) async fn process_plugins(State(app): State<App>) -> PResult<Json<Value>> {
    Ok(Json(
        json!({"process_plugins":app.store.process_plugins()?,"execution_enabled":app.plugins.is_enabled()}),
    ))
}
pub(super) async fn process_plugin(
    State(app): State<App>,
    Path((id, version)): Path<(String, String)>,
) -> PResult<Json<Value>> {
    Ok(Json(app.store.process_plugin(&id, &version)?))
}
pub(super) async fn process_plugin_action(
    State(app): State<App>,
    Path((id, version)): Path<(String, String)>,
    Json(input): Json<Action>,
) -> PResult<Json<Value>> {
    Ok(Json(
        app.plugins.action(&id, &version, &input.action).await?,
    ))
}
pub(super) fn key(headers: &HeaderMap) -> PResult<&str> {
    headers
        .get("idempotency-key")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| fault(400, "idempotency_key_required"))
}
pub(super) async fn fleets(State(app): State<App>) -> PResult<Json<Value>> {
    Ok(Json(json!({"fleets":app.store.fleets()?})))
}
pub(super) async fn fleet(State(app): State<App>, Path(id): Path<String>) -> PResult<Json<Value>> {
    Ok(Json(app.store.fleet(&id)?))
}
pub(super) async fn operation(
    State(app): State<App>,
    Path(id): Path<String>,
) -> PResult<Json<Value>> {
    Ok(Json(app.store.operation(&id)?))
}
pub(super) async fn fleet_action(
    State(app): State<App>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(input): Json<FleetAction>,
) -> PResult<(StatusCode, Json<Value>)> {
    Ok((
        StatusCode::ACCEPTED,
        Json(app.store.fleet_action(&id, &input, key(&headers)?)?),
    ))
}
pub(super) async fn spawn(
    State(app): State<App>,
    headers: HeaderMap,
    Json(input): Json<FleetPlan>,
) -> PResult<(StatusCode, Json<Value>)> {
    Ok((
        StatusCode::ACCEPTED,
        Json(app.store.create_fleet(&input, key(&headers)?)?),
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Action {
    action: String,
}
pub(super) async fn transition(
    State(app): State<App>,
    Path(agent): Path<String>,
    Json(input): Json<Action>,
) -> PResult<Json<Value>> {
    Ok(Json(app.store.transition(&agent, &input.action)?))
}
pub(super) async fn enqueue(
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
pub(super) async fn session(
    State(app): State<App>,
    Path(agent): Path<String>,
) -> PResult<Json<Value>> {
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
pub(super) async fn cancel(
    State(app): State<App>,
    Path(run): Path<String>,
) -> PResult<Json<Value>> {
    let result = app.store.cancel(&run)?;
    if let Some(token) = app.cancellations.lock().await.get(&run) {
        token.cancel();
    }
    Ok(Json(result))
}
pub(super) async fn events(
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

pub(super) async fn run(State(app): State<App>, Path(id): Path<String>) -> PResult<Json<Value>> {
    Ok(Json(app.store.run(&id)?))
}
pub(super) async fn goals(State(app): State<App>) -> PResult<Json<Value>> {
    Ok(Json(json!({"goals":app.store.list("goals")?})))
}
pub(super) async fn goal(State(app): State<App>, Path(id): Path<String>) -> PResult<Json<Value>> {
    Ok(Json(app.store.goal(&id)?))
}
pub(super) async fn create_goal(
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
pub(super) struct Review {
    review: String,
    #[serde(default = "first_round")]
    expected_round: i64,
    #[serde(default)]
    expected_context_revision: i64,
}
pub(super) fn first_round() -> i64 {
    1
}
pub(super) async fn accept_goal(
    State(app): State<App>,
    Path(id): Path<String>,
    Json(input): Json<Review>,
) -> PResult<Json<Value>> {
    Ok(Json(app.store.accept_goal_context(
        &id,
        &input.review,
        input.expected_round,
        input.expected_context_revision,
    )?))
}
pub(super) async fn cancel_goal(
    State(app): State<App>,
    Path(id): Path<String>,
) -> PResult<Json<Value>> {
    let result = app.store.cancel_goal(&id)?;
    let cancellations = app.cancellations.lock().await;
    for run in result["run_ids"].as_array().unwrap() {
        if let Some(token) = cancellations.get(run.as_str().unwrap()) {
            token.cancel();
        }
    }
    Ok(Json(result))
}

pub(super) async fn send_message(
    State(app): State<App>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(input): Json<harness3_platform::messaging::MessageInput>,
) -> PResult<(StatusCode, Json<Value>)> {
    Ok((
        StatusCode::ACCEPTED,
        Json(app.store.send_message(
            harness3_platform::messaging::Sender::Operator,
            &id,
            &input,
            key(&headers)?,
        )?),
    ))
}
pub(super) async fn messages(
    State(app): State<App>,
    Query(query): Query<harness3_platform::messaging::MessageQuery>,
) -> PResult<Json<Value>> {
    Ok(Json(app.store.messages(&query)?))
}
pub(super) async fn message(
    State(app): State<App>,
    Path(id): Path<String>,
) -> PResult<Json<Value>> {
    Ok(Json(app.store.message(&id)?))
}
pub(super) async fn message_action(
    State(app): State<App>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(input): Json<harness3_platform::messaging::MessageAction>,
) -> PResult<Json<Value>> {
    Ok(Json(app.store.message_action(
        &id,
        &input,
        key(&headers)?,
    )?))
}

pub(super) async fn research_list(State(app): State<App>) -> PResult<Json<Value>> {
    Ok(Json(
        json!({"channels":app.store.channels(None)?["channels"],"artifacts":app.store.recent_artifacts()?,"research_jobs":app.store.researches()?}),
    ))
}
pub(super) async fn research_create(
    State(app): State<App>,
    headers: HeaderMap,
    Json(input): Json<harness3_platform::research::ResearchInput>,
) -> PResult<(StatusCode, Json<Value>)> {
    Ok((
        StatusCode::ACCEPTED,
        Json(app.store.create_research(&input, key(&headers)?)?),
    ))
}
pub(super) async fn research_show(
    State(app): State<App>,
    Path(id): Path<String>,
) -> PResult<Json<Value>> {
    Ok(Json(app.store.research(&id)?))
}
pub(super) async fn research_cancel(
    State(app): State<App>,
    Path(id): Path<String>,
) -> PResult<Json<Value>> {
    Ok(Json(app.store.cancel_research(&id)?))
}

pub(super) async fn rework_goal(
    State(app): State<App>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(input): Json<harness3_platform::artifacts::ReworkInput>,
) -> PResult<(StatusCode, Json<Value>)> {
    Ok((
        StatusCode::ACCEPTED,
        Json(app.store.rework_goal(&id, &input, key(&headers)?)?),
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ArtifactQuery {
    goal_id: Option<String>,
    #[serde(default)]
    after: i64,
    limit: Option<usize>,
}
pub(super) async fn artifacts(
    State(app): State<App>,
    Query(q): Query<ArtifactQuery>,
) -> PResult<Json<Value>> {
    Ok(Json(app.store.artifacts(
        q.goal_id.as_deref(),
        q.after,
        q.limit.unwrap_or(50),
    )?))
}
pub(super) async fn artifact(
    State(app): State<App>,
    Path(id): Path<String>,
) -> PResult<Json<Value>> {
    Ok(Json(app.store.artifact(&id)?))
}

pub(super) async fn goal_context(
    State(app): State<App>,
    Path(id): Path<String>,
) -> PResult<Json<Value>> {
    Ok(Json(app.store.goal_context(&id)?))
}
pub(super) async fn context_history(
    State(app): State<App>,
    Path((id, key)): Path<(String, String)>,
) -> PResult<Json<Value>> {
    Ok(Json(app.store.context_history(&id, &key)?))
}
pub(super) async fn context_publish(
    State(app): State<App>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(input): Json<harness3_platform::collaboration::ContextInput>,
) -> PResult<Json<Value>> {
    Ok(Json(app.store.publish_context(
        harness3_platform::collaboration::Author::Operator,
        Some(&id),
        &input,
        key(&headers)?,
    )?))
}

pub(super) async fn task_show(
    State(app): State<App>,
    Path((id, task)): Path<(String, String)>,
) -> PResult<Json<Value>> {
    Ok(Json(app.store.task(&id, &task)?))
}
pub(super) async fn task_claim(
    State(app): State<App>,
    Path((id, task)): Path<(String, String)>,
    headers: HeaderMap,
    Json(input): Json<harness3_platform::claims::ClaimInput>,
) -> PResult<(StatusCode, Json<Value>)> {
    Ok((
        StatusCode::ACCEPTED,
        Json(app.store.claim_task(&id, &task, &input, key(&headers)?)?),
    ))
}
pub(super) async fn task_release(
    State(app): State<App>,
    Path((id, task)): Path<(String, String)>,
    headers: HeaderMap,
    Json(input): Json<harness3_platform::claims::ReleaseInput>,
) -> PResult<Json<Value>> {
    Ok(Json(app.store.release_task(
        &id,
        &task,
        &input,
        key(&headers)?,
    )?))
}

pub(super) async fn retry_verification(
    State(app): State<App>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(input): Json<harness3_platform::verification::Retry>,
) -> PResult<(StatusCode, Json<Value>)> {
    Ok((
        StatusCode::ACCEPTED,
        Json(app.store.retry_verification(&id, &input, key(&headers)?)?),
    ))
}

#[derive(Deserialize, Default)]
pub(super) struct WorkspaceQuery {
    #[serde(default)]
    path: String,
    goal_id: Option<String>,
}
pub(super) async fn workspace_catalog(State(app): State<App>) -> PResult<Json<Value>> {
    Ok(Json(app.store.workspace_catalog()?))
}
pub(super) async fn workspace_files(
    State(app): State<App>,
    Path(id): Path<String>,
    Query(q): Query<WorkspaceQuery>,
) -> PResult<Json<Value>> {
    Ok(Json(app.store.workspace_files(&id, &q.path)?))
}
pub(super) async fn workspace_file(
    State(app): State<App>,
    Path(id): Path<String>,
    Query(q): Query<WorkspaceQuery>,
) -> PResult<Json<Value>> {
    Ok(Json(app.store.workspace_read(&id, &q.path)?))
}
pub(super) async fn workspace_artifacts(
    State(app): State<App>,
    Query(q): Query<WorkspaceQuery>,
) -> PResult<Json<Value>> {
    Ok(Json(app.store.workspace_artifacts(q.goal_id.as_deref())?))
}
pub(super) async fn workspace_artifact(
    State(app): State<App>,
    Path(id): Path<String>,
) -> PResult<Json<Value>> {
    Ok(Json(app.store.workspace_artifact(&id)?))
}
pub(super) async fn channels(
    State(app): State<App>,
    Query(q): Query<WorkspaceQuery>,
) -> PResult<Json<Value>> {
    Ok(Json(app.store.channels(q.goal_id.as_deref())?))
}
pub(super) async fn channel_show(
    State(app): State<App>,
    Path(id): Path<String>,
) -> PResult<Json<Value>> {
    Ok(Json(app.store.channel(
        harness3_platform::channels::Actor::Operator,
        &id,
    )?))
}
pub(super) async fn channel_create(
    State(app): State<App>,
    headers: HeaderMap,
    Json(input): Json<harness3_platform::channels::CreateChannel>,
) -> PResult<Json<Value>> {
    Ok(Json(app.store.create_channel(&input, key(&headers)?)?))
}
pub(super) async fn channel_post(
    State(app): State<App>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(input): Json<harness3_platform::channels::ChannelPost>,
) -> PResult<Json<Value>> {
    Ok(Json(app.store.post_channel(
        harness3_platform::channels::Actor::Operator,
        &id,
        &input,
        key(&headers)?,
    )?))
}
pub(super) async fn channel_members(
    State(app): State<App>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(input): Json<harness3_platform::channels::ChannelMembers>,
) -> PResult<Json<Value>> {
    Ok(Json(app.store.set_channel_members(
        &id,
        &input,
        key(&headers)?,
    )?))
}
pub(super) async fn channel_subscription(
    State(app): State<App>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(input): Json<harness3_platform::channels::ChannelSubscription>,
) -> PResult<Json<Value>> {
    Ok(Json(app.store.subscribe_channel(
        harness3_platform::channels::Actor::Operator,
        &id,
        &input,
        key(&headers)?,
    )?))
}

pub(super) async fn goal_autonomy(
    State(app): State<App>,
    Path(id): Path<String>,
) -> PResult<Json<Value>> {
    Ok(Json(app.store.goal_autonomy(&id)?))
}
