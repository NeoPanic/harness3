# 自主任务板、领取与追加

母体绑定 `platform.tasks@1.0.0` 后，当前 Goal 的 Agent 可以查看任务板、请求自己下一项工作，以及使用本地已发布母体追加必要任务。宿主从当前 Run 绑定 Goal、任务与实例身份；工具不接受调用者伪造这些身份。

## 操作流程

1. 当前 Agent 调用 `platform.tasks.board` 查看已完成与待处理任务、候选池和认领 generation。
2. 遇到额外研究需求时调用 `platform.tasks.append`：填写任务 key、指令、已有依赖、母体版本与参数。平台自动增加当前任务依赖，保留原图，等待本次 Run 成功。
3. 来源 Run 成功后，协调器创建新 Agent 池和任务，并让尚未派发的直接下游（例如汇总）等待新增贡献；新任务完成后发布产物，参与汇总、验证与验收。同一来源追加的研究任务可以并行。
4. 若当前 Agent 已在某个待领取任务的候选池中，可以调用 `platform.tasks.claim` 提交下一任务意图。当前 Run 成功并释放实例后，协调器沿用原来的事务认领接口派发后续工作；Agent 无需保持当前 Run 等待。
5. Web 的“自主任务操作”显示来源、目标、待生效/追加/已领取/失败以及对应 Run。

## 工具输入

`platform.tasks.board` 参数为 `{}`。

```json
{
  "key": "follow-up",
  "instructions": "核对缺失的数据来源，并保留不能核实的结论。",
  "depends_on": ["research-1"],
  "preset_name": "research-team",
  "preset_version": "1.1.0",
  "parameters": {},
  "command_key": "research-follow-up-1"
}
```

以上为 `platform.tasks.append` 输入。`depends_on` 仅允许同 Goal 已存在任务，当前任务自动纳入依赖；一次 Run 最多提交 8 项，整个 Goal 包含待追加与待委派额度后最多 64 项。新任务使用独立会话，并采用目标母体的工具。当前采用可信本地完全权限模式，可选择任何已发布母体；不设团队费用预算，也不额外收窄已发布母体的工具能力。

```json
{
  "task_key": "pending-review",
  "expected_generation": 0,
  "command_key": "take-review-next"
}
```

以上为 `platform.tasks.claim` 输入。目标必须待领取，当前 Agent 必须已在批准候选池中；一条 Run 只能有一个待执行的下一任务请求。控制面的事务接口创建 Run 和 Attempt 后才完成实际认领；工具返回的是待执行意图的接纳回执。

频道通知唤醒的 Run 可读取任务板，此时 `source_task=null`、`can_request_tasks=false`；追加与下一任务领取仍需来源任务 Run。

## 持久状态与边界

`autonomy_intents` 保存来源 Run、轮次、实例、generation、请求指纹、结果与错误。同一来源 Run 和 command_key 重放原接纳回执，修改内容返回冲突。来源失败、取消或结果未知都不会激活请求；已完成来源的追加分配失败会回滚分配并显示错误。下一任务的认领使用固定幂等键，进程在认领完成、意图结算前中断时仍可重读原回执。

任务板支持追加任务，并补充尚未派发的直接下游依赖，不删除或修改已运行节点，也不自动迁移任意子树。已经派发的汇总不会被打断；这种情况下需追加新的整合任务或后续发起返工。新任务已有依赖的祖先不会反向等待它，避免产生循环；更新后重新计算拓扑顺序。当前不提供任意图重写或跨机器任务租约。验证驱动的局部修复使用[验证策略](verification.zh-CN.md#自动返工与重验)。
