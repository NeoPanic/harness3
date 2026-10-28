use harness3::{
    CancellationToken, ModelInput, Turn,
    session::{Session, SessionStore},
};
use harness3_platform::{
    api::{App, router},
    domain::*,
    models::{Models, Provider},
    runtime,
    store::Store,
};
use serde_json::{Value, json};
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokio::sync::Mutex;
use tower::ServiceExt;

fn preset() -> Preset {
    Preset {
        name: "inspector".into(),
        version: "1.0.0".into(),
        purpose: "inspect".into(),
        objective: "count text with evidence".into(),
        instructions: "Inspect input text.".into(),
        protocol: Protocol::Demo,
        plugin: None,
    }
}
fn seeded() -> Store {
    let store = Store::memory().unwrap();
    store.publish(preset()).unwrap();
    store
}
fn spawn(store: &Store, count: usize, key: &str) -> Value {
    store
        .spawn(
            &Spawn {
                preset_name: "inspector".into(),
                preset_version: "1.0.0".into(),
                count,
            },
            key,
        )
        .unwrap()
}
fn input(text: &str) -> RunInput {
    RunInput { text: text.into() }
}
fn fault_code<T>(result: PResult<T>) -> String {
    match result {
        Err(error) => error.1,
        Ok(_) => panic!("expected failure"),
    }
}

