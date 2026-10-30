# 能力与源码索引

本表与 Web 能力页共享 `docs/implementation-status.json`。能力状态描述已提供的功能与缺口；验证列仅适用于该行已提供的部分。平台概览见[当前平台状态](status.zh-CN.md)，分层与约束见[架构设计](managed-agents.zh-CN.md)。

| 能力 | 状态 | 验证范围 | 当前行为与限制 |
| --- | --- | --- | --- |
| Harness 执行循环 | 已实现 | 原生测试 | 模型→工具→模型循环、独立会话、观察与持久回执；运行内核可脱离平台使用。 |
| 不可变母体发布 | 已实现 | 原生测试 | 用途、目标、指令、模型协议与插件按版本锁定；保存原始发布定义、单继承来源和能力摘要。 |
| 母体继承与参数化 | 部分实现 | 原生测试 + Web 联调 | 固定版本单继承最多 16 层，参数 schema/默认值与创建时模板展开，最多 16 个插件的能力锁及冲突校验。没有主体权限收窄、预算/资源锁或在线升级。 |
| 有限批量实例化 | 已实现 | 原生测试 + Web 联调 | HTTP 以 202 接纳创建 Operation，协调器分批创建，每批最多 8 个实例；256 容量含待创建预留，幂等重试复用原回执。 |
| 实例生命周期与 TTL | 部分实现 | 原生测试 + Web 联调 | 支持暂停、恢复、终止、排空和 TTL，Agent 池支持 finite/maintain 扩缩容。没有空闲策略、stop/start、配置升级或单实例 Operation。 |
| Agent 池与异步 Operation | 部分实现 | 原生测试 + Web 联调 | Agent 池提供异步创建/扩缩容/终止、持久槽位与代际、容量预留和重启协调；没有排空期限、Operation 单独取消或远程执行配置。 |
| 受限 Agent 派生 | 部分实现 | 原生测试 + Web 联调 | Agent 按批准母体/参数派生子任务，宿主绑定父任务、Run 和实例；最多 6 层、单次 8 项、每根 48 个后代，Goal 共 64 项。没有任意子树授权、迁移或分布式租约。 |
| 持久队列与取消 | 已实现 | 原生测试 + Web 联调 | 持久 Run 队列提供 8 个本地并发槽、单实例串行、幂等提交与协作取消。 |
| SQLite 与恢复 | 部分实现 | 原生测试 | SQLite 事务、检查点、事件和 daemon 独占锁；恢复时保留已知回执，未知结果进入中断。没有完整备份运维、外部对账或事件分页。 |
| 三类模型协议 | 部分实现 | 原生测试 | 提供 Chat Completions、Responses、Anthropic 三类非流式工具循环；Responses 经过真实服务验证。没有流式输出或分片中断契约。 |
| 内置工具与提示词包 | 已实现 | 原生测试 | 提供 text.inspect 只读工具与在线提示词包安装；摘要固定，新母体显式绑定版本后生效。 |
| 可执行插件契约与 SDK | 部分实现 | 原生测试 + Web 联调 | 版本化 manifest/RPC、严格 schema 子集、握手、健康、进度、取消、错误分类、Rust SDK 与 Python 示例；示例覆盖统计、HTTP 读取和文件交接。没有配置/权限/依赖锁、多能力包或完整对账协议。 |
| 外部插件与热加载 | 部分实现 | 原生测试 + Web 联调 | 本地进程包在线安装、内容寻址、调用前摘要检查、版本并存、母体快照、健康检查及禁用/取消。需显式启用，以系统用户运行；没有 MCP loader、watch、进程池、rollout、沙箱或作用域授权。 |
| 对等 Agent 共享任务板 | 部分实现 | 原生测试 + Web 联调 | 任务发布无需绑定 Preset 或实例；控制面模型选型并创建实例，Agent 使用统一工具原子认领、拆分和读取同根结果。来源成功后激活子任务，全部完成后原任务重新选型与认领；支持取消、已知结果结算与回收。独立选型模型支持三类通用协议及百炼 decision-model-preview 的结构化选择、置信度校验和事件记录，失败退避。尚未接入 Goal 频道、文件产物与独立验证，没有分布式租约或费用硬预算。 |
| 显式目标与任务依赖 | 已实现 | 原生测试 + Web 联调 | 持久依赖图最多 64 项任务，传递上游输出与 Run ID，支持取消、候选池及人工验收；可自动选择就绪候选或等待人工认领。 |
| 固定调研模板与动态委派 | 部分实现 | 原生测试 + Web 联调 | 兼容调研模板固定规划、研究和汇总分工，支持递归委派、原实例续作与独立验证；Goal 内 Agent 可读取任务、来源成功后追加或请求接续，验证器按证据选择局部返工。该模板与对等任务板独立。没有已运行节点重写、任意子树迁移或通用自主修复。 |
| 候选池认领与执行尝试 | 部分实现 | 原生测试 + Web 联调 | 任务最多 16 位候选；认领、Run 和 Attempt 同事务，记录 generation 与幂等回执，每轮最多 8 次尝试，仅未开始可释放。platform.tasks.board/claim 提供任务板及下一任务请求；没有分布式租约、执行后对账或预算预留。 |
| 可靠协作消息 | 部分实现 | 原生测试 + Web 联调 | 持久 Outbox/Inbox、稳定消息 ID、幂等去重、投递与消费分离、暂停积压和未知结果人工处置；Agent 使用 platform.messaging 交接，四入口共用。没有远程投递或 Grant 授权。 |
| 协作频道与订阅唤醒 | 部分实现 | 原生测试 + Web 联调 | 同 Goal 成员分组、持久发言、显式订阅与消息唤醒；参与者自动进入团队讨论，订阅默认关，显式移除不补回，通知回复不再广播。没有跨机器投递、全文分页或自动冲突裁决。 |
| 共享上下文与版本化黑板 | 部分实现 | 原生测试 + Web 联调 | Goal 内发现、决策和疑问支持宿主身份、CAS 修订/撤回、来源及产物引用、派发快照与验收版本绑定；四入口可读写。没有黑板自动冲突裁决。 |
| 实例工作区与文件/JSON交接 | 部分实现 | 原生测试 + Web 联调 | 实例独立目录、UTF-8 文件读写、文件/JSON 不可变快照、同 Goal 共享读取与 Web 预览/下载；保留来源状态和摘要，成功来源可进入独立验证。没有 OS 沙箱或通用代码执行环境。 |
| 产物、验证与返工 | 部分实现 | 原生测试 + Web 联调 | 文本贡献、文件/JSON 快照保存版本、摘要与来源；当前成功任务证据进入冻结验证清单，支持有界自动返工、人工局部返工及按失败产物定位任务和下游。给定证据与抓取回执不构成外部事实认证。 |
| Web 执行证据检查 | 已实现 | 原生测试 + Web 联调 | 桌面导航按场景、协作、运行、配置分层；独立赛博工作站以卡通队员姿态展示实例状态、消息光点、任务依赖与委派和系统设备岛，详情点击展开，支持全屏、暂停与轮询同步。展示团队任务、频道、共享区、文件产物、验证返工，以及实例、Run、请求快照、回执和事件。详情、下载、候选认领与自主操作共用后端状态。 |
| API / CLI / MCP 入口 | 部分实现 | 原生测试 + Web 联调 | HTTP/Web、共享本地客户端、CLI 与 MCP stdio 共用状态；44 个 MCP 工具，默认开放 22 个查询工具。没有 MCP resources、Streamable HTTP、细粒度授权、并发取消或 CLI --wait。 |
| 实时事件与查询 | 部分实现 | 原生测试 | 持久事件游标与 Web 轮询；没有 SSE、通用列表游标或统一事件订阅契约。 |
| 预算预留与账本 | 规划中 | 待实现 | 提供单 Run 轮次、工具次数及超时限制；没有团队费用硬上限、原子预算预留、结算账本或费用归属。 |
| 身份、授权与审批 | 部分实现 | 原生测试 | loopback、本地 token 与浏览器会话认证；没有 Principal/Grant、持久审批、租户隔离或插件作用域授权。 |
| 监督 Worker 与执行隔离 | 规划中 | 待实现 | 使用嵌入式本地 Worker；没有独立进程、所有权 fencing、跨进程租约或执行沙箱。 |
| 远程控制面与 PostgreSQL | 规划中 | 待实现 | 没有多机器 Worker、网络分区对账、分布式背压或滚动升级兼容契约。 |
| 发行、生态与验收套件 | 部分实现 | 原生测试 | 提供社区文档、CI 和源码本地启动；没有完整发行物/容器交付、插件分发生态或单/多 Agent 对照评估。 |

