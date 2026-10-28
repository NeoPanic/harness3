use harness3::{CancellationToken, Context};
use harness3_platform::{
    domain::*,
    plugins::{ProcessHost, Snapshot},
    store::Store,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{path::PathBuf, time::Duration};

struct Fixture {
    root: PathBuf,
    store: Store,
    host: ProcessHost,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("harness3-plugins-{}", uuid::Uuid::new_v4()));
        let store = Store::memory().unwrap();
        let host = ProcessHost::enabled(store.clone(), root.join("cache")).unwrap();
        Self { root, store, host }
    }
    fn package(&self, version: &str, mode: &str, timeout: u64) -> PathBuf {
        let source = self.root.join(format!("source-{version}-{mode}"));
        std::fs::create_dir_all(&source).unwrap();
        let script = include_bytes!("fixtures/process-plugin.py");
        std::fs::write(source.join("worker.py"), script).unwrap();
        std::fs::write(source.join("mode.txt"), mode).unwrap();
        let manifest = json!({"api_version":"harness3.plugins/v1","id":"test.text","version":version,"description":"test external process","instructions":"Use test.text.inspect.","runtime":{"protocol":"harness3-plugin-rpc/v1","entry":"worker.py"},
            "files":{"worker.py":format!("{:x}",Sha256::digest(script)),"mode.txt":format!("{:x}",Sha256::digest(mode.as_bytes()))},"timeout_ms":timeout,
            "tools":[{"name":"inspect","description":"inspect text","input_schema":{"type":"object","properties":{"text":{"type":"string","maxLength":100}},"required":["text"],"additionalProperties":false},"output_schema":{"type":"object","properties":{"characters":{"type":"integer","minimum":0}},"required":["characters"],"additionalProperties":true},"mutates":false,"parallel_safe":true}]});
        std::fs::write(source.join("manifest.json"), manifest.to_string()).unwrap();
        source
    }
    async fn install(&self, version: &str, mode: &str, timeout: u64) -> Snapshot {
        let row = self
            .host
            .install(self.package(version, mode, timeout))
            .await
            .unwrap();
        serde_json::from_value(row["definition"].clone()).unwrap()
    }
    async fn invoke(
        &self,
        snapshot: &Snapshot,
        cancel: CancellationToken,
    ) -> harness3::Result<harness3::ToolOutput> {
        self.host
            .invoke(
                snapshot,
                "inspect",
                json!({"text":"Harness³"}),
                &context(),
                cancel,
            )
            .await
    }
}
fn context() -> Context {
    Context {
        run_id: "run_test".into(),
        source: "test".into(),
        event_id: "run_test".into(),
        task: "agent".into(),
        original_text: "not sent".into(),
        metadata: Value::Null,
    }
}
fn preset(version: &str, plugin_version: &str) -> Preset {
    Preset {
        name: "worker".into(),
        version: version.into(),
        purpose: "inspect".into(),
        objective: "report".into(),
        instructions: "Use test.text.inspect".into(),
        protocol: Protocol::Demo,
        plugin: Some(PluginRef {
            id: "test.text".into(),
            version: plugin_version.into(),
        }),
    }
}
#[tokio::test]
async fn installation_pins_files_schemas_and_sessions_while_versions_coexist() {
    let f = Fixture::new();
    let source = f.package("1", "normal", 2000);
    let row = f.host.install(source.clone()).await.unwrap();
    assert_eq!(f.host.install(source.clone()).await.unwrap(), row);
    let first: Snapshot = serde_json::from_value(row["definition"].clone()).unwrap();
    let published = f.store.publish(preset("1", "1")).unwrap();
    let second = f.install("2", "normal", 2000).await;
    assert_ne!(first.digest, second.digest);
    assert_eq!(published["plugin_snapshot"]["digest"], first.digest);
    std::fs::remove_dir_all(source).unwrap();
    let output = f.invoke(&first, CancellationToken::new()).await.unwrap();
    assert_eq!(output.content["characters"], 8);
    assert_eq!(output.content["version"], "1");
    assert_eq!(output.content["home_inherited"], false);
    assert_eq!(
        f.invoke(&second, CancellationToken::new())
            .await
            .unwrap()
            .content["version"],
        "2"
    );
    assert_eq!(f.store.publish(preset("1", "1")).unwrap(), published);
    assert!(f.store.publish(preset("1", "2")).is_err());
    assert_eq!(
        f.host.tools(first).unwrap()[0].spec().name,
        "test.text.inspect"
    );
    f.host.action("test.text", "1", "disable").await.unwrap();
    assert_eq!(f.store.publish(preset("1", "1")).unwrap(), published);
    assert_eq!(
        f.store.publish(preset("new", "1")).unwrap_err().1,
        "plugin_disabled"
    );
    assert_eq!(
        f.store
            .publish_definition(
                json!({"name":"child","version":"1","extends":{"name":"worker","version":"1"}})
            )
            .unwrap_err()
            .1,
        "plugin_disabled"
    );
}
#[tokio::test]
async fn changed_versions_failed_candidates_and_bad_files_do_not_replace_live_code() {
    let f = Fixture::new();
    let first = f.install("1", "normal", 2000).await;
    assert_eq!(
        f.host
            .install(f.package("1", "unhealthy", 2000))
            .await
            .unwrap_err()
            .1,
        "plugin_version_immutable"
    );
    assert_eq!(
        f.host
            .install(f.package("2", "unhealthy", 2000))
            .await
            .unwrap_err()
            .1,
        "plugin_unhealthy"
    );
    assert_eq!(
        f.host
            .install(f.package("3", "bad-handshake", 2000))
            .await
            .unwrap_err()
            .1,
        "plugin_capabilities_mismatch"
    );
    let source = f.package("4", "normal", 2000);
    std::fs::write(source.join("worker.py"), "tampered").unwrap();
    assert_eq!(
        f.host.install(source).await.unwrap_err().1,
        "plugin_digest_mismatch"
    );
    assert_eq!(f.store.process_plugins().unwrap().len(), 1);
    assert!(f.invoke(&first, CancellationToken::new()).await.is_ok());
}
#[tokio::test]
async fn cache_tampering_and_invalid_input_are_rejected_before_tool_dispatch() {
    let f = Fixture::new();
    let snapshot = f.install("1", "normal", 2000).await;
    let error = f
        .host
        .invoke(
            &snapshot,
            "inspect",
            json!({"text":12}),
            &context(),
            CancellationToken::new(),
        )
        .await
        .err()
        .unwrap();
    assert_eq!(error.code, "plugin_input_schema_mismatch");
    let file = f.root.join("cache").join(&snapshot.digest).join("mode.txt");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    std::fs::write(file, "changed").unwrap();
    assert_eq!(
        f.invoke(&snapshot, CancellationToken::new())
            .await
            .err()
            .unwrap()
            .code,
        "plugin_cache_tampered"
    );
    assert!(
        !f.store
            .events(0)
            .unwrap()
            .iter()
            .any(|e| e["kind"] == "plugin.invocation_started")
    );
}
#[tokio::test]
async fn plugin_outputs_disconnects_and_deadlines_keep_unknown_outcome_receipts() {
    let f = Fixture::new();
    for mode in ["invalid-output", "disconnect", "oversized", "hang"] {
        let snapshot = f.install(mode, mode, 150).await;
        let error = tokio::time::timeout(
            Duration::from_secs(3),
            f.invoke(&snapshot, CancellationToken::new()),
        )
        .await
        .unwrap()
        .err()
        .unwrap();
        assert_eq!(error.code, "plugin_outcome_unknown", "{mode}");
        assert_eq!(error.details["outcome_unknown"], true);
    }
    assert_eq!(
        f.store
            .events(0)
            .unwrap()
            .iter()
            .filter(|e| e["kind"] == "plugin.invocation_settled"
                && e["data"]["state"] == "outcome_unknown")
            .count(),
        4
    );
}
#[tokio::test]
async fn disable_stops_admission_cancels_in_flight_and_can_explicitly_reenable() {
    let f = Fixture::new();
    let snapshot = f.install("1", "hang", 10000).await;
    let host = f.host.clone();
    let copy = snapshot.clone();
    let running = tokio::spawn(async move {
        host.invoke(
            &copy,
            "inspect",
            json!({"text":"x"}),
            &context(),
            CancellationToken::new(),
        )
        .await
    });
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if f.store
                .events(0)
                .unwrap()
                .iter()
                .any(|e| e["kind"] == "plugin.progress")
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    f.host.action("test.text", "1", "disable").await.unwrap();
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(2), running)
            .await
            .unwrap()
            .unwrap()
            .err()
            .unwrap()
            .code,
        "plugin_outcome_unknown"
    );
    assert_eq!(
        f.invoke(&snapshot, CancellationToken::new())
            .await
            .err()
            .unwrap()
            .code,
        "plugin_disabled"
    );
    assert_eq!(
        f.store.publish(preset("1", "1")).unwrap_err().1,
        "plugin_disabled"
    );
    f.host.action("test.text", "1", "enable").await.unwrap();
    assert!(f.host.tools(snapshot).is_ok());
}
#[tokio::test]
async fn caller_cancellation_and_daemon_opt_in_are_enforced() {
    let f = Fixture::new();
    let snapshot = f.install("1", "hang", 10000).await;
    let token = CancellationToken::new();
    let cancel = token.clone();
    let future = f.invoke(&snapshot, token);
    tokio::pin!(future);
    tokio::time::timeout(Duration::from_secs(5),async {
        loop {
            tokio::select! {
                result=&mut future=>panic!("invocation ended before cancellation: {:?}",result.err().map(|e|e.code)),
                _=tokio::time::sleep(Duration::from_millis(10))=> {
                    if f.store.events(0).unwrap().iter().any(|e|e["kind"]=="plugin.progress") {break;}
                }
            }
        }
    }).await.unwrap();
    cancel.cancel();
    let result = future.await;
    assert_eq!(result.err().unwrap().code, "plugin_outcome_unknown");
    let disabled = ProcessHost::disabled(f.store.clone());
    assert_eq!(
        disabled
            .install(f.package("2", "normal", 2000))
            .await
            .unwrap_err()
            .1,
        "process_plugins_disabled"
    );
    assert!(disabled.tools(snapshot.clone()).is_err());
}
#[cfg(unix)]
#[tokio::test]
async fn symlinks_and_directory_escape_cannot_enter_an_installed_package() {
    use std::os::unix::fs::symlink;
    let f = Fixture::new();
    let source = f.package("1", "normal", 2000);
    std::fs::rename(source.join("worker.py"), source.join("real.py")).unwrap();
    symlink("real.py", source.join("worker.py")).unwrap();
    assert_eq!(
        f.host.install(source.clone()).await.unwrap_err().1,
        "plugin_symlink_denied"
    );
    let mut manifest: Value =
        serde_json::from_slice(&std::fs::read(source.join("manifest.json")).unwrap()).unwrap();
    manifest["runtime"]["entry"] = json!("../worker.py");
    std::fs::write(source.join("manifest.json"), manifest.to_string()).unwrap();
    assert_eq!(
        f.host.install(source).await.unwrap_err().1,
        "invalid_plugin_manifest"
    );
}
#[test]
fn schema_contract_rejects_unsupported_keywords_and_validates_nested_values() {
    use harness3_plugin_protocol::{check_schema, validate_value};
    assert!(check_schema(&json!({"type":"string","pattern":".*"})).is_err());
    assert!(check_schema(&json!({"type":"string","maxItems":2})).is_err());
    let schema = json!({"type":"object","properties":{"values":{"type":"array","maxItems":2,"items":{"type":"integer","minimum":0}}},"required":["values"],"additionalProperties":false});
    check_schema(&schema).unwrap();
    validate_value(&schema, &json!({"values":[0,1]})).unwrap();
    for bad in [
        json!({}),
        json!({"values":[-1]}),
        json!({"values":[0,1,2]}),
        json!({"values":[],"extra":1}),
    ] {
        assert!(validate_value(&schema, &bad).is_err());
    }
}

