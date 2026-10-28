# Harness³ Managed Agents：目标架构与落地方案

状态：目标设计提案，包含尚未实现的功能。原始分析基线为 `e46af25`；目前已落地本地内核平台预览，以及显式协作任务图、贡献传递、人工验收和目标 CLI，实际启动方式、API 和功能边界以[平台预览指南](kernel-preview.zh-CN.md)及 Web 实现进度为准。本文的完整架构、命令、目录和验收项仍描述目标形态，不代表全部已经实现。

## 1. 产品目标与交付边界

Harness³ 的目标是让用户在本地启动一套可管理的 Agent 协作平台：先定义母体 Agent（Preset），再通过控制面 API 批量创建 N 个独立实例，由平台管理其生命周期与协作基础。用户从 API、MCP、CLI、Web 操作同一份状态，观察任务分派、工具调用、消息、产物和成本，并在中断后恢复。

每个 Agent 拥有独立的配置、会话、能力权限和工作区；平台提供共享任务上下文、可靠消息、任务认领、产物验证与整合。首次体验应能解释“哪个 Agent 因为什么在做什么、交付了什么、结果如何验证、发生故障后如何处理”。

可交付的第一版应包含运行时、管理服务、存储、Worker、模型适配器、四类入口和一个完整演示。单独的 crate、静态架构图或只有最终聊天答复的演示不足以验证这个目标。

品牌中的 N 个 harness 相乘表达可组合性。架构不预设三个 Agent 或固定角色数量；实例规模由控制面按配额、预算与 Worker 容量管理。无限衍生表达开放的组合空间，实际实例创建、派生深度、并发和存续时间都受显式资源约束。

平台采用“一切能力皆插件”的扩展范式：模型、工具、外部工具连接器、技能、提示词、渠道与策略通过统一插件描述、安装、绑定和版本管理。核心保留不可绕过的状态、权限、回执和生命周期契约。官方模型接入范围限定为 OpenAI-compatible Chat Completions、OpenAI Responses、Anthropic Messages 三类协议。

## 2. 对照当前代码的缺口

逐项实现状态、验证边界与当前模块 → 目标目录映射维护在 [实现进度](implementation-status.zh-CN.md)，与 Web 的能力页共享机器可读清单。本文的 P0–P4 和第 12 节目录为完整目标；已有代码不等于整个阶段验收通过。当前已经落地显式任务图、依赖证据传递和人工验收，仍需补自主规划、可靠消息、产物返工、外部插件和治理。

| 目标能力 | 当前依据 | 还缺什么 |
| --- | --- | --- |
| 母体与实例管理 | 平台 `Preset`、不可变发布、批量创建、持久生命周期、TTL 与排空 | 继承与参数化、维持型扩缩容、异步 Operation、谱系与受限派生 |
| 独立 Harness | 独立实例 ID/Session、`Harness`、可替换 `AgentDriver` | 独立进程、文件隔离、远程运行所有权和租约 |
| 模型与工具循环 | `models.rs` 三协议非流式适配、凭证加载、协议 fixture；Responses 真实联调 | 流式增量、分片中断契约、重试与多模态边界 |
| 插件与热加载 | 内置只读工具、提示词包在线安装、摘要与母体快照绑定 | 可执行插件协议/SDK、进程与 MCP loader、新旧版本并存、排空/回滚/撤权 |
| 会话与 checkpoint | SQLite `SessionStore`、revision 校验、持久请求与回执 | 正式迁移工具、备份运维、长历史存储与远程存储 |
| 恢复与唤醒 | daemon 独占锁、已知回执恢复、未知结果中断、TTL 协调 | 持久 Inbox、唤醒调度、Worker 监管；未知工具结果不能盲目续跑 |
| Worker 调度 | 本地持久队列、4 槽并发、单实例串行 | 多进程认领、续租、心跳、fencing、跨组公平性和背压 |
| 私有工作区 | 尚无平台 workspace 实现 | 根目录授权、产物寻址、路径逃逸防护、OS/容器沙箱 |
| Agent 间消息与频道 | 任务依赖输出附 Run ID 传递 | 组织消息、Inbox/Outbox、去重、路由、因果关联与确认 |
| 任务认领与组织上下文 | 持久 Goal/WorkItem、显式依赖、取消传播 | 自主拆解、动态委派、Attempts、共享黑板和结构化组织上下文 |
| 贡献整合与验证 | 文本贡献、失败阻断、人工验收结论 | 版本化 Artifact、结构化 Contribution、自动验证、返工与评审轮次 |
| 预算与配额 | 单 Run Limits、TokenUsage、上下文预算、64 实例配额 | 跨 Agent 原子预算预留、费用账本、速率限制与未知用量对账 |
| 运行身份与审计 | loopback/token、持久平台事件、内核 Policy | Principal/Grant、委派链、凭证范围、审批、撤权与租户隔离 |
| API / MCP / CLI / Web | 单二进制 daemon、HTTP/Web、goals CLI | 通用 client/CLI、对外 MCP、SSE/分页、发行物与容器 |
| 质量与可解释性 | CI 原生检查、持久恢复/协议/目标测试、真实 Responses 目标验收 | 多进程故障注入、权限与预算测试、单/多 Agent 基线评估、发行端到端验收 |

因此，已有部分适合继续作为执行内核；新增工作主要落在平台服务、协作协议、可靠性实现与产品入口。不要把所有管理职责塞入 `engine.rs`。

## 3. 理论基础如何落实

现有代码已经采用“观察模型输出—执行工具—返回观察”的交互循环，并具有日志、权限和恢复边界。缺少的是显式的系统模型与验证方法，暂时没有必要提出新的通用 Agent 理论。

| 基础 | 用途 | 需要写成的工程契约 |
| --- | --- | --- |
| 部分可观测决策、ReAct 式观察/行动循环 | 模型只掌握有限历史和工具观察，必须区分事实、推测与结果 | Observation、证据引用、工具回执、终止条件；不能用自述的成功代替工具证据 |
| Actor 模型 | Agent 具有独立身份、局部状态，通过消息协作 | 每个会话的有效写入者、持久 Inbox、消息 envelope、监督与唤醒；Agent 不等于常驻进程 |
| 共享黑板与组织协作协议 | 让多个 Agent 围绕共同目标发布证据与贡献 | 共享目标/任务/产物的版本、可见范围、认领规则、冲突裁决；无需共享所有私有思考历史 |
| 状态机与持久执行 | 把意图、认领、执行、回执和结算连接起来 | 转移前置条件、事务边界、幂等键、检查点、恢复路径、结果未知的处置 |
| 分布式系统的租约与 fencing | Worker 断连后可能仍在运行，旧所有者不能继续提交有效状态 | 单调 generation、存储写入校验、执行端凭证或隔离；仅有超时或心跳缺失不足以证明旧进程已停止 |
| 最小权限与能力委派 | 工具调用与 Agent 消息不能自行扩大权限 | Principal、scope、凭证引用、预算上限；子任务的有效权限不得超过委派者和目标 Agent 权限的交集 |
| 反馈控制与评估 | 让系统依据外部结果修正执行，而非只累积文字 | 明确验收标准、独立验证、有限重做、停止/升级条件；比较单 Agent 与协作系统的质量、成本和耗时 |

