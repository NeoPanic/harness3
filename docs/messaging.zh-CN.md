# 本地持久协作消息 v1

消息链路与普通运行、显式 Goal 任务共用实例串行约束和四个 Worker 槽位。当前支持单机 SQLite 中的点对点任务消息；频道、工作认领/Attempts、远程投递与 Principal/Grant 尚未实现。

## 状态与事务

- 接纳：不可变消息封套、Outbox `pending`、幂等回执与事件在同一事务提交。回执只证明接纳；同一 key 与相同内容始终返回原回执，用详情查询当前状态。变更正文或上下文后复用 key 返回 409。
- 投递：协调器将 Inbox `pending` 与 Outbox `delivered` 原子提交。稳定消息 ID 和 Inbox 主键去重。接收者已排空/终止时标记 `undeliverable`，不会转投补建的新身份。
- 派发确认：接收者 ready 且不存在未处置的未知消费结果时，原子创建唯一 queued Run、将实例设为 busy，并记录 Inbox `queued`、`run_id`、`ack_at`。忙碌/暂停实例保留积压；同一实例按消息 seq 派发，暂停实例不会占住其他实例的扫描窗口。
- 消费：Run 领取后 Inbox 为 `running`；运行完成为 `consumed`，明确失败/取消为 `failed`/`cancelled`。模型给出的答案是否满足任务要求仍需核对证据，`consumed` 不表示业务验收通过。
- 未知效果：启动恢复发现中断，或运行报告包含未知工具效果时，Inbox 为 `unknown`。即使模型随后生成答复，未知工具效果仍会阻止该实例后续 Inbox 派发。平台不自动重放消费；操作者核对 Run/外部效果后填写处置说明，解除阻塞，原消息保持 unknown。

每轮最多处理 32 次投递、32 次状态结算和 32 条候选消费。消息协调先于 Goal 协调；两者均等待实例 ready，不预留专用实例，也不承诺跨来源公平性。快照状态可能比 Run 终态晚一个协调周期。

可丢弃尚未投递的 Outbox 或尚未派发的 Inbox；派发后只能通过 Run 取消入口请求协作取消。discard/resolve 都要求非空处置说明和幂等 key，保留审计事件。resolve 不重试、不撤销已发生的外部效果。未知消费阻塞只作用于 Inbox，操作者仍可主动提交普通 Run。

## 身份与第一方组织插件

母体可显式绑定 `platform.messaging@1.0.0`，内置目录返回种类 `organization`、工具契约及内容摘要。发布时固定完整快照；同 ID 的外部包会被拒绝。无需开启 `--allow-process-plugins`，但没有绑定的 Agent 不会获得发送工具。

`platform.messaging.send({recipient_id,text,key})` 只接受接收者、正文和幂等 key。发送实例、来源 Run、Goal 与因果关系由宿主推导；Agent 不能在参数中自称其他身份。当前来源必须为未请求取消的运行中 Run，实例为 busy，并绑定宿主支持的精确能力快照。

接收者必须与发送者同一实例组，或双方属于发送者当前运行关联的活动 Goal。消息消费产生的后续发送继承前置消息 ID、根关联 ID、目标和深度；普通 Goal Run 从 WorkItem 推导目标。携带 Goal 上下文时接收者必须仍是该活动 Goal 的成员。操作者发送使用本地 token 身份，不伪造 Agent 来源，可显式填写目标/关联/前置消息；前置消息的上下文不可被覆盖。

这些约束是本地组织能力的最小作用域。当前没有细粒度 Grant、租户隔离、审批或预算预留。消息正文进入模型时明确标为不可信任务数据，不能覆盖系统指令或授予工具权限；这不构成对提示词注入的隔离证明。发送回执中的 `changed=true` 表示已有持久接纳证据，幂等重放也会返回原接纳证据。

## 上限

| 项目 | 当前限制 |
| --- | --- |
| 正文 | 非空，最多 8,000 UTF-8 字节 |
| 幂等 key | 1–160 字节；HTTP client 使用可打印 ASCII |
| 单实例 / 工作区积压 | 64 / 512 条，包含待投递、待消费、活动消费及未处置 unknown |
| 每个来源 Run 发送 | 最多 8 条 |
| 因果深度 | 根为 0，最大 8 |
| 同一关联 ID | 生命周期内最多 128 条，处置不重置计数 |
| 处置说明 | 非空，最多 2,000 UTF-8 字节 |
| 查询 | seq 升序，默认 50、最多 100；快照仅最近 100 条 |

限额校验与接纳写入处于同一事务。独立数据库连接竞争可能返回存储冲突；重试须保持相同 key 与内容。消息当前没有自动过期与历史清理策略。

## 四入口

| 操作 | HTTP | CLI | MCP |
| --- | --- | --- | --- |
| 发送 | `POST /api/v1/agent-instances/{id}/messages` | `messages send ID --file message.json --key KEY` | `messages_send` |
| 查询 | `GET /api/v1/messages?after=0&limit=50` | `messages list --after 0 --limit 50` | `messages_list` |
| 实例 Inbox / Outbox | 增加 `instance_id=ID&direction=inbox` 或 `outbox` | `--instance-id ID --direction inbox` | 同名字段 |
| 详情 | `GET /api/v1/messages/{id}` | `messages show ID` | `messages_show` |
| 处置 | `POST /api/v1/messages/{id}/actions` | `messages action ID discard --note TEXT --key KEY` | `messages_action` |

HTTP 写请求需要 `Idempotency-Key`。发送返回 202；正文是 `{"text":"请检查交接资料"}`，可增加 `goal_id`、`correlation_id`、`causation_id`。处置正文为 `{"action":"discard","note":"取消这个尚未派发的请求"}` 或 `resolve`。MCP 写操作需要 `--allow-write`；发送可能触发付费模型调用。

Web **协作消息**提供结构化发送表单、独立投递/消费列、因果封套、原始消息、消费 Run 链接和处置表单。列表轮询，详情手动刷新。母体发布表单可直接选择内置组织插件；当前没有自动寻找伙伴或广播。

## 验证

`tests/messaging.rs` 覆盖去重、三阶段状态、FIFO、暂停积压、终止身份、崩溃恢复、未知工具效果、人工处置、发送身份/能力/作用域、限额和并发连接。Responses fixture 通过真实 Provider → Harness → 组织工具 → 接收 Run 验证跨 Agent 交接，无外部模型费用。`tests/entrances.rs` 覆盖 CLI/HTTP/MCP 共用回执、只读限制、认证及身份伪造拒绝。