#[tokio::test]
async fn runtime_worker_dispatches_an_external_tool_and_preserves_model_and_tool_evidence() {
    use axum::{Json, Router, routing::post};
    use harness3_platform::{
        models::{Models, Provider},
        runtime,
    };
    use std::{collections::HashMap, sync::Arc};
    use tokio::sync::Mutex;
    let f = Fixture::new();
    let snapshot = f.install("1", "normal", 2000).await;
    let service=Router::new().route("/responses",post(|Json(body):Json<Value>|async move {
        assert!(body["instructions"].as_str().unwrap().contains("prompt companion"));
        assert!(body["instructions"].as_str().unwrap().contains("Inspect Harness³"));
        if let Some(output)=body["input"].as_array().unwrap().iter().find(|i|i["type"]=="function_call_output") {
            let content:Value=serde_json::from_str(output["output"].as_str().unwrap()).unwrap();
            assert_eq!(content["characters"],8);
            assert_eq!(content["version"],"1");
            Json(json!({"status":"completed","output":[{"type":"message","content":[{"type":"output_text","text":"verified external receipt"}]}]}))
        }else {
            let tool=body["tools"].as_array().unwrap().iter().find(|t|t["description"]=="inspect text").unwrap();
            Json(json!({"status":"completed","output":[{"type":"function_call","call_id":"external-call","name":tool["name"],"arguments":"{\"text\":\"Harness³\"}"}]}))
        }
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = tokio::spawn(async move {
        axum::serve(listener, service).await.unwrap();
    });
    let models = Models {
        responses: Some(Arc::new(
            Provider::new(
                Protocol::OpenaiResponses,
                "fixture".into(),
                format!("http://127.0.0.1:{port}"),
                String::new(),
            )
            .unwrap(),
        )),
        ..Models::default()
    };
    let mut definition = preset("1", "1");
    definition.protocol = Protocol::OpenaiResponses;
    f.store.publish(definition).unwrap();
    f.store
        .install(Bundle {
            id: "local.companion".into(),
            version: "1".into(),
            description: "companion".into(),
            instructions: "prompt companion".into(),
        })
        .unwrap();
    f.store.publish_definition(json!({"name":"composed","version":"1","extends":{"name":"worker","version":"1"},"objective":"Inspect {{topic}}","plugins":[{"id":"test.text","version":"1"},{"id":"local.companion","version":"1"}],"parameters_schema":{"type":"object","properties":{"topic":{"type":"string"}},"required":["topic"],"additionalProperties":false},"parameter_defaults":{"topic":"Harness³"}})).unwrap();
    let batch = f
        .store
        .spawn(
            &Spawn {
                preset_name: "composed".into(),
                preset_version: "1".into(),
                count: 1,
            },
            "runtime",
        )
        .unwrap();
    let agent = batch["instance_ids"][0].as_str().unwrap();
    let run = f
        .store
        .enqueue(
            agent,
            &RunInput {
                text: "Harness³".into(),
            },
            "external-run",
        )
        .unwrap();
    let shutdown = CancellationToken::new();
    let worker = tokio::spawn(runtime::worker_loop_with_plugins(
        f.store.clone(),
        models,
        Arc::new(Mutex::new(HashMap::new())),
        shutdown.clone(),
        f.host.clone(),
    ));
    let id = run["id"].as_str().unwrap();
    let view = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let view = f.store.run(id).unwrap();
            if !["queued", "running"].contains(&view["state"].as_str().unwrap()) {
                break view;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    shutdown.cancel();
    worker.await.unwrap();
    server.abort();
    assert_eq!(view["state"], "completed", "{view}");
    assert_eq!(
        view["report"]["observations"][0]["call"]["name"],
        "test.text.inspect"
    );
    assert_eq!(
        view["report"]["observations"][0]["output"]["content"]["version"],
        "1"
    );
    let instance = f
        .store
        .list("instances")
        .unwrap()
        .into_iter()
        .find(|i| i["id"] == agent)
        .unwrap();
    assert_eq!(
        instance["snapshot"]["plugin_snapshots"][0]["digest"],
        snapshot.digest
    );
    use harness3::session::SessionStore;
    let session = f.store.load(agent).await.unwrap();
    assert!(!session.requests.is_empty());
    assert!(
        f.store
            .events(0)
            .unwrap()
            .iter()
            .any(|e| e["kind"] == "plugin.invocation_settled" && e["data"]["state"] == "completed")
    );
}

#[tokio::test]
async fn executable_catalog_api_uses_existing_auth_and_exposes_real_enable_disable_state() {
    use axum::{body::Body, http::Request};
    use harness3_platform::{
        api::{App, router},
        models::Models,
    };
    use std::{collections::HashMap, sync::Arc};
    use tokio::sync::Mutex;
    use tower::ServiceExt;
    let f = Fixture::new();
    let source = f.package("1", "normal", 2000);
    let app = router(App {
        store: f.store.clone(),
        plugins: f.host.clone(),
        token: "a".repeat(64),
        models: Models::default(),
        cancellations: Arc::new(Mutex::new(HashMap::new())),
    });
    let request = |path: &str, body: Value, auth: bool| {
        let mut builder = Request::builder()
            .uri(path)
            .method("POST")
            .header("Host", "127.0.0.1")
            .header("Content-Type", "application/json");
        if auth {
            builder = builder.header("Authorization", format!("Bearer {}", "a".repeat(64)));
        }
        builder.body(Body::from(body.to_string())).unwrap()
    };
    assert_eq!(
        app.clone()
            .oneshot(request(
                "/api/v1/process-plugins",
                json!({"source_dir":source}),
                false
            ))
            .await
            .unwrap()
            .status(),
        401
    );
    assert!(f.store.process_plugins().unwrap().is_empty());
    assert_eq!(
        app.clone()
            .oneshot(request(
                "/api/v1/process-plugins",
                json!({"source_dir":source}),
                true
            ))
            .await
            .unwrap()
            .status(),
        200
    );
    assert_eq!(
        app.clone()
            .oneshot(request(
                "/api/v1/process-plugins/test.text/1/actions",
                json!({"action":"disable"}),
                true
            ))
            .await
            .unwrap()
            .status(),
        200
    );
    assert_eq!(
        f.store.process_plugin("test.text", "1").unwrap()["state"],
        "disabled"
    );
    assert_eq!(
        app.clone()
            .oneshot(request(
                "/api/v1/process-plugins/test.text/1/actions",
                json!({"action":"enable"}),
                true
            ))
            .await
            .unwrap()
            .status(),
        200
    );
    assert_eq!(
        f.store.process_plugin("test.text", "1").unwrap()["state"],
        "enabled"
    );
}
