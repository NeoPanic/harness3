# 任务候选池、认领与执行尝试

WorkItem 描述需要完成的工作，Attempt 记录它的一次认领与执行，Run 保存那次实际模型/工具调用。释放或下一轮返工会生成新的尝试，旧 Run 的结算状态与来源记录保留，不被覆盖。

## 操作方式

创建协作目标时，每项任务仍指定一个主实例；可展开“候选实例与认领方式”，添加最多 15 个备用候选，或勾选人工认领。默认行为保持为固定实例自动派发。

- **自动分配**：依赖成功后，控制面按批准顺序选择首个就绪、未过期候选。主实例忙碌或暂停时可选择备用实例。同一实例被原子标记 busy，不能同时拿到两个已接纳 Run。
- **人工认领**：依赖就绪后仍保持 pending，由操作者从批准候选中选择实例。领取成功表示 Attempt 和 Run 已接纳，实际执行可能仍在等待 Worker 槽位。
- **释放认领**：只允许释放 queued 且从未开始的 Run。平台取消旧 Run、保留 Attempt 与释放原因、清空当前任务关联，并将该任务改为人工认领。后续领取递增 generation，旧页面不能释放新尝试。
- **已开始的执行**：运行中、已结算或结果未知的 Run 不允许通过释放入口重试。可使用现有取消流程收尾；取消不撤销外部效果。

任务卡片展示实际实例、候选数量、自动/人工方式与历史台账。台账按 generation 倒序排列，包含轮次、实例、Run、认领来源、开始/完成时间、错误与释放原因；每个旧 Run 可独立检查。

候选由操作者明确批准，允许不同母体和模型配置。平台依据生命周期与可用性选择，不判断候选是否具备完成研究任务的语义能力，也不会自动给候选新增插件。复用实例沿用其已有会话，需要隔离历史时先创建新实例。

## 事务与身份约束

认领在同一 SQLite 事务检查 Goal 运行状态、轮次、当前 generation、任务未认领、依赖成功、候选身份、实例就绪与截止时间；随后创建唯一 Run、Attempt、共享区派发快照、实例 busy 状态、事件与幂等回执。失败整体回滚。

`expected_generation` 是该任务最近一次尝试的代次，新任务为 0；成功认领分配下一个代次。代次跨评审轮次持续增加，每轮最多 8 次认领尝试。并发认领只能有一个提交者，其他操作需要重新读取；超时后用相同 key 和内容查询原接纳结果，不能换 key 猜测重试。

释放也匹配轮次和 generation，并与 Worker 的 queued→running 事务竞争：Worker 已开始则释放失败。Run 结算同时核对实例身份；旧 Attempt、错误实例或已释放 Run 不能结算当前尝试。单 daemon 的数据目录独占锁仍是执行所有权边界，这些代次不构成多机器 Worker 的租约或外部副作用 fencing。

取消覆盖当前尝试和未认领工作。人工要求返工时，每项任务继续使用新团队或明确的新分配，新一轮从单候选自动派发开始，不沿用旧候选池或人工等待策略；新旧轮次的 Attempt 都保留。调整返工团队的候选策略尚未开放。

## API / CLI / MCP

| 操作 | HTTP | CLI | MCP |
| --- | --- | --- | --- |
| 任务与尝试详情 | `GET /api/v1/goals/{id}/tasks/{task}` | `tasks show ID TASK` | `tasks_show` |
| 认领并接纳执行 | `POST /api/v1/goals/{id}/tasks/{task}/claim` | `tasks claim ID TASK --file claim.json --key KEY` | `tasks_claim` |
| 释放未开始尝试 | `POST /api/v1/goals/{id}/tasks/{task}/release` | `tasks release ID TASK --file release.json --key KEY` | `tasks_release` |

两个写入口需要 `Idempotency-Key`。外部 MCP 默认只读；写入仍需 `--allow-write`。四入口使用同一本地操作者身份，尚无细粒度 Principal/Grant。

创建 Goal 的任务对象支持以下可选字段：

```json
{
  "key": "research",
  "instance_id": "agent_primary",
  "candidates": ["agent_backup"],
  "manual_claim": true,
  "instructions": "比较资料并提供证据",
  "depends_on": []
}
```

`candidates` 仅列备用实例，不重复主实例；最多 15 个且不可重复。所有候选必须存在且未终止/排空，认领时重新校验状态与到期时间。

认领输入：

```json
{"expected_round": 1, "expected_generation": 0, "instance_id": "agent_backup"}
```

释放输入：

```json
{"expected_round": 1, "expected_generation": 1, "note": "尚未开始，改由另一候选执行"}
```

示例见 [pool-goal.json](../examples/clients/pool-goal.json)、[claim.json](../examples/clients/claim.json) 和 [release.json](../examples/clients/release.json)。先替换为实际实例 ID。创建 Goal 仍限制最多 64 项任务、32 个活动/待验收 Goal，并沿用全局实例与 Worker 容量。

## 自主接续与执行边界

绑定 `platform.tasks@1.0.0` 的 Agent 可以读取任务板，并在当前 Run 中请求自己候选池内的下一项任务。请求持久化后，平台等待来源 Run 成功和实例释放，再按相同的认领事务创建 Attempt 与 Run。工具不允许模型自称其他实例，也不能绕过候选、generation 与依赖检查；详见[自主任务板](autonomy.zh-CN.md)。

认领以单 daemon、SQLite 和本地 Worker 为边界。没有跨机器租约/续租、fencing、开始后的安全重试/对账或自动角色匹配。当前不设任务费用预算，候选是否具备完成任务的能力由操作者选择母体与插件时确定。
