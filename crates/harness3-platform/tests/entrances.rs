use harness3_platform::{
    api::{App, router},
    client::{Command, PlatformClient},
    domain::{Preset, Protocol},
    mcp::McpServer,
    models::Models,
    store::Store,
};
use serde_json::{Value, json};
use std::{collections::HashMap, path::PathBuf, sync::Arc};
use tokio::{io::AsyncReadExt, sync::Mutex};

struct Fixture {
    store: Store,
    root: PathBuf,
    port: u16,
    server: tokio::task::JoinHandle<()>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.server.abort();
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
impl Fixture {
    async fn new() -> Self {
        let root = std::env::temp_dir().join(format!("harness3-client-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let token = "a".repeat(64);
        std::fs::write(root.join("token"), &token).unwrap();
        let store = Store::memory().unwrap();
        store
            .publish(Preset {
                name: "worker".into(),
                version: "1".into(),
                purpose: "inspect".into(),
                objective: "report".into(),
                instructions: "inspect".into(),
                protocol: Protocol::Demo,
                plugin: None,
            })
            .unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let app = router(App {
            plugins: harness3_platform::plugins::ProcessHost::disabled(store.clone()),
            store: store.clone(),
            token,
            cancellations: Arc::new(Mutex::new(HashMap::new())),
            models: Models::from_env(),
        });
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        Self {
            store,
            root,
            port,
            server,
        }
    }
    fn client(&self) -> PlatformClient {
        PlatformClient::local(self.port, &self.root).unwrap()
    }
    async fn cli(&self, args: &[&str]) -> std::process::Output {
        let args: Vec<_> = args.iter().map(|s| s.to_string()).collect();
        let port = self.port.to_string();
        let root = self.root.clone();
        tokio::task::spawn_blocking(move || {
            std::process::Command::new(env!("CARGO_BIN_EXE_harness3"))
                .args(&args)
                .args(["--port", &port, "--data-dir"])
                .arg(root)
                .output()
                .unwrap()
        })
        .await
        .unwrap()
    }
}
async fn initialize(server: &mut McpServer) {
    assert_eq!(server.handle(json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"test","version":"1"}}})).await.unwrap()["result"]["protocolVersion"], "2025-11-25");
    assert!(
        server
            .handle(json!({"jsonrpc":"2.0","method":"notifications/initialized"}))
            .await
            .is_none()
    );
}
async fn call(server: &mut McpServer, tool: &str, args: Value) -> Value {
    server.handle(json!({"jsonrpc":"2.0","id":"call","method":"tools/call","params":{"name":tool,"arguments":args}})).await.unwrap()
}
fn fleet_args() -> Value {
    json!({"plan":{"preset_name":"worker","preset_version":"1","count":2,"mode":"maintain"},"key":"shared-create"})
}
#[tokio::test]
async fn mcp_and_http_share_fleet_receipts_conflicts_and_operations() {
    let f = Fixture::new().await;
    let mut mcp = McpServer::new(f.client(), true);
    initialize(&mut mcp).await;
    let receipt = call(&mut mcp, "fleets_create", fleet_args()).await;
    assert_eq!(receipt["result"]["isError"], false);
    let receipt = &receipt["result"]["structuredContent"];
    let command = serde_json::from_value(
        json!({"tool":"fleets_create","plan":fleet_args()["plan"],"key":"shared-create"}),
    )
    .unwrap();
    assert_eq!(&f.client().execute(command).await.unwrap(), receipt);
    assert_eq!(f.store.list("instances").unwrap().len(), 0);
    f.store.reconcile_fleets().unwrap();
    let id = receipt["fleet_id"].as_str().unwrap().to_owned();
    let view = f
        .client()
        .execute(Command::FleetsShow { id: id.clone() })
        .await
        .unwrap();
    assert_eq!(view["current_count"], 2);
    let operation = call(
        &mut mcp,
        "operations_show",
        json!({"id":receipt["operation_id"]}),
    )
    .await;
    assert_eq!(
        operation["result"]["structuredContent"]["state"],
        "completed"
    );
    let mut conflict = fleet_args();
    conflict["plan"]["count"] = json!(3);
    let failure = call(&mut mcp, "fleets_create", conflict).await;
    assert_eq!(failure["result"]["isError"], true);
    assert!(failure.to_string().contains("idempotency_conflict"));
    let stale = call(
        &mut mcp,
        "fleets_scale",
        json!({"id":id,"expected_revision":99,"count":3,"key":"stale"}),
    )
    .await;
    assert_eq!(stale["result"]["isError"], true);
    assert_eq!(f.store.fleet(&id).unwrap()["desired_count"], 2);
}
#[tokio::test]
async fn read_only_mcp_hides_writes_and_rejects_argument_tool_override() {
    let f = Fixture::new().await;
    let mut mcp = McpServer::new(f.client(), false);
    initialize(&mut mcp).await;
    let catalog = mcp
        .handle(json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}))
        .await
        .unwrap();
    let tools = catalog["result"]["tools"].as_array().unwrap();
    assert!(
        tools
            .iter()
            .all(|t| t["annotations"]["readOnlyHint"] == true)
    );
    assert!(!tools.iter().any(|t| t["name"] == "fleets_create"));
    assert_eq!(
        call(&mut mcp, "fleets_create", fleet_args()).await["error"]["code"],
        -32602
    );
    assert_eq!(
        call(&mut mcp, "snapshot", json!({"tool":"fleets_create"})).await["error"]["code"],
        -32602
    );
    assert_eq!(
        call(&mut mcp, "snapshot", json!({"unexpected":true})).await["error"]["code"],
        -32602
    );
    assert!(f.store.fleets().unwrap().is_empty());
    assert_eq!(
        call(&mut mcp, "presets_list", json!({})).await["result"]["structuredContent"]["presets"]
            [0]["name"],
        "worker"
    );
}
#[tokio::test]
async fn protocol_lifecycle_and_bad_requests_produce_json_rpc_errors() {
    let f = Fixture::new().await;
    let mut mcp = McpServer::new(f.client(), true);
    assert_eq!(
        mcp.handle(json!({"jsonrpc":"2.0","id":1,"method":"tools/list"}))
            .await
            .unwrap()["error"]["code"],
        -32600
    );
    assert_eq!(
        mcp.handle(json!([])).await.unwrap()["error"]["code"],
        -32600
    );
    assert_eq!(
        mcp.handle(json!({"jsonrpc":"2.0","id":{},"method":"ping"}))
            .await
            .unwrap()["error"]["code"],
        -32600
    );
    initialize(&mut mcp).await;
    assert_eq!(
        mcp.handle(json!({"jsonrpc":"2.0","id":7,"method":"unknown"}))
            .await
            .unwrap()["error"]["code"],
        -32601
    );
    assert_eq!(
        call(&mut mcp, "fleets_show", json!({"id":"../snapshot"})).await["result"]["isError"],
        true
    );
    assert_eq!(
        call(&mut mcp, "fleets_show", json!({"id":".."})).await["result"]["isError"],
        true
    );
    assert_eq!(
        call(&mut mcp, "events_list", json!({"after":-1})).await["error"]["code"],
        -32602
    );
    for version in ["2024-11-05", "future-version"] {
        let mut adapter = McpServer::new(f.client(), false);
        let reply = adapter.handle(json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":version}})).await.unwrap();
        assert_eq!(
            reply["result"]["protocolVersion"],
            if version == "future-version" {
                "2025-11-25"
            } else {
                version
            }
        );
        adapter
            .handle(json!({"jsonrpc":"2.0","method":"notifications/initialized"}))
            .await;
        let reply = call(&mut adapter, "presets_list", json!({})).await;
        assert_eq!(
            reply["result"].get("structuredContent").is_some(),
            version != "2024-11-05"
        );
        assert_eq!(reply["result"]["content"][0]["type"], "text");
    }
}
#[tokio::test]
async fn stdio_framing_preserves_ids_and_does_not_reply_to_notifications() {
    let f = Fixture::new().await;
    let mut mcp = McpServer::new(f.client(), false);
    let input = b"bad json\n{\"jsonrpc\":\"2.0\",\"method\":\"notifications/cancelled\"}\n{\"jsonrpc\":\"2.0\",\"id\":\"p\",\"method\":\"ping\"}\n";
    let mut output = Vec::new();
    mcp.serve(&input[..], &mut output).await.unwrap();
    let lines: Vec<Value> = String::from_utf8(output)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0]["error"]["code"], -32700);
    assert_eq!(lines[1], json!({"jsonrpc":"2.0","id":"p","result":{}}));
    let oversized = vec![b' '; 1024 * 1024 + 1];
    assert!(
        mcp.serve(&oversized[..], Vec::new())
            .await
            .unwrap_err()
            .to_string()
            .contains("1 MiB")
    );
}
#[tokio::test]
async fn wrong_auth_and_bad_ids_do_not_expose_local_credentials() {
    let f = Fixture::new().await;
    std::fs::write(f.root.join("token"), "b".repeat(64)).unwrap();
    let error = f
        .client()
        .execute(Command::Snapshot)
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("401: authentication_required"));
    assert!(!error.contains(&"b".repeat(64)));
    std::fs::write(f.root.join("token"), "not a token").unwrap();
    assert!(PlatformClient::local(f.port, &f.root).is_err());
}
#[tokio::test]
async fn cli_creates_inspects_and_terminates_the_same_mcp_fleet() {
    let f = Fixture::new().await;
    let plan = f.root.join("fleet.json");
    std::fs::write(&plan, fleet_args()["plan"].to_string()).unwrap();
    let args = [
        "fleets",
        "create",
        "--file",
        plan.to_str().unwrap(),
        "--key",
        "shared-create",
    ];
    let output = f.cli(&args).await;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt: Value = serde_json::from_slice(&output.stdout).unwrap();
    let mut mcp = McpServer::new(f.client(), true);
    initialize(&mut mcp).await;
    assert_eq!(
        call(&mut mcp, "fleets_create", fleet_args()).await["result"]["structuredContent"],
        receipt
    );
    f.store.reconcile_fleets().unwrap();
    let id = receipt["fleet_id"].as_str().unwrap();
    let output = f.cli(&["fleets", "show", id]).await;
    let view: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(view["current_count"], 2);
    let output = f
        .cli(&[
            "fleets",
            "terminate",
            id,
            "--expected-revision",
            "1",
            "--key",
            "stop",
        ])
        .await;
    assert!(output.status.success());
    f.store.reconcile_fleets().unwrap();
    assert_eq!(
        call(&mut mcp, "fleets_show", json!({"id":id})).await["result"]["structuredContent"]["state"],
        "terminated"
    );
    let output = f.cli(&["fleets", "show", "../snapshot"]).await;
    assert!(!output.status.success());
}
#[tokio::test]
async fn cli_mcp_subprocess_emits_only_protocol_on_stdout() {
    let f = Fixture::new().await;
    let port = f.port.to_string();
    let root = f.root.clone();
    let output = tokio::task::spawn_blocking(move || {
        use std::io::Write;
        let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_harness3"))
            .args(["mcp", "serve", "--port", &port, "--data-dir"])
            .arg(root)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}\n")
            .unwrap();
        child.wait_with_output().unwrap()
    })
    .await
    .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout).unwrap(),
        json!({"jsonrpc":"2.0","id":1,"result":{}})
    );
    assert!(output.stderr.is_empty());
}
#[tokio::test]
async fn cli_and_mcp_share_goal_dependencies_review_and_cancellation() {
    let f = Fixture::new().await;
    let receipt = f
        .store
        .create_fleet(
            &serde_json::from_value(fleet_args()["plan"].clone()).unwrap(),
            "goal-members",
        )
        .unwrap();
    f.store.reconcile_fleets().unwrap();
    let fleet = f
        .store
        .fleet(receipt["fleet_id"].as_str().unwrap())
        .unwrap();
    let plan = json!({"title":"cross entrance","objective":"inspect then combine","acceptance":"receipts checked",
        "tasks":[{"key":"inspect","instance_id":fleet["slots"][0]["instance_id"],"instructions":"inspect input"},
        {"key":"combine","instance_id":fleet["slots"][1]["instance_id"],"instructions":"combine evidence","depends_on":["inspect"]}]});
    let file = f.root.join("goal.json");
    std::fs::write(&file, plan.to_string()).unwrap();
    let output = f
        .cli(&[
            "goals",
            "create",
            "--file",
            file.to_str().unwrap(),
            "--key",
            "same-goal",
        ])
        .await;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt: Value = serde_json::from_slice(&output.stdout).unwrap();
    let mut mcp = McpServer::new(f.client(), true);
    initialize(&mut mcp).await;
    assert_eq!(
        call(
            &mut mcp,
            "goals_create",
            json!({"plan":plan,"key":"same-goal"})
        )
        .await["result"]["structuredContent"],
        receipt
    );
    let id = receipt["id"].as_str().unwrap();
    let view = call(&mut mcp, "goals_show", json!({"id":id})).await;
    assert_eq!(
        view["result"]["structuredContent"]["tasks"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        call(
            &mut mcp,
            "goals_accept",
            json!({"id":id,"review":"too early"})
        )
        .await["result"]["isError"],
        true
    );
    assert_eq!(
        call(&mut mcp, "goals_cancel", json!({"id":id})).await["result"]["isError"],
        false
    );
    let output = f.cli(&["goals", "show", id]).await;
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout).unwrap()["state"],
        "cancelled"
    );
}
#[tokio::test]
async fn transport_flushes_each_response_before_eof() {
    use tokio::io::AsyncWriteExt;
    let f = Fixture::new().await;
    let (mut peer, stream) = tokio::io::duplex(4096);
    let (reader, writer) = tokio::io::split(stream);
    let mut server = McpServer::new(f.client(), false);
    let task = tokio::spawn(async move {
        server
            .serve(tokio::io::BufReader::new(reader), writer)
            .await
    });
    peer.write_all(b"{\"jsonrpc\":\"2.0\",\"id\":9,\"method\":\"ping\"}\n")
        .await
        .unwrap();
    let mut result = vec![0u8; 100];
    let count = tokio::time::timeout(std::time::Duration::from_secs(2), peer.read(&mut result))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        serde_json::from_slice::<Value>(&result[..count]).unwrap()["id"],
        9
    );
    drop(peer);
    task.await.unwrap().unwrap();
}

