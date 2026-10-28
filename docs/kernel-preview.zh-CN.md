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
2. **创建实例**：选择母体版本和数量，一次最多 64 个，且非终结实例总量不超过 64。实例拥有独立 ID、配置快照和会话。当前只支持有限批次；创建后待命，不会自动生成任务或互相协作。
3. **Agent 实例 → 检查**：查看母体/插件快照、所属批次和会话修订；就绪实例可暂停，暂停后可恢复；空闲实例可终止，历史仍保留。执行中的实例先取消运行，结算后再做生命周期动作。
4. **提交运行**：提交用户输入，Worker 从持久队列取出任务，真实调用 Harness。当前四个 Worker slot，每个实例同时最多一个已接纳运行；超出容量的实例在队列中等待。
5. **执行记录 → 检查**：查看输出、失败原因、工具参数与回执、用量、模型请求快照、完整会话和运行事件。平台 queue Run ID 与内核 Report.run_id 分开保存，详情按 Report.run_id 关联请求快照。
6. **插件与能力**：安装声明式提示词包，再发布一个绑定该版本的新母体。在线安装无需重启；旧实例不改变绑定。提示词包没有可执行入口，不能当作外部代码插件使用。
7. **实现进度**：逐项显示当前版本的已实现、部分实现与规划中能力。**事件日志**读取 SQLite 事件游标，展示控制面和执行过程中的真实变化。

内置 `text.inspect` 工具只能统计文本的 Unicode 标量数、行数和 UTF-8 字节数，不访问文件和网络。确定性演示模型会实际调用该工具，再根据回执生成固定格式答复；界面会明确标注 Demo，不能用它验证模型推理能力。真实模型由用户任务决定是否调用工具，可输入“请调用文本统计工具检查字符串 Harness³ 并按回执报告结果”来验证完整循环。

## HTTP API

当前路径以 `/api/v1` 为前缀，属于预览协议，与长期设计提案的最终 API 不保证完全相同。JSON 请求上限 128 KiB，用户输入上限 64,000 字节。需要认证的请求使用 `Authorization: Bearer <平台 token>`，不发送模型密钥。

| 方法 / 路径 | 行为 |
| --- | --- |
| `GET /health` | 无认证的最小存活检查 |
| `GET /api/v1/snapshot` | Presets、实例、最近运行、提示词插件、模型可用配置与能力清单 |
| `POST /api/v1/presets` | 发布不可变 Preset 版本；非 demo 模型必须已配置 |
| `POST /api/v1/plugins` | 安装不可变声明式提示词包 |
| `POST /api/v1/agent-fleets` | 原子创建有限批次，返回实例 ID；需要 Idempotency-Key，成功返回 201 |
| `POST /api/v1/agent-instances/{id}/actions` | `suspend` / `resume` / `terminate` |
| `POST /api/v1/agent-instances/{id}/runs` | 持久化执行请求，返回 202；需要 Idempotency-Key |
| `GET /api/v1/agent-instances/{id}/session` | 完整会话、checkpoint deltas、请求、回执 |
| `POST /api/v1/runs/{id}/cancel` | 取消排队运行或协作取消已开始运行 |
| `GET /api/v1/events?after=0` | 从指定游标增量读取最多 200 条持久事件 |

批量创建 body：

```json
{"preset_name":"demo-inspector","preset_version":"1.0.0","count":3}
```

提交运行 body：

```json
{"text":"请统计 Harness³ 的字符数"}
```

同一作用域内使用相同幂等键和内容可复用原接受回执；更改内容会返回 409。运行接受回执表示已排队，最终结果通过 snapshot 中对应 Run 查询。中断后的显式重试需要新的键，并先核对已有结果。

列表暂未提供分页：snapshot 最多返回 200 个母体/插件/实例和最近 100 条运行；Web 保留最近 500 条已读取事件，展示最近 150 条。历史仍保存在数据库中，事件可按游标补读；完整会话接口目前可能随历史增长变大，因此本版用于本地开发检查。

## 首版边界

- 已有：单机 SQLite 控制面、不可变母体和提示词绑定、独立实例/会话、有界 Worker 队列、核心工具循环、幂等与取消、原始请求/回执/事件检查、三类协议适配与 Web。
- 尚无：Agent 组织任务与协作、可靠组织消息、自动派生、维持型实例组扩缩容、TTL、外部进程/MCP 插件、代码热切换、费用账本、审批、工作区/容器沙箱、多租户与远程 Worker。
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
```

平台集成测试覆盖不可变版本、插件快照、批次幂等和配额、生命周期、取消、独立会话、SQLite stale writer、重启后的已知/未知结果、三协议映射、首条输入、工具别名、真实本地 HTTP fixture 和认证边界。测试使用本地服务与确定性模型，不依赖付费账号。真实服务 smoke test 需显式配置，不能用 fixture 通过代替供应商兼容验证。
