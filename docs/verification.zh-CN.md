# 独立验证与有界自动返工

创建 Goal 或自主调研时可以指定 `verification`。所有任务交付后，目标进入 `verifying`，平台创建独立实例与会话，核查本轮固定产物和共享上下文。最终验收仍由用户完成。

## 配置与入口

Web 创建目标和自主调研表单提供“独立验证与自动返工”：选择固定版本母体及参数，逐行填写 1–8 项检查，选择最多 0–2 次自动返工。API 的 GoalPlan / ResearchInput 使用相同对象：

```json
{
  "verification": {
    "preset_name": "demo-verifier",
    "preset_version": "1.0.0",
    "parameters": {},
    "checks": [
      {"key": "evidence", "description": "关键结论是否有产物证据支持？"},
      {"key": "uncertainty", "description": "是否明确说明未核实的信息？"}
    ],
    "max_auto_reworks": 0,
    "auto_rework_scope": "failed-artifacts"
  }
}
```

`demo-verifier` 只演示读证据与提交报告，不判断内容质量；配置 Responses 后可用 `research-verifier`。验证母体的模型、基础指令和参数生效，但插件工具不继承。当前验证实例仅有 `platform.verification.evidence` 和 `platform.verification.report`，不能检索外部事实或执行外部写入。

## 证据与结论

验证启动时固定产物清单、摘要、轮次和共享上下文版本。清单包含本轮任务实际使用的最终文本，以及对应成功 Run 发布的文件和 JSON；局部返工沿用的成功 Run 及其快照继续纳入，被替换的旧 Run 不混入新清单。证据工具以 Unicode 字符分页，每次最多 12,000 字符，并记录读取区间。报告必须逐项返回 `passed` / `failed` / `unknown`、理由和产物引用；通过项必须至少引用一个完整读取的清单内产物。外部 Goal 的产物、未读引用、缺项或重复检查项均被拒绝。

报告提交后不可修改，只有对应 Run 成功才生效。Run 失败、取消、结果未知或缺少报告都不能形成验证通过。运行期间共享上下文发生变化，结论标记 `stale`；通过后再改上下文也会阻止验收，需重新验证。模型结论仍受证据质量与模型能力限制，读取完整性不等于事实真实性。

有效结果有失败项时为 `changes-requested`；其余结果中有无法判断项则为 `inconclusive`；全部通过才为 `passed`。Web 展示每项理由、证据链接、验证实例与 Run、历史尝试和自动返工轮次，未生效的报告明确标记。

## 自动返工与重验

显式允许自动返工后，失败检查触发新团队返工，并把检查理由带入任务输入。`auto_rework_scope` 支持两种策略：

- `failed-artifacts`：从所有失败检查引用的冻结产物定位来源任务，重跑这些任务及其下游，其余成功贡献沿用原始身份和版本。任一失败检查没有产物引用时，回退为整图返工。
- `all` 或省略：重跑整张任务图。

Web 默认选择按失败证据定位任务及下游。每个目标最多自动返工两次，包含人工返工在内总共最多三轮；下一轮重新验证完整交付，包括沿用证据。证据不足、分配失败或运行失败留给用户处理，容量不足时回滚返工分配并保留失败原因。当前不设团队费用预算，重复执行可能重复外部工具效果及模型费用；自动定位依赖报告引用和已声明任务依赖，不提供通用外部副作用对账。

每轮最多三次验证，重新验证使用新实例和最新上下文：

- Web：目标或调研详情 → 独立验证 → 重新验证。
- HTTP：`POST /api/v1/goals/{id}/verification/retry`，带 `Idempotency-Key`；body 为 `{"expected_round":1,"expected_verification_id":"verification_..."}`。
- CLI：`harness3 goals verify ID --expected-round 1 --expected-verification-id VERIFICATION_ID --key RETRY_KEY`。
- MCP：`verification_retry`，需要 `--allow-write`；读取完整记录使用 Goal 查询。

验证期限为 180 秒，Agent 池 TTL 为 300 秒；取消会停止排队运行并等待活动 Run 收尾。数据库保存策略、冻结清单、报告和读取区间。执行采用单 daemon 的本地状态协调，不提供分布式 Worker 或确定性历史重放。