“收集 → 认领 → 行动 → 验证 → 整合”适合作为组织协作的逻辑阶段。阶段允许回退和并发，不能强制所有任务按固定五步选择工具。工具策略与任务分解可由模型决定，权限、状态转移、费用边界和证据校验由平台执行。

这一模型需要形成可审查规格：母体/实例对象与状态、插件与三类模型协议、消息与委派、故障与恢复、权限/预算与评估。后续再根据真实问题引入拍卖式任务分配、复杂规划或长期记忆，避免先堆积抽象。

参考入口：[ReAct](https://arxiv.org/abs/2210.03629)、[Actor Model of Computation](https://arxiv.org/abs/1008.1459)、[Lamport：Time, Clocks, and the Ordering of Events](https://lamport.azurewebsites.net/pubs/time-clocks.pdf)。这些是设计背景，不构成对 Harness³ 正确性或性能的证明。

## 4. 目标架构

```mermaid
flowchart TB
    Web[Web Console] --> HTTP[HTTP API + SSE]
    CLI[CLI] --> HTTP[HTTP API + SSE]
    SDK[API clients / SDK] --> HTTP
    MCP[MCP Server: tools / resources] --> HTTP
    HTTP --> App
    App --> Control[控制面: Preset / N个实例 / 扩缩容 / 身份 / 生命周期]
    Control --> Plugins[Plugin Manager: 安装 / 热加载 / 版本快照]
    App --> Org[组织服务: Goal / WorkItem / Inbox / Artifact / Review]
    Control --> DB[(SQLite: 状态 / checkpoints / inbox-outbox / 预算 / 事件)]
    Org --> DB
    DB --> Scheduler[Scheduler + Worker Supervisor]
    Scheduler --> WA[Worker slot A]
    Scheduler --> WB[Worker slot B]
    Scheduler --> WC[Worker slot C]
    WA --> HA[独立 Harness / Session A]
    WB --> HB[独立 Harness / Session B]
    WC --> HC[独立 Harness / Session C]
    HA --> Caps[按版本绑定的 Model / Tools / Skills / Policies]
    HB --> Caps
    HC --> Caps
    HA --> Org
    HB --> Org
    HC --> Org
    Plugins --> Caps
    Caps --> Env[插件进程 / MCP服务 / 受授权的工作区与外部系统]
    DB --> Events[持久平台事件 + SSE 投影]
    Events --> Web
```

所有入口通过同一 HTTP API 调用 Application Service，共用应用命令、权限检查、对象 ID、幂等和错误语义。CLI/MCP 不应自行创建 Registry、数据库或另一个执行循环。

控制面负责“谁可以在什么约束下运行”；组织服务负责“为了什么目标、如何协作和验收”；Harness 负责“一次执行如何与模型和工具交互并留下可信回执”。组织服务持有平台状态，Harness 通过受授权工具或服务端口访问它。

本地模式先将控制面、组织服务、调度器和嵌入式 Worker pool 放在一个守护进程中，使用 SQLite 与本地文件存储。进程内分层保留未来拆分边界。此模式只支持受信任工具，不能宣称容器级安全隔离。多进程 Worker、PostgreSQL、多机器与容器后端在相同契约上逐步增加。

## 5. 核心对象与不变量

| 对象 | 职责 |
| --- | --- |
| AgentPreset / PresetVersion | 用户定义的母体 Agent：主要功能、默认目标、指令、模型、插件、权限和资源配置；发布版本不可变，可有显式继承关系 |
| SpawnOperation / AgentFleet | 一次批量创建的进度与结果，以及持续管理 N 个同类实例的实例组；记录期望数量、当前数量和扩缩容策略 |
| AgentInstance | PresetVersion 的独立实例，含唯一 ID、实例参数、工作区、身份、生命周期与派生来源；不可变母体快照不与兄弟实例共享可变会话 |
| PluginPackage / PluginVersion / PluginBinding | 外部插件包、内容寻址的不可变版本，以及某 Preset/实例获准使用的能力和配置绑定 |
| Worker / WorkerSlot | 执行容量与所有者身份；一个 Worker 可执行多个 Agent 的不同会话，不能把 Worker ID 当作 Agent ID |
| Goal | 用户意图、验收标准、根预算、截止时间及最终状态 |
| WorkItem | 可认领的工作单元，含依赖、认领者、generation、允许的尝试与产物要求；名称区别于当前 `Request.task` 的 profile 选择键 |
| Session / Run / Attempt | 会话历史、单次输入执行、某 WorkItem 的一次尝试；调度重试不能覆盖旧 Run 结果 |
| Message / Inbox / Outbox | 消息内容、待消费状态、待可靠发送状态；统一 message_id、causation_id、correlation_id、作用域与权限 |
| Artifact / Contribution / Review | 版本化输出、某次尝试的提交、基于指定版本和标准的验收证据 |
| Principal / Grant / BudgetReservation | 谁在操作、获准做什么、执行前预留的额度；密钥本体不进入消息或日志 |
| PlatformEvent / AuditRecord | 可重放的平台事实与权限操作记录；与可丢失的调试 Trace 分开存储 |

必须保证：

1. 同一会话只有一个有效写入所有者；旧 Worker 的 generation 不能写入新所有者状态。存储 fencing 不会撤销已经发出的外部请求，外部效果还需执行端的幂等、凭证撤销或隔离。
2. WorkItem 的认领、Attempt 创建、预算预留和相关 Outbox 记录在可证明的事务边界内提交；跨存储操作通过可恢复协议衔接。
3. 消息默认至少一次投递，消费者以稳定键去重。应用命令另有作用域内幂等键与内容指纹，不把不同入口的同一请求当作不同任务。
4. 宿主身份从认证上下文取得；不能信任客户端填入 `InboundEvent.payload.context` 的 principal、权限或预算。现有 Source 适配层需要做显式映射与保留字段过滤。
5. 共享上下文保存版本化事实、证据与产物引用；Agent 私有会话不自动广播，任务数据和检索文本不获得指令权限。
6. 模型提出“完成”只会提交待验收贡献；Goal 成功需要满足验收标准的 Review，以及已持久化的整合产物。
7. 预算预留在调用前原子检查，调用后按供应商 usage 对账；缺失 usage 标记未知并保守处理。费用硬上限需要明确提供商限价、单次输出限制及进行中请求的结算边界，不能仅靠事后累计。
8. 取消停止新增工作并向子任务传播，已开始的操作进入收尾。无法证实外部结果时保留 outcome-unknown，不能把它展示为已回滚或可安全重试。

## 6. 状态与协作闭环

建议的 WorkItem 主路径：

```mermaid
stateDiagram-v2
    [*] --> queued
    queued --> claimed: 事务认领与预算预留
    claimed --> running: 所有权确认
    running --> waiting: 等待依赖/消息/审批
    waiting --> queued: 唤醒条件满足
    running --> review_pending: 提交贡献
    review_pending --> completed: 验证与整合通过
    review_pending --> queued: 有界返工
    running --> outcome_unknown: 无法确认外部效果
    running --> failed: 确定失败且不能继续
    queued --> cancelled: 取消
    waiting --> cancelled: 取消
    running --> cancelling: 停止新增动作
    cancelling --> cancelled: 已确认收尾
    cancelling --> outcome_unknown: 收尾结果不明
    claimed --> queued: 确认未开始且释放所有权
```

图展示主路径；实施前还需转移表，定义各状态的超时、取消、权限撤销与预算耗尽行为。例如 review_pending 的取消必须处理已存在的贡献，不能抹去历史。Task 状态与 Attempt/Run 状态分别保存：WorkItem 被唤醒重排队，旧 Attempt 的结算事实仍然存在。

一次典型流程：用户提交 Goal → 协调 Agent 或显式模板创建 WorkItems → 平台按能力和约束投递 → Agent 认领后读取私有会话与共享证据 → 通过工具行动 → 提交结构化 Contribution → 验证 Agent/确定性检查器验收 → 协调 Agent 整合指定版本的产物 → 平台结算预算并发布最终事件。

第一版可用明确的协调者/执行者/评审者三角色模板验证平台；用户仍能创建其他拓扑。委派必须有最大深度、最大子任务数、总预算、截止时间和明确责任者，避免循环委派和消息风暴。不要把固定角色拓扑写死进 Harness。

等待应当是持久状态：记录等待条件、释放 Worker，唤醒时生成后续 Attempt 和输入事件。当前 `Policy` 的异步等待本身不提供持久审批；保持 future 挂起会占用执行容量，进程重启后也不能自行恢复。

恢复至少区分三种情况：尚未开始的工作可重新认领；有完整回执的操作复用证据；已开始且结果未知的操作先对账或等待人工裁决。现有 `recover_interrupted` 只能作为恢复协议中的一步：宿主先证明旧运行已停止，再结算中断记录，之后显式创建续行事件。不能直接把它宣传成自动断点续跑。

## 7. 母体 Agent、批量实例化与生命周期

### 7.1 Preset 是可发布的母体定义

母体 Agent 用 `AgentPreset` 表达，是用户可维护的定义；`PresetVersion` 是校验后发布的不可变版本。定义包含主要功能、适用范围、预期目标/验收模板、系统指令、模型引用、插件依赖、工具与资源权限、默认预算以及实例参数 schema。运行中的父 Agent 另有 `parent_instance_id`，不与定义层的母体混用。

Preset 可以基于一个已发布版本衍生：发布时解析继承与参数，生成完整快照与依赖锁，保留 `extends` 来源以便比较。首版只支持单继承，禁止循环；衍生版本只能在发布者获准的权限内调整能力。升级母体不自动改写既有实例的版本。实例化默认不复制母体作者或父实例的私有会话、凭证和外部操作身份；需要种子上下文时，必须显式提供获准的数据快照及来源。

示意格式如下，配置 schema 与字段仍需在 specs 中定稿：

```yaml
api_version: harness3/v1
kind: AgentPreset
metadata:
  name: researcher
  version: 1.0.0
spec:
  purpose: 从给定资料中提取有来源的研究结论
  objective_template: 回答指定主题，所有主要结论附来源，并满足提交时的验收标准
  instructions_file: ./instructions.md
  model:
    profile: research-default  # 引用平台中按三类协议之一配置的模型
  plugins:
    - id: community.research-tools
      version: 1.2.0
      capabilities: [tools.read_document, tools.submit_contribution]
  parameters_schema:
    type: object
    properties:
      topic: {type: string}
    required: [topic]
    additionalProperties: false
  permissions:
    workspace: private
    shared_resources: [goal-evidence]
    delegation:
      allowed: false
  limits:
    max_concurrent_runs: 1
    max_output_tokens_per_call: 4096
    max_tool_calls_per_run: 32
  lifecycle:
    idle_policy: suspend
    ttl_seconds: 86400
```

默认目标是可参数化的任务意图，实例创建不会自动宣告正在处理某个 Goal。调用者可以只创建待命实例，也可以同时绑定现有 `goal_id` 和初始任务。平台将实例参数、实际目标与最终配置快照记录下来；具体目标的验收标准可以比母体模板更严格。

### 7.2 控制面对外提供 N 实例启动 API

```http
POST /v1/agent-fleets
Idempotency-Key: research-batch-001
Content-Type: application/json

{
  "preset": {"name": "researcher", "version": "1.0.0"},
  "count": 20,
  "mode": "finite",
  "parameters": {"topic": "给定资料的观点比较"},
  "goal_id": "goal-example",
  "allocation": "shared-work-queue",
  "lifecycle": {"ttl_seconds": 3600}
}
```

响应为 202，返回 `operation_id`、`fleet_id`、期望实例数、操作状态和查询链接。实例列表分页读取，创建进度通过平台事件观察；不能在一次请求中阻塞到 N 个模型调用完成。绑定同一 Goal 默认表示实例从共享任务队列认领各自工作，不代表向 20 个实例广播同一任务。

控制面先校验 Preset 与插件锁、参数、调用者权限、实例数配额和预算范围，持久化 SpawnOperation/AgentFleet 与实例槽位，再由协调循环逐步创建实例。幂等键约束请求作用域与内容指纹，`(fleet_id, slot, generation)` 唯一键避免重试时重复创建。大批量创建按有界批次推进，API 必须暴露部分就绪、失败原因和当前数量。

`desired_count` 表示用户希望保有的非终结实例数量；实际正在执行的数量还受 Worker 容量和队列限制。实例参数和生命周期覆盖项只能在母体允许的范围内收窄权限与资源，不能借创建请求扩大能力。实例创建后处于待命，不强制常驻模型连接或占用 Worker。扩容调整 desired_count；缩容先标记选中实例 drain，停止派发新工作，按可配置期限等待收尾，必要时进入取消/未知结果处理，随后退休。缩容到零仍保留实例组、历史与产物。

实例组策略区分 `maintain` 与 `finite`，创建请求显式选择，交互界面默认 finite：maintain 持续补足非终结实例数量；finite 只创建本批指定数量，完成或 TTL 到期后不补建，报告批次终态。显式终止整个实例组会停止其协调循环；单独终止 maintain 组中的实例会触发补建，必须在 API/UI 明示。实例初始化失败按有界重试处理，权限不足或配置错误不会无限补建。

### 7.3 实例生命周期由控制面协调

实例同时保存 desired state、observed state、generation 和状态原因。生命周期与单次 Run/WorkItem 状态分离，建议至少包含 provisioning、ready、busy、suspended、draining、stopped、failed、terminated；ready/busy 是 Worker 反馈的观测状态，用户不能通过 PATCH 任意填写。

- `suspend`：暂停派发，当前运行按声明策略收尾，保留工作区、会话与 Inbox；恢复后继续消费授权消息。
- `stop/start`：释放/重新建立执行资源，保留持久身份与历史；重启先处理旧运行所有权及未知结果。
- `terminate`：进入终结状态，禁止新运行；审计、回执和产物按保留策略保存，工作区清理由独立 GC 执行。
- `upgrade`：显式创建新的实例配置 revision，在可接受边界切换 Preset/插件锁；默认不继承旧供应商的 opaque continuation，必要时从可移植历史新建会话。

控制面通过 desired/observed 协调循环、Worker 心跳、租约与 generation 管理这些动作，服务重启后可继续协调。暂停、缩容、重启与终止各自有可查询的 Operation，不能只更新数据库中的显示标签。

### 7.4 运行中 Agent 的衍生

Agent 可通过获准的组织插件调用 `agents.spawn`，引用允许的 PresetVersion 并请求 N 个子实例。请求仍进入相同控制面路径，模型不能直接启动进程、修改实例数据库或获得管理员凭证。记录根 Goal、父实例、父 Operation 和谱系；总实例配额、最大派生深度、预算、TTL 与委派 scope 逐层收窄，循环引用和超额请求在创建前拒绝。

父实例终止时如何处理子实例由预先授权的 `cascade` 或 `detach` 策略决定，detach 需要新的明确责任者和预算归属。首版只做用户/API 显式创建与受限的子实例创建，不引入自动繁殖或未经授权的无限扩容。

## 8. 一切能力皆插件：契约与热加载

### 8.1 扩展面与核心边界

插件统一安装与管理，可以贡献 Model、Tool、Skill、PromptSection、Source、Transport、Policy、Observer/TraceSink，以及组织工具和验证器。存储、Worker 执行环境等基础设施后端也通过受控 Provider 接口注册，但首版不支持将正在使用的数据库热切换到另一后端。

插件声明“提供什么能力、需要什么权限”；平台决定“哪些能力可以绑定给哪些实例”。组织消息、任务认领、产物提交可以由第一方插件暴露给 Agent，底层事务、状态校验与身份检查仍由组织服务执行。策略插件可以进一步收窄权限，不能关闭平台强制授权、预算、fencing 与审计。

插件包不要求 Rust 编写。外部作者可提供声明式资源、实现版本化 RPC 的独立进程，或声明远程 MCP/HTTP 服务。官方内置 Rust 能力也经过相同 manifest、版本与绑定目录，以便用户理解统一的发现和权限模型。技能包允许只包含资源，无需可执行文件。

### 8.2 包格式与运行方式

插件 manifest 至少包含 ID、不可变版本、插件协议版本、宿主兼容范围、入口/运行方式、能力与输入输出 schema、配置 schema、权限需求、依赖、内容摘要及健康检查约定。名字以发布者命名空间限定，能力以 `plugin_id/capability` 引用，避免不同作者的工具重名。

```yaml
api_version: harness3.plugins/v1
id: community.research-tools
version: 1.2.0
host_compatibility: ">=0.3,<1.0"
runtime:
  type: process
  command: ./bin/research-tools
  protocol: harness3-plugin-rpc/v1
capabilities:
  - name: tools.read_document
    kind: tool
    schema: ./schemas/read_document.json
  - name: tools.submit_contribution
    kind: tool
    schema: ./schemas/submit_contribution.json
config_schema: ./schemas/config.json
permissions:
  workspace: read
  services: [artifacts.read, contributions.submit]
```

此处版本号用于说明格式，插件能力命名和宿主兼容范围不代表当前已有可运行实现。安装时对完整包计算 digest 并写入依赖锁；配置与密钥使用引用，密钥本体不写入插件锁或会话快照。安装包、启动插件进程和给 Agent 授权是不同操作，安装完成不自动产生实例调用权限。

| 运行方式 | 用途 | 热加载与隔离边界 |
| --- | --- | --- |
| 声明式资源包 | Presets、技能、提示词、schema | 校验并生成不可变资源快照，新绑定引用新版本 |
| 独立插件进程 | 外部工具、服务与可执行能力 | 建立新版本进程与版本化 RPC，旧调用排空后回收；仅分进程不等于沙箱 |
| 远程 MCP / HTTP | 外部工具目录与服务 | 固定发现的 schema/能力快照，连接和凭证由平台托管；远端代码不可由平台锁定，需记录服务版本或标记版本保证范围 |
| 内置 Rust Provider | 三类模型协议、存储与核心运行集成 | 随宿主二进制发布；配置和绑定可热更新，变更编译代码需要升级宿主 |

首版外部代码热加载采用独立进程/远程协议。Rust 动态库缺少可依赖的稳定 Rust ABI，运行中卸载也有 future、线程与资源生命周期风险，因此不把 `dlopen/dlclose` 作为首版插件机制。将来如引入 Wasm 或稳定 C ABI 插件，应有独立适配器和契约测试。

进程插件 RPC 至少定义握手/能力发现、调用、进度、取消、健康检查与关闭，以及 invocation_id、deadline、授权上下文、schema 版本和错误分类。每项外部能力另声明副作用类别、幂等与对账支持、并行安全性，默认按有副作用且不可安全重放处理；声明不能代替平台授权或工具自身验证。重连后的查询/对账是独立能力；RPC 中断不会自动意味着远端工具没有执行。插件使用平台下发的最小能力凭证，不能自行声称调用者身份。

### 8.3 热加载、切换与回滚

热加载表示 daemon 无需重启即可安装、激活和绑定新的外部插件版本。推荐流程为：发现文件变化/收到安装命令 → 暂存不可变包 → 校验 manifest、digest、schema 与依赖 → 启动并检查健康 → 构建候选注册快照 → 原子发布新版本 → 对获准实例显式切换 → 排空旧调用 → 回收无引用运行资源。

开发模式可 watch 插件目录并发布候选版本；发布失败保留旧快照。生产实例默认固定 PresetVersion 与插件锁，安装新版本不会自动修改全部实例。用户通过 activate/rollout 选择新实例默认版本或指定实例组升级，状态与进度由控制面暴露。

一次 Run 固定使用同一注册与 schema 快照。跨 Run 的 Session 默认保留绑定，切换前检查已加载工具定义、历史回执与供应商 continuation 的兼容性；不兼容时创建新的会话 revision/新 Session，保留旧历史。当前 Registry 是 Harness 持有的注册快照，`FileBundle` 会重新读文件；平台需要在绑定层把资源物化成不可变版本，不能让一次运行前后读到不同文件内容。

旧插件进程在最后一个调用/引用释放后退出，但插件包、schema、digest 和历史绑定按保留策略保存，以支持请求重建。回滚恢复旧版本绑定，只影响后续运行，不能撤销新版本已产生的外部效果。禁用/撤销权限属于紧急控制：先阻止新调用，再取消或隔离进行中的调用，保留已发生或未知的结果；不能等待下一次常规 rollout 才生效。

远程服务重新发现时若 schema 或服务版本与绑定不符，应阻止不兼容调用并生成候选绑定，不能静默覆盖实例使用的工具目录；保存本地快照不能保证第三方远端行为永远不变。

版本发布本身原子完成，大规模实例 rollout 可分批并有部分成功，不能宣称同时切换全部实例。平台应显示期望版本、实际版本、健康、失败原因和可回滚范围。

## 9. 模型接入范围：三类协议

官方模型接入只实现以下三类；模型 ID、base URL、密钥引用与能力配置由用户提供，不按供应商品牌重复开发适配器。

| 配置标识 | 协议 | 必需行为 |
| --- | --- | --- |
| `openai-compatible` | OpenAI-compatible Chat Completions | 消息与工具调用映射、流式输出、工具结果关联、usage/finish reason 与错误映射；兼容服务按能力探测，不默认全量等价 |
| `openai-responses` | OpenAI Responses | 输入/输出 item、工具调用与回传、流式事件、usage、continuation 及可移植历史边界；不作为 Chat Completions 的简单 URL 替换 |
| `anthropic` | Anthropic Messages | system/messages、content blocks、tool_use/tool_result、流式块与增量 JSON、usage/stop reason 与错误映射 |

统一 `Model` 端口保留三类协议的必要信息；不支持的能力显式拒绝，避免静默降级。官方适配器按插件 Provider 注册，Preset 引用平台模型 profile；实例化时解析为非秘密配置快照与密钥引用。供应商 continuation 只能由对应适配器解释，不能直接跨协议/模型重用。

共同契约覆盖工具 schema 支持范围、tool call ID、流式中断、取消、输出上限、上下文窗口、缓存用量、超时与错误分类。usage 缺失保持未知，脱敏的 provider metadata 与可重建输入分开处理。重试采用明确的分层策略，工具外部效果与结果未知不会因为模型适配器存在重试能力而被自动重放。

P1 必须交付三类协议的契约测试与可配置适配器。CI 默认用协议 fixture 覆盖成功、错误、流式分片和中断；真实服务 smoke test 由显式密钥配置启用，不让社区测试依赖付费账号。本地模型服务只要通过其中一类协议的兼容性检查即可使用，不增加第四套官方协议。

## 10. 统一入口与协议草案

应用命令至少包含 PublishPreset、SpawnFleet、ScaleFleet、TransitionInstance、InstallPlugin、ActivatePlugin、RolloutBindings、CreateGoal、SubmitMessage、CancelGoal、ResolveApproval、RetryWorkItem，以及 Get/List/Watch 查询。创建/扩缩容/插件切换都是可追踪 Operation，执行不占用一个长时间同步请求。

| 操作 | HTTP 草案 | CLI 草案 | MCP 草案 | Web |
| --- | --- | --- | --- | --- |
| 发布母体版本 | `POST /v1/presets/{name}/versions` | `presets publish -f preset.yaml` | `presets_publish` | Preset 编辑、版本比较 |
| 批量启动 N 个实例 | `POST /v1/agent-fleets` | `agents spawn --preset researcher@1.0.0 --count 20` | `agents_spawn` | 母体选择、数量、实例参数 |
| 查询实例组 | `GET /v1/agent-fleets/{id}` | `agents fleet inspect ID` | `agent_fleets_get` | 期望/当前数量、就绪进度 |
| 扩缩容 | `POST /v1/agent-fleets/{id}/scale` | `agents scale ID --count 5` | `agents_scale` | 数量调整、drain 进度 |
| 查询实例 | `GET /v1/agent-instances/{id}` | `agents inspect ID` | `agents_get` | 身份、母体版本、生命周期 |
| 实例生命周期动作 | `POST /v1/agent-instances/{id}/actions` | `agents suspend/resume/stop/start/terminate ID` | `agents_transition` | 暂停、恢复、终止与操作进度 |
| 安装/发布插件 | `POST /v1/plugins` | `plugins install ./plugin` | `plugins_install` | 来源、版本、能力、权限审查 |
| 切换插件绑定 | `POST /v1/plugin-rollouts` | `plugins rollout --fleet ID --lock bindings.lock` | `plugins_rollout` | 分批切换、排空、回滚 |
| 提交目标 | `POST /v1/goals` | `goals create -f goal.yaml` | `goals_create` | 目标输入与验收标准 |
| 查看目标 | `GET /v1/goals/{id}` | `goals inspect ID` | `goals_get` | 目标详情 |
| 观察事件 | `GET /v1/events?after=CURSOR&goal_id=ID`（SSE） | `goals watch ID` | `events_list`，可选订阅资源 | 时间线、任务图 |
| 给实例发消息 | `POST /v1/agent-instances/{id}/messages` | `agents message ID` | `agents_message` | 会话/频道页 |
| 取消目标 | `POST /v1/goals/{id}/cancel` | `goals cancel ID` | `goals_cancel` | 取消与收尾状态 |
| 读取产物 | `GET /v1/artifacts/{id}` | `artifacts show ID` | `artifacts_get` / resource | 预览、下载、版本 |

安装插件、发布母体、扩缩容与运行任务分别授权。MCP 客户端获得普通任务权限时不能顺带获得插件进程启动或平台管理权限。客户端可查询 `GET /v1/operations/{id}` 并通过持久事件追踪操作结果。

写命令返回 receipt 与对象 ID；后续查询负责读取运行结果。HTTP 对异步接受返回 202，MCP tool call 返回结构化 receipt，CLI 可通过 `--wait` 追踪；三者不能混淆“已接受”和“已完成”。错误结构统一包含 code、message、retryability、operation_id，区分确定失败和结果未知。

SSE 使用持久平台事件游标和重连补读，明确保留期；过期游标返回需要重新取快照的错误。Trace 可以附加在执行详情中，但不能驱动可靠任务状态或唤醒。

第一版 MCP Server 可以走 stdio，作为本地守护进程的 HTTP 客户端，用同一 token 和权限体系。之后再增加 Streamable HTTP 传输。平台“向外暴露 MCP 能力”与“通过插件调用外部 MCP 工具”分别由入口适配器与插件加载器负责，复用各自的权限和错误契约。

## 11. 本地启动与首次体验

下面是待实现的产品契约，不能在当前 README 中作为可执行安装步骤宣传。

```sh
# 目标：从 release 获得单个二进制后
harness3 up --demo
# 输出本地 Web 地址、数据目录、启动状态，以及浏览器登录引导
# 默认包含 SQLite、嵌入式 Worker pool、静态 Web、插件管理、模拟模型与演示母体

harness3 presets publish -f examples/presets/researcher/preset.yaml
harness3 agents spawn --preset researcher@1.0.0 --count 20
harness3 agents fleet inspect <fleet-id>
harness3 plugins install ./examples/plugins/research-tools

# 其他入口连接同一个 daemon
harness3 goals create -f examples/managed-agents/research/goal.yaml
harness3 goals watch <goal-id>
harness3 mcp serve --transport stdio
harness3 doctor
harness3 down
```

源代码启动目标为 `cargo run -p harness3-cli -- up --demo`；另提供 Docker Compose 作为可选路径。命令对应的包和镜像都需要实际构建并经 CI 验证，当前尚不存在。

`up --demo` 绑定 loopback，生成或复用本地访问凭证，重复启动不会重复创建演示 Agents；重启保留同一数据目录。Web 登录后让用户复制 API token，不把长期 token 放进 URL、命令历史或启动日志。默认不需要 Redis、PostgreSQL、Node 开发服务器、云账号或向量库。静态 Web 由发布二进制携带。

提供两种明确标识的模式：

- **确定性演示**：模型由脚本适配器驱动，但真实经过 Goal、消息、认领、Harness、工具、检查点、产物、Review 与事件链路；用于首启、回归测试和无账号体验。界面始终标注模拟模型，不能把预写答案冒充实际推理。
- **真实模型模式**：从 OpenAI-compatible、OpenAI Responses、Anthropic 三类配置中选择模型，或接入通过相应协议检查的本地模型端点；运行同一协作案例。`doctor` 检查工具调用能力、配置和网络，记录提供商兼容边界。密钥从环境变量或受保护配置读取。

首个演示建议采用离线资料研究：给定三份随仓库提供的文本，用户要求比较观点并生成有来源引用的报告。协调者划分任务，研究者提取证据，评审者检查引用并触发一次可见返工，最终整合 Markdown 报告。任务不依赖浏览器和外部写操作，结果可以用确定性规则检查。

Web 首屏必须展示可操作的完整路径：启动就绪 → 选择或编辑 Preset → 指定数量并创建实例组 → 查看 N 个实例的身份、版本和生命周期 → 提交演示目标 → 正在执行/等待/评审的时间线 → 工具参数与回执 → 证据引用和产物预览 → 用量与费用状态 → 最终验收。三角色是默认演示模板，用户可以从同一母体扩容执行者。提供插件目录、安装状态、能力授权与版本切换页面，使用户能看到新插件被指定实例采用。提供同一 goal_id 的 CLI/API/MCP 使用示例，方便用户验证四个入口操作同一目标。

演示还应包含可重复的“重启恢复”和“人工取消”操作；不要仅展示顺利完成的聊天。恢复场景先选无外部副作用的任务，并明确区分重新调度和原 Run 续行。

## 12. 目标目录结构与模块职责

以下为目标布局，不代表当前已创建。当前实际目录与目标路径的逐项映射见 [目录落点](implementation-status.zh-CN.md#当前模块--目标目录)。目录深入到职责模块与关键文件，用来指导实现；模块规模小的时候可先保留为一个 crate 内的子模块，再按依赖边界拆分。

```text
harness3/
├── Cargo.toml                         # workspace、统一依赖与 feature 管理
├── Cargo.lock
├── README.md / README.zh-CN.md
├── CONTRIBUTING.md / SECURITY.md / CODE_OF_CONDUCT.md / LICENSE
├── .github/
│   ├── workflows/                     # core/protocol/e2e、发行物构建、容器与升级检查
│   ├── ISSUE_TEMPLATE/
│   └── pull_request_template.md
├── crates/
│   ├── harness3-core/                 # Cargo 包名保留 harness3，领域中立的单 Agent 内核
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs                 # 公共 traits 与执行数据类型
│   │       ├── engine.rs / driver.rs  # 模型工具循环、可替换执行入口
│   │       ├── registry.rs            # 一次运行使用的能力注册快照
│   │       ├── session.rs / context.rs # 会话、回执、投影、上下文压缩
│   │       ├── skills.rs / prompt.rs / interaction.md
│   │       └── policy.rs / trace.rs / transport.rs / work.rs
│   ├── harness3-domain/               # 平台对象、状态机、不变量；不依赖 HTTP/SQL
│   │   └── src/
│   │       ├── presets/               # 母体、版本、继承、参数与发布规则
│   │       ├── agents/                # 实例、实例组、谱系与生命周期状态
│   │       ├── operations/            # 批量创建、扩缩容、升级等异步操作
│   │       ├── goals/                 # Goal、WorkItem、依赖、Attempt、验收状态
│   │       ├── collaboration/         # 消息、频道、委派、贡献、Review
│   │       ├── artifacts/             # 产物版本、内容摘要、证据与可见性
│   │       ├── plugins/               # 包版本、能力声明、绑定锁与 rollout 状态
│   │       ├── identity/              # Principal、Grant、scope 与凭证引用
│   │       ├── budgets/               # 额度、预留、用量结算与未知用量
│   │       └── events/                # 平台事件、审计事实、幂等键与错误分类
│   ├── harness3-platform/             # 应用服务与控制面协调，不直接实现入口协议
│   │   └── src/
│   │       ├── commands/              # 发布母体、SpawnFleet、ScaleFleet 等用例
│   │       ├── queries/               # 列表、详情、分页、快照与操作进度
│   │       ├── control/
│   │       │   ├── presets.rs / spawn.rs / lineage.rs
│   │       │   └── reconciler.rs / lifecycle.rs / rollout.rs
│   │       ├── scheduling/            # 队列、公平性、容量、租约、续租、背压
│   │       ├── collaboration/         # 分派、路由、委派、认领与整合用例
│   │       ├── wakeups/               # Inbox、依赖、定时器、审批唤醒
│   │       ├── recovery/              # 所有权确认、结果对账、显式续行
│   │       ├── authorization/         # 各入口共用的授权与能力收窄
│   │       ├── accounting/            # 配额预留、预算与费用账本服务
│   │       └── ports/                 # repository、队列、clock、插件/Worker 管理端口
│   ├── harness3-storage/              # 平台持久化适配器
│   │   ├── migrations/sqlite/         # 初始表、索引、schema 版本与升级迁移
│   │   └── src/
│   │       ├── sqlite/                # 连接、事务、仓储、revision/fencing 校验
│   │       ├── sessions/              # 当前 SessionStore 的持久实现
│   │       ├── inbox_outbox/          # 原子写入、投递状态、消费去重
│   │       ├── event_log/             # 平台事件、审计、游标与保留期
│   │       ├── blob_store/            # 本地产物/插件包，后续接对象存储
│   │       └── maintenance/           # 备份、迁移、GC、历史与包版本保留
│   ├── harness3-worker/               # Agent 运行装配、执行容量与监督
│   │   └── src/
│   │       ├── supervisor/            # 启停、心跳、失联与子进程监管
│   │       ├── runner/                # 实例快照 → Registry/Harness → Run 结算
│   │       ├── ownership/             # lease/generation、撤销与收尾
│   │       ├── workspace/             # 私有目录、路径约束、产物访问
│   │       └── backends/              # embedded 首版；process/container 后续实现
│   ├── harness3-plugin-protocol/      # 对外发布的稳定、版本化插件协议类型
│   │   └── src/
│   │       ├── manifest.rs / capabilities.rs / schemas.rs
│   │       └── rpc.rs / handshake.rs / invocation.rs / errors.rs
│   ├── harness3-plugin-sdk/           # Rust 插件作者 SDK，依赖协议而非内部存储
│   │   └── src/
│   │       ├── authoring/             # 能力声明、配置与工具实现 helper
│   │       ├── runtime/               # stdio RPC 服务、进度、取消与健康检查
│   │       └── testing/               # 插件契约测试夹具与测试宿主
│   ├── harness3-plugins/              # 宿主侧插件管理器
│   │   └── src/
│   │       ├── catalog/               # 已安装版本、来源、digest、依赖锁
│   │       ├── installation/          # 包校验、暂存、依赖解析、发布
│   │       ├── loaders/               # builtin / resources / process / mcp / http
│   │       ├── bindings/              # 配置、授权、Registry 快照、会话兼容检查
│   │       ├── reload/                # watch、候选验证、原子激活、排空、回滚
│   │       └── supervision/           # 插件健康、进程池、关闭、对账与引用回收
│   ├── harness3-models/               # 官方模型 Provider；只实现三类协议
│   │   └── src/
│   │       ├── openai_compatible/     # Chat Completions 请求/响应与 SSE 映射
│   │       ├── openai_responses/      # Responses items、工具回传与 continuation
│   │       ├── anthropic/             # Messages blocks、tool_use/result 与流式事件
│   │       └── common/                # HTTP、密钥引用、能力配置、用量/错误映射
│   ├── harness3-server/               # HTTP + SSE + Web 静态资源宿主
│   │   └── src/
│   │       ├── bootstrap/             # 配置、存储、插件、Worker 与服务装配
│   │       ├── api/v1/                # presets / fleets / instances / operations
│   │       │                          # plugins / goals / messages / artifacts
│   │       ├── auth/                  # token/会话校验与 Principal 映射
│   │       ├── events/                # SSE、游标、断线补读与快照接口
│   │       └── web/                   # 嵌入资源、路由回退、首启登录引导
│   ├── harness3-client/               # CLI/MCP 共用的类型化 HTTP 客户端
│   │   └── src/
│   │       ├── commands.rs / queries.rs / auth.rs
│   │       └── events.rs / pagination.rs / errors.rs
│   ├── harness3-cli/                  # 发布二进制 harness3
│   │   └── src/
│   │       ├── main.rs / config.rs
│   │       ├── local/                 # up/down、doctor、数据目录、服务发现
│   │       ├── commands/              # presets / agents / plugins / goals / artifacts
│   │       └── output/                # 人类输出、JSON、watch 与退出码
│   └── harness3-mcp/                  # 对外 MCP Server，使用同一平台客户端
│       └── src/
│           ├── server.rs / capabilities.rs
│           ├── tools/                 # 控制面与任务操作，分别鉴权
│           ├── resources/             # 目标状态、产物、平台文档
│           └── transports/            # stdio 首版，Streamable HTTP 后续
├── plugins/                           # 随平台发布的第一方插件包
│   ├── organization/                  # 消息、任务认领、委派、受限 agents.spawn
│   │   ├── plugin.yaml / schemas/
│   │   └── src/                       # 调用平台用例，不绕过组织服务事务
│   ├── workspace/                     # 文件/产物访问，路径与权限受控
│   │   └── plugin.yaml / schemas/ / src/
│   └── verification/                  # 确定性检查器与 Review 能力
│       └── plugin.yaml / schemas/ / src/
├── presets/                           # 开箱可用的母体，可在用户空间衍生
│   ├── coordinator/                   # 每项含 preset.yaml、instructions.md、参数 schema
│   ├── researcher/
│   └── reviewer/
├── web/
│   ├── package.json / lockfile
│   └── src/
│       ├── app/                       # 路由、认证、首启配置与通用布局
│       ├── features/
│       │   ├── presets/               # 定义、继承、版本、实例化表单
│       │   ├── agents/                # 实例组扩缩容、生命周期、谱系、实例详情
│       │   ├── plugins/               # 目录、授权、版本、热加载、rollout
│       │   ├── goals/                 # 任务图、时间线、消息、返工与验收
│       │   └── artifacts/             # 预览、来源、版本比较与下载
│       ├── api/                       # OpenAPI 类型客户端、SSE 与游标恢复
│       └── components/                # 状态、操作进度、权限/预算与错误展示
├── schemas/
│   ├── presets/ / plugins/            # 发布的配置与 manifest JSON Schema
│   └── api/                           # OpenAPI、公共对象与事件 schema
├── specs/
│   ├── agents/                        # Preset/继承、批量创建、生命周期、谱系
│   ├── plugins/                       # 包/RPC/绑定、热加载、撤销、兼容与回滚
│   ├── models/                        # 三类协议行为契约与兼容能力矩阵
│   ├── collaboration/                 # 任务、消息、委派、贡献、评审与验收
│   ├── reliability/                   # 所有权、故障矩阵、事务、幂等与恢复
│   └── governance/                    # 身份、预算、审计、保留期与评估协议
├── examples/
│   ├── quickstart.rs                  # 保留独立内核示例
│   ├── presets/researcher/            # 自定义母体、参数与发布/启动示例
│   ├── plugins/research-tools/        # 外部进程插件示例与热加载演示
│   │   ├── plugin.yaml / schemas/
│   │   └── src/ / README.md
│   ├── managed-agents/research/       # Goal、协作模板、离线资料、验收定义
│   └── clients/                       # API curl、MCP 配置与 CLI 等价操作
├── tests/
│   ├── contracts/                     # Preset、插件 RPC、三类模型协议、存储端口
│   ├── e2e/                           # 批量创建、生命周期、协作、四入口同源
│   ├── fault_injection/               # 所有权丢失、崩溃、重复消息、插件失联
│   ├── reload/                        # 运行中切换、排空、回滚、schema 兼容
│   ├── evaluation/                    # 单/多 Agent 等预算对照与质量指标
│   └── fixtures/                      # 模拟协议流、插件包、预置资料、损坏快照
├── scripts/                           # 构建 Web、打包、首启/e2e 冒烟、发布校验
├── deploy/
│   ├── docker/                        # Dockerfile 与本地 Compose
│   └── examples/                      # 配置、服务托管、多进程 Worker 示例
└── docs/
    ├── managed-agents.zh-CN.md / runtime.md / brand/
    ├── getting-started/               # 安装、demo、三类模型配置、真实模型运行
    ├── concepts/                      # 母体/实例/Worker/会话/插件/组织模型
    ├── guides/                        # 批量启动、衍生、扩缩容、插件开发/热加载
    ├── reference/                     # API、CLI、MCP、配置与错误码
    └── operations/                    # 备份、恢复、升级、预算与故障处置
```

依赖方向：入口适配器依赖 client/应用服务，platform 依赖 domain、core 与 ports；storage、worker、plugins 和 models 实现相应端口，由 server bootstrap 装配。插件作者只依赖公开协议/SDK；外部插件通过协议访问受授权平台能力，不依赖内部 SQL 表或 platform 私有 Rust 类型。Web 只依赖公开 HTTP/事件契约。

models 是官方三类协议的实现，plugins 是统一发现、授权、版本绑定与热加载的宿主机制，两者职责不同。对外 MCP Server 属于入口，远程 MCP 工具属于插件 loader，两者不共用一套未经区分的权限。

拆分时先建立最小 workspace 和版本化协议，再按实现规模拆出 crate；以上目录不要求一次性建立空壳。迁移核心路径时保留 Cargo 包 `harness3`、现有公开 API 和独立示例，避免平台演进迫使只用运行时的用户引入数据库、HTTP、插件进程或 Web 构建依赖。

## 13. 分阶段交付与完成条件

| 阶段 | 可演示结果 | 必须通过的验收 |
| --- | --- | --- |
| P0：规格与最小契约 | 固定 Preset/实例/实例组、插件协议与热加载、三类模型契约、协作与恢复规则 | 发布母体 → 启动 N 实例 → 绑定能力 → 任务验收的对象与状态可逐项解释，四入口语义一致 |
| P1：可持久化控制面与执行链路 | SQLite + daemon + 三类模型适配器，Preset 发布、N 实例批量创建、生命周期 API/CLI 与 Web 概览；初期验证单实例执行 | 幂等批量创建与配额有效；重启恢复控制面协调；三类协议契约通过；指定实例完成真实模型受限工具任务 |
| P2：本地 Managed Agents MVP | `up --demo` 从 Presets 启动协作实例组，四入口共用状态，第三方进程/远程插件可安装、绑定和热加载 | 扩缩容与受限派生、协作/返工/验收、预算/取消通过；插件新旧并存、排空、回滚与权限撤销经过真实平台链路 |
| P3：恢复与执行隔离 | 监督 Worker、多进程后端、审批等待、故障注入与对账 | 旧 Worker 不能提交有效状态；未知写入不盲重试；等待释放容量；身份和工作区越权被拒绝 |
| P4：远程与生态 | PostgreSQL、多机器 Worker、插件 SDK/分发生态与模板、发布自动化 | 分区、升级兼容、队列背压与配额在多 Worker 下有效；恢复/扩缩容不会重复产生业务效果 |

P0 只固定第一条垂直路径所需契约，不等待完整理论体系才开发。P1 直接围绕母体与实例控制面建立可靠存储和执行入口；P2 补齐协作与外部插件运行时热加载，并同步实现所需的最小所有权与恢复保护，不能把正确性全部延期到 P3。P3 扩大故障覆盖和执行隔离范围。多机器和复杂组织拓扑以实测需求进入后续阶段。

首次公开的“完整平台快速启动”应以 P2 为门槛。P1 可以作为单 Agent 服务预览发布，但需标注产品阶段。

## 14. 评估与验收清单

工程正确性和协作收益分开衡量：确定性演示证明链路和不变量，真实模型评估衡量任务效果。至少使用同样模型、资料、验收器与总预算，对比单 Agent 和协作平台；分别报告任务通过率、引用正确率、工具错误率、Token/费用、墙钟耗时和返工次数。多 Agent 不预设优于单 Agent。

发布 Managed Agents MVP 前，自动化验收至少覆盖：

- 在 CI 的干净环境构建发行物并启动，不依赖开发机已有数据库或服务；记录首启步骤数与耗时。
- 无 API Key 完成确定性演示；三类模型协议均有 fixture 契约测试，有有效配置可切换真实模型并完成同一类目标；两种模式显式区分。
- 同一 PresetVersion 批量创建 N 个独立实例，幂等重试不重复创建；配额拒绝与部分就绪进度明确，实例有独立会话、身份、工作区和资源绑定。
- 实例组扩缩容、finite/maintain、TTL、暂停/恢复/终止与父子实例策略符合规格，重启后协调继续，创建实例不要求每个实例常驻一个 Worker。
- Preset 继承无循环，发布版本与依赖锁不可变；母体或插件升级不静默改变已有实例。
- 第三方插件在不重启 daemon 的情况下安装和激活；一次 Run 的 schema/版本不漂移，新旧调用可并存，失败候选不污染旧 Registry。
- 插件排空、失联、结果未知、撤销与回滚有可见状态；未授权插件不能调用平台管理能力，回滚不抹去已发生的外部效果。
- CLI 创建的 goal_id 能被 Web/API/MCP 读取和操作，认证拒绝与幂等冲突行为一致。
- 两个 Worker 同时认领只产生一个有效所有者；旧 generation 无法写入；队列和持久会话事务失败后可恢复。
- 重复命令和消息不重复生成任务或产物；消费者重启后可补读平台事件。
- 在派发前、工具执行后回执前、结算时注入崩溃，已知结果保留、未知结果可见，外部写入需对账才决定后续动作。
- Agent 只能访问获准任务、工作区与凭证；委派和来自 API/MCP 的客户端字段不能扩大权限。
- 并发预算预留不超额；未知 usage 和已发出请求的费用边界不被隐藏。
- 验收失败不会标记 Goal 成功；模型中断、评审失败、用户取消都有可追踪终态或待处理状态。

后续按实现清单补齐维持型实例组与异步 Operation、通用 CLI/MCP、外部插件和协作返工；每项能力同时更新 Web、文档与验收证据。P3/P4 的隔离和远程语义需要专门故障测试，不以单机演示代替。本文不承诺尚未测量的工期或集群容量。
