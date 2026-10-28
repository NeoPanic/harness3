# Harness³ 本地平台使用指南

Harness³ 在本地管理 Preset、实例、任务和工具执行。默认流程从共享任务板发布工作开始：控制面动态选择 Preset 并启动实例，Agent 自行认领、执行或拆分。已有 Agent 池、单实例 Run、显式 Goal 和固定调研模板继续可用。入口共用同一个 daemon 与持久状态；运行时 crate `harness3` 也可独立使用。

## 启动与登录

要求 Rust 1.89+。在仓库根目录执行，无需 Node、外部数据库或前端开发服务器：

```sh
cargo run --locked -p harness3-platform -- up --demo
```

打开 [http://127.0.0.1:7333](http://127.0.0.1:7333)，将 `.harness3/token` 的内容粘贴到登录框。平台 token 用于本地认证，与模型 API Key 分开。Web 使用 HttpOnly / SameSite=Strict 会话 cookie；API 支持 Bearer token。

SQLite、token、插件缓存、实例文件和 daemon 独占锁保存在数据目录，默认 `.harness3/`。使用 `--data-dir` 和 `--port` 可调整目录和端口；同一数据目录只能运行一个 daemon。默认监听 loopback。

Ctrl-C 停止派发新工作，并等待已开始运行协作取消、收尾。重启保留已提交状态和已知回执，排队工作可以继续处理；中断且结果未知的运行不会自动重放或续跑。备份时应包含数据库和工作区文件，不能只复制 SQLite。

## 配置模型

平台支持 OpenAI-compatible、OpenAI Responses 和 Anthropic。以 Responses 为例，通过环境变量设置服务根路径和模型，并用自己的凭证加载方式设置 `HARNESS3_OPENAI_RESPONSES_API_KEY`：

```sh
export HARNESS3_OPENAI_RESPONSES_BASE_URL=https://api.openai.com/v1
export HARNESS3_OPENAI_RESPONSES_MODEL=gpt-6-luna
cargo run --locked -p harness3-platform -- up
```

| 协议 | 环境变量前缀 | 追加 Endpoint | 默认 BASE_URL |
| --- | --- | --- | --- |
| OpenAI-compatible | `HARNESS3_OPENAI_COMPATIBLE_` | `/chat/completions` | `https://api.openai.com/v1` |
| OpenAI Responses | `HARNESS3_OPENAI_RESPONSES_` | `/responses` | `https://api.openai.com/v1` |
| Anthropic | `HARNESS3_ANTHROPIC_` | `/messages` | `https://api.anthropic.com/v1` |

每个前缀读取 `MODEL`、`BASE_URL`、`API_KEY`。`MODEL` 必需；不要求认证的本地服务可以省略 API Key。`BASE_URL` 包含 `/v1` 等服务根路径。密钥不应放入 Preset、插件指令或 Git。

也可引用已有的 Responses 配置目录：

```sh
cargo run --locked -p harness3-platform -- up \
  --codex-config-dir /path/to/codex-config \
  --model gpt-6-luna
```

该方式读取 `config.toml` 选定 provider 的 `base_url`、`wire_api` 和 `env_key`；没有 `env_key` 时从同目录 `auth.json` 读取 `OPENAI_API_KEY`。仅接受 `wire_api = "responses"`，不修改原配置或复制密钥到数据库，不使用 OAuth token 替代 API Key。

配置在 daemon 启动时固定，切换需重启。模型或端点改变后，有历史会话的旧实例可能拒绝继续运行；创建新实例可隔离供应商 continuation。UI 的“已配置”表示配置已读取，实际调用结果需从 Run 检查。

适配器支持非流式文本、函数工具调用、回传、用量和错误状态。Responses 保留必要 output items 与 encrypted reasoning continuation。流式、多模态、供应商内置搜索和通用自动重试不在当前适配范围。

## 先发布一个任务

1. 打开“任务板”，填写标题、工作要求和验收标准并发布；无需先创建实例或选择 Preset。
2. 查看控制面的选型记录、启动实例和 Agent 认领。进入“Agent 工作现场”观察实际执行与任务关系。
3. Agent 拆分工作后，子任务进入同一任务板；控制面为它们继续选型和启动实例。子任务完成后，普通实例可重新认领原任务进行整合。

`--demo` 强制使用本地确定性选型和执行，覆盖两项子任务及整合，不执行语义调研；环境中已经配置的真实模型与选型服务也不会被调用。真实执行需要配置模型并发布适合任务的 Preset。选型可以独立使用低延迟模型，详见[共享任务板](task-board.zh-CN.md)。新任务板当前提供 Web、HTTP 和 `board` CLI；MCP 继续提供其已支持的兼容流程。

## 可选：固定调研模板

1. 打开侧栏“场景 → 自主调研”，填写目标和验收标准，选择 2–6 位研究者。
2. 无账号体验选择 `demo-team@1.0.0`；配置 Responses 后使用 `research-team@1.1.0`。平台创建规划、研究和汇总实例，规划完成后并行派发研究任务。
3. 在调研详情查看任务板、成员、来源 Run、各项贡献和汇总。Demo 会实际读取任务板、发频道消息、发布文件/JSON 快照与共享上下文；它使用确定性输出，不执行语义调研或自主追加任务。
4. 真实模型可以通过团队母体绑定的工具发布发现、共享文件、按需追加任务，或请求领取自己候选池内的下一任务。需要递归派生并由原 Agent 续作时，在创建表单启用动态委派并定义角色。
5. 可选独立验证，交付后按固定证据核查；允许自动返工时，可按失败产物定位来源任务及下游。最终验收由用户完成，也可人工要求修改指定任务及其下游。

调研能力取决于实际插件。默认团队工具提供协作与文件交接，没有网页检索工具时应只分析用户资料和模型已有知识。具体流程见[自主调研](research.zh-CN.md)。

## 对象与侧栏分层

侧栏按“场景 → 协作 → 运行 → 配置 → 开发”组织。场景用于完成用户目标；协作区检查分工与共享证据；运行区管理执行身份和生命周期；配置区定义母体及能力。

| 对象 | 用途 |
| --- | --- |
| Preset（母体） | 不可变版本，定义用途、目标、模型协议、指令、参数和插件绑定 |
| Agent 池（API/CLI 名称 Fleet） | 管理同一 Preset 配置下的实例数量、创建和回收，不代表协作分工 |
| Agent 实例 | 独立身份、配置快照和持久会话，同一时刻最多一个已接纳 Run |
| 任务板任务 | 发布时无实例绑定，控制面选型后由 Agent 认领，记录拆分来源与结果 |
| Run | 一次输入及其模型/工具循环，保存请求、回执、事件、输出和用量 |
| Goal / WorkItem / Attempt | 共同目标、依赖任务，以及任务每次认领与执行的身份记录 |
| Artifact / 工作区 | 不可变文本与文件/JSON 交付，以及实例自己的可变文件目录 |
| 频道 / 共享上下文 | 团队讨论与订阅唤醒，以及可修订、可追溯的发现与决策 |
| Operation | Agent 池创建、扩缩容等异步操作的接纳与完成状态 |

普通 Run 只执行单个 Agent。任务板按任务按需启动实例；兼容团队协作使用 Goal、调研模板或显式消息交接。批量创建 Agent 池本身不会产生任务。

## Agent 池与单实例运行

在“母体 Presets”发布固定版本，再从“Agent 池”选择母体、数量和参数：

- `finite` 创建有限批次，成员终止后不自动补足。
- `maintain` 持续维持启用槽位，成员终止后以新身份、独立会话和递增 generation 补建；暂停成员仍占槽位。
- 扩缩容要求匹配 `expected_revision`。缩容排空大编号槽位，已接纳的运行结算后回收；缩到零仍可扩容。终止整个池后不可恢复。
- TTL 从每个实例实际创建时开始，可为 1–2,592,000 秒或省略。到期取消排队 Run，并请求活动 Run 协作取消；已知成功结果仍保留。

创建返回 HTTP 202 与 `fleet_id`、`operation_id`，表示意图和容量已持久化；实例分批创建。Operation 完成表示实例创建完成，业务结果需查看 Run 或 Goal。

实例详情可暂停、恢复、终止或“排空并终止”。排空拒绝新任务、等待已接纳工作结算，不能撤回；终止后保留历史会话和回执。

```sh
./target/debug/harness3 fleets create --file examples/clients/fleet.json --key pool-create-1
./target/debug/harness3 fleets show FLEET_ID
./target/debug/harness3 operations show OPERATION_ID
./target/debug/harness3 fleets scale FLEET_ID --count 3 --expected-revision 1 --key pool-scale-1
./target/debug/harness3 runs submit AGENT_ID --file input.txt --key run-submit-1
./target/debug/harness3 runs show RUN_ID
```

## 显式任务图

“协作目标”使用已有实例定义共同目标、验收标准、任务和依赖。独立任务并行，下游收到直接依赖的输出、实例、Run 和产物身份后执行。任务按拓扑顺序提交，`depends_on` 只能引用前面已定义的 key。

候选池可提供备用实例，也可让操作者人工领取。任务运行中的 Agent 可请求后续领取或追加任务；追加只影响尚未派发的直接下游。失败、取消或结果未知的上游会阻断依赖后继，独立任务继续结算。下游上下文超过 64,000 UTF-8 字节时明确阻断，不截断证据。

全部任务成功后，有验证策略的 Goal 进入 `verifying`，否则进入 `awaiting-review`。Run 成功只证明执行成功；用户核对本轮产物、共享上下文和验收标准后提交评审，Goal 才成为 `accepted`。

取消停止后续派发、取消排队工作，并等待活动 Run 收尾，不撤销已发生的工具效果。返工最多三轮（含初始轮），可选择整图或指定任务及其下游；未受影响的成功产物保留原身份和版本。

## HTTP、CLI 与 MCP

需要认证的 HTTP 请求使用 `Authorization: Bearer <平台 token>`。新任务板入口为 `/api/board`，已有平台资源路径前缀为 `/api/v1`。创建与其他幂等写操作使用 `Idempotency-Key`：同作用域、同键、同内容返回原接纳回执，改内容返回 409。接纳后通过对象 ID 查询状态；超时重试保持原键与原内容。

CLI 从 `--data-dir` 读取平台 token，只调用 loopback HTTP，不直接写数据库。各命令支持 `--port` 和 `--data-dir`，输出 JSON：

```sh
./target/debug/harness3 snapshot
./target/debug/harness3 board publish --file examples/clients/board-task.json --key study-1
./target/debug/harness3 board list
./target/debug/harness3 board show TASK_ID
./target/debug/harness3 presets list
./target/debug/harness3 research create --file examples/clients/research.json --key research-1
./target/debug/harness3 research show RESEARCH_ID
./target/debug/harness3 goals create --file goal.json --key goal-create-1
./target/debug/harness3 goals show GOAL_ID
./target/debug/harness3 events --after 0
```

MCP stdio 连接已启动的 daemon：

```sh
./target/debug/harness3 mcp serve --transport stdio --data-dir /path/to/.harness3
# 需要写入时追加 --allow-write
```

客户端配置见 [mcp.json](../examples/clients/mcp.json)。适配器仅读取平台 token；stdout 用于逐行 JSON-RPC，诊断使用 stderr。默认只公布查询工具，`--allow-write` 授予本地操作者写权限。支持协议版本 2024-11-05、2025-03-26、2025-06-18、2025-11-25；没有 resources、Streamable HTTP、并发调用或协议级请求取消，取消 Run/Goal 使用显式工具。

HTTP JSON 请求和 CLI 文件上限为 128 KiB，用户输入上限为 64,000 UTF-8 字节。列表有返回窗口；完整 Run、Goal、Operation 通过 ID 查询，事件按游标补读。平台不自动重试写入。

## 专题指南

| 需要完成的工作 | 文档 |
| --- | --- |
| 发布任务，动态选型、认领、拆分与回投 | [共享任务板](task-board.zh-CN.md) |
| 显式使用固定规划 / 研究 / 汇总团队 | [固定调研模板](research.zh-CN.md) |
| 定义母体、继承、多个插件和实例参数 | [母体与参数](presets.zh-CN.md) |
| 安装自定义本地工具 | [进程插件](process-plugins.zh-CN.md) |
| 理解候选池、人工领取与执行尝试 | [任务认领](task-claims.zh-CN.md) |
| Goal 内 Agent 请求接续或追加工作 | [Goal 自主协作](autonomy.zh-CN.md) |
| 子 Agent 委派与父 Agent 续作 | [动态委派](dynamic-delegation.zh-CN.md) |
| 发布发现、修订及处理并发冲突 | [共享上下文](shared-context.zh-CN.md) |
| 点对点交接、消费及未知结果处置 | [协作消息](messaging.zh-CN.md) |
| 分组讨论与订阅唤醒 | [协作频道](channels.zh-CN.md) |
| 写文件、发布文件/JSON 快照并交接 | [文件工作区](workspaces.zh-CN.md) |
| 检查交付、人工验收和局部返工 | [产物与评审](artifacts.zh-CN.md) |
| 固定证据、独立验证和自动局部修复 | [验证与自动返工](verification.zh-CN.md) |
| 理解持久执行与 Temporal 的边界 | [Durable Workflow](durable-workflows.zh-CN.md) |

设计目标见[架构提案](managed-agents.zh-CN.md)，范围见[当前建设范围](status.zh-CN.md)，代码目录和能力状态见[实现清单](capabilities.zh-CN.md)。

## 本地运行边界

全局最多 256 个非终结实例及预留容量，8 个本地 Worker；每个 Goal 最多 64 项任务、最多 32 个活动或待验收 Goal。递归委派最多 6 层、一次 8 个子任务、每根 48 个后代；这些是规模限制，当前不设团队费用预算账本。

平台采用单 daemon、SQLite 与本地进程，未提供远程 Worker、多租户、任意任务图重写或操作系统沙箱。本地 token 具有整个平台访问权。进程插件以 daemon 所在 OS 用户权限执行，需显式启用 `--allow-process-plugins`；插件版本和能力绑定固定在 Preset，安装新版本不会改写已有实例。

持久接纳和幂等回执避免重复派发，不保证外部工具效果恰好一次。工具断连、超时或进程中断可能产生未知结果；平台保留证据并停止依赖传播，操作者需先核对实际效果。独立验证核查已有证据，不能自动证明外部事实。

## 开发检查

```sh
cargo check --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
cargo run --locked --example quickstart
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
```

集成测试使用本地服务与确定性模型，不要求供应商账号。真实模型连通与兼容性应使用自己的配置单独验证。