## 当前模块 → 目标目录

| 能力 | 当前落点 | 目标目录 |
| --- | --- | --- |
| Harness 执行循环 | `crates/harness3-core/src/engine.rs`<br>`crates/harness3-core/src/session.rs` | `crates/harness3-core/src/` |
| 不可变母体发布 | `crates/harness3-platform/src/domain/mod.rs`<br>`crates/harness3-platform/src/storage/sqlite.rs`<br>`crates/harness3-platform/src/control/presets/mod.rs` | `crates/harness3-domain/src/presets/` |
| 母体继承与参数化 | `crates/harness3-platform/src/control/presets/mod.rs`<br>`web/src/features/presets/presets.js`<br>`crates/harness3-platform/tests/presets.rs` | `crates/harness3-domain/src/presets/` |
| 有限批量实例化 | `crates/harness3-platform/src/control/fleets.rs`<br>`crates/harness3-platform/src/storage/sqlite.rs` | `crates/harness3-platform/src/control/` |
| 实例生命周期与 TTL | `crates/harness3-platform/src/control/lifecycle.rs`<br>`crates/harness3-platform/src/storage/sqlite.rs` | `crates/harness3-platform/src/control/`<br>`crates/harness3-domain/src/agents/` |
| Agent 池与异步 Operation | `crates/harness3-platform/src/control/fleets.rs`<br>`web/src/features/agents/fleets.js`<br>`crates/harness3-platform/tests/fleets.rs` | `crates/harness3-domain/src/operations/`<br>`crates/harness3-platform/src/control/` |
| 受限 Agent 派生 | `crates/harness3-platform/src/collaboration/research/mod.rs`<br>`web/src/features/research/research.js`<br>`crates/harness3-platform/src/collaboration/delegation/mod.rs` | `crates/harness3-domain/src/agents/` |
| 持久队列与取消 | `crates/harness3-platform/src/storage/sqlite.rs`<br>`crates/harness3-platform/src/worker/runner.rs` | `crates/harness3-platform/src/scheduling/` |
| SQLite 与恢复 | `crates/harness3-platform/src/storage/sqlite.rs`<br>`crates/harness3-platform/src/storage/schema.sql` | `crates/harness3-storage/src/` |
| 三类模型协议 | `crates/harness3-platform/src/models/mod.rs` | `crates/harness3-models/src/` |
| 内置工具与提示词包 | `crates/harness3-platform/src/worker/runner.rs`<br>`crates/harness3-platform/src/storage/sqlite.rs` | `crates/harness3-plugins/src/catalog/` |
| 可执行插件契约与 SDK | `crates/harness3-plugin-protocol/src/lib.rs`<br>`crates/harness3-plugin-sdk/src/lib.rs`<br>`crates/harness3-plugin-sdk/tests/contract.rs`<br>`examples/plugins/text-inspector/manifest.json`<br>`examples/plugins/http-reader/manifest.json`<br>`examples/plugins/http-reader/test_worker.py`<br>`examples/plugins/workspace-preset/preset.json` | `crates/harness3-plugin-protocol/src/`<br>`crates/harness3-plugin-sdk/src/` |
| 外部插件与热加载 | `crates/harness3-platform/src/plugins/mod.rs`<br>`crates/harness3-platform/src/plugins/package.rs`<br>`crates/harness3-platform/src/plugins/process.rs`<br>`web/src/features/plugins/plugins.js`<br>`crates/harness3-platform/tests/process_plugins.rs` | `crates/harness3-plugins/src/loaders/`<br>`crates/harness3-plugins/src/reload/` |
| 对等 Agent 共享任务板 | `crates/harness3-platform/src/scheduling/board/mod.rs`<br>`crates/harness3-platform/src/scheduling/board/tools.rs`<br>`crates/harness3-platform/src/scheduling/board/schema.sql`<br>`crates/harness3-platform/src/scheduling/routing.rs`<br>`crates/harness3-platform/src/scheduling/decision.rs`<br>`web/src/features/board/board.js`<br>`crates/harness3-platform/tests/board.rs`<br>`crates/harness3-platform/tests/routing.rs`<br>`crates/harness3-platform/tests/decision.rs` | `crates/harness3-platform/src/collaboration/`<br>`crates/harness3-platform/src/scheduling/` |
| 显式目标与任务依赖 | `crates/harness3-platform/src/collaboration/coordination.rs` | `crates/harness3-platform/src/collaboration/` |
| 固定调研模板与动态委派 | `crates/harness3-platform/src/collaboration/research/mod.rs`<br>`crates/harness3-platform/src/collaboration/research/tools.rs`<br>`web/src/features/research/research.js`<br>`crates/harness3-platform/tests/research.rs`<br>`crates/harness3-platform/src/collaboration/delegation/mod.rs`<br>`crates/harness3-platform/src/collaboration/delegation/tools.rs`<br>`crates/harness3-platform/tests/delegation.rs`<br>`crates/harness3-platform/src/collaboration/autonomy/mod.rs`<br>`crates/harness3-platform/src/collaboration/autonomy/tools.rs`<br>`web/src/features/autonomy/autonomy.js`<br>`crates/harness3-platform/tests/autonomy.rs` | `plugins/organization/`<br>`crates/harness3-platform/src/collaboration/` |
| 候选池认领与执行尝试 | `crates/harness3-platform/src/collaboration/claims.rs`<br>`crates/harness3-platform/src/collaboration/coordination.rs`<br>`web/src/features/goals/tasks.js`<br>`crates/harness3-platform/tests/claims.rs`<br>`crates/harness3-platform/src/collaboration/autonomy/mod.rs` | `crates/harness3-domain/src/goals/`<br>`crates/harness3-platform/src/collaboration/` |
| 可靠协作消息 | `crates/harness3-platform/src/collaboration/messaging/mod.rs`<br>`crates/harness3-platform/src/collaboration/messaging/tools.rs`<br>`web/src/features/messages/messages.js`<br>`crates/harness3-platform/tests/messaging.rs` | `crates/harness3-storage/src/inbox_outbox/`<br>`crates/harness3-domain/src/collaboration/` |
| 协作频道与订阅唤醒 | `crates/harness3-platform/src/collaboration/channels/mod.rs`<br>`crates/harness3-platform/src/collaboration/channels/tools.rs`<br>`web/src/features/channels/channels.js`<br>`crates/harness3-platform/tests/channels.rs` | `crates/harness3-platform/src/collaboration/channels/`<br>`crates/harness3-storage/src/inbox_outbox/` |
| 共享上下文与版本化黑板 | `crates/harness3-platform/src/collaboration/context/mod.rs`<br>`crates/harness3-platform/src/collaboration/context/tools.rs`<br>`crates/harness3-platform/src/control/organization.rs`<br>`web/src/features/collaboration/context.js`<br>`crates/harness3-platform/tests/collaboration.rs` | `crates/harness3-domain/src/collaboration/`<br>`crates/harness3-platform/src/collaboration/` |
| 实例工作区与文件/JSON交接 | `crates/harness3-platform/src/workspaces/mod.rs`<br>`crates/harness3-platform/src/workspaces/tools.rs`<br>`web/src/features/workspaces/workspaces.js`<br>`crates/harness3-platform/tests/workspaces.rs` | `crates/harness3-platform/src/workspaces/`<br>`crates/harness3-domain/src/artifacts/` |
| 产物、验证与返工 | `crates/harness3-platform/src/workspaces/artifacts.rs`<br>`crates/harness3-platform/src/collaboration/coordination.rs`<br>`web/src/features/artifacts/artifacts.js`<br>`crates/harness3-platform/tests/artifacts.rs`<br>`crates/harness3-platform/src/collaboration/verification/mod.rs`<br>`crates/harness3-platform/src/collaboration/verification/tools.rs`<br>`web/src/features/goals/verification.js`<br>`crates/harness3-platform/tests/verification.rs`<br>`crates/harness3-platform/src/workspaces/mod.rs`<br>`crates/harness3-platform/src/workspaces/tools.rs`<br>`web/src/features/workspaces/workspaces.js`<br>`crates/harness3-platform/tests/workspaces.rs` | `crates/harness3-domain/src/artifacts/`<br>`plugins/verification/` |
| Web 执行证据检查 | `web/src/app/app.js`<br>`web/src/features/studio/studio.js`<br>`web/src/features/studio/studio.css`<br>`web/src/features/goals/goals.js`<br>`web/src/features/agents/fleets.js`<br>`web/src/features/messages/messages.js`<br>`web/src/features/artifacts/artifacts.js`<br>`web/src/features/collaboration/context.js`<br>`web/src/features/goals/tasks.js`<br>`web/src/features/channels/channels.js`<br>`web/src/features/workspaces/workspaces.js`<br>`web/src/features/autonomy/autonomy.js` | `web/src/features/` |
| API / CLI / MCP 入口 | `crates/harness3-platform/src/entrypoints/server/api.rs`<br>`crates/harness3-platform/src/entrypoints/client.rs`<br>`crates/harness3-platform/src/entrypoints/cli/main.rs`<br>`crates/harness3-platform/src/entrypoints/cli/cli.rs`<br>`crates/harness3-platform/src/entrypoints/mcp.rs`<br>`crates/harness3-platform/tests/entrances.rs`<br>`examples/clients/mcp.json` | `crates/harness3-client/src/`<br>`crates/harness3-cli/src/`<br>`crates/harness3-mcp/src/` |
| 实时事件与查询 | `crates/harness3-platform/src/entrypoints/server/api.rs` | `crates/harness3-server/src/events/` |
| 预算预留与账本 | 尚无实现模块 | `crates/harness3-platform/src/accounting/`<br>`crates/harness3-domain/src/budgets/` |
| 身份、授权与审批 | `crates/harness3-platform/src/entrypoints/server/api.rs` | `crates/harness3-platform/src/authorization/`<br>`crates/harness3-domain/src/identity/` |
| 监督 Worker 与执行隔离 | 尚无实现模块 | `crates/harness3-worker/src/` |
| 远程控制面与 PostgreSQL | 尚无实现模块 | `crates/harness3-storage/src/`<br>`crates/harness3-worker/src/backends/` |
| 发行、生态与验收套件 | `.github/workflows/check.yml`<br>`docs/getting-started.zh-CN.md` | `deploy/`<br>`examples/`<br>`tests/`<br>`docs/` |

