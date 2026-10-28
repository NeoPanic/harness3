# 实现进度与目录落点

本表与 Web 实现进度共享 `docs/implementation-status.json`。状态按具体能力划分，不能据条目数量计算项目完成百分比。`待验证` 表示代码已写入，尚未完成本轮集成验证。

完整目标目录和不变量见 [设计文档](managed-agents.zh-CN.md#12-目标目录结构与模块职责)。当前有根运行内核包、平台包、插件协议包与参考 SDK 包；其余目标目录用于依赖拆分，不要求提前创建空包。

| 阶段 | 能力 | 实现状态 | 验证 | 当前行为 / 缺口 |
| --- | --- | --- | --- | --- |
| P1 | Harness 执行循环 | 已实现 | 既有测试；见预览指南边界 | 模型→工具→模型、会话与回执；领域中立内核保留在根包。 |
| P1 | 不可变母体发布 | 已实现 | 既有测试；见预览指南边界 | 用途、目标、指令、模型协议与多个插件按版本锁定；保留原始发布定义、单继承来源与能力摘要。 |
| P2 | 母体继承与参数化 | 部分实现 | 原生测试 + Web 联调 | 固定版本单继承（最多 16 层）、严格参数 schema/默认值、创建时模板展开、最多 16 个插件的能力锁与冲突校验已实现；Web 可定义与实例化。按主体的权限收窄、预算/资源锁与配置升级待补。 |
| P1 | 有限批量实例化 | 已实现 | 原生测试 + Web 联调 | HTTP 以 202 接纳创建 Operation，每轮最多创建 8 个实例；全局 64 容量包含未创建的预留槽位，幂等重试返回原回执。 |
| P1 | 实例生命周期与 TTL | 部分实现 | 原生测试 + Web 联调 | 暂停/恢复/终止、排空与 TTL；实例组支持 finite/maintain 扩缩容。空闲策略、stop/start、配置升级和单实例 Operation 尚未实现。 |
| P2 | 维持型实例组与异步 Operation | 部分实现 | 原生测试 + Web 联调 | finite/maintain、异步创建/扩缩容/终止、持久槽位与代际、容量预留、分批协调、重启续作已落地；尚无排空期限、Operation 单独取消与远程执行配置。 |
| P2 | 受限 Agent 派生 | 部分实现 | 原生测试 + Web 联调 | 自主调研的初始团队之外，Agent 可按批准母体/参数主动派生子任务；宿主固定父任务/Run/实例，支持最多 3 层、每根 12 个后代、整 Goal 16 项与自动回收。任意子树生命周期授权、迁移与分布式租约待补。 |
| P1 | 持久队列与取消 | 已实现 | 既有测试；见预览指南边界 | 4 个本地并发槽、单实例串行、幂等提交、协作取消。 |
| P1 | SQLite 与恢复 | 部分实现 | 既有测试；见预览指南边界 | 事务、检查点、事件、daemon 锁；已知回执保留，未知结果中断。待补迁移运维、对账与事件分页。 |
| P1 | 三类模型协议 | 部分实现 | 既有测试；见预览指南边界 | Chat Completions / Responses / Anthropic 非流式工具循环；Responses 已真实联调。流式与分片中断契约待补。 |
| P1 | 内置工具与提示词包 | 已实现 | 既有测试；见预览指南边界 | text.inspect 只读工具；提示词包在线安装、摘要固定，新母体绑定后生效。 |
| P2 | 可执行插件契约与 SDK | 部分实现 | 原生测试 + Web 联调 | 已实现版本化 manifest/RPC、严格 schema 子集、握手/健康/进度/取消、错误分类、Rust 同步 SDK 和 Python 示例；配置/权限/依赖锁、多种能力与对账协议待补。 |
| P2 | 外部插件与热加载 | 部分实现 | 原生测试 + Web 联调 | 进程包在线安装、内容寻址、逐调用摘要检查、版本并存、母体快照、健康检查、禁用/取消和旧版本重新绑定已实现；显式 opt-in、清空继承环境，仍以 OS 用户权限运行。MCP loader、watch、进程池/rollout、沙箱与作用域授权待补。 |
| P2 | 显式目标与任务依赖 | 已实现 | 既有测试；见预览指南边界 | 最多 16 项任务；持久依赖图、上游输出和 Run ID 传递、取消与人工验收。 支持任务候选实例池，自动选择就绪成员或等待人工认领。 |
| P2 | 自主规划与动态委派 | 部分实现 | 原生测试 + Web 联调 | 已有自主规划、并行研究/汇总、人工三轮返工；新增运行中递归委派、不同角色母体、子任务贡献后原实例新 Run 续作，成功结算后才展开，失败阻断，旧计划与 Attempt 保留。独立验证与授权整图自动返工已有实现；人工局部返工已实现；任意图修改、自主局部修复与费用预算待补。 |
| P2 | 候选池认领与执行尝试 | 部分实现 | 原生测试 + Web 联调 | 任务显式批准最多 16 位候选，自动分配或人工认领；认领/Run/Attempt 同事务，单任务 generation、幂等回执、仅未开始尝试可释放、每轮最多 8 次、跨轮历史和重启保留已实现。四入口可检查与操作；分布式租约/Worker fencing、Agent 自主认领工具、执行后对账重试和预算预留待补。 |
| P2 | 可靠协作消息 | 部分实现 | 原生测试 + Web 联调 | 本地持久 Outbox/Inbox、稳定消息 ID/幂等去重、投递/派发确认/消费分离、暂停积压、未知结果人工处置和内置 platform.messaging 发送工具已落地；四入口共用。频道、远程投递与 Grant 待补；WorkItem 候选认领与 Attempt 台账见独立能力。 |
| P2 | 共享上下文与版本化黑板 | 部分实现 | 原生测试 + Web 联调 | Goal 内发现/决策/疑问、宿主发布身份、CAS 修订/撤回、来源与产物引用、派发快照和验收版本绑定已实现，四入口可查改。默认提供共享协作母体；频道/可见性分组、订阅唤醒与自动冲突裁决待补；认领台账见独立能力。 |
| P2 | 产物、验证与返工 | 部分实现 | 原生测试 + Web 联调 | 任务文本贡献按轮次保存不可变产物、SHA-256、Run/实例来源和非空/大小检查；人工验收固定清单。有界返工（最多 3 轮）默认新团队，保留旧贡献与评审，四入口可查。独立验证冻结产物与上下文、逐项证据引用，成功 Run 后结论生效；支持最多两次授权自动返工与每轮三次重验。支持人工选定任务及下游局部返工，沿用产物保留来源；结构化/文件产物、外部事实核验与自主局部修复待补。 |
| P1 | Web 执行证据检查 | 已实现 | 既有测试；见预览指南边界 | 实例、请求快照、回执、会话、事件、协作目标和消息来自同一后端；消息详情区分投递、消费与未知结果处置。 新增协作产物、原文下载、版本/评审历史和新团队返工。 共享区支持结构化修订、历史、来源检查和并发冲突草稿保留。 任务卡片增加候选池、人工认领/排队释放和完整执行尝试台账。 目标/调研表单可批准委派角色和规模，任务卡片显示子任务、来源与续作；规划 Run 不冒充最终贡献。 |
| P2 | API / CLI / MCP 入口 | 部分实现 | 原生测试 + Web 联调 | HTTP/Web、共享 loopback client、通用 CLI 与对外 MCP stdio 已落地；共 44 个工具，默认开放 22 个查询工具。进程插件执行还需 daemon 显式启用。尚无 MCP resources/Streamable HTTP、细粒度授权、并发取消和 CLI --wait。 |
| P2 | 实时事件与查询 | 部分实现 | 既有测试；见预览指南边界 | 持久事件游标、Web 轮询；待实现 SSE、列表游标和统一查询契约。 |
| P2 | 预算预留与账本 | 规划中 | 待实现 | 当前只有单 Run 轮次/工具数/超时上限；待实现团队预算、原子预留、结算与费用归属。 |
| P3 | 身份、授权与审批 | 部分实现 | 既有测试；见预览指南边界 | 当前为 loopback + 本地 token；Principal/Grant、审批等待、租户隔离与插件权限未实现。 |
| P3 | 监督 Worker 与执行隔离 | 规划中 | 待实现 | 待实现独立进程、所有权 fencing、租约、工作区沙箱与故障注入。 |
| P4 | 远程控制面与 PostgreSQL | 规划中 | 待实现 | 待实现多机器 Worker、网络分区对账、背压和升级兼容。 |
| P4 | 发行、生态与验收套件 | 部分实现 | 既有测试；见预览指南边界 | 已有社区文档、CI 与本地启动；待补 release/容器、插件分发、参考模板和端到端评估。 |

## 当前模块 → 目标目录

| 能力 | 当前落点 | 目标目录 |
| --- | --- | --- |
| Harness 执行循环 | `src/engine.rs`<br>`src/session.rs` | `crates/harness3-core/src/` |
| 不可变母体发布 | `crates/harness3-platform/src/domain.rs`<br>`crates/harness3-platform/src/store.rs`<br>`crates/harness3-platform/src/presets/mod.rs` | `crates/harness3-domain/src/presets/` |
| 母体继承与参数化 | `crates/harness3-platform/src/presets/mod.rs`<br>`crates/harness3-platform/web/presets.js`<br>`crates/harness3-platform/tests/presets.rs` | `crates/harness3-domain/src/presets/` |
| 有限批量实例化 | `crates/harness3-platform/src/fleets.rs`<br>`crates/harness3-platform/src/store.rs` | `crates/harness3-platform/src/control/` |
| 实例生命周期与 TTL | `crates/harness3-platform/src/lifecycle.rs`<br>`crates/harness3-platform/src/store.rs` | `crates/harness3-platform/src/control/`<br>`crates/harness3-domain/src/agents/` |
| 维持型实例组与异步 Operation | `crates/harness3-platform/src/fleets.rs`<br>`crates/harness3-platform/web/fleets.js`<br>`crates/harness3-platform/tests/fleets.rs` | `crates/harness3-domain/src/operations/`<br>`crates/harness3-platform/src/control/` |
| 受限 Agent 派生 | `crates/harness3-platform/src/research/mod.rs`<br>`crates/harness3-platform/web/research.js`<br>`crates/harness3-platform/src/delegation/mod.rs` | `crates/harness3-domain/src/agents/` |
| 持久队列与取消 | `crates/harness3-platform/src/store.rs`<br>`crates/harness3-platform/src/runtime.rs` | `crates/harness3-platform/src/scheduling/` |
| SQLite 与恢复 | `crates/harness3-platform/src/store.rs`<br>`crates/harness3-platform/src/schema.sql` | `crates/harness3-storage/src/` |
| 三类模型协议 | `crates/harness3-platform/src/models.rs` | `crates/harness3-models/src/` |
| 内置工具与提示词包 | `crates/harness3-platform/src/runtime.rs`<br>`crates/harness3-platform/src/store.rs` | `crates/harness3-plugins/src/catalog/` |
| 可执行插件契约与 SDK | `crates/harness3-plugin-protocol/src/lib.rs`<br>`crates/harness3-plugin-sdk/src/lib.rs`<br>`crates/harness3-plugin-sdk/tests/contract.rs`<br>`examples/plugins/text-inspector/manifest.json` | `crates/harness3-plugin-protocol/src/`<br>`crates/harness3-plugin-sdk/src/` |
| 外部插件与热加载 | `crates/harness3-platform/src/plugins/mod.rs`<br>`crates/harness3-platform/src/plugins/package.rs`<br>`crates/harness3-platform/src/plugins/process.rs`<br>`crates/harness3-platform/web/plugins.js`<br>`crates/harness3-platform/tests/process_plugins.rs` | `crates/harness3-plugins/src/loaders/`<br>`crates/harness3-plugins/src/reload/` |
| 显式目标与任务依赖 | `crates/harness3-platform/src/coordination.rs` | `crates/harness3-platform/src/collaboration/` |
| 自主规划与动态委派 | `crates/harness3-platform/src/research/mod.rs`<br>`crates/harness3-platform/src/research/tools.rs`<br>`crates/harness3-platform/web/research.js`<br>`crates/harness3-platform/tests/research.rs`<br>`crates/harness3-platform/src/delegation/mod.rs`<br>`crates/harness3-platform/src/delegation/tools.rs`<br>`crates/harness3-platform/tests/delegation.rs` | `plugins/organization/`<br>`crates/harness3-platform/src/collaboration/` |
| 候选池认领与执行尝试 | `crates/harness3-platform/src/claims.rs`<br>`crates/harness3-platform/src/coordination.rs`<br>`crates/harness3-platform/web/tasks.js`<br>`crates/harness3-platform/tests/claims.rs` | `crates/harness3-domain/src/goals/`<br>`crates/harness3-platform/src/collaboration/` |
| 可靠协作消息 | `crates/harness3-platform/src/messaging/mod.rs`<br>`crates/harness3-platform/src/messaging/tools.rs`<br>`crates/harness3-platform/web/messages.js`<br>`crates/harness3-platform/tests/messaging.rs` | `crates/harness3-storage/src/inbox_outbox/`<br>`crates/harness3-domain/src/collaboration/` |
| 共享上下文与版本化黑板 | `crates/harness3-platform/src/collaboration/mod.rs`<br>`crates/harness3-platform/src/collaboration/tools.rs`<br>`crates/harness3-platform/src/organization.rs`<br>`crates/harness3-platform/web/collaboration.js`<br>`crates/harness3-platform/tests/collaboration.rs` | `crates/harness3-domain/src/collaboration/`<br>`crates/harness3-platform/src/collaboration/` |
| 产物、验证与返工 | `crates/harness3-platform/src/artifacts.rs`<br>`crates/harness3-platform/src/coordination.rs`<br>`crates/harness3-platform/web/artifacts.js`<br>`crates/harness3-platform/tests/artifacts.rs`<br>`crates/harness3-platform/src/verification/mod.rs`<br>`crates/harness3-platform/src/verification/tools.rs`<br>`crates/harness3-platform/web/verification.js`<br>`crates/harness3-platform/tests/verification.rs` | `crates/harness3-domain/src/artifacts/`<br>`plugins/verification/` |
| Web 执行证据检查 | `crates/harness3-platform/web/app.js`<br>`crates/harness3-platform/web/goals.js`<br>`crates/harness3-platform/web/fleets.js`<br>`crates/harness3-platform/web/messages.js`<br>`crates/harness3-platform/web/artifacts.js`<br>`crates/harness3-platform/web/collaboration.js`<br>`crates/harness3-platform/web/tasks.js` | `web/src/features/` |
| API / CLI / MCP 入口 | `crates/harness3-platform/src/api.rs`<br>`crates/harness3-platform/src/client.rs`<br>`crates/harness3-platform/src/main.rs`<br>`crates/harness3-platform/src/cli.rs`<br>`crates/harness3-platform/src/mcp.rs`<br>`crates/harness3-platform/tests/entrances.rs`<br>`examples/clients/mcp.json` | `crates/harness3-client/src/`<br>`crates/harness3-cli/src/`<br>`crates/harness3-mcp/src/` |
| 实时事件与查询 | `crates/harness3-platform/src/api.rs` | `crates/harness3-server/src/events/` |
| 预算预留与账本 | 尚无实现模块 | `crates/harness3-platform/src/accounting/`<br>`crates/harness3-domain/src/budgets/` |
| 身份、授权与审批 | `crates/harness3-platform/src/api.rs` | `crates/harness3-platform/src/authorization/`<br>`crates/harness3-domain/src/identity/` |
| 监督 Worker 与执行隔离 | 尚无实现模块 | `crates/harness3-worker/src/` |
| 远程控制面与 PostgreSQL | 尚无实现模块 | `crates/harness3-storage/src/`<br>`crates/harness3-worker/src/backends/` |
| 发行、生态与验收套件 | `.github/workflows/check.yml`<br>`docs/kernel-preview.zh-CN.md` | `deploy/`<br>`examples/`<br>`tests/`<br>`docs/` |

## 当前源码目录

以下从当前文件生成，供对照第 12 节目标树；只列运行内核、平台和客户端示例，不包含构建产物。

```text
src/
├── context.rs
├── driver.rs
├── engine.rs
├── interaction.md
├── lib.rs
├── policy.rs
├── prompt.rs
├── registry.rs
├── session.rs
├── skills.rs
├── trace.rs
├── transport.rs
└── work.rs
crates/harness3-platform/
├── src/
│   ├── collaboration/
│   │   ├── mod.rs
│   │   └── tools.rs
│   ├── delegation/
│   │   ├── mod.rs
│   │   └── tools.rs
│   ├── messaging/
│   │   ├── mod.rs
│   │   └── tools.rs
│   ├── plugins/
│   │   ├── mod.rs
│   │   ├── package.rs
│   │   └── process.rs
│   ├── presets/
│   │   └── mod.rs
│   ├── research/
│   │   ├── mod.rs
│   │   └── tools.rs
│   ├── verification/
│   │   ├── mod.rs
│   │   └── tools.rs
│   ├── api.rs
│   ├── artifacts.rs
│   ├── claims.rs
│   ├── cli.rs
│   ├── client.rs
│   ├── coordination.rs
│   ├── domain.rs
│   ├── fleets.rs
│   ├── lib.rs
│   ├── lifecycle.rs
│   ├── main.rs
│   ├── mcp.rs
│   ├── models.rs
│   ├── organization.rs
│   ├── runtime.rs
│   ├── schema.sql
│   └── store.rs
├── tests/
│   ├── fixtures/
│   │   └── process-plugin.py
│   ├── artifacts.rs
│   ├── claims.rs
│   ├── collaboration.rs
│   ├── coordination.rs
│   ├── delegation.rs
│   ├── entrances.rs
│   ├── fleets.rs
│   ├── messaging.rs
│   ├── platform.rs
│   ├── presets.rs
│   ├── process_plugins.rs
│   ├── research.rs
│   └── verification.rs
├── web/
│   ├── app.css
│   ├── app.js
│   ├── artifacts.js
│   ├── collaboration.js
│   ├── fleets.js
│   ├── goals.js
│   ├── index.html
│   ├── messages.js
│   ├── plugins.js
│   ├── presets.js
│   ├── research.js
│   ├── tasks.js
│   └── verification.js
└── Cargo.toml
crates/harness3-plugin-protocol/
├── src/
│   └── lib.rs
└── Cargo.toml
crates/harness3-plugin-sdk/
├── examples/
│   └── text.rs
├── src/
│   └── lib.rs
├── tests/
│   └── contract.rs
└── Cargo.toml
examples/clients/
├── README.md
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
└── text-inspector/
    ├── manifest.json
    └── worker.py
```

维护方式：修改 JSON 后运行 `python3 scripts/render-progress.py`；CI 使用 `--check` 检查文档同步、当前源码目录和路径存在。验收清单仍以设计文档为准。
