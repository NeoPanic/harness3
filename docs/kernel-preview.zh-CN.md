# Harness³ 本地内核平台预览

这是已实现的第一版控制面与 Web 检查台。运行时 crate `harness3` 仍可独立使用；平台位于 `crates/harness3-platform`。完整 Managed Agents 目标见[架构提案](managed-agents.zh-CN.md)。

## 启动

在仓库根目录执行，要求 Rust 1.89+，无需 Node、外部数据库或前端开发服务器：

```sh
cargo run --locked -p harness3-platform -- up --demo
```

打开 [http://127.0.0.1:7333](http://127.0.0.1:7333)，将本地 `.harness3/token` 文件内容粘贴到登录框。此 token 只用于平台认证，与模型 API Key 不同。Web 使用 HttpOnly / SameSite=Strict 会话 cookie；API 也支持 Bearer token。退出登录清除浏览器会话。

SQLite、token 和独占 daemon 锁均保存在 `.harness3/`，该目录已被 Git 忽略。可通过 `--data-dir` 和 `--port` 指定路径与端口；同一个数据目录同时只允许一个 daemon。默认只监听 loopback，不应直接作为公网或多租户服务部署。

Ctrl-C 会停止派发新工作、协作取消并等待已开始的运行收尾。重启会保留所有已提交状态，结算遗留中断记录；排队工作可继续处理。平台不会自动重放结果未知的操作，也不会自动续跑原 Run。

本机 Rust 开发可以在原生 `cargo check --workspace --all-targets --locked` 通过后使用 `cargo blaze run` / `cargo blaze test`；交付验证和 CI 使用原生 Cargo。

## 使用指定的 Responses 配置

可以引用现有 Codex 配置目录中的服务地址和 API Key，使用指定模型：

```sh
cargo run --locked -p harness3-platform -- up \
  --codex-config-dir "$CODEX_HOME" \
  --model gpt-6-luna
```

若未设置 `CODEX_HOME`，请把该参数换成你的实际配置目录。此参数读取 `config.toml` 中选定 provider 的 `base_url`、`wire_api` 和 `env_key`；无 env_key 时从该目录的 `auth.json` 读取 `OPENAI_API_KEY`。仅接受 `wire_api = "responses"`，不修改原配置，不复制密钥到平台数据库，不支持 OAuth token 代替 API Key。

配置成功后会发布 `researcher@1.0.0` 母体，使用 Responses profile；另保留无需账号的 `demo-inspector@1.0.0`。模型 ID 和端点配置在 daemon 启动时固定，切换 profile 需重启服务。已使用实例会检查历史模型身份：若模型/端点发生变化，后续运行拒绝复用旧会话并要求创建新实例，避免将供应商 continuation 发给不同模型服务；UI 的“已配置”只表示配置已读取，不表示供应商已经完成连通验证。

也可以从环境变量配置三类协议，`BASE_URL` 应包含 `/v1` 等服务根路径，平台追加对应 endpoint：

| 协议 | 环境变量前缀 | Endpoint | 默认 BASE_URL |
| --- | --- | --- | --- |
| OpenAI-compatible | `HARNESS3_OPENAI_COMPATIBLE_` | `/chat/completions` | `https://api.openai.com/v1` |
| OpenAI Responses | `HARNESS3_OPENAI_RESPONSES_` | `/responses` | `https://api.openai.com/v1` |
| Anthropic | `HARNESS3_ANTHROPIC_` | `/messages` | `https://api.anthropic.com/v1` |

每个前缀下读取 `MODEL`、`BASE_URL`、`API_KEY`。`MODEL` 必需，API_KEY 可在不要求认证的本地兼容服务中留空。使用 shell 的安全凭证加载方式设置密钥；不要将密钥写入 Preset、插件指令、Git 或示例文件。CLI 不会输出模型密钥，API 和模型配置列表不返回密钥。

目前模型适配器支持非流式文本、函数工具调用、工具回传、用量解析和明确的错误状态。Responses 保留必要的 output items / encrypted reasoning continuation；工具名在协议层生成合法稳定别名，再映射回内核名称。未实现流式增量、多模态、内置搜索工具或通用重试。供应商不支持的输出或截断响应会失败，不会被当成成功。

## 从 UI 验证功能

1. **母体 Presets**：发布名称、版本、用途、目标、指令与已配置的模型协议；修改已有版本会冲突。尚无继承、参数模板或自动派生。
2. **创建实例**：选择母体版本和数量，一次最多 64 个；非终结实例加上等待创建/补足的预留容量不超过 64。实例拥有独立 ID、配置快照和会话。支持有限批次与持续维持；创建以 Operation 异步接纳，每轮最多创建 8 个实例。创建后待命，不会自动生成任务或互相协作。
3. **Agent 实例 → 检查**：查看母体/插件快照、所属批次和会话修订；就绪实例可暂停，暂停后可恢复；空闲实例可终止，历史仍保留。可从检查面板“排空并终止”：停止接收新任务，等待已接纳的运行结算后终止。创建时可设置 TTL，过期取消排队并协作取消运行。
4. **提交运行**：提交用户输入，Worker 从持久队列取出任务，真实调用 Harness。当前四个 Worker slot，每个实例同时最多一个已接纳运行；超出容量的实例在队列中等待。
5. **执行记录 → 检查**：查看输出、失败原因、工具参数与回执、用量、模型请求快照、完整会话和运行事件。平台 queue Run ID 与内核 Report.run_id 分开保存，详情按 Report.run_id 关联请求快照。
6. **插件与能力**：安装声明式提示词包，再发布一个绑定该版本的新母体。在线安装无需重启；旧实例不改变绑定。提示词包没有可执行入口，不能当作外部代码插件使用。
7. **实现进度**：逐项显示当前版本的已实现、部分实现与规划中能力。**事件日志**读取 SQLite 事件游标，展示控制面和执行过程中的真实变化。
8. **协作目标**：定义共同目标、验收标准和任务分工；为每项任务选择实例并勾选上游依赖。独立任务并行，依赖任务收到上游输出与 Run ID 后执行。任务全部成功进入待验收，由用户核对证据并填写验收结论。

内置 `text.inspect` 工具只能统计文本的 Unicode 标量数、行数和 UTF-8 字节数，不访问文件和网络。确定性演示模型会实际调用该工具，再根据回执生成固定格式答复；界面会明确标注 Demo，不能用它验证模型推理能力。真实模型由用户任务决定是否调用工具，可输入“请调用文本统计工具检查字符串 Harness³ 并按回执报告结果”来验证完整循环。

## 显式协作任务图

控制面已经支持持久化共同目标（Goal）与任务（WorkItem）。这是一层有界的依赖协调器：用户提供分工计划，平台保证派发、结果传递和状态结算。模型自主拆解、动态委派、频道、可靠 Inbox、自动返工和独立验证器尚未实现。

每个目标最多 16 项任务，按拓扑顺序提交：`depends_on` 只能引用计划中先出现的任务 key，拒绝重复 key、重复依赖、前向引用与循环。最多 32 个运行中、正在取消或待验收的目标；目标列表展示最近 100 条。目标标题最多 200 UTF-8 字节，目标、验收标准、任务指令与验收结论分别最多 8,000 字节，HTTP body 仍受 128 KiB 限制。

- 创建计划与幂等回执在同一 SQLite 事务提交。协调器将任务与 Run 原子绑定；重启、重复协调和同键重试不会重复派发。计划发布后不可修改；重做需要创建新目标。
- 独立任务可并行；同一实例的任务与普通运行共用串行约束，最多四个 Worker 执行。计划不预占实例，忙碌或暂停时等待，终止实例会阻断未派发任务。排队中的运行仍保持实例 busy，不能直接终止。
- 下游仅接收直接依赖的输出、task key、实例 ID 与 Run ID，序列化为带证据身份的 JSON，作为本次运行输入的一部分持久保存。上游全文被标记为不可信证据；这不是对提示词注入的隔离证明。上游工具原始回执通过 Run 链接检查，不自动复制完整私有会话。
- 目标与证据合计超过 64,000 UTF-8 字节时，下游明确阻断为 `dependency_context_too_large`，不会截断证据后继续。上游失败、取消或中断未知会阻断所有依赖后继；无依赖的其他任务继续结算。
- 实例复用既有持久会话，可能影响任务输出；需要隔离目标历史时创建新实例。不同目标可以选择同一实例，按可用性串行运行；当前不承诺公平调度或专用资源。
- 任务全部 completed 后，目标进入 `awaiting-review`；completed 只表示 Harness 执行成功，不证明任务结果满足语义要求。用户核对验收标准后提交结论，目标才成为 `accepted`。当前不支持验收后修改或撤销。
- 取消目标会原子停止后续派发、取消排队 Run，并对正在执行的 Run 请求协作取消。等待已启动工作结算后目标为 cancelled；若运行在取消前已完成，保留 completed 和实际输出。取消不撤销既有结果。
- 启动恢复沿用原有 Run 契约：保留已知回执，未知结果进入 interrupted，不自动重放；待派发任务和排队任务继续，未知上游阻断下游。

示例计划 `goal.json`（将实例 ID 替换成实际创建的实例；可重复使用同一 ID 来串行验证）：

```json
{
  "title": "文本统计与证据汇总",
  "objective": "分别统计两段文本，再依据两份回执汇总",
  "acceptance": "统计值与原始回执一致，汇总引用两个上游 Run ID",
  "tasks": [
    {"key":"inspect-a","instance_id":"agent_REPLACE_A","instructions":"调用 text.inspect，仅统计 Harness³","depends_on":[]},
    {"key":"inspect-b","instance_id":"agent_REPLACE_B","instructions":"调用 text.inspect，仅统计 Agents","depends_on":[]},
    {"key":"combine","instance_id":"agent_REPLACE_C","instructions":"根据上游证据汇总统计表，引用各自完整 Run ID","depends_on":["inspect-a","inspect-b"]}
  ]
}
```

三个任务都使用 Demo 母体时，可验证真实调度、工具循环和贡献传递，但汇总任务只会统计其完整输入。要验证语义汇总，应创建三个新的 Responses 实例，使用真实模型。

### CLI 与 Web 共用状态

CLI 只连接指定端口的本地 daemon，从 `--data-dir` 读取平台 token，经同一认证 HTTP API 操作，不直接写数据库，也不接受模型密钥参数：

```sh
# 已构建二进制，也可用 cargo run --locked -p harness3-platform -- goals ...
./target/debug/harness3 goals create --file goal.json --key research-check-1
./target/debug/harness3 goals list
./target/debug/harness3 goals show goal_REPLACE
./target/debug/harness3 goals accept goal_REPLACE --review "已核对统计与来源回执"
./target/debug/harness3 goals cancel goal_REPLACE
# 可在 goals 后指定 --port 7333 --data-dir .harness3
```

创建命令要求显式 `--key`，网络失败后使用相同计划和 key 可取回原接纳回执；内容改变需要新的 key。CLI 输出 JSON，HTTP 失败时以非零状态退出。以下命令同样通过共享客户端调用 HTTP：

```sh
./target/debug/harness3 snapshot
./target/debug/harness3 presets list
./target/debug/harness3 presets publish --file preset.json
./target/debug/harness3 plugins list
./target/debug/harness3 plugins install --file bundle.json
./target/debug/harness3 fleets create --file examples/clients/fleet.json --key local-demo-1
./target/debug/harness3 fleets list
./target/debug/harness3 fleets show fleet_REPLACE
./target/debug/harness3 fleets scale fleet_REPLACE --count 3 --expected-revision 1 --key scale-1
./target/debug/harness3 fleets terminate fleet_REPLACE --expected-revision 2 --key terminate-1
./target/debug/harness3 operations show operation_REPLACE
./target/debug/harness3 instances list
./target/debug/harness3 instances action agent_REPLACE pause
./target/debug/harness3 instances session agent_REPLACE
./target/debug/harness3 runs submit agent_REPLACE --file input.txt --key input-1
./target/debug/harness3 runs show run_REPLACE
./target/debug/harness3 runs cancel run_REPLACE
./target/debug/harness3 events --after 0
```

每组命令都可指定 `--port` 和 `--data-dir`。`runs submit` 读取 UTF-8 原文，其余发布/创建命令读取 JSON；文件读取上限 128 KiB，序列化后的 HTTP 请求仍受同一上限约束，服务端额外限制用户输入为 64,000 字节。CLI 不自动重试写入，也没有 `--wait`；通过 Operation / Run / Goal ID 查询真实终态。

### 对外 MCP stdio

```sh
./target/debug/harness3 mcp serve --transport stdio --data-dir /absolute/path/to/.harness3
# 给该客户端启用本地操作者写权限时，显式追加 --allow-write
```

可复制配置见 [examples/clients/mcp.json](../examples/clients/mcp.json)，替换二进制与数据目录的绝对路径。适配器只读取本地平台 token，不读取模型密钥，不启动 daemon。连接固定 loopback，禁用代理与 HTTP 重定向。stdout 只输出逐行 JSON-RPC，诊断使用 stderr。

通过 initialize → notifications/initialized → tools/list 完成握手。支持协议版本 2024-11-05 / 2025-03-26 / 2025-06-18 / 2025-11-25；新版本返回 structuredContent 与兼容文本。工具覆盖母体、提示词包、实例组、实例、Run、Goal、Operation 和事件查询；创建、扩缩容与执行返回 HTTP 的原始接纳回执。MCP 使用同一对象 ID、幂等 key 和 revision，不直接操作数据库。

默认只公布和允许查询工具，`--allow-write` 才暴露写操作。此开关授予该进程本地操作者权限，尚无按工具/对象限制的 Grant；带真实模型的 runs/goals 可能产生调用费用。对外 MCP 与未来外部 MCP 插件 loader 属于两个独立方向。当前没有 resources、Streamable HTTP、并发调用和协议级请求取消；取消 Run/Goal 使用显式工具。HTTP 请求超时 10 秒，单条 MCP 输入限制 1 MiB。

本轮验证中，新建的三个 `gpt-6-luna` Responses 实例分别产生 `Harness³ = 8/1/9` 与 `Agents = 6/1/6` 的字符/行/字节回执，汇总任务引用了两个真实 Run ID。另一次复用历史实例的执行者统计错了对象，汇总者如实指出缺失证据，目标保留待验收。这说明协作链路可用，执行成功仍不能替代验收，也不能据此推断多 Agent 效果优于单 Agent。

## HTTP API

当前路径以 `/api/v1` 为前缀，属于预览协议，与长期设计提案的最终 API 不保证完全相同。JSON 请求上限 128 KiB，用户输入上限 64,000 字节。需要认证的请求使用 `Authorization: Bearer <平台 token>`，不发送模型密钥。

| 方法 / 路径 | 行为 |
| --- | --- |
| `GET /health` | 无认证的最小存活检查 |
| `GET /api/v1/snapshot` | Presets、实例、最近运行、提示词插件、模型可用配置与能力清单 |
| `POST /api/v1/presets` | 发布不可变 Preset 版本；非 demo 模型必须已配置 |
| `POST /api/v1/plugins` | 安装不可变声明式提示词包 |
| `POST /api/v1/agent-fleets` | 异步接纳 finite/maintain 实例组，返回 operation_id / fleet_id；需要 Idempotency-Key，成功返回 202 |
| `POST /api/v1/agent-instances/{id}/actions` | `suspend` / `resume` / `terminate` |
| `POST /api/v1/agent-instances/{id}/runs` | 持久化执行请求，返回 202；需要 Idempotency-Key |
| `GET /api/v1/agent-instances/{id}/session` | 完整会话、checkpoint deltas、请求、回执 |
| `POST /api/v1/runs/{id}/cancel` | 取消排队运行或协作取消已开始运行 |
| `GET /api/v1/runs/{id}` | 按 ID 查询完整运行，不受 snapshot 最近 100 条限制 |
| `GET /api/v1/goals` | 最近 100 条目标摘要与任务完成数 |
| `POST /api/v1/goals` | 提交 GoalPlan，要求 Idempotency-Key，接纳返回 202 |
| `GET /api/v1/goals/{id}` | 目标、验收、任务、依赖与完整关联 Run |
| `POST /api/v1/goals/{id}/cancel` | 原子停止派发并取消/排空已有工作 |
| `POST /api/v1/goals/{id}/accept` | Body `{"review":"验收结论"}`，仅待验收目标可接受 |
| `GET /api/v1/events?after=0` | 从指定游标增量读取最多 200 条持久事件 |

批量创建 body：

```json
{"preset_name":"demo-inspector","preset_version":"1.0.0","count":3,"mode":"finite","ttl_seconds":3600}
```

`ttl_seconds` 可省略或为 null；有效范围 1–2,592,000 秒（30 天），从创建时计时。截止时间与实例事务写入 SQLite，重启后继续协调；同一幂等键不会刷新截止时间。旧数据库迁移至 schema v4（包含 v3 截止时间表），旧实例保持无期限。

`POST /api/v1/agent-instances/{id}/actions` 接受 `{"action":"drain"}`：有已接纳工作时进入 `draining`，结算后终止；没有工作时立即终止。排空不可撤回，历史会话与回执保留。TTL 到期会取消尚未开始的 Run，并持久标记正在执行的 Run 取消请求；最终结果仍以真实回执为准，取消竞态中已经完成的结果不会改写为失败。

完整进度和目录落点见 [实现清单](implementation-status.zh-CN.md)，与 Web 共用 JSON 清单。

提交运行 body：

```json
{"text":"请统计 Harness³ 的字符数"}
```

同一作用域内使用相同幂等键和内容可复用原接受回执；更改内容会返回 409。运行接受回执表示已排队，最终结果通过 snapshot 中对应 Run 查询。中断后的显式重试需要新的键，并先核对已有结果。

列表暂未提供分页：snapshot 最多返回 200 个母体/插件/实例和最近 100 条运行；Web 保留最近 500 条已读取事件，展示最近 150 条。历史仍保存在数据库中，事件可按游标补读；完整会话接口目前可能随历史增长变大，因此本版用于本地开发检查。

## 首版边界

- 已有：单机 SQLite 控制面、不可变母体和提示词绑定、独立实例/会话、有界 Worker 队列、核心工具循环、幂等与取消、显式协作任务图/贡献传递/人工验收、原始请求/回执/事件检查、三类协议适配、Web、通用 CLI 和对外 MCP stdio。
- 尚无：自主任务拆解与动态委派、可靠组织消息、自动派生、自动验收/返工、外部进程/MCP 插件、代码热切换、费用账本、审批、工作区/容器沙箱、多租户与远程 Worker。
- SQLite schema 当前为 v4，首次打开 v1–v3 数据目录自动增量迁移，保留原实例、运行、会话与事件。旧版二进制会拒绝较新 schema 数据目录，回退前需恢复自行备份的旧数据库；当前没有降级工具。
- 本地 token 给予整个本地平台访问权，尚无细粒度角色授权。会话和工具结果可能含用户数据，token 与数据库不能公开分享。
- 单 daemon 独占锁与 SQLite 修订控制适用于此本机执行模式，不构成多机租约/fencing。诊断事件已持久化，但不是防篡改的合规审计系统。
- 工具侧外部幂等仍然必要。本版内置只读工具没有外部写效果；扩展可执行工具前必须实现其权限、取消与未知结果契约。

## 开发验证

```sh
cargo check --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
cargo run --locked --example quickstart
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
node --check crates/harness3-platform/web/app.js
node --check crates/harness3-platform/web/goals.js
```

平台集成测试覆盖不可变版本、插件快照、批次幂等和配额、生命周期、取消、独立会话、SQLite stale writer、重启后的已知/未知结果、三协议映射、首条输入、工具别名、真实本地 HTTP fixture 和认证边界。测试使用本地服务与确定性模型，不依赖付费账号。真实服务 smoke test 需显式配置，不能用 fixture 通过代替供应商兼容验证。

## TTL / 排空实现验证

TTL/排空阶段的原生 `cargo check --workspace --all-targets --locked`、`cargo clippy --workspace --all-targets --locked -- -D warnings` 与 35 项集成测试通过；quickstart 和 rustdoc 构建通过。新增测试覆盖排空后的拒绝与已接纳工作、TTL 幂等与配额释放、运行取消竞态、持久取消传播至 Worker、数据库重开后的过期结算。

Web 联调从 Demo 母体创建一个 60 秒 TTL 实例，确认截止时间可见、到期自动终止、历史记录保留。进度页在 1024 / 1920 px 检查了展开目录和长路径，没有发现横向内容溢出。本次没有重新调用付费模型，也未验证 Safari / Firefox 或屏幕阅读器。

## 持久实例组与异步 Operation

创建 API 返回 HTTP 202 与 `operation_id`、`fleet_id`、`desired_count`、`revision` 和查询链接。接纳回执表示意图与容量已持久化，实例在协调循环中分批就绪；客户端需查询实例组获取成员列表。原有 Rust `Store.spawn` 保留同步有限批次接口，也会注册槽位并遵守全局预留容量。

| API | 语义 |
| --- | --- |
| `GET /api/v1/agent-fleets` | 最近 200 个实例组摘要，含目标、当前、就绪、执行和排空数 |
| `GET /api/v1/agent-fleets/{id}` | 当前槽位、generation、实例身份与最近 20 次操作 |
| `POST /api/v1/agent-fleets/{id}/actions` | scale 或 terminate，要求 Idempotency-Key 和 expected_revision，成功 202 |
| `GET /api/v1/operations/{id}` | 当前操作状态和该 revision 的 target_count，不受列表上限影响 |

扩缩容请求示例：`{"action":"scale","expected_revision":1,"count":5}`。终止请求：`{"action":"terminate","expected_revision":2}`。revision 冲突返回 409；未完成操作期间拒绝再次扩缩容，终止可以将旧操作标记为 superseded。幂等重试会在 revision 校验前返回原接纳回执。

- maintain 持续维持启用槽位；成员终止后使用新身份、独立会话和递增 generation 补足。暂停成员仍占有槽位。
- finite 成员终止后不自动补足；显式扩缩容按该次请求填足启用槽位，之后继续遵循不自动补足策略。
- 缩容从大槽位号开始排空，即刻拒绝新任务，保留已经接纳的排队/运行任务，结算后退休；未结算成员仍占容量。缩到零后可再次扩容。
- 终止整个组会停止补建并排空全部成员；终态无法恢复，历史实例和回执保留。
- TTL 按每个实例实际创建时间开始计时；maintain 成员到期后会补建并重新计时。Web 默认 finite。
- 配额为所有非终结实例，加上启用槽位中等待首次创建或 maintain 补足的空缺预留。并发创建尚未就绪的组也不能绕过 64 上限。
- 每轮最多创建 8 个实例；创建 Operation 完成表示该批实例已创建，模型连通性或业务目标达成仍需独立 Run/Goal 验收。
- v1–v3 旧批次迁移成 finite 实例组和 generation 1 槽位；保留原实例、队列、会话、TTL、幂等回执，不伪造历史 Operation。

当前未实现排空超时策略、Operation 单独取消、stop/start、实例配置升级、空闲回收、远程 Worker 和跨组公平调度。

旧同步创建的幂等键在升级后仍被识别：同键同内容返回原 completed 回执，可能没有 operation_id；不同模式或内容返回 409，不会创建第二个实例组。

实例组 Web 联调记录：`fleet_563f2100fe6147e19c49c07ea6d0e89a` 从 2 成员扩至 3，终止槽位 0 的成员后补建为 generation 2，再缩容到零并整体终止。全部操作和历史身份保留在本地数据中。自动化测试另外覆盖预留容量、并发协调、缩容保留运行回执、旧数据/旧幂等键迁移及重启续作。

实例组阶段原生 check/clippy 与全工作区 50 项集成测试通过；其中 15 项覆盖实例组和 Operation。测试与本地 Web 演示不依赖付费模型。

## 共享客户端与入口验证

入口阶段新增 9 项集成测试，连同既有 50 项均通过原生 Cargo 验证；check、clippy（`-D warnings`）、rustdoc、quickstart、三个 Web 脚本语法和进度生成检查通过。测试启动真实 loopback HTTP 服务和 CLI/MCP 子进程，覆盖同一实例组及 Goal 的跨入口幂等、revision 冲突、提前验收拒绝、取消、默认只读、未知参数、路径逃逸拒绝、协议协商、逐行 framing、超大输入拒绝、EOF 退出与响应 flush。

重启本地 daemon 后，CLI 与 MCP 读取历史 `fleet_563f2100fe6147e19c49c07ea6d0e89a`，返回相同的 revision 4 终态、槽位和 Operation 历史；默认 tools/list 只返回 12 个查询工具。Web 能力页确认显示新的入口范围和当前源码 → 目标目录映射，1280 px 下无横向溢出。没有新增付费模型调用；尚未验证第三方 MCP 客户端的配置与交互，协议兼容结论限于本仓库测试及本地 stdio 实测。
