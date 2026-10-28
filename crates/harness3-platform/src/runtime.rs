use crate::{domain::*, models::Models, store::Store};
use harness3::{session::SessionStore, *};
use serde_json::{Value, json};
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokio::sync::Mutex;

pub type Cancellations = Arc<Mutex<HashMap<String, CancellationToken>>>;
struct InputSource;
impl Source for InputSource {
    fn name(&self) -> &str {
        "platform"
    }
    fn decode(&self, event: &InboundEvent) -> Result<Request> {
        Ok(Request {
            task: "agent".into(),
            text: event.payload["text"]
                .as_str()
                .ok_or_else(|| Error::new("invalid_input"))?
                .into(),
        })
    }
}
pub struct DemoModel;
#[async_trait::async_trait]
impl Model for DemoModel {
    fn metadata(&self) -> Value {
        json!({"protocol":"demo","deterministic":true})
    }
    async fn complete(
        &self,
        input: ModelInput,
        _live: &LiveOutput,
        cancel: CancellationToken,
    ) -> Result<ModelReply> {
        // A small interruptible delay makes the real execution phases inspectable.
        tokio::select! {_=cancel.cancelled()=>return Err(Error::new("cancelled")),_=tokio::time::sleep(Duration::from_millis(500))=>{}}
        if let Some(spec) = input
            .tools
            .iter()
            .find(|s| s.name == "platform.research.plan")
        {
            let finished = matches!(input.turns.last(), Some(Turn::Tools(_)));
            let count = spec.parameters["properties"]["tasks"]["minItems"]
                .as_u64()
                .unwrap() as usize;
            return Ok(ModelReply {
                text: if finished {
                    "[确定性演示] 分工计划已提交；研究任务由平台派发。".into()
                } else {
                    String::new()
                },
                calls: if finished {
                    vec![]
                } else {
                    vec![Call {
                        id: "demo-plan".into(),
                        name: spec.name.clone(),
                        arguments: json!({"tasks":(0..count).map(|i|json!({"title":format!("演示维度 {}",i+1),"instructions":format!("确定性演示：检查第 {} 个维度的输入文本，报告工具统计；这不是语义调研。",i+1)})).collect::<Vec<_>>(),"synthesis":"确定性演示：检查上游贡献文本；不宣称执行真实调研。"}),
                    }]
                },
                usage: None,
                continuation: Value::Null,
                reasoning: None,
            });
        }
        if input
            .tools
            .iter()
            .any(|s| s.name == "platform.context.read")
        {
            let previous = if let Some(Turn::Tools(observations)) = input.turns.last() {
                observations.first()
            } else {
                None
            };
            let (text, calls) = match previous {
                Some(o)
                    if o.call.name == "platform.context.read"
                        && o.call.id == "demo-context-refresh" =>
                {
                    (
                        format!(
                            "[确定性演示 / Demo] 已通过共享上下文工具发布独立记录，并再次读取目标共享区。当前版本 {}，共 {} 条记录。该演示验证多实例读写链路，不执行语义研究。",
                            o.output.content["revision"],
                            o.output.content["entries"].as_array().map_or(0, Vec::len)
                        ),
                        vec![],
                    )
                }
                Some(o) if o.call.name == "platform.context.read" => {
                    let run = o.output.content["source_run_id"]
                        .as_str()
                        .ok_or_else(|| Error::new("context_scope_unavailable"))?;
                    (
                        String::new(),
                        vec![Call {
                            id: "demo-context-publish".into(),
                            name: "platform.context.publish".into(),
                            arguments: json!({"key":format!("demo.{run}"),"expected_revision":0,"title":"确定性演示贡献","body":"此记录由当前任务的实际 Agent 发布，用于验证共享上下文；不代表语义研究或事实验证。","kind":"observation","command_key":"demo-publish"}),
                        }],
                    )
                }
                Some(o) if o.call.name == "platform.context.publish" => (
                    String::new(),
                    vec![Call {
                        id: "demo-context-refresh".into(),
                        name: "platform.context.read".into(),
                        arguments: json!({}),
                    }],
                ),
                _ => (
                    String::new(),
                    vec![Call {
                        id: "demo-context-read".into(),
                        name: "platform.context.read".into(),
                        arguments: json!({}),
                    }],
                ),
            };
            return Ok(ModelReply {
                text,
                calls,
                usage: None,
                continuation: Value::Null,
                reasoning: None,
            });
        }
        let (text, calls) = if let Some(Turn::Tools(observations)) = input.turns.last() {
            let evidence = observations
                .first()
                .ok_or_else(|| Error::new("missing_tool_result"))?;
            (
                format!(
                    "[确定性演示 / Demo]\n已通过 text.inspect 检查输入。\n\n字符数：{}\n行数：{}\nUTF-8 字节数：{}\n\n这是一次真实 Harness 工具循环；演示模型不执行语义推理；任务依赖调度与贡献传递由平台负责。",
                    evidence.output.content["characters"],
                    evidence.output.content["lines"],
                    evidence.output.content["bytes"]
                ),
                vec![],
            )
        } else {
            (
                String::new(),
                vec![Call {
                    id: "inspect-1".into(),
                    name: "text.inspect".into(),
                    arguments: json!({"text":input.turns.iter().rev().find_map(|turn|if let Turn::User(text)=turn{Some(text.as_str())}else{None}).unwrap_or(&input.text)}),
                }],
            )
        };
        Ok(ModelReply {
            text,
            calls,
            usage: None,
            continuation: Value::Null,
            reasoning: None,
        })
    }
}
struct InspectText;
#[async_trait::async_trait]
impl Tool for InspectText {
    fn spec(&self) -> ToolSpec {
        ToolSpec{name:"text.inspect".into(),description:"Inspect Unicode scalar count, lines and UTF-8 bytes in text. Read-only; does not read files.".into(),parameters:json!({"type":"object","properties":{"text":{"type":"string","maxLength":64000}},"required":["text"],"additionalProperties":false}),mutates:false}
    }
    fn parallel_safe(&self) -> bool {
        true
    }
    async fn execute(
        &self,
        _: &Context,
        arguments: Value,
        _: CancellationToken,
    ) -> Result<ToolOutput> {
        let object = arguments
            .as_object()
            .filter(|v| v.len() == 1)
            .ok_or_else(|| Error::new("invalid_arguments"))?;
        let text = object
            .get("text")
            .and_then(Value::as_str)
            .filter(|s| s.len() <= 64000)
            .ok_or_else(|| Error::new("invalid_arguments"))?;
        Ok(ToolOutput {
            content: json!({"characters":text.chars().count(),"lines":text.lines().count(),"bytes":text.len()}),
            changed: false,
        })
    }
}
struct RunObserver {
    store: Store,
    run: String,
}
#[async_trait::async_trait]
impl Observer for RunObserver {
    async fn on_event(&self, _: &Context, event: &Event) -> Result<()> {
        self.store
            .record(
                &format!("execution.{}", event.kind),
                &self.run,
                json!({"sequence":event.sequence,"data":event.data}),
            )
            .map_err(|_| Error::new("observer_storage_error"))
    }
}
async fn execute(
    store: &Store,
    models: &Models,
    work: &Value,
    cancel: CancellationToken,
    plugins: &crate::plugins::ProcessHost,
) -> PResult<Value> {
    let agent = work["instance_id"]
        .as_str()
        .ok_or_else(|| fault(500, "invalid_work"))?;
    let run = work["id"]
        .as_str()
        .ok_or_else(|| fault(500, "invalid_work"))?;
    let snapshot = &work["snapshot"];
    let preset = crate::presets::runtime_preset(snapshot)?;
    let bound = crate::presets::bindings(snapshot);
    let planning = store.planner_job(run)?;
    let mut instructions = format!(
        "{}\nPurpose: {}\nObjective: {}",
        preset.instructions, preset.purpose, preset.objective
    );
    for plugin in &bound {
        instructions.push_str(&format!(
            "\nPlugin instructions:\n{}",
            plugin["instructions"]
                .as_str()
                .or_else(|| plugin["manifest"]["instructions"].as_str())
                .unwrap_or_default()
        ));
    }
    if let Some(job) = &planning {
        let mut worker_tools = vec![
            json!({"name":"text.inspect","description":"Count Unicode characters, lines and bytes in supplied text; no filesystem or network access."}),
        ];
        for plugin in &bound {
            if plugin["kind"] == "process"
                && let Some(tools) = plugin["manifest"]["tools"].as_array()
            {
                for tool in tools {
                    worker_tools.push(json!({"name":format!("{}.{}",plugin["id"].as_str().unwrap_or_default(),tool["name"].as_str().unwrap_or_default()),"description":tool["description"].as_str().unwrap_or_default().chars().take(512).collect::<String>()}));
                }
            }
            if plugin["kind"] == "organization" {
                for tool in plugin["tools"].as_array().unwrap() {
                    worker_tools
                        .push(json!({"name":tool["name"],"description":tool["description"]}));
                }
            }
        }
        instructions = format!(
            "You are the planning Agent for a bounded research team. Your only tool submits a plan; you cannot execute research tools, spawn recursively or pick identities. Produce exactly {} independent assignments and one synthesis brief. The selected worker role is:\n{}\nAvailable worker capabilities (declarations only): {}\nNever claim web research unless an actual retrieval tool is available. Submit the plan then stop.",
            job["researchers"],
            instructions,
            serde_json::to_string(&worker_tools)?
        );
    }
    let model: Arc<dyn Model> = if preset.protocol == Protocol::Demo {
        Arc::new(DemoModel)
    } else {
        models.get(&preset.protocol)?
    };
    let history = store.load(agent).await.map_err(core_error)?;
    if let Some(previous) = history
        .requests
        .iter()
        .rev()
        .find(|request| request.purpose.is_none())
        && previous.model_metadata != model.metadata()
    {
        return Err(fault(409, "model_profile_changed_create_new_instance"));
    }
    let mut registry = Registry::default();
    registry.source(Arc::new(InputSource)).map_err(core_error)?;
    registry.model("selected", model).map_err(core_error)?;
    let mut tools = Vec::new();
    if let Some(job) = &planning {
        let tool = crate::research::tool(
            store.clone(),
            run,
            job["researchers"].as_u64().unwrap() as usize,
        );
        tools.push(tool.spec().name);
        registry.tool(tool).map_err(core_error)?;
    } else {
        registry.tool(Arc::new(InspectText)).map_err(core_error)?;
        tools.push("text.inspect".to_owned());
        for snapshot in bound.iter().filter(|p| p["kind"] == "organization") {
            for tool in crate::organization::tools(snapshot, store.clone(), run)? {
                tools.push(tool.spec().name);
                registry.tool(tool).map_err(core_error)?;
            }
        }
        for snapshot in bound.into_iter().filter(|p| p["kind"] == "process") {
            for tool in plugins.tools(serde_json::from_value(snapshot)?)? {
                tools.push(tool.spec().name);
                registry.tool(tool).map_err(core_error)?;
            }
        }
    }
    registry.observer(Arc::new(RunObserver {
        store: store.clone(),
        run: run.into(),
    }));
    registry
        .task(AgentProfile {
            name: "agent".into(),
            instructions,
            tools,
            required_tool: planning.as_ref().map(|_| "platform.research.plan".into()),
            skills: vec![],
        })
        .map_err(core_error)?;
    let harness = Harness::new(
        registry,
        Limits {
            rounds: 12,
            tool_calls: 24,
            timeout: Duration::from_secs(120),
        },
    );
    let report = harness
        .run_session(
            store,
            agent,
            InboundEvent {
                source: "platform".into(),
                id: run.into(),
                payload: json!({"text":work["input"]}),
            },
            "selected",
            cancel,
        )
        .await
        .map_err(core_error)?;
    Ok(serde_json::to_value(report)?)
}
fn core_error(error: Error) -> Fault {
    fault(500, &error.code)
}