#[tokio::test]
async fn inherited_parameterized_presets_use_one_contract_across_cli_mcp_and_http() {
    let f = Fixture::new().await;
    let definition = json!({"name":"derived","version":"1","extends":{"name":"worker","version":"1"},"objective":"Inspect {{topic}}","plugins":[],"parameters_schema":{"type":"object","properties":{"topic":{"type":"string","minLength":1}},"required":["topic"],"additionalProperties":false}});
    let path = f.root.join("derived.json");
    std::fs::write(&path, definition.to_string()).unwrap();
    let output = f
        .cli(&["presets", "publish", "--file", path.to_str().unwrap()])
        .await;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let published: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(published["instructions"], "inspect");
    assert_eq!(published["extends"]["name"], "worker");
    let mut mcp = McpServer::new(f.client(), true);
    initialize(&mut mcp).await;
    let replay = call(&mut mcp, "presets_publish", json!({"preset":definition})).await;
    assert_eq!(replay["result"]["structuredContent"], published);
    let bad=call(&mut mcp,"fleets_create",json!({"plan":{"preset_name":"derived","preset_version":"1","count":1,"parameters":{"extra":"wrong"}},"key":"parameterized"})).await;
    assert_eq!(bad["result"]["isError"], true);
    assert_eq!(f.store.capacity().unwrap()["reserved"], 0);
    let plan = json!({"preset_name":"derived","preset_version":"1","count":1,"parameters":{"topic":"Harness³"}});
    let accepted = call(
        &mut mcp,
        "fleets_create",
        json!({"plan":plan,"key":"parameterized"}),
    )
    .await;
    assert_eq!(accepted["result"]["isError"], false);
    let path = f.root.join("fleet.json");
    std::fs::write(&path, plan.to_string()).unwrap();
    let output = f
        .cli(&[
            "fleets",
            "create",
            "--file",
            path.to_str().unwrap(),
            "--key",
            "parameterized",
        ])
        .await;
    assert!(output.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout).unwrap(),
        accepted["result"]["structuredContent"]
    );
    f.store.reconcile_fleets().unwrap();
    let instances = f.store.list("instances").unwrap();
    assert_eq!(instances[0]["snapshot"]["objective"], "Inspect Harness³");
    let mut typo = definition.clone();
    typo["typo"] = true.into();
    assert!(
        f.client()
            .execute(Command::PresetsPublish { preset: typo })
            .await
            .is_err()
    );
}

