# 共享任务板：发布、选型、认领与拆分

任务板接纳工作要求与验收标准，发布者无需选择 Agent、Preset 或研究者数量。控制面根据待处理任务和已发布的能力目录选择 Preset，创建实例；实例启动后通过工具原子认领任务。所有任务板 Agent 使用同一套认领和拆分协议。

## 一次工作如何推进

1. 用户发布 `title`、`instructions`、`acceptance`。接纳事务保存任务与幂等回执，任务尚无执行者。
2. 控制面扫描可执行任务，根据内容和 Preset 能力选型，并在容量允许时创建独立实例。选型结果记录名称、版本和理由；启动本身不取得任务所有权。
3. 实例使用自己的 Harness 模型 / 工具循环，调用 `platform.board.read` 获取任务上下文，再调用 `platform.board.claim` 原子认领。宿主从正在执行的 Run 绑定实例身份。
4. Agent 可以直接完成，也可以调用 `platform.board.split` 提交更细的工作要求。子任务输入同样不含 Preset 或实例选择，控制面负责后续选型。
5. 拆分请求的来源 Run 成功后，子任务才进入可执行状态；来源失败、取消或结果未知时不继续激活子任务。等待子任务期间释放执行容量。
6. 子任务全部成功后，父任务重新可认领。控制面为整合工作重新选型，普通实例读取子任务结果完成整合，无需保留固定规划者或汇总者。

父子任务记录用于表达拆分来源和结果依赖。创建子任务不会给来源 Agent 增加管理其他实例的权限；实例生命周期由控制面管理。

## 发布与查询

Web 的“任务板”和 CLI 提供发布、列表、详情和取消。HTTP 使用本地平台认证，创建请求需要 `Idempotency-Key`：同键同内容重放原回执，改内容返回冲突。

| 操作 | HTTP |
| --- | --- |
| 发布任务 | `POST /api/board/tasks` |
| 查看任务板与路由信息 | `GET /api/board` |
| 查看任务详情 | `GET /api/board/tasks/{id}` |
| 取消任务 | `POST /api/board/tasks/{id}/cancel` |

CLI 共用 `--port`、`--data-dir` 配置，输出 JSON：

```sh
./target/debug/harness3 board publish --file examples/clients/board-task.json --key study-1
./target/debug/harness3 board list
./target/debug/harness3 board show TASK_ID
./target/debug/harness3 board cancel TASK_ID
```

任务板尚未添加 MCP 工具；已有 MCP 入口继续使用其已声明的 Goal 和调研操作。

请求示例见 [board-task.json](../examples/clients/board-task.json)：

```json
{
  "title": "分析本地任务板的并发约束",
  "instructions": "分析并发认领和失败恢复。必要时拆分独立子任务，最后整合结果。",
  "acceptance": "给出状态变化和失败处理，标明未验证的假设。"
}
```

详情中的任务来源、选型、实例、Run 和结果可以追溯。“已启动”与“已认领”是两个不同阶段；模型生成完成文本也不能替代有效认领和成功 Run 结算。Agent 通过 `platform.board.result` 分页读取同根任务的完整执行报告，任务板预览不能替代完整证据。

## Preset 选型模型

选型属于控制面。它接收任务与可用 Preset 目录，输出结构化的 Preset 名称、版本和理由。平台校验选择后才启动实例，选型模型不能执行任意命令或修改实例权限。

可以单独配置轻量、低延迟模型，独立于 Agent 实际执行使用的模型：

| 环境变量 | 用途 |
| --- | --- |
| `HARNESS3_ROUTER_PROTOCOL` | 显式配置时必需；`openai-compatible`、`openai-responses`、`anthropic` 或 `bailian-decision` |
| `HARNESS3_ROUTER_MODEL` | 通用协议显式配置时必需；百炼决策模型默认 `decision-model-preview` |
| `HARNESS3_ROUTER_BASE_URL` | 显式配置时必需；对应协议的服务根路径 |
| `HARNESS3_ROUTER_API_KEY` | 选型服务凭证；通用协议可省，百炼可回退到 `DASHSCOPE_API_KEY` |
| `HARNESS3_ROUTER_MIN_CONFIDENCE` | 仅百炼决策模型；0–1，默认 0，可显式调高 |

示例配置接入兼容 Chat Completions 的服务，模型名与地址按实际服务填写；凭证通过自己的密钥加载方式提供：

```sh
export HARNESS3_ROUTER_PROTOCOL=openai-compatible
export HARNESS3_ROUTER_MODEL=your-routing-model
export HARNESS3_ROUTER_BASE_URL=http://127.0.0.1:8000/v1
cargo run --locked -p harness3-cli -- up
```

未单独配置时，优先复用已配置的 Responses、OpenAI-compatible 或 Anthropic 服务。没有可用模型时采用确定性 Demo 路由，仅选择 Demo Preset。真实执行服务可用时，不把 Demo 输出作为真实任务的替代结果。显式传入 `--demo` 会强制使用本地确定性选型和执行，即使环境中已经配置真实模型或选型服务，也不调用这些服务。

