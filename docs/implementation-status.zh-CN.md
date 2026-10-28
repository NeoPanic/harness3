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
| P2 | 受限 Agent 派生 | 规划中 | 待实现 | 待实现父子谱系、深度/数量限制和子 Agent 生命周期授权。 |
| P1 | 持久队列与取消 | 已实现 | 既有测试；见预览指南边界 | 4 个本地并发槽、单实例串行、幂等提交、协作取消。 |
| P1 | SQLite 与恢复 | 部分实现 | 既有测试；见预览指南边界 | 事务、检查点、事件、daemon 锁；已知回执保留，未知结果中断。待补迁移运维、对账与事件分页。 |
| P1 | 三类模型协议 | 部分实现 | 既有测试；见预览指南边界 | Chat Completions / Responses / Anthropic 非流式工具循环；Responses 已真实联调。流式与分片中断契约待补。 |
| P1 | 内置工具与提示词包 | 已实现 | 既有测试；见预览指南边界 | text.inspect 只读工具；提示词包在线安装、摘要固定，新母体绑定后生效。 |
| P2 | 可执行插件契约与 SDK | 部分实现 | 原生测试 + Web 联调 | 已实现版本化 manifest/RPC、严格 schema 子集、握手/健康/进度/取消、错误分类、Rust 同步 SDK 和 Python 示例；配置/权限/依赖锁、多种能力与对账协议待补。 |
| P2 | 外部插件与热加载 | 部分实现 | 原生测试 + Web 联调 | 进程包在线安装、内容寻址、逐调用摘要检查、版本并存、母体快照、健康检查、禁用/取消和旧版本重新绑定已实现；显式 opt-in、清空继承环境，仍以 OS 用户权限运行。MCP loader、watch、进程池/rollout、沙箱与作用域授权待补。 |
| P2 | 显式目标与任务依赖 | 已实现 | 既有测试；见预览指南边界 | 最多 16 项任务；持久依赖图、上游输出和 Run ID 传递、取消与人工验收。 |
| P2 | 自主规划与动态委派 | 规划中 | 待实现 | 待实现 Planner、组织插件、动态派生、依赖更新和终止条件。 |
| P2 | 可靠协作消息 | 规划中 | 待实现 | 待实现 Inbox/Outbox、消息去重、投递确认、工作认领与 Attempts。 |
| P2 | 产物、验证与返工 | 规划中 | 待实现 | 现有文本输出可人工验收；版本化产物、结构化贡献、验证规则、返工及重试历史待实现。 |
| P1 | Web 执行证据检查 | 已实现 | 既有测试；见预览指南边界 | 实例、请求快照、回执、会话、事件和协作目标来自同一后端。 |
| P2 | API / CLI / MCP 入口 | 部分实现 | 原生测试 + Web 联调 | HTTP/Web、共享 loopback client、通用 CLI 与对外 MCP stdio 已落地；共 26 个工具，默认开放 13 个查询工具。进程插件执行还需 daemon 显式启用。尚无 MCP resources/Streamable HTTP、细粒度授权、并发取消和 CLI --wait。 |
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
| 受限 Agent 派生 | 尚无实现模块 | `crates/harness3-domain/src/agents/` |
| 持久队列与取消 | `crates/harness3-platform/src/store.rs`<br>`crates/harness3-platform/src/runtime.rs` | `crates/harness3-platform/src/scheduling/` |
| SQLite 与恢复 | `crates/harness3-platform/src/store.rs`<br>`crates/harness3-platform/src/schema.sql` | `crates/harness3-storage/src/` |
| 三类模型协议 | `crates/harness3-platform/src/models.rs` | `crates/harness3-models/src/` |
| 内置工具与提示词包 | `crates/harness3-platform/src/runtime.rs`<br>`crates/harness3-platform/src/store.rs` | `crates/harness3-plugins/src/catalog/` |
| 可执行插件契约与 SDK | `crates/harness3-plugin-protocol/src/lib.rs`<br>`crates/harness3-plugin-sdk/src/lib.rs`<br>`crates/harness3-plugin-sdk/tests/contract.rs`<br>`examples/plugins/text-inspector/manifest.json` | `crates/harness3-plugin-protocol/src/`<br>`crates/harness3-plugin-sdk/src/` |
| 外部插件与热加载 | `crates/harness3-platform/src/plugins/mod.rs`<br>`crates/harness3-platform/src/plugins/package.rs`<br>`crates/harness3-platform/src/plugins/process.rs`<br>`crates/harness3-platform/web/plugins.js`<br>`crates/harness3-platform/tests/process_plugins.rs` | `crates/harness3-plugins/src/loaders/`<br>`crates/harness3-plugins/src/reload/` |
| 显式目标与任务依赖 | `crates/harness3-platform/src/coordination.rs` | `crates/harness3-platform/src/collaboration/` |
| 自主规划与动态委派 | 尚无实现模块 | `plugins/organization/`<br>`crates/harness3-platform/src/collaboration/` |
| 可靠协作消息 | 尚无实现模块 | `crates/harness3-storage/src/inbox_outbox/`<br>`crates/harness3-domain/src/collaboration/` |
| 产物、验证与返工 | 尚无实现模块 | `crates/harness3-domain/src/artifacts/`<br>`plugins/verification/` |
| Web 执行证据检查 | `crates/harness3-platform/web/app.js`<br>`crates/harness3-platform/web/goals.js`<br>`crates/harness3-platform/web/fleets.js` | `web/src/features/` |
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
│   ├── plugins/
│   │   ├── mod.rs
│   │   ├── package.rs
│   │   └── process.rs
│   ├── presets/
│   │   └── mod.rs
│   ├── api.rs
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
│   ├── runtime.rs
│   ├── schema.sql
│   └── store.rs
├── tests/
│   ├── fixtures/
│   │   └── process-plugin.py
│   ├── coordination.rs
│   ├── entrances.rs
│   ├── fleets.rs
│   ├── platform.rs
│   ├── presets.rs
│   └── process_plugins.rs
├── web/
│   ├── app.css
│   ├── app.js
│   ├── fleets.js
│   ├── goals.js
│   ├── index.html
│   ├── plugins.js
│   └── presets.js
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
├── composed-preset.json
├── fleet.json
├── mcp.json
├── parameterized-fleet.json
├── process-fleet.json
└── process-preset.json
examples/plugins/
└── text-inspector/
    ├── manifest.json
    └── worker.py
```

维护方式：修改 JSON 后运行 `python3 scripts/render-progress.py`；CI 使用 `--check` 检查文档同步、当前源码目录和路径存在。验收清单仍以设计文档为准。
