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
    "max_auto_reworks": 0
  }
}
```

`demo-verifier` 只演示读证据与提交报告，不判断内容质量；配置 Responses 后可用 `research-verifier`。验证母体的模型、基础指令和参数生效，但插件工具不继承。当前验证实例仅有 `platform.verification.evidence` 和 `platform.verification.report`，不能检索外部事实或执行外部写入。

## 证据与结论

验证启动时固定产物清单、摘要、轮次和共享上下文版本。证据工具以 Unicode 字符分页，每次最多 12,000 字符，并记录读取区间。报告必须逐项返回 `passed` / `failed` / `unknown`、理由和产物引用；通过项必须至少引用一个完整读取的本轮产物。外部 Goal 的产物、未读引用、缺项或重复检查项均被拒绝。

报告提交后不可修改，只有对应 Run 成功才生效。Run 失败、取消、结果未知或缺少报告都不能形成验证通过。运行期间共享上下文发生变化，结论标记 `stale`；通过后再改上下文也会阻止验收，需重新验证。模型结论仍受证据质量与模型能力限制，读取完整性不等于事实真实性。

有效结果有失败项时为 `changes-requested`；其余结果中有无法判断项则为 `inconclusive`；全部通过才为 `passed`。Web 展示每项理由、证据链接、验证实例与 Run、历史尝试和自动返工轮次，未生效的报告明确标记。

## 自动返工与重验

显式允许自动返工后，检查不通过触发整图新团队返工，并把检查理由带入任务输入。每个目标最多自动返工两次，包含人工返工在内总共最多三轮。证据不足、分配失败或运行失败留给用户处理；容量不足时回滚返工分配并保留失败原因。重复执行可能重复外部工具效果及模型费用，当前没有局部返工、费用预算或通用外部副作用对账。

每轮最多三次验证，重新验证使用新实例和最新上下文：

- Web：目标或调研详情 → 独立验证 → 重新验证。
- HTTP：`POST /api/v1/goals/{id}/verification/retry`，带 `Idempotency-Key`；body 为 `{"expected_round":1,"expected_verification_id":"verification_..."}`。
- CLI：`harness3 goals verify ID --expected-round 1 --expected-verification-id VERIFICATION_ID --key RETRY_KEY`。
- MCP：`verification_retry`，需要 `--allow-write`；读取完整记录使用 Goal 查询。

验证期限为 180 秒，实例组 TTL 为 300 秒；取消会停止排队运行并等待活动 Run 收尾。数据库 schema v12 保存策略、冻结清单、报告和读取区间。当前仍是单 daemon 的本地状态协调，不具备分布式 Worker 或确定性历史重放。

## 实现落点

```text
crates/harness3-platform/
├── src/verification/
│   ├── mod.rs       # 策略、证据读取、结算、返工、取消与验收门槛
│   └── tools.rs     # 宿主绑定的验证工具
├── web/verification.js  # 配置、逐项报告、历史与重新验证
└── tests/verification.rs # 证据范围、独立身份、过期结论与返工约束
```

## 验证记录

9 组验证核心测试覆盖 Unicode 分页与读取缺口、宿主身份与证据范围、报告不可变、失败/未知运行禁止生效、上下文过期与重验、自动返工授权上限、取消排空和重启恢复。隔离 Demo 的浏览器联调完成两位研究者、汇总、独立验证、重验与人工验收；1024 / 1440 / 1920 px 无页面横向溢出。Demo 结果不用于声称真实模型核查质量，尚未进行屏幕阅读器与跨浏览器验证。

真实 Responses `gpt-6-luna` 验证记录：`research_414f7393ac24486ca890fc8c6bb9c15f`，Goal `goal_4e602aeed998440f8b8d5458b75f3590`。两位研究者运行区间重叠，汇总成功后独立验证 Run `run_63973fe3e08e4460a55fb607a5232e9f` 读取三份产物并提交通过报告，未调用其他工具，验证实例自动回收。目标保留待人工验收。这证明一次真实模型工具链可用，不代表通用事实核验准确率或多 Agent 优于单 Agent。

跨入口测试另验证 CLI 重新验证请求经 HTTP 接纳，MCP 使用同一幂等键取得相同回执，默认只读 MCP 拒绝写操作，且不会重复创建验证任务。