候选目录只包含能够使用默认参数实例化、执行模型已配置且所需进程插件允许运行的 Preset。旧组织插件依赖 Goal 上下文，当前不进入任务板选型目录。任务板的 `read`、`claim`、`split`、`result` 工具由运行时统一注入，无需在 Preset 中额外绑定。

启动时会为已配置的执行协议发布 `board-generalist-responses`、`board-generalist-compatible` 或 `board-generalist-anthropic` 通用预设。它们支持给定材料分析、按需拆分与子任务结果整合；不固定为规划者或汇总者。发布专业 Preset 后，后续任务也可进入该类型的选型范围。选型输入包含子任务的祖先材料，避免将共享上下文中的资料误判为缺失。

单次选型最多等待 20 秒，最多并发 4 项决策。超时、无效响应或目录之外的选择会保留待路由任务，按 5 秒起步、上限 60 秒指数退避，错误记录可以查询；任务仍可取消。Rust 可减少实例装配和并发管理的开销；模型推理延迟仍取决于选型模型，具体执行还受工具和网络耗时影响。

### 百炼 decision-model-preview

百炼决策模型使用 TypeSafe System One 的 `POST /compatible-mode/v1/systemone` 接口，返回结构化选择。适配器将可用 Preset 映射为一个 `choice` 问题的选项，另加 `no_suitable_preset`，每次选出一项适合当前任务的 Preset。不同待处理任务可以选中不同 Preset。

用实际业务空间 ID 替换下面的 `{WorkspaceId}`，并在本机凭证环境中提供 `HARNESS3_ROUTER_API_KEY` 或 `DASHSCOPE_API_KEY`：

```sh
export HARNESS3_ROUTER_PROTOCOL=bailian-decision
export HARNESS3_ROUTER_BASE_URL='https://{WorkspaceId}.cn-beijing.maas.aliyuncs.com/compatible-mode/v1'
export HARNESS3_ROUTER_MODEL=decision-model-preview
export HARNESS3_ROUTER_MIN_CONFIDENCE=0
cargo run --locked -p harness3-cli -- up
```

`BASE_URL` 也接受以 `/systemone` 结尾的完整地址。该配置只负责选型，执行 Agent 仍需自己的模型服务配置和对应 Preset；配置选型密钥不会自动提供业务工具。

适配器校验 `answers.preset` 的选择、置信度和概率字段。默认阈值为 0，接受模型选出的合法候选；显式提高阈值后，低于阈值的结果保留为待路由。选择 `no_suitable_preset` 同样保留待路由。

多个 Preset 的能力接近时，模型的置信度可能分散。阈值用于调整选型策略，不承担权限授权；即使阈值为 0，候选目录、字段校验、容量限制与原子认领仍须通过。调高阈值可能让已有可用执行者的任务持续等待，应结合实际选项和决策记录调整。

成功决策的选项映射、请求 ID、概率、服务返回的延迟和输入 Token 数保存在 `board.routing_decided` 事件中，API Key 不写入事件。这些返回值用于排查一次决策，不构成端到端性能保证。

## Demo 与实际能力

`up --demo` 强制在本地使用确定性模型，覆盖发布、选型、认领、拆分和结果整合的平台路径，不调用环境中配置的收费模型。Demo 根任务拆成两个子任务，子任务结算后再启动实例认领原任务并整合。输出明确标记为演示，不构成语义研究或外部事实核验。

Harness 负责通用模型 / 工具循环、检查点、取消与回执；Preset 决定用途、指令、模型协议和绑定工具。浏览网页、读数据库或执行其他业务动作，需要相应 Preset 实际提供工具。

## 生命周期与当前边界

任务板使用本地 SQLite 事务保护认领，记录来源 Run 并收敛执行状态。Agent 可以认领同根任务中已选定相同 Preset 名称和版本的待执行工作，同一 Run 认领一项；并发竞争只允许一个成功者。

全局最多 256 个活跃实例及预留、8 个 Worker 槽；任务板最多 64 项活动根任务，每根包含根任务在内最多 64 项。每次拆分最多 8 项、深度最多 6 层，每项任务只允许拆分一次。执行结束后控制面回收对应实例，保留任务、Run、结果与选型记录。

取消停止新增派发，并处理任务范围内的待运行与活动 Run。已发生的工具副作用无法通过取消撤销；中断结果未知时不自动重试外部动作。

当前为单 daemon、本地 Worker 的任务板。已有 Goal 的频道、共享上下文、文件产物、独立验证和人工验收属于另一套流程契约，尚未自动接入任务板。`completed` 表示已知执行成功，验收标准作为任务要求提供给 Agent，不代表经过独立评审。尚无分布式租约、跨机器调度、团队费用硬预算或通用公平性保证。

兼容用例分别见[固定调研模板](research.zh-CN.md)、[Goal 候选池认领](task-claims.zh-CN.md)和[Goal 内追加与接续](autonomy.zh-CN.md)。这些用例继续保存各自的状态与接口，任务板无需依赖它们的预设角色。
