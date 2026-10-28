use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::time::{SystemTime, UNIX_EPOCH};

pub type PResult<T> = Result<T, Fault>;

#[derive(Debug)]
pub struct Fault(pub u16, pub String);
impl std::fmt::Display for Fault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.1)
    }
}
impl std::error::Error for Fault {}
impl From<rusqlite::Error> for Fault {
    fn from(_: rusqlite::Error) -> Self {
        Self(500, "storage_error".into())
    }
}
impl From<serde_json::Error> for Fault {
    fn from(_: serde_json::Error) -> Self {
        Self(500, "stored_data_invalid".into())
    }
}
pub fn fault(code: u16, message: &str) -> Fault {
    Fault(code, message.into())
}
pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
pub fn id(prefix: &str) -> String {
    format!("{prefix}_{}", uuid::Uuid::new_v4().simple())
}
pub fn valid_name(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 80
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Protocol {
    Demo,
    OpenaiCompatible,
    OpenaiResponses,
    Anthropic,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginRef {
    pub id: String,
    pub version: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Preset {
    pub name: String,
    pub version: String,
    pub purpose: String,
    pub objective: String,
    pub instructions: String,
    pub protocol: Protocol,
    #[serde(default)]
    pub plugin: Option<PluginRef>,
}
impl Preset {
    pub fn validate(&self) -> PResult<()> {
        if !valid_name(&self.name)
            || !valid_name(&self.version)
            || self.purpose.trim().is_empty()
            || self.objective.trim().is_empty()
            || self.instructions.trim().is_empty()
            || self.purpose.len() > 2000
            || self.objective.len() > 8000
            || self.instructions.len() > 32000
        {
            return Err(fault(400, "invalid_preset"));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bundle {
    pub id: String,
    pub version: String,
    pub description: String,
    pub instructions: String,
}
impl Bundle {
    pub fn validate(&self) -> PResult<()> {
        if !valid_name(&self.id)
            || !valid_name(&self.version)
            || self.description.trim().is_empty()
            || self.description.len() > 2000
            || self.instructions.trim().is_empty()
            || self.instructions.len() > 32000
        {
            return Err(fault(400, "invalid_prompt_bundle"));
        }
        Ok(())
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Spawn {
    pub preset_name: String,
    pub preset_version: String,
    pub count: usize,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunInput {
    pub text: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GoalPlan {
    pub title: String,
    pub objective: String,
    pub acceptance: String,
    pub tasks: Vec<TaskPlan>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskPlan {
    pub key: String,
    pub instance_id: String,
    pub instructions: String,
    #[serde(default)]
    pub depends_on: Vec<String>,
}
impl GoalPlan {
    pub fn validate(&self) -> PResult<()> {
        if self.title.trim().is_empty()
            || self.title.len() > 200
            || self.objective.trim().is_empty()
            || self.objective.len() > 8000
            || self.acceptance.trim().is_empty()
            || self.acceptance.len() > 8000
            || self.tasks.is_empty()
            || self.tasks.len() > 16
        {
            return Err(fault(400, "invalid_goal"));
        }
        let mut seen = std::collections::HashSet::new();
        for task in &self.tasks {
            let mut dependencies = std::collections::HashSet::new();
            if !valid_name(&task.key)
                || !valid_name(&task.instance_id)
                || seen.contains(&task.key)
                || task.instructions.trim().is_empty()
                || task.instructions.len() > 8000
                || task
                    .depends_on
                    .iter()
                    .any(|dep| !seen.contains(dep) || !dependencies.insert(dep))
            {
                return Err(fault(400, "invalid_task_graph"));
            }
            seen.insert(task.key.clone());
        }
        Ok(())
    }
}

pub fn capabilities() -> Value {
    json!([
        {"id":"core","name":"Harness 执行内核","status":"implemented","detail":"真实模型 → 工具 → 模型循环、会话快照、回执与上下文预算。"},
        {"id":"presets","name":"Preset 母体与版本","status":"implemented","detail":"不可变发布、指定版本批量实例化，实例独立身份与会话。尚无继承与自动派生。"},
        {"id":"lifecycle","name":"本地实例控制面","status":"partial","detail":"有限批次创建、暂停/恢复/终止、运行取消。配额 64、Worker 并发 4；尚无维持型扩缩容、TTL、多机器调度。"},
        {"id":"storage","name":"SQLite 持久化","status":"implemented","detail":"事务与 revision 检查、独占 daemon 锁、检查点、幂等命令、持久事件、启动时结算中断记录。"},
        {"id":"models","name":"三类模型协议","status":"partial","detail":"Chat Completions / Responses / Anthropic 非流式文本与工具循环。未实现多模态、流式增量和自动重试；真实账号需自行配置。"},
        {"id":"plugins","name":"插件与热加载","status":"partial","detail":"内置 text.inspect 工具，声明式提示词包可在线安装并绑定新 Preset；版本固定。外部进程/MCP 代码加载尚未实现。"},
        {"id":"inspection","name":"执行检查与 Web","status":"implemented","detail":"查看真实实例、运行、工具回执、请求快照、会话与事件；HTTP API 与同一 Web 状态源。"},
        {"id":"collaboration","name":"Agent 组织协作","status":"partial","detail":"持久共同目标、显式依赖任务图、上游贡献与 Run ID 传递、取消与人工验收；Web / API / CLI 共用状态。尚无自主拆解、Agent 通信、返工和动态委派。"},
        {"id":"governance","name":"治理与隔离","status":"planned","detail":"当前仅本地 token 与内置只读工具；多租户、费用账本、工作区沙箱、审批尚未实现。"},
        {"id":"mcp","name":"MCP 与远程 Worker","status":"planned","detail":"本轮未实现 MCP 服务、外部 MCP 工具、多机 Worker 或自动恢复原 Run。"}
    ])
}