## 当前源码目录

以下从实际文件生成，列出 workspace 各 crate、独立 Web 源码与客户端/插件示例，不包含构建产物。架构设计中的目标树表达职责拆分，不要求预先创建空包。

```text
crates/
├── harness3-core/
│   ├── src/
│   │   ├── context.rs
│   │   ├── driver.rs
│   │   ├── engine.rs
│   │   ├── interaction.md
│   │   ├── lib.rs
│   │   ├── policy.rs
│   │   ├── prompt.rs
│   │   ├── registry.rs
│   │   ├── session.rs
│   │   ├── skills.rs
│   │   ├── trace.rs
│   │   ├── transport.rs
│   │   └── work.rs
│   └── Cargo.toml
├── harness3-platform/
│   ├── src/
│   │   ├── collaboration/
│   │   │   ├── autonomy/
│   │   │   │   ├── mod.rs
│   │   │   │   ├── schema.sql
│   │   │   │   └── tools.rs
│   │   │   ├── channels/
│   │   │   │   ├── mod.rs
│   │   │   │   └── tools.rs
│   │   │   ├── context/
│   │   │   │   ├── mod.rs
│   │   │   │   └── tools.rs
│   │   │   ├── delegation/
│   │   │   │   ├── mod.rs
│   │   │   │   └── tools.rs
│   │   │   ├── messaging/
│   │   │   │   ├── mod.rs
│   │   │   │   └── tools.rs
│   │   │   ├── research/
│   │   │   │   ├── mod.rs
│   │   │   │   └── tools.rs
│   │   │   ├── verification/
│   │   │   │   ├── mod.rs
│   │   │   │   └── tools.rs
│   │   │   ├── claims.rs
│   │   │   ├── coordination.rs
│   │   │   └── mod.rs
│   │   ├── control/
│   │   │   ├── presets/
│   │   │   │   └── mod.rs
│   │   │   ├── fleets.rs
│   │   │   ├── lifecycle.rs
│   │   │   ├── mod.rs
│   │   │   └── organization.rs
│   │   ├── domain/
│   │   │   └── mod.rs
│   │   ├── entrypoints/
│   │   │   ├── cli/
│   │   │   │   ├── cli.rs
│   │   │   │   └── main.rs
│   │   │   ├── server/
│   │   │   │   ├── api.rs
│   │   │   │   └── mod.rs
│   │   │   ├── client.rs
│   │   │   ├── mcp.rs
│   │   │   └── mod.rs
│   │   ├── models/
│   │   │   └── mod.rs
│   │   ├── plugins/
│   │   │   ├── mod.rs
│   │   │   ├── package.rs
│   │   │   └── process.rs
│   │   ├── scheduling/
│   │   │   ├── board/
│   │   │   │   ├── mod.rs
│   │   │   │   ├── schema.sql
│   │   │   │   └── tools.rs
│   │   │   ├── decision.rs
│   │   │   ├── mod.rs
│   │   │   └── routing.rs
│   │   ├── storage/
│   │   │   ├── mod.rs
│   │   │   ├── schema.sql
│   │   │   └── sqlite.rs
│   │   ├── worker/
│   │   │   ├── mod.rs
│   │   │   └── runner.rs
│   │   ├── workspaces/
│   │   │   ├── artifacts.rs
│   │   │   ├── mod.rs
│   │   │   └── tools.rs
│   │   ├── lib.rs
│   │   └── limits.rs
│   ├── tests/
│   │   ├── fixtures/
│   │   │   └── process-plugin.py
│   │   ├── artifacts.rs
│   │   ├── autonomy.rs
│   │   ├── board.rs
│   │   ├── channels.rs
│   │   ├── claims.rs
│   │   ├── collaboration.rs
│   │   ├── coordination.rs
│   │   ├── decision.rs
│   │   ├── delegation.rs
│   │   ├── entrances.rs
│   │   ├── fleets.rs
│   │   ├── messaging.rs
│   │   ├── platform.rs
│   │   ├── presets.rs
│   │   ├── process_plugins.rs
│   │   ├── research.rs
│   │   ├── routing.rs
│   │   ├── verification.rs
│   │   └── workspaces.rs
│   └── Cargo.toml
├── harness3-plugin-protocol/
│   ├── src/
│   │   └── lib.rs
│   └── Cargo.toml
└── harness3-plugin-sdk/
    ├── examples/
    │   └── text.rs
    ├── src/
    │   └── lib.rs
    ├── tests/
    │   └── contract.rs
    └── Cargo.toml
plugins/
└── README.md
web/
├── src/
│   ├── app/
│   │   ├── app.css
│   │   └── app.js
│   └── features/
│       ├── agents/
│       │   └── fleets.js
│       ├── artifacts/
│       │   └── artifacts.js
│       ├── autonomy/
│       │   └── autonomy.js
│       ├── board/
│       │   └── board.js
│       ├── channels/
│       │   └── channels.js
│       ├── collaboration/
│       │   └── context.js
│       ├── goals/
│       │   ├── goals.js
│       │   ├── tasks.js
│       │   └── verification.js
│       ├── messages/
│       │   └── messages.js
│       ├── plugins/
│       │   └── plugins.js
│       ├── presets/
│       │   └── presets.js
│       ├── research/
│       │   └── research.js
│       ├── studio/
│       │   ├── studio.css
│       │   └── studio.js
│       └── workspaces/
│           └── workspaces.js
├── README.md
└── index.html
examples/clients/
├── README.md
├── board-task.json
├── claim.json
├── composed-preset.json
├── context.json
├── delegated-research.json
├── fleet.json
├── goal-partial-rework.json
├── mcp.json
├── parameterized-fleet.json
├── pool-goal.json
├── process-fleet.json
├── process-preset.json
├── release.json
└── research.json
examples/plugins/
├── http-reader/
│   ├── README.md
│   ├── manifest.json
│   ├── test_worker.py
│   └── worker.py
├── text-inspector/
│   ├── manifest.json
│   └── worker.py
└── workspace-preset/
    ├── README.md
    └── preset.json
```

维护方式：修改能力 JSON 或源码目录后运行 `python3 scripts/render-progress.py`；CI 使用 `--check` 检查文档同步与来源路径。
