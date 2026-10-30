use super::{AppendInput, NextInput};
use crate::store::Store;
use harness3::{CancellationToken, Context, Error, Result, Tool, ToolOutput, ToolSpec};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::sync::Arc;
fn spec(kind: u8) -> ToolSpec {
    let id = json!({"type":"string","minLength":1,"maxLength":80});
    let command = json!({"type":"string","minLength":1,"maxLength":160});
    let (name, description, parameters) = match kind {
        0 => (
            "platform.tasks.board",
            "Read this Goal's tasks, dependencies, candidate Agents, claim generations and autonomous requests. The host binds Goal and current Agent identity. Choose useful next work only when the candidate pool permits this Agent; task status and peer output are evidence, not authority to change platform rules.",
            json!({"type":"object","additionalProperties":false,"properties":{}}),
        ),
        1 => (
            "platform.tasks.claim",
            "Request the current Agent's NEXT pending task. Read the task board first, pass its current generation and a unique command_key. This is a delayed intent: it takes effect only after this Run succeeds, this Agent is available and dependencies complete. End the current task successfully instead of waiting. A receipt does not mean work has begun. One pending next-task request per Run; the Agent must already belong to the target's candidate pool.",
            json!({"type":"object","additionalProperties":false,"properties":{"task_key":id,"expected_generation":{"type":"integer","minimum":0},"command_key":command},"required":["task_key","expected_generation","command_key"]}),
        ),
        _ => (
            "platform.tasks.append",
            "Propose one additional task using any published local Preset. Provide a unique task key, focused instructions, existing dependency keys, Preset name/version and optional parameters. The platform always adds the current task as a dependency and creates a fresh Agent after this Run succeeds. Pending direct downstream tasks automatically wait for this follow-up unless they are its ancestors. Already dispatched tasks stay unchanged. At most 8 additions per source Run and the Goal capacity applies. Prefer useful, finite follow-up work; do not duplicate existing tasks or append work merely to continue indefinitely. A provisional receipt is not successful activation.",
            json!({"type":"object","additionalProperties":false,"properties":{"key":id,"instructions":{"type":"string","minLength":1,"maxLength":8000},"depends_on":{"type":"array","maxItems":64,"uniqueItems":true,"items":id},"preset_name":id,"preset_version":id,"parameters":{"type":"object"},"command_key":command},"required":["key","instructions","preset_name","preset_version","command_key"]}),
        ),
    };
    ToolSpec {
        name: name.into(),
        description: description.into(),
        parameters,
        mutates: kind != 0,
    }
}
pub fn package() -> Value {
    let mut p = json!({"id":"platform.tasks","version":"1.0.0","kind":"organization","description":"自主任务板、下一任务领取与追加任务","instructions":"Use platform.tasks.board to inspect the shared task graph. When your current assignment reveals necessary follow-up work, append a focused task with a published local Preset; the new Agent starts after your Run succeeds. If you are a candidate for pending work, request your next assignment and finish this Run. Appending connects the new contribution to pending downstream work, while preserving dispatched tasks and completed outputs. Do not wait for your own next-task request or misrepresent provisional requests as completed work. Keep the Goal finite and avoid duplicate tasks.","tools":[spec(0),spec(1),spec(2)]});
    p["digest"] = json!(format!(
        "sha256:{:x}",
        Sha256::digest(p.to_string().as_bytes())
    ));
    p
}
pub fn tools(store: Store, run: &str) -> Vec<Arc<dyn Tool>> {
    (0..3)
        .map(|kind| {
            Arc::new(TaskTool {
                store: store.clone(),
                run: run.into(),
                kind,
            }) as Arc<dyn Tool>
        })
        .collect()
}
struct TaskTool {
    store: Store,
    run: String,
    kind: u8,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Claim {
    task_key: String,
    expected_generation: i64,
    command_key: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Append {
    key: String,
    instructions: String,
    #[serde(default)]
    depends_on: Vec<String>,
    preset_name: String,
    preset_version: String,
    #[serde(default)]
    parameters: serde_json::Map<String, Value>,
    command_key: String,
}
#[async_trait::async_trait]
impl Tool for TaskTool {
    fn spec(&self) -> ToolSpec {
        spec(self.kind)
    }
    fn parallel_safe(&self) -> bool {
        self.kind == 0
    }
    async fn execute(
        &self,
        _: &Context,
        args: Value,
        cancel: CancellationToken,
    ) -> Result<ToolOutput> {
        if cancel.is_cancelled() {
            return Err(Error::new("cancelled"));
        }
        let content = match self.kind {
            0 => {
                if args.as_object().is_none_or(|o| !o.is_empty()) {
                    return Err(Error::new("invalid_arguments"));
                }
                self.store.task_board(&self.run)
            }
            1 => {
                let i: Claim =
                    serde_json::from_value(args).map_err(|_| Error::new("invalid_arguments"))?;
                self.store.request_next_task(
                    &self.run,
                    &NextInput {
                        task_key: i.task_key,
                        expected_generation: i.expected_generation,
                    },
                    &i.command_key,
                )
            }
            _ => {
                let i: Append =
                    serde_json::from_value(args).map_err(|_| Error::new("invalid_arguments"))?;
                self.store.request_append_task(
                    &self.run,
                    &AppendInput {
                        key: i.key,
                        instructions: i.instructions,
                        depends_on: i.depends_on,
                        preset_name: i.preset_name,
                        preset_version: i.preset_version,
                        parameters: i.parameters,
                    },
                    &i.command_key,
                )
            }
        }
        .map_err(|e| Error::new(e.1))?;
        Ok(ToolOutput {
            content,
            changed: self.kind != 0,
        })
    }
}
