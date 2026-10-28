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
) -> PResult<Value> {
    let agent = work["instance_id"]
        .as_str()
        .ok_or_else(|| fault(500, "invalid_work"))?;
    let run = work["id"]
        .as_str()
        .ok_or_else(|| fault(500, "invalid_work"))?;
    let mut snapshot = work["snapshot"].clone();
    let plugin = snapshot
        .as_object_mut()
        .and_then(|v| v.remove("plugin_snapshot"));
    let preset: Preset = serde_json::from_value(snapshot)?;
    let mut instructions = format!(
        "{}\nPurpose: {}\nObjective: {}",
        preset.instructions, preset.purpose, preset.objective
    );
    if let Some(plugin) = plugin {
        instructions.push_str(&format!(
            "\nPlugin instructions:\n{}",
            plugin["instructions"].as_str().unwrap_or_default()
        ));
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
    registry.tool(Arc::new(InspectText)).map_err(core_error)?;
    registry.observer(Arc::new(RunObserver {
        store: store.clone(),
        run: run.into(),
    }));
    registry
        .task(AgentProfile {
            name: "agent".into(),
            instructions,
            tools: vec!["text.inspect".into()],
            required_tool: None,
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
    store.reconcile_lifecycle()?;
    store.reconcile_goals()
}

pub async fn worker_loop(
    store: Store,
    models: Models,
    cancellations: Cancellations,
    shutdown: CancellationToken,
) {
    let mut tasks = tokio::task::JoinSet::new();
    loop {
        if shutdown.is_cancelled() {
            break;
        }
        if let Err(error) = store.reconcile_lifecycle() {
            eprintln!("Lifecycle reconciliation failed: {error}");
        }
        {
            let tokens = cancellations.lock().await;
            for (run, token) in tokens.iter() {
                if store.cancel_requested(run).unwrap_or(true) {
                    token.cancel();
                }
            }
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
                    tasks.spawn(async move {
                        if store.cancel_requested(&run).unwrap_or(true) {
                            cancel.cancel();
                        }
                        let outcome = execute(&store, &models, &work, cancel).await;
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