#[tokio::test]
async fn durable_messages_share_http_cli_mcp_identity_idempotency_and_readonly_guards() {
    use harness3_platform::{domain::Spawn, messaging::MessageInput};
    let f = Fixture::new().await;
    let agent = f
        .store
        .spawn(
            &Spawn {
                preset_name: "worker".into(),
                preset_version: "1".into(),
                count: 1,
            },
            "messages",
        )
        .unwrap()["instance_ids"][0]
        .as_str()
        .unwrap()
        .to_owned();
    let input = json!({"text":"跨入口消息 🧪"});
    let path = f.root.join("message.json");
    std::fs::write(&path, input.to_string()).unwrap();
    let output = f
        .cli(&[
            "messages",
            "send",
            &agent,
            "--file",
            path.to_str().unwrap(),
            "--key",
            "same-message",
        ])
        .await;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt: Value = serde_json::from_slice(&output.stdout).unwrap();
    let replay = f
        .client()
        .execute(Command::MessagesSend {
            id: agent.clone(),
            input: serde_json::from_value::<MessageInput>(input.clone()).unwrap(),
            key: "same-message".into(),
        })
        .await
        .unwrap();
    assert_eq!(receipt, replay);
    let mut mcp = McpServer::new(f.client(), true);
    initialize(&mut mcp).await;
    assert_eq!(
        call(
            &mut mcp,
            "messages_send",
            json!({"id":agent,"input":input,"key":"same-message"})
        )
        .await["result"]["structuredContent"],
        receipt
    );
    let id = receipt["id"].as_str().unwrap();
    assert_eq!(
        call(&mut mcp, "messages_show", json!({"id":id})).await["result"]["structuredContent"]["sender_kind"],
        "operator"
    );
    let list = call(
        &mut mcp,
        "messages_list",
        json!({"instance_id":agent,"direction":"inbox","limit":1}),
    )
    .await;
    assert_eq!(list["result"]["structuredContent"]["messages"][0]["id"], id);
    let mut readonly = McpServer::new(f.client(), false);
    initialize(&mut readonly).await;
    assert_eq!(
        call(
            &mut readonly,
            "messages_send",
            json!({"id":agent,"input":input,"key":"write"})
        )
        .await["error"]["code"],
        -32602
    );
    assert_eq!(
        call(
            &mut readonly,
            "messages_action",
            json!({"id":id,"action":"discard","note":"x","key":"write"})
        )
        .await["error"]["code"],
        -32602
    );
    let http = reqwest::Client::new();
    let base = format!("http://127.0.0.1:{}", f.port);
    assert_eq!(
        http.get(format!("{base}/api/v1/messages"))
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    let response = http
        .post(format!("{base}/api/v1/agent-instances/{agent}/messages"))
        .bearer_auth("a".repeat(64))
        .header("Idempotency-Key", "spoof")
        .json(&json!({"text":"spoof","sender_kind":"agent","source_run_id":"fake"}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 422);
    let discarded = call(
        &mut mcp,
        "messages_action",
        json!({"id":id,"action":"discard","note":"operator checked","key":"discard"}),
    )
    .await;
    assert_eq!(
        discarded["result"]["structuredContent"]["delivery_state"],
        "discarded"
    );
    assert_eq!(f.store.recent_messages().unwrap().len(), 1);
}

#[tokio::test]
async fn autonomous_research_is_one_idempotent_job_across_cli_http_mcp() {
    use harness3_platform::research::ResearchInput;
    let f = Fixture::new().await;
    let input = json!({"title":"Independent research","objective":"Compare architectures","acceptance":"Evidence and limitations","preset_name":"worker","preset_version":"1","researchers":2});
    let path = f.root.join("research.json");
    std::fs::write(&path, input.to_string()).unwrap();
    let output = f
        .cli(&[
            "research",
            "create",
            "--file",
            path.to_str().unwrap(),
            "--key",
            "same-research",
        ])
        .await;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        f.client()
            .execute(Command::ResearchCreate {
                input: serde_json::from_value::<ResearchInput>(input.clone()).unwrap(),
                key: "same-research".into()
            })
            .await
            .unwrap(),
        receipt
    );
    let mut mcp = McpServer::new(f.client(), true);
    initialize(&mut mcp).await;
    assert_eq!(
        call(
            &mut mcp,
            "research_create",
            json!({"input":input,"key":"same-research"})
        )
        .await["result"]["structuredContent"],
        receipt
    );
    let id = receipt["id"].as_str().unwrap();
    assert_eq!(
        call(&mut mcp, "research_show", json!({"id":id})).await["result"]["structuredContent"]["researchers"],
        2
    );
    let mut readonly = McpServer::new(f.client(), false);
    initialize(&mut readonly).await;
    assert_eq!(
        call(
            &mut readonly,
            "research_create",
            json!({"input":input,"key":"blocked"})
        )
        .await["error"]["code"],
        -32602
    );
    assert_eq!(call(&mut readonly,"research_list",json!({})).await["result"]["structuredContent"]["research_jobs"].as_array().unwrap().len(),1);
    call(&mut mcp, "research_cancel", json!({"id":id})).await;
    f.store.reconcile_research().unwrap();
    assert_eq!(f.store.research(id).unwrap()["state"], "cancelled");
    assert_eq!(f.store.capacity().unwrap()["reserved"], 0);
}

#[tokio::test]
async fn artifact_versions_and_rework_share_http_cli_mcp_and_stale_review_guards() {
    use harness3_platform::{
        artifacts::ReworkInput,
        domain::{GoalPlan, Spawn, TaskPlan},
    };
    let f = Fixture::new().await;
    let fleet = f
        .store
        .spawn(
            &Spawn {
                preset_name: "worker".into(),
                preset_version: "1".into(),
                count: 1,
            },
            "artifact-worker",
        )
        .unwrap();
    let goal = f
        .store
        .create_goal(
            &GoalPlan {
                verification: None,
                title: "review evidence".into(),
                objective: "analyze".into(),
                acceptance: "human review".into(),
                tasks: vec![TaskPlan {
                    key: "report".into(),
                    instance_id: fleet["instance_ids"][0].as_str().unwrap().into(),
                    instructions: "analyze".into(),
                    depends_on: vec![],
                    ..Default::default()
                }],
            },
            "artifact-goal",
        )
        .unwrap();
    let id = goal["id"].as_str().unwrap();
    f.store.reconcile_goals().unwrap();
    let run = f.store.claim().unwrap().unwrap();
    f.store
        .finish(
            run["id"].as_str().unwrap(),
            run["instance_id"].as_str().unwrap(),
            "completed",
            json!({"output":"first evidence"}),
        )
        .unwrap();
    f.store.reconcile_goals().unwrap();
    let list = f
        .client()
        .execute(Command::ArtifactsList {
            goal_id: Some(id.into()),
            after: 0,
            limit: None,
        })
        .await
        .unwrap();
    let aid = list["artifacts"][0]["id"].as_str().unwrap();
    let mut mcp = McpServer::new(f.client(), true);
    initialize(&mut mcp).await;
    let view = call(&mut mcp, "artifacts_show", json!({"id":aid})).await;
    assert_eq!(
        view["result"]["structuredContent"]["content"],
        "first evidence"
    );
    let cli = f.cli(&["artifacts", "show", aid]).await;
    assert!(cli.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&cli.stdout).unwrap()["id"],
        aid
    );
    let input = ReworkInput {
        expected_round: 1,
        note: "add evidence".into(),
        assignments: vec![],
    };
    let receipt = call(
        &mut mcp,
        "goals_rework",
        json!({"id":id,"input":input,"key":"revise"}),
    )
    .await;
    assert_eq!(receipt["result"]["structuredContent"]["round"], 2);
    assert_eq!(
        f.client()
            .execute(Command::GoalsRework {
                id: id.into(),
                input,
                key: "revise".into()
            })
            .await
            .unwrap(),
        receipt["result"]["structuredContent"]
    );
    f.store.reconcile_fleets().unwrap();
    f.store.reconcile_goals().unwrap();
    let run = f.store.claim().unwrap().unwrap();
    f.store
        .finish(
            run["id"].as_str().unwrap(),
            run["instance_id"].as_str().unwrap(),
            "completed",
            json!({"output":"revised evidence"}),
        )
        .unwrap();
    f.store.reconcile_goals().unwrap();
    let stale = call(&mut mcp, "goals_accept", json!({"id":id,"review":"stale"})).await;
    assert_eq!(stale["result"]["isError"], true);
    assert!(stale.to_string().contains("goal_round_conflict"));
    let accepted = f
        .cli(&[
            "goals",
            "accept",
            id,
            "--review",
            "checked revised evidence",
            "--expected-round",
            "2",
        ])
        .await;
    assert!(accepted.status.success());
    assert_eq!(
        f.store.goal(id).unwrap()["rounds"][1]["manifest"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let mut readonly = McpServer::new(f.client(), false);
    initialize(&mut readonly).await;
    assert!(
        call(
            &mut readonly,
            "goals_rework",
            json!({"id":id,"input":{"expected_round":2,"note":"again"},"key":"no-write"})
        )
        .await
        .get("error")
        .is_some()
    );
    assert_eq!(
        call(&mut readonly, "artifacts_show", json!({"id":aid})).await["result"]["isError"],
        false
    );
}

#[tokio::test]
async fn shared_context_uses_one_version_contract_across_http_cli_and_mcp() {
    use harness3_platform::domain::{GoalPlan, Spawn, TaskPlan};
    let f = Fixture::new().await;
    let fleet = f
        .store
        .spawn(
            &Spawn {
                preset_name: "worker".into(),
                preset_version: "1".into(),
                count: 1,
            },
            "context-worker",
        )
        .unwrap();
    let goal = f
        .store
        .create_goal(
            &GoalPlan {
                verification: None,
                title: "shared findings".into(),
                objective: "compare".into(),
                acceptance: "review sources".into(),
                tasks: vec![TaskPlan {
                    key: "report".into(),
                    instance_id: fleet["instance_ids"][0].as_str().unwrap().into(),
                    instructions: "report".into(),
                    depends_on: vec![],
                    ..Default::default()
                }],
            },
            "context-goal",
        )
        .unwrap();
    let id = goal["id"].as_str().unwrap();
    let input = json!({"key":"scope","expected_revision":0,"title":"Research scope","body":"Only supplied material","kind":"decision"});
    let file = f.root.join("context.json");
    std::fs::write(&file, input.to_string()).unwrap();
    let cli = f
        .cli(&[
            "context",
            "publish",
            id,
            "--file",
            file.to_str().unwrap(),
            "--key",
            "initial",
        ])
        .await;
    assert!(
        cli.status.success(),
        "{}",
        String::from_utf8_lossy(&cli.stderr)
    );
    let receipt: Value = serde_json::from_slice(&cli.stdout).unwrap();
    let mut mcp = McpServer::new(f.client(), true);
    initialize(&mut mcp).await;
    assert_eq!(
        call(
            &mut mcp,
            "context_publish",
            json!({"id":id,"input":input,"key":"initial"})
        )
        .await["result"]["structuredContent"],
        receipt
    );
    let board = f
        .client()
        .execute(Command::ContextRead { id: id.into() })
        .await
        .unwrap();
    assert_eq!(board["revision"], 1);
    assert_eq!(board["entries"][0]["author_kind"], "operator");
    let mut revised = input.clone();
    revised["expected_revision"] = json!(1);
    revised["body"] = json!("Scope clarified by operator");
    assert_eq!(
        call(
            &mut mcp,
            "context_publish",
            json!({"id":id,"input":revised,"key":"revision"})
        )
        .await["result"]["isError"],
        false
    );
    let stale = call(
        &mut mcp,
        "context_publish",
        json!({"id":id,"input":input,"key":"stale"}),
    )
    .await;
    assert!(stale.to_string().contains("context_revision_conflict"));
    let cli = f.cli(&["context", "history", id, "scope"]).await;
    assert!(cli.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&cli.stdout).unwrap()["versions"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let mut readonly = McpServer::new(f.client(), false);
    initialize(&mut readonly).await;
    assert_eq!(
        call(&mut readonly, "context_read", json!({"id":id})).await["result"]["structuredContent"]
            ["revision"],
        2
    );
    assert!(
        call(
            &mut readonly,
            "context_publish",
            json!({"id":id,"input":revised,"key":"denied"})
        )
        .await
        .get("error")
        .is_some()
    );
    f.store.reconcile_goals().unwrap();
    let run = f.store.claim().unwrap().unwrap();
    f.store
        .finish(
            run["id"].as_str().unwrap(),
            run["instance_id"].as_str().unwrap(),
            "completed",
            json!({"output":"verified contribution"}),
        )
        .unwrap();
    f.store.reconcile_goals().unwrap();
    let stale = call(
        &mut mcp,
        "goals_accept",
        json!({"id":id,"review":"stale","expected_context_revision":1}),
    )
    .await;
    assert!(stale.to_string().contains("context_revision_conflict"));
    let cli = f
        .cli(&[
            "goals",
            "accept",
            id,
            "--review",
            "checked shared scope",
            "--expected-context-revision",
            "2",
        ])
        .await;
    assert!(
        cli.status.success(),
        "{}",
        String::from_utf8_lossy(&cli.stderr)
    );
    assert_eq!(
        f.store.goal(id).unwrap()["rounds"][0]["shared_context"]["revision"],
        2
    );
}

#[tokio::test]
async fn task_claims_and_attempts_share_cli_http_mcp_receipts_and_ownership_conflicts() {
    use harness3_platform::{
        claims::{ClaimInput, ReleaseInput},
        domain::Spawn,
    };
    let f = Fixture::new().await;
    let fleet = f
        .store
        .spawn(
            &Spawn {
                preset_name: "worker".into(),
                preset_version: "1".into(),
                count: 2,
            },
            "candidates",
        )
        .unwrap();
    let a = fleet["instance_ids"][0].as_str().unwrap();
    let b = fleet["instance_ids"][1].as_str().unwrap();
    let plan = json!({"title":"shared candidate task","objective":"inspect","acceptance":"review","tasks":[{"key":"report","instance_id":a,"candidates":[b],"manual_claim":true,"instructions":"inspect","depends_on":[]}]});
    let file = f.root.join("goal.json");
    std::fs::write(&file, plan.to_string()).unwrap();
    let cli = f
        .cli(&[
            "goals",
            "create",
            "--file",
            file.to_str().unwrap(),
            "--key",
            "pool-goal",
        ])
        .await;
    assert!(
        cli.status.success(),
        "{}",
        String::from_utf8_lossy(&cli.stderr)
    );
    let created: Value = serde_json::from_slice(&cli.stdout).unwrap();
    let id = created["id"].as_str().unwrap();
    let shared = f
        .client()
        .execute(
            serde_json::from_value(json!({"tool":"goals_create","plan":plan,"key":"pool-goal"}))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(created, shared);
    let mut readonly = McpServer::new(f.client(), false);
    initialize(&mut readonly).await;
    assert_eq!(harness3_platform::mcp::tools(true).len(), 44);
    assert_eq!(harness3_platform::mcp::tools(false).len(), 22);
    let t = call(
        &mut readonly,
        "tasks_show",
        json!({"id":id,"task":"report"}),
    )
    .await;
    assert_eq!(t["result"]["structuredContent"]["claim"]["mode"], "manual");
    let input = ClaimInput {
        expected_round: 1,
        expected_generation: 0,
        instance_id: b.into(),
    };
    assert!(
        call(
            &mut readonly,
            "tasks_claim",
            json!({"id":id,"task":"report","input":input,"key":"claim"})
        )
        .await
        .get("error")
        .is_some()
    );
    let mut mcp = McpServer::new(f.client(), true);
    initialize(&mut mcp).await;
    let claimed = call(
        &mut mcp,
        "tasks_claim",
        json!({"id":id,"task":"report","input":input,"key":"claim"}),
    )
    .await;
    assert_eq!(claimed["result"]["isError"], false);
    let receipt = f
        .client()
        .execute(Command::TasksClaim {
            id: id.into(),
            task: "report".into(),
            input: input.clone(),
            key: "claim".into(),
        })
        .await
        .unwrap();
    assert_eq!(receipt, claimed["result"]["structuredContent"]);
    let file = f.root.join("release.json");
    std::fs::write(
        &file,
        serde_json::to_vec(&ReleaseInput {
            expected_round: 1,
            expected_generation: 1,
            note: "Choose primary instead".into(),
        })
        .unwrap(),
    )
    .unwrap();
    let cli = f
        .cli(&[
            "tasks",
            "release",
            id,
            "report",
            "--file",
            file.to_str().unwrap(),
            "--key",
            "release",
        ])
        .await;
    assert!(
        cli.status.success(),
        "{}",
        String::from_utf8_lossy(&cli.stderr)
    );
    let stale = call(
        &mut mcp,
        "tasks_claim",
        json!({"id":id,"task":"report","input":input,"key":"stale"}),
    )
    .await;
    assert!(stale.to_string().contains("task_generation_conflict"));
    let file = f.root.join("claim.json");
    std::fs::write(
        &file,
        serde_json::to_vec(&ClaimInput {
            expected_round: 1,
            expected_generation: 1,
            instance_id: a.into(),
        })
        .unwrap(),
    )
    .unwrap();
    let cli = f
        .cli(&[
            "tasks",
            "claim",
            id,
            "report",
            "--file",
            file.to_str().unwrap(),
            "--key",
            "second",
        ])
        .await;
    assert!(
        cli.status.success(),
        "{}",
        String::from_utf8_lossy(&cli.stderr)
    );
    let cli = f.cli(&["tasks", "show", id, "report"]).await;
    assert!(cli.status.success());
    let t: Value = serde_json::from_slice(&cli.stdout).unwrap();
    assert_eq!(t["attempts"].as_array().unwrap().len(), 2);
    assert_eq!(t["attempts"][1]["state"], "released");
    assert_eq!(t["attempts"][0]["instance_id"], a);
    let run = f.store.claim().unwrap().unwrap();
    let denied=call(&mut mcp,"tasks_release",json!({"id":id,"task":"report","input":{"expected_round":1,"expected_generation":2,"note":"too late"},"key":"too-late"})).await;
    assert!(denied.to_string().contains("task_already_started"));
    f.store
        .finish(
            run["id"].as_str().unwrap(),
            a,
            "completed",
            json!({"output":"done"}),
        )
        .unwrap();
    f.store.reconcile_goals().unwrap();
    assert_eq!(
        f.store.task(id, "report").unwrap()["attempts"][0]["state"],
        "completed"
    );
}

#[tokio::test]
async fn delegation_policy_and_runtime_expansion_are_visible_across_cli_http_and_mcp() {
    let f = Fixture::new().await;
    f.store.publish_definition(json!({"name":"delegate","version":"1","purpose":"coordinate","objective":"collect evidence","instructions":"delegate","protocol":"demo","plugins":[{"id":"platform.delegation","version":"1.0.0"}]})).unwrap();
    let fleet = f
        .store
        .spawn(
            &harness3_platform::domain::Spawn {
                preset_name: "delegate".into(),
                preset_version: "1".into(),
                count: 1,
            },
            "parent",
        )
        .unwrap();
    let plan = json!({"title":"cross-entry delegation","objective":"fan out then resume","acceptance":"verify children and original continuation","tasks":[{"key":"root","instance_id":fleet["instance_ids"][0],"instructions":"compare evidence","delegation":{"max_depth":1,"max_children":2,"max_descendants":2,"roles":[{"name":"worker","preset_name":"worker","preset_version":"1"}]}}]});
    let file = f.root.join("delegation.json");
    std::fs::write(&file, plan.to_string()).unwrap();
    let output = f
        .cli(&[
            "goals",
            "create",
            "--file",
            file.to_str().unwrap(),
            "--key",
            "delegate-cli",
        ])
        .await;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt: Value = serde_json::from_slice(&output.stdout).unwrap();
    let goal = receipt["id"].as_str().unwrap();
    f.store.reconcile_goals().unwrap();
    let run = f.store.claim().unwrap().unwrap();
    let delegated = f
        .store
        .submit_delegation(
            run["id"].as_str().unwrap(),
            &harness3_platform::delegation::Plan {
                tasks: vec![harness3_platform::delegation::Child {
                    role: "worker".into(),
                    title: "independent child".into(),
                    instructions: "inspect".into(),
                }],
                continuation: "check evidence".into(),
            },
        )
        .unwrap();
    f.store
        .finish(
            run["id"].as_str().unwrap(),
            run["instance_id"].as_str().unwrap(),
            "completed",
            json!({"output":"handoff"}),
        )
        .unwrap();
    f.store.reconcile_fleets().unwrap();
    f.store.reconcile_goals().unwrap();
    let http = f
        .client()
        .execute(Command::GoalsShow { id: goal.into() })
        .await
        .unwrap();
    assert_eq!(http["tasks"].as_array().unwrap().len(), 2);
    let mut server = McpServer::new(f.client(), false);
    initialize(&mut server).await;
    let reply = call(&mut server, "tasks_show", json!({"id":goal,"task":"root"})).await;
    let text = reply["result"]["content"][0]["text"].as_str().unwrap();
    let body: Value = serde_json::from_str(text).unwrap();
    assert_eq!(body["delegations"][0]["id"], delegated["id"]);
    assert_eq!(
        body["delegation_scope"]["policy"]["roles"][0]["name"],
        "worker"
    );
    assert_eq!(body["state"], "pending");
}

#[tokio::test]
async fn verifier_retry_cli_and_mcp_share_http_receipt_and_readonly_boundary() {
    use harness3_platform::domain::{GoalPlan, Spawn};
    let f = Fixture::new().await;
    let agent = f
        .store
        .spawn(
            &Spawn {
                preset_name: "worker".into(),
                preset_version: "1".into(),
                count: 1,
            },
            "verify-agent",
        )
        .unwrap();
    let plan:GoalPlan=serde_json::from_value(json!({"title":"verify","objective":"check evidence","acceptance":"supported","tasks":[{"key":"work","instance_id":agent["instance_ids"][0],"instructions":"produce evidence"}],"verification":{"preset_name":"worker","preset_version":"1","checks":[{"key":"check","description":"supported"}],"max_auto_reworks":0}})).unwrap();
    let g = f.store.create_goal(&plan, "verify-goal").unwrap();
    let id = g["id"].as_str().unwrap();
    f.store.reconcile_goals().unwrap();
    let r = f.store.claim().unwrap().unwrap();
    f.store
        .finish(
            r["id"].as_str().unwrap(),
            r["instance_id"].as_str().unwrap(),
            "completed",
            json!({"output":"evidence"}),
        )
        .unwrap();
    for _ in 0..3 {
        f.store.reconcile_fleets().unwrap();
        f.store.reconcile_goals().unwrap();
    }
    let r = f.store.claim().unwrap().unwrap();
    f.store
        .finish(
            r["id"].as_str().unwrap(),
            r["instance_id"].as_str().unwrap(),
            "failed",
            json!({"error":{"code":"test_failure"}}),
        )
        .unwrap();
    f.store.reconcile_goals().unwrap();
    let goal = f.store.goal(id).unwrap();
    let old = goal["verification"]["current"]["id"].as_str().unwrap();
    let output = f
        .cli(&[
            "goals",
            "verify",
            id,
            "--expected-round",
            "1",
            "--expected-verification-id",
            old,
            "--key",
            "shared-retry",
        ])
        .await;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt: Value = serde_json::from_slice(&output.stdout).unwrap();
    let args = json!({"id":id,"input":{"expected_round":1,"expected_verification_id":old},"key":"shared-retry"});
    let mut mcp = McpServer::new(f.client(), true);
    initialize(&mut mcp).await;
    let result = call(&mut mcp, "verification_retry", args.clone()).await;
    assert_eq!(result["result"]["structuredContent"], receipt);
    let mut readonly = McpServer::new(f.client(), false);
    initialize(&mut readonly).await;
    let denied = call(&mut readonly, "verification_retry", args).await;
    assert!(denied["error"].is_object() || denied["result"]["isError"] == true);
    assert_eq!(
        f.store.goal(id).unwrap()["verification"]["history"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}
