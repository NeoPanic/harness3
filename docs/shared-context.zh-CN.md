# 目标共享上下文

共享上下文让同一 Goal 的 Agent 发布中间发现、决策和疑问，并读取其他参与者的更新。它与任务的不可变最终产物分开保存：共享记录可以修订和撤回，每个历史版本保留；平台记录来源身份和执行状态，不判断内容是否真实。

## 本地体验

启动平台后，在“自主调研”选择 `demo-team@1.0.0`，无需模型账号即可体验多实例读写、文件交接和频道讨论。每位研究者发布带宿主身份的记录；汇总实例在上游成功后收到共享区快照，并可调用工具读取最新内容。仅体验共享上下文时也可选择 `demo-collaborator@1.0.0`。Demo 输出为确定性演示，不进行语义研究。

配置 Responses 后推荐 `research-team@1.1.0`，其中包含 `platform.context@1.0.0`；仅需共享区时可使用 `research-collaborator@1.0.0`。真实模型是否发布记录取决于实际工具调用；没有检索插件时仍只能分析输入资料和模型已有知识。

目标与调研详情显示“共享上下文”：类型、修订号、所属轮次、来源实例/Run、来源状态和证据产物。操作者可发布、修订、撤回并查看不可变历史。更新冲突保留草稿；点击“读取最新版本以核对”后对照新内容合并，再提交。Run 详情保留派发时快照；验收历史保留当时固定的共享区。

## 身份与可见范围

- 共享区按 Goal 隔离；该 Goal 所有任务派发时都会固定当前共享区快照并接收该快照，后续修改不回写旧 Run 输入。
- 实例显式绑定 `platform.context@1.0.0` 才能调用实时读写工具。宿主从当前运行的 WorkItem 推导 Goal、Run 和实例；模型不能指定其他目标或冒充发布者。
- 实时工具服务于仍在运行、未取消、所属 Goal 运行中的参与者，也支持具有有效频道成员关系的通知消费 Run。普通单实例 Run 和规划 Run 不获得共享区工具权限；规划阶段仅允许提交计划。
- 平台 token 持有者通过 API/CLI/MCP/Web 以操作者身份发布。目标运行中或待验收时可更新，终结后只读；尚无 Principal/Grant 细分授权。
- 不自动共享私有会话，不自动广播任务，不因发布记录而触发新模型运行。共享内容属于不可信任务数据，不能扩大权限。

## 修订、证据与上限

记录包含 `key`、`expected_revision`、`title`、`body`、`kind`、`status`、`artifact_ids`。类型为 observation / decision / question；状态为 active / withdrawn。新键用 `expected_revision=0`，更新须匹配当前条目版本。每次成功写入同时递增 Goal 的共享区版本，追加历史、幂等回执与事件；拒绝写入时整个事务回滚。

| 限制 | 当前值 |
| --- | --- |
| 每个 Goal 的键 / 总修订次数 | 32 / 128，撤回不释放历史容量 |
| 当前共享区序列化大小 | 24 KiB，包含来源元数据 |
| 每个 Agent Run 的成功写入 | 8 次，重放同一幂等回执不重复计数 |
| 键 / 标题 / 正文 | 80 个 ASCII 名称字符 / 200 / 4,000 UTF-8 字节 |
| 每条记录的产物引用 | 最多 8 个，不重复 |

引用只接受同一 Goal 的成功产物，并检查来源运行无未知效果和产物摘要。来源 Run 运行中、失败、中断或取消的记录保持可见，显示未结算状态。验收必须匹配当前轮次与共享区版本；有效记录的来源 Run 必须成功且无未知效果，否则拒绝验收。操作者需调查并明确修订或撤回不可靠记录。

验收在同一事务固定产物清单和共享区快照；后续审阅可定位当时的完整版本。返工保留共享区及其每条记录的原始轮次，新团队可读取既有发现；新记录不会被伪装成旧轮次贡献。

## 四个入口

| 操作 | HTTP | CLI | MCP |
| --- | --- | --- | --- |
| 读取 | `GET /api/v1/goals/{id}/context` | `context show ID` | `context_read` |
| 历史 | `GET /api/v1/goals/{id}/context/{key}` | `context history ID ENTRY` | `context_history` |
| 发布 / 修订 / 撤回 | `POST /api/v1/goals/{id}/context` | `context publish ID --file context.json --key KEY` | `context_publish` |
| 验收 | `POST /api/v1/goals/{id}/accept` | `goals accept ID --review TEXT --expected-round N --expected-context-revision N` | `goals_accept` |

HTTP 写入共享区需要 `Idempotency-Key`。Agent 工具为 `platform.context.read` 和 `platform.context.publish`；后者使用 `command_key`。超时重试保持同一 key 和相同内容，冲突合并后使用新 key。外部 MCP 仍默认只读，写入需 `--allow-write`。

示例文件见 [context.json](../examples/clients/context.json)。

```json
{
  "key": "evidence.scope",
  "expected_revision": 0,
  "title": "资料范围",
  "body": "本轮仅对用户提供的资料进行比较，尚未联网核实。",
  "kind": "observation",
  "status": "active",
  "artifact_ids": []
}
```

## 与其他协作能力的关系

共享上下文适合持续更新的发现与决策；[频道](channels.zh-CN.md)提供分组讨论和订阅唤醒，[文件工作区](workspaces.zh-CN.md)提供不可变文件/JSON 快照，[任务认领](task-claims.zh-CN.md)管理执行尝试。[独立验证](verification.zh-CN.md)可以核查固定共享区与产物，仍不能自动证明外部事实。

并发修订需显式读取最新版本并合并，平台不自动裁决内容冲突。共享区按 Goal 隔离，不提供跨目标共享协议或外部检索记忆库。