pub async fn recover(store: &Store) -> PResult<()> {
    // The daemon's OS file lock proves no older platform process still owns these sessions.
    for agent in store.active_sessions()? {
        let session = store.load(&agent).await.map_err(core_error)?;
        if let Some(active) = session.active_run {
            harness3::session::recover_interrupted(store, &agent, &active)
                .await
                .map_err(core_error)?;
        }
    }
    store.recover_startup()?;
    store.reconcile_fleets()?;
    store.reconcile_research()?;
    store.reconcile_messages()?;
    store.reconcile_goals()
}

pub async fn worker_loop(
    store: Store,
    models: Models,
    cancellations: Cancellations,
    shutdown: CancellationToken,
) {
    let plugins = crate::plugins::ProcessHost::disabled(store.clone());
    worker_loop_with_plugins(store, models, cancellations, shutdown, plugins).await
}

pub async fn worker_loop_with_plugins(
    store: Store,
    models: Models,
    cancellations: Cancellations,
    shutdown: CancellationToken,
    plugins: crate::plugins::ProcessHost,
) {
    let mut tasks = tokio::task::JoinSet::new();
    loop {
        if shutdown.is_cancelled() {
            break;
        }
        if let Err(error) = store.reconcile_fleets() {
            eprintln!("Fleet reconciliation failed: {error}");
        }
        {
            let tokens = cancellations.lock().await;
            for (run, token) in tokens.iter() {
                if store.cancel_requested(run).unwrap_or(true) {
                    token.cancel();
                }
            }
        }
        if let Err(error) = store.reconcile_research() {
            eprintln!("Research coordination failed: {error}");
        }
        if let Err(error) = store.reconcile_messages() {
            eprintln!("Message reconciliation failed: {error}");
        }
        if let Err(error) = store.reconcile_goals() {
            eprintln!("Goal coordination failed: {error}");
        }
        if tasks.len() < 4 {
            match store.claim() {
                Ok(Some(work)) => {
                    let run = work["id"].as_str().unwrap_or_default().to_string();
                    let agent = work["instance_id"].as_str().unwrap_or_default().to_string();
                    let cancel = shutdown.child_token();
                    cancellations
                        .lock()
                        .await
                        .insert(run.clone(), cancel.clone());
                    let store = store.clone();
                    let cancellations = cancellations.clone();
                    let models = models.clone();
                    let plugins = plugins.clone();
                    tasks.spawn(async move {
                        if store.cancel_requested(&run).unwrap_or(true) {
                            cancel.cancel();
                        }
                        let outcome = execute(&store, &models, &work, cancel, &plugins).await;
                        let report = match outcome {
                            Ok(report) => report,
                            Err(error) => json!({"error":{"code":error.1}}),
                        };
                        let state = match report["error"]["code"].as_str() {
                            None => "completed",
                            Some("cancelled") => "cancelled",
                            Some(_) => "failed",
                        };
                        if let Err(error) = store.finish(&run, &agent, state, report) {
                            eprintln!("Cannot settle {run}: {error}");
                        }
                        cancellations.lock().await.remove(&run);
                    });
                    continue;
                }
                Ok(None) => {}
                Err(error) => eprintln!("Queue claim failed: {error}"),
            }
        }
        tokio::select! {
            _=shutdown.cancelled()=>break,
            _=tokio::time::sleep(Duration::from_millis(150))=>{},
            _=tasks.join_next(),if !tasks.is_empty()=>{},
        }
    }
    // Cancellation is cooperative; drain all started Harness calls before releasing the lock.
    shutdown.cancel();
    while let Some(result) = tasks.join_next().await {
        if let Err(error) = result {
            eprintln!("Worker failed: {error}");
        }
    }
}