#[test]
fn immutable_presets_and_atomic_idempotent_batches() {
    let store = seeded();
    let mut changed = preset();
    changed.instructions = "different".into();
    assert_eq!(
        fault_code(store.publish(changed)),
        "preset_version_immutable"
    );
    let first = spawn(&store, 3, "batch");
    assert_eq!(first, spawn(&store, 3, "batch"));
    assert_eq!(store.list("instances").unwrap().len(), 3);
    assert_eq!(
        fault_code(store.spawn(
            &Spawn {
                preset_name: "inspector".into(),
                preset_version: "1.0.0".into(),
                count: 4
            },
            "batch"
        )),
        "idempotency_conflict"
    );
    assert_eq!(
        fault_code(store.spawn(
            &Spawn {
                preset_name: "inspector".into(),
                preset_version: "1.0.0".into(),
                count: 64
            },
            "too-many"
        )),
        "instance_quota_exceeded"
    );
    assert_eq!(store.list("instances").unwrap().len(), 3);
}
#[test]
fn prompt_plugin_snapshot_cannot_drift() {
    let store = seeded();
    let bundle = Bundle {
        id: "local.prompt".into(),
        version: "1".into(),
        description: "test".into(),
        instructions: "old instructions".into(),
    };
    store.install(bundle.clone()).unwrap();
    let mut p = preset();
    p.name = "bound".into();
    p.plugin = Some(PluginRef {
        id: bundle.id.clone(),
        version: bundle.version.clone(),
    });
    let old = store.publish(p).unwrap();
    let mut changed = bundle.clone();
    changed.instructions = "silently changed".into();
    assert_eq!(
        fault_code(store.install(changed.clone())),
        "plugin_version_immutable"
    );
    changed.version = "2".into();
    store.install(changed).unwrap();
    assert_eq!(old["plugin_snapshot"]["instructions"], "old instructions");
    assert_eq!(
        store
            .list("presets")
            .unwrap()
            .iter()
            .find(|p| p["name"] == "bound")
            .unwrap()["definition"],
        old
    );
}
#[test]
fn lifecycle_and_single_active_run_enforced() {
    let store = seeded();
    let batch = spawn(&store, 1, "batch");
    let agent = batch["instance_ids"][0].as_str().unwrap();
    store.transition(agent, "suspend").unwrap();
    assert_eq!(
        fault_code(store.enqueue(agent, &input("hi"), "a")),
        "instance_not_ready"
    );
    store.transition(agent, "resume").unwrap();
    let run = store.enqueue(agent, &input("hi"), "a").unwrap();
    assert_eq!(run, store.enqueue(agent, &input("hi"), "a").unwrap());
    assert_eq!(
        fault_code(store.enqueue(agent, &input("different"), "a")),
        "idempotency_conflict"
    );
    assert_eq!(
        fault_code(store.enqueue(agent, &input("second"), "b")),
        "instance_not_ready"
    );
    assert_eq!(
        fault_code(store.transition(agent, "terminate")),
        "invalid_transition_or_instance_busy"
    );
    store.cancel(run["id"].as_str().unwrap()).unwrap();
    assert!(store.claim().unwrap().is_none());
    store.transition(agent, "terminate").unwrap();
    assert_eq!(
        fault_code(store.transition(agent, "resume")),
        "invalid_transition_or_instance_busy"
    );
}
#[tokio::test]
async fn sqlite_session_store_rejects_stale_writer_and_survives_reopen() {
    let path = std::env::temp_dir().join(format!("harness3-test-{}.sqlite", uuid::Uuid::new_v4()));
    let store = Store::open(&path).unwrap();
    let session = Session::default();
    store.save("s", 0, &session).await.unwrap();
    assert_eq!(
        store.save("s", 0, &session).await.unwrap_err().code,
        "session_conflict"
    );
    drop(store);
    let store = Store::open(&path).unwrap();
    assert_eq!(store.load("s").await.unwrap().revision, 1);
    drop(store);
    let _ = std::fs::remove_file(path);
}
#[tokio::test]
async fn real_harness_execution_is_persisted_and_agents_are_independent() {
    let store = seeded();
    let batch = spawn(&store, 2, "batch");
    let agents = batch["instance_ids"].as_array().unwrap();
    let run = store
        .enqueue(agents[0].as_str().unwrap(), &input("Harness³"), "task")
        .unwrap();
    let stop = CancellationToken::new();
    let cancellations = Arc::new(Mutex::new(HashMap::new()));
    let worker = tokio::spawn(runtime::worker_loop(
        store.clone(),
        Models::default(),
        cancellations,
        stop.clone(),
    ));
    for _ in 0..100 {
        if store.list("runs").unwrap()[0]["state"] == "completed" {
            break;
        }
        tokio::time::sleep(Duration::from_millis(30)).await;
    }
    stop.cancel();
    worker.await.unwrap();
    let settled = store.list("runs").unwrap();
    assert_eq!(settled[0]["state"], "completed");
    assert_eq!(
        settled[0]["report"]["observations"][0]["output"]["content"]["characters"],
        8
    );
    assert_eq!(
        run,
        store
            .enqueue(agents[0].as_str().unwrap(), &input("Harness³"), "task")
            .unwrap()
    );
    let session = store.load(agents[0].as_str().unwrap()).await.unwrap();
    assert_eq!(session.requests.len(), 2);
    assert!(!session.events.is_empty());
    assert_eq!(
        store
            .load(agents[1].as_str().unwrap())
            .await
            .unwrap()
            .revision,
        0
    );
}
#[tokio::test]
async fn startup_recovery_does_not_replay_an_uncertain_run() {
    let store = seeded();
    let batch = spawn(&store, 1, "batch");
    let agent = batch["instance_ids"][0].as_str().unwrap();
    store.enqueue(agent, &input("operation"), "run").unwrap();
    store.claim().unwrap().unwrap();
    runtime::recover(&store).await.unwrap();
    assert_eq!(store.list("runs").unwrap()[0]["state"], "interrupted");
    assert!(store.claim().unwrap().is_none());
    assert_eq!(store.list("instances").unwrap()[0]["state"], "ready");
}
#[tokio::test]
async fn recovery_preserves_known_receipt_after_platform_settlement_crash() {
    let store = seeded();
    let batch = spawn(&store, 1, "batch");
    let agent = batch["instance_ids"][0].as_str().unwrap();
    let run = store.enqueue(agent, &input("operation"), "run").unwrap();
    store.claim().unwrap().unwrap();
    let mut session = Session::default();
    session.completed.insert(
        serde_json::to_string(&("platform", run["id"].as_str().unwrap())).unwrap(),
        harness3::Report {
            run_id: "kernel-uuid".into(),
            output: Some("known output".into()),
            error: None,
            observations: vec![],
            usage: None,
            generation_ms: 0,
            execution_ms: None,
        },
    );
    store.save(agent, 0, &session).await.unwrap();
    runtime::recover(&store).await.unwrap();
    let runs = store.list("runs").unwrap();
    assert_eq!(runs[0]["state"], "completed");
    assert_eq!(runs[0]["report"]["output"], "known output");
}
#[tokio::test]
async fn cancellation_drains_harness_and_releases_instance() {
    let store = seeded();
    let batch = spawn(&store, 1, "batch");
    let agent = batch["instance_ids"][0].as_str().unwrap();
    let run = store.enqueue(agent, &input("cancel me"), "run").unwrap();
    let id = run["id"].as_str().unwrap();
    let stop = CancellationToken::new();
    let cancellations = Arc::new(Mutex::new(HashMap::new()));
    let worker = tokio::spawn(runtime::worker_loop(
        store.clone(),
        Models::default(),
        cancellations.clone(),
        stop.clone(),
    ));
    for _ in 0..100 {
        if cancellations.lock().await.contains_key(id) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    store.cancel(id).unwrap();
    if let Some(token) = cancellations.lock().await.get(id) {
        token.cancel();
    }
    for _ in 0..100 {
        if store.list("runs").unwrap()[0]["state"] == "cancelled" {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    stop.cancel();
    worker.await.unwrap();
    assert_eq!(store.list("runs").unwrap()[0]["state"], "cancelled");
    assert_eq!(store.list("instances").unwrap()[0]["state"], "ready");
    assert!(store.load(agent).await.unwrap().active_run.is_none());
}
fn model_input() -> ModelInput {
    ModelInput {
        system: "system".into(),
        text: "hello".into(),
        tools: vec![],
        turns: vec![],
    }
}
#[test]
fn three_protocols_map_tools_and_usage_without_claiming_truncated_success() {
    let chat = json!({"choices":[{"finish_reason":"tool_calls","message":{"content":null,"tool_calls":[{"id":"c1","type":"function","function":{"name":"text.inspect","arguments":"{\"text\":\"hi\"}"}}]}}],"usage":{"prompt_tokens":10,"completion_tokens":3}});
    let reply = Provider::decode(&Protocol::OpenaiCompatible, chat.clone()).unwrap();
    assert_eq!(reply.calls[0].arguments["text"], "hi");
    assert_eq!(reply.usage.unwrap().input, 10);
    let mut truncated = chat;
    truncated["choices"][0]["finish_reason"] = json!("length");
    assert_eq!(
        Provider::decode(&Protocol::OpenaiCompatible, truncated)
            .err()
            .expect("expected provider failure")
            .code,
        "provider_incomplete"
    );
    let responses = json!({"status":"completed","output":[{"type":"reasoning","encrypted_content":"opaque"},{"type":"function_call","call_id":"c2","name":"text.inspect","arguments":"{\"text\":\"hi\"}"}],"usage":{"input_tokens":14,"output_tokens":4}});
    let reply = Provider::decode(&Protocol::OpenaiResponses, responses).unwrap();
    assert_eq!(reply.calls[0].id, "c2");
    assert_eq!(
        reply.continuation["output"][0]["encrypted_content"],
        "opaque"
    );
    let provider = Provider::new(
        Protocol::OpenaiResponses,
        "fixture".into(),
        "http://localhost/v1".into(),
        "secret".into(),
    )
    .unwrap();
    let mut request = model_input();
    request.turns.push(Turn::Assistant(reply));
    let body = provider.request(&request, 123);
    assert_eq!(body["input"][1]["encrypted_content"], "opaque");
    assert_eq!(body["max_output_tokens"], 123);
    assert!(!body.to_string().contains("secret"));
    let anthropic = json!({"stop_reason":"tool_use","content":[{"type":"tool_use","id":"a1","name":"text.inspect","input":{"text":"hi"}}],"usage":{"input_tokens":10,"output_tokens":3,"cache_read_input_tokens":20,"cache_creation_input_tokens":5}});
    let reply = Provider::decode(&Protocol::Anthropic, anthropic).unwrap();
    assert_eq!(reply.calls[0].id, "a1");
    assert_eq!(reply.usage.unwrap().input, 35);
}
#[test]
fn protocol_refusals_invalid_json_and_unsupported_blocks_are_explicit() {
    let bad = json!({"status":"completed","output":[{"type":"function_call","call_id":"1","name":"tool","arguments":"{"}]});
    assert_eq!(
        Provider::decode(&Protocol::OpenaiResponses, bad)
            .err()
            .expect("expected invalid tool JSON")
            .code,
        "invalid_tool_json"
    );
    let refusal = json!({"status":"completed","output":[{"type":"message","content":[{"type":"refusal","refusal":"no"}]}]});
    assert_eq!(
        Provider::decode(&Protocol::OpenaiResponses, refusal)
            .err()
            .expect("expected provider failure")
            .code,
        "provider_refusal"
    );
    assert!(
        Provider::new(
            Protocol::OpenaiResponses,
            "x".into(),
            "https://secret:password@example.com/v1".into(),
            "".into()
        )
        .is_err()
    );
}
#[test]
fn anthropic_merges_adjacent_context_and_preserves_tool_results() {
    let provider = Provider::new(
        Protocol::Anthropic,
        "fixture".into(),
        "http://localhost/v1".into(),
        "".into(),
    )
    .unwrap();
    let mut input = model_input();
    input.turns.push(Turn::Context(json!({"fact":"x"})));
    let body = provider.request(&input, 400);
    assert_eq!(body["messages"].as_array().unwrap().len(), 1);
    assert_eq!(body["messages"][0]["content"].as_array().unwrap().len(), 2);
}
#[tokio::test]
async fn api_auth_origin_and_csrf_boundaries() {
    let app = router(App {
        store: seeded(),
        token: "a".repeat(64),
        cancellations: Arc::new(Mutex::new(HashMap::new())),
        models: Models::default(),
    });
    let req = |path: &str, token: Option<&str>, origin: Option<&str>, host: &str| {
        let mut builder = axum::http::Request::builder()
            .uri(path)
            .header("Host", host);
        if let Some(token) = token {
            builder = builder.header("Authorization", format!("Bearer {token}"));
        }
        if let Some(origin) = origin {
            builder = builder.header("Origin", origin);
        }
        builder.body(axum::body::Body::empty()).unwrap()
    };
    assert_eq!(
        app.clone()
            .oneshot(req("/api/v1/snapshot", None, None, "127.0.0.1:7333"))
            .await
            .unwrap()
            .status(),
        401
    );
    assert_eq!(
        app.clone()
            .oneshot(req(
                "/api/v1/snapshot",
                Some(&"a".repeat(64)),
                None,
                "127.0.0.1:7333"
            ))
            .await
            .unwrap()
            .status(),
        200
    );
    assert_eq!(
        app.clone()
            .oneshot(req(
                "/api/v1/snapshot",
                Some(&"a".repeat(64)),
                Some("https://evil.example"),
                "127.0.0.1:7333"
            ))
            .await
            .unwrap()
            .status(),
        403
    );
    assert_eq!(
        app.oneshot(req("/", None, None, "attacker.example"))
            .await
            .unwrap()
            .status(),
        403
    );
}
#[tokio::test]
async fn provider_posts_real_http_with_expected_tool_protocol() {
    use axum::{Json, Router, routing::post};
    let service=Router::new().route("/v1/responses",post(|Json(body):Json<Value>|async move{
        assert_eq!(body["model"],"fixture");assert_eq!(body["store"],false);
        Json(json!({"status":"completed","output":[{"type":"message","content":[{"type":"output_text","text":"fixture reply"}]}]}))
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = tokio::spawn(async move { axum::serve(listener, service).await.unwrap() });
    let provider = Provider::new(
        Protocol::OpenaiResponses,
        "fixture".into(),
        format!("http://127.0.0.1:{port}/v1"),
        "".into(),
    )
    .unwrap();
    use harness3::Model;
    let reply = provider
        .complete(
            model_input(),
            &harness3::LiveOutput::default(),
            CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(reply.text, "fixture reply");
    assert!(reply.usage.is_none());
    server.abort();
}

#[tokio::test]
async fn qualified_tool_names_are_aliased_on_wire_and_restored_in_core() {
    use axum::{Json, Router, routing::post};
    use harness3::{Model, ToolSpec};
    let service=Router::new().route("/v1/responses",post(|Json(body):Json<Value>|async move{
        let name=body["tools"][0]["name"].as_str().unwrap();
        assert!(name.bytes().all(|b|b.is_ascii_alphanumeric()||b"_-".contains(&b)));
        assert_ne!(name,"text.inspect");
        Json(json!({"status":"completed","output":[{"type":"function_call","call_id":"wire-call","name":name,"arguments":"{\"text\":\"hi\"}"}]}))
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = tokio::spawn(async move { axum::serve(listener, service).await.unwrap() });
    let provider = Provider::new(
        Protocol::OpenaiResponses,
        "fixture".into(),
        format!("http://127.0.0.1:{port}/v1"),
        "".into(),
    )
    .unwrap();
    let mut input = model_input();
    input.tools.push(ToolSpec {
        name: "text.inspect".into(),
        description: "Inspect text".into(),
        parameters: json!({"type":"object"}),
        mutates: false,
    });
    let reply = provider
        .complete(
            input,
            &harness3::LiveOutput::default(),
            CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(reply.calls[0].name, "text.inspect");
    assert_ne!(reply.continuation["output"][0]["name"], "text.inspect");
    server.abort();
}

#[test]
fn all_provider_requests_include_the_first_user_text_once() {
    for protocol in [
        Protocol::OpenaiCompatible,
        Protocol::OpenaiResponses,
        Protocol::Anthropic,
    ] {
        let provider = Provider::new(
            protocol.clone(),
            "fixture".into(),
            "http://localhost/v1".into(),
            "".into(),
        )
        .unwrap();
        let request = provider.request(&model_input(), 128);
        let expected = match protocol {
            Protocol::OpenaiCompatible => &request["messages"][1]["content"],
            Protocol::OpenaiResponses => &request["input"][0]["content"],
            Protocol::Anthropic => &request["messages"][0]["content"][0]["text"],
            Protocol::Demo => unreachable!(),
        };
        assert_eq!(expected, "hello");
    }
}

#[test]
fn simultaneous_claims_have_one_owner() {
    let store = seeded();
    let batch = spawn(&store, 1, "batch");
    let agent = batch["instance_ids"][0].as_str().unwrap();
    store.enqueue(agent, &input("one claim"), "run").unwrap();
    let barrier = Arc::new(std::sync::Barrier::new(3));
    let mut handles = vec![];
    for _ in 0..2 {
        let store = store.clone();
        let barrier = barrier.clone();
        handles.push(std::thread::spawn(move || {
            barrier.wait();
            store.claim().unwrap()
        }));
    }
    barrier.wait();
    let claimed = handles
        .into_iter()
        .map(|handle| handle.join().unwrap())
        .filter(Option::is_some)
        .count();
    assert_eq!(claimed, 1);
}
#[tokio::test]
async fn recovery_clears_a_crashed_active_harness_without_losing_receipts() {
    let store = seeded();
    let batch = spawn(&store, 1, "batch");
    let agent = batch["instance_ids"][0].as_str().unwrap();
    let run = store.enqueue(agent, &input("crashed"), "run").unwrap();
    store.claim().unwrap();
    let session = Session {
        active_run: Some("kernel-active".into()),
        active_delivery: Some(
            serde_json::to_string(&("platform", run["id"].as_str().unwrap())).unwrap(),
        ),
        ..Session::default()
    };
    store.save(agent, 0, &session).await.unwrap();
    runtime::recover(&store).await.unwrap();
    let recovered = store.load(agent).await.unwrap();
    assert!(recovered.active_run.is_none());
    assert_eq!(store.list("runs").unwrap()[0]["state"], "interrupted");
    assert!(store.claim().unwrap().is_none());
}
#[tokio::test]
async fn provider_error_messages_redact_credentials() {
    use axum::{Json, Router, http::StatusCode, routing::post};
    use harness3::Model;
    let service = Router::new().route(
        "/v1/responses",
        post(|| async {
            (
                StatusCode::BAD_REQUEST,
                Json(json!({"error":{"message":"invalid fixture-secret-key in request"}})),
            )
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = tokio::spawn(async move { axum::serve(listener, service).await.unwrap() });
    let provider = Provider::new(
        Protocol::OpenaiResponses,
        "fixture".into(),
        format!("http://127.0.0.1:{port}/v1"),
        "fixture-secret-key".into(),
    )
    .unwrap();
    let error = provider
        .complete(
            model_input(),
            &harness3::LiveOutput::default(),
            CancellationToken::new(),
        )
        .await
        .err()
        .expect("expected rejected request");
    assert_eq!(error.code, "provider_http_400");
    assert!(error.message.unwrap().contains("[redacted]"));
    server.abort();
}

#[tokio::test]
async fn changed_model_identity_cannot_reuse_provider_history() {
    use harness3::session::RequestRecord;
    let store = seeded();
    let batch = spawn(&store, 1, "batch");
    let agent = batch["instance_ids"][0].as_str().unwrap();
    let session = Session {
        turns: vec![Turn::User("old input".into())],
        requests: vec![RequestRecord {
            purpose: None,
            override_input: None,
            run_id: "old".into(),
            model: "selected".into(),
            model_metadata: json!({"protocol":"different-model"}),
            system: "old".into(),
            tools: vec![],
            history_start: 0,
            history_end: 1,
            compaction: None,
        }],
        ..Session::default()
    };
    store.save(agent, 0, &session).await.unwrap();
    store.enqueue(agent, &input("new input"), "new").unwrap();
    let stop = CancellationToken::new();
    let worker = tokio::spawn(runtime::worker_loop(
        store.clone(),
        Models::default(),
        Arc::new(Mutex::new(HashMap::new())),
        stop.clone(),
    ));
    for _ in 0..100 {
        if store.list("runs").unwrap()[0]["state"] == "failed" {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    stop.cancel();
    worker.await.unwrap();
    assert_eq!(
        store.list("runs").unwrap()[0]["report"]["error"]["code"],
        "model_profile_changed_create_new_instance"
    );
    assert_eq!(store.load(agent).await.unwrap().requests.len(), 1);
}

#[tokio::test]
async fn recovery_preserves_a_known_cancelled_receipt() {
    let store = seeded();
    let batch = spawn(&store, 1, "batch");
    let agent = batch["instance_ids"][0].as_str().unwrap();
    let run = store
        .enqueue(agent, &input("cancelled before platform commit"), "run")
        .unwrap();
    store.claim().unwrap();
    let mut session = Session::default();
    session.completed.insert(
        serde_json::to_string(&("platform", run["id"].as_str().unwrap())).unwrap(),
        harness3::Report {
            run_id: "kernel-cancelled".into(),
            output: None,
            error: Some(harness3::Error::new("cancelled")),
            observations: vec![],
            usage: None,
            generation_ms: 0,
            execution_ms: None,
        },
    );
    store.save(agent, 0, &session).await.unwrap();
    runtime::recover(&store).await.unwrap();
    assert_eq!(store.list("runs").unwrap()[0]["state"], "cancelled");
}
