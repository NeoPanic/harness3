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
